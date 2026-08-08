//! Fallback USD rates when `costUsdTicks` is absent.
//!
//! **Prefer server-reported ticks always.** These rates are only used when a
//! turn has no positive `costUsdTicks` / `cost_usd_ticks`.
//!
//! Resolution chain for list-price fallback:
//! custom.toml → models_dev.json catalog → embedded seed → placeholder.
//!
//! Cost display is gated by [`CostMode`] stored in `~/.grok/usage/pricing.toml`.

use serde::{Deserialize, Serialize};

use super::catalog::{
    fuzzy_lookup, load_catalog, load_custom, lookup_rates_in, seed_catalog, PLACEHOLDER_RATES,
};

/// How historical / estimated cost is attributed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CostMode {
    /// Do not attribute USD (tokens still recorded).
    Off,
    /// Attribute cost for all models.
    #[default]
    All,
    /// Attribute cost only for official (xAI/Grok) models.
    OfficialOnly,
}

impl CostMode {
    pub fn as_str(self) -> &'static str {
        match self {
            CostMode::Off => "off",
            CostMode::All => "all",
            CostMode::OfficialOnly => "official_only",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            CostMode::Off => "off",
            CostMode::All => "all",
            CostMode::OfficialOnly => "official",
        }
    }

    /// Cycle Off → All → OfficialOnly → Off (caller should clamp after).
    pub fn next(self) -> Self {
        match self {
            CostMode::Off => CostMode::All,
            CostMode::All => CostMode::OfficialOnly,
            CostMode::OfficialOnly => CostMode::Off,
        }
    }
}

/// User pricing preferences (`~/.grok/usage/pricing.toml`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PricingConfig {
    #[serde(default)]
    pub mode: CostMode,
    /// Show live per-turn `$` on the prompt info line and subagent frames.
    #[serde(default)]
    pub live_display: bool,
    /// When true, refresh models.dev catalog on usage sync.
    #[serde(default = "default_true")]
    pub auto_sync_catalog: bool,
}

fn default_true() -> bool {
    true
}

impl Default for PricingConfig {
    fn default() -> Self {
        Self {
            mode: CostMode::All,
            live_display: false,
            auto_sync_catalog: true,
        }
    }
}

impl PricingConfig {
    pub fn path() -> Option<std::path::PathBuf> {
        xai_grok_config::user_grok_home().map(|h| h.join("usage").join("pricing.toml"))
    }

    /// Load from disk; create with defaults if missing.
    pub fn load() -> Self {
        let Some(path) = Self::path() else {
            return Self::default();
        };
        if !path.is_file() {
            let cfg = Self::default();
            let _ = cfg.save();
            return cfg;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        toml::from_str(&text).unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::path().ok_or_else(|| "no grok home".to_string())?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let body = format!(
            r#"# Grok usage cost attribution
# mode: off | all | official_only
mode = "{}"
live_display = {}
auto_sync_catalog = {}
"#,
            self.mode.as_str(),
            self.live_display,
            self.auto_sync_catalog,
        );
        std::fs::write(path, body).map_err(|e| e.to_string())
    }
}

/// Non-subscription auth cannot use OfficialOnly — clamp to All.
pub fn clamp_mode_for_auth(mode: CostMode, is_official_subscription: bool) -> CostMode {
    if !is_official_subscription && mode == CostMode::OfficialOnly {
        CostMode::All
    } else {
        mode
    }
}

/// Whether this model should receive a non-zero cost under `mode`.
pub fn should_include_model(mode: CostMode, model: &str) -> bool {
    match mode {
        CostMode::Off => false,
        CostMode::All => true,
        CostMode::OfficialOnly => is_official_model(model),
    }
}

/// Compact USD: `$0.12` / `$12.34` (two decimals; tiny values show more precision).
pub fn format_usd(v: f64) -> String {
    if !v.is_finite() {
        return "$0.00".into();
    }
    let abs = v.abs();
    if abs > 0.0 && abs < 0.01 {
        format!("${v:.4}")
    } else {
        format!("${v:.2}")
    }
}

/// Estimate USD from token counts for a model id (fallback only).
///
/// Resolution: custom → models_dev catalog → embedded seed → placeholder.
/// Does **not** apply [`CostMode`]; callers gate with [`should_include_model`].
pub fn estimate_usd(model: &str, input: u64, output: u64, cached_read: u64) -> f64 {
    let (in_per_m, out_per_m, cache_per_m) = rates_for_model(model);
    // Cached tokens are billed at the cached rate; the rest of input at full input rate.
    // This matches how usage is typically split: cached_read ⊆ input.
    let billable_in = input.saturating_sub(cached_read.min(input));
    (billable_in as f64 / 1_000_000.0) * in_per_m
        + (cached_read as f64 / 1_000_000.0) * cache_per_m
        + (output as f64 / 1_000_000.0) * out_per_m
}

/// `(input, output, cached_input)` USD per 1M tokens.
fn rates_for_model(model: &str) -> (f64, f64, f64) {
    let custom = load_custom();
    let catalog = load_catalog();
    if let Some(r) = lookup_rates_in(&custom, &catalog, model) {
        return r;
    }
    // Explicit seed pass (load_catalog already falls back to seed when file missing,
    // but if the on-disk file exists and omits a model, still try seed).
    if let Some(r) = fuzzy_lookup(&seed_catalog().models, model) {
        return r.as_tuple();
    }
    PLACEHOLDER_RATES
}

/// Whether this model id counts toward official subscription reverse-calc.
pub fn is_official_model(model: &str) -> bool {
    let m = model.to_ascii_lowercase();
    m.starts_with("grok") || m.contains("grok-") || m.contains("grok_")
}

/// Convert ticks to dollars.
///
/// xAI / Grok report cost as **integer micro-dollars at 1e10 scale**:
/// `1 USD = 10_000_000_000 ticks` (see `USD_TICKS_PER_USD` in shell, and
/// `cost_in_usd_ticks` on the Responses API usage object).
pub fn ticks_to_usd(ticks: i64) -> f64 {
    ticks as f64 / 10_000_000_000.0
}

/// Cycle cost mode for the Usage modal (`p` key), respecting auth clamp.
/// Returns the mode that was saved.
pub fn cycle_cost_mode(is_official_subscription: bool) -> CostMode {
    let mut cfg = PricingConfig::load();
    let mut next = cfg.mode.next();
    next = clamp_mode_for_auth(next, is_official_subscription);
    // If clamp kept us on All after OfficialOnly→… from non-sub All→OfficialOnly,
    // advance again to Off so the key always changes something.
    if next == cfg.mode {
        next = match cfg.mode {
            CostMode::Off => CostMode::All,
            CostMode::All => CostMode::Off,
            CostMode::OfficialOnly => CostMode::Off,
        };
        next = clamp_mode_for_auth(next, is_official_subscription);
    }
    cfg.mode = next;
    let _ = cfg.save();
    cfg.mode
}

/// Toggle `live_display` in pricing.toml (prompt / subagent frame live `$`).
/// Returns the new value.
pub fn toggle_live_display() -> bool {
    let mut cfg = PricingConfig::load();
    cfg.live_display = !cfg.live_display;
    let _ = cfg.save();
    cfg.live_display
}

/// USD to show in the `/usage` UI for one model row under `mode`.
///
/// - `Off` / non-included models → `0`
/// - Prefer stored `cost_usd` when &gt; 0 (ticks or prior estimate)
/// - Otherwise re-estimate from tokens (covers scans done while mode was `off`)
pub fn display_cost_usd(mode: CostMode, model: &str, m: &super::store::ModelTotals) -> f64 {
    if !should_include_model(mode, model) {
        return 0.0;
    }
    if m.cost_usd > 1e-12 {
        return m.cost_usd;
    }
    if m.total_tokens() > 0 || m.cached_read > 0 {
        return estimate_usd(model, m.input, m.output, m.cached_read);
    }
    0.0
}

/// Human labels for the cost-mode strip: `off` / `all` / `official`.
pub fn cost_mode_ui_label(mode: CostMode) -> &'static str {
    match mode {
        CostMode::Off => "off",
        CostMode::All => "all",
        CostMode::OfficialOnly => "official",
    }
}

/// Longer help for the current mode (shown next to the cost row).
pub fn cost_mode_help(mode: CostMode) -> &'static str {
    match mode {
        CostMode::Off => "no $ attribution",
        CostMode::All => "all models",
        CostMode::OfficialOnly => "grok* only",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage_activity::catalog::{lookup_rates_in, seed_catalog, CustomPrices, ModelRates};
    use std::collections::BTreeMap;

    #[test]
    fn ticks_scale() {
        assert!((ticks_to_usd(10_000_000_000) - 1.0).abs() < 1e-12);
        assert!((ticks_to_usd(444_552_000) - 0.0444552).abs() < 1e-9);
    }

    #[test]
    fn grok_45_list_price() {
        // Uses seed/catalog rates: 1M uncached in + 0 cache + 1M out → $2 + $6
        let seed = seed_catalog();
        let r = lookup_rates_in(&CustomPrices::default(), &seed, "grok-4.5").unwrap();
        let u = (1_000_000.0 / 1_000_000.0) * r.0 + (1_000_000.0 / 1_000_000.0) * r.1;
        assert!((u - 8.0).abs() < 1e-9);

        let r2 = lookup_rates_in(&CustomPrices::default(), &seed, "grok-4.5-build").unwrap();
        assert!((r2.0 - 2.0).abs() < 1e-9);
    }

    #[test]
    fn build_01_list_price() {
        let seed = seed_catalog();
        let r = lookup_rates_in(&CustomPrices::default(), &seed, "grok-build-0.1").unwrap();
        let u = r.0 + r.1; // 1M in + 1M out
        assert!((u - 3.0).abs() < 1e-9);
    }

    #[test]
    fn estimate_usd_uses_resolution_math() {
        // Direct estimate path (loads disk/seed); should be finite non-negative.
        let u = estimate_usd("grok-4.5", 1_000_000, 1_000_000, 0);
        assert!(u.is_finite() && u > 0.0);
        // Unknown model → placeholder 1+5
        let u2 = estimate_usd("totally-unknown-model-xyz-999", 1_000_000, 1_000_000, 0);
        assert!((u2 - 6.0).abs() < 1e-9);
    }

    #[test]
    fn display_cost_respects_mode() {
        use crate::usage_activity::store::ModelTotals;
        let m = ModelTotals {
            input: 1_000_000,
            output: 0,
            cost_usd: 2.0,
            official: true,
            ..Default::default()
        };
        assert!((display_cost_usd(CostMode::All, "grok-4.5", &m) - 2.0).abs() < 1e-9);
        assert_eq!(display_cost_usd(CostMode::Off, "grok-4.5", &m), 0.0);
        let ext = ModelTotals {
            input: 1_000_000,
            cost_usd: 9.0,
            official: false,
            ..Default::default()
        };
        assert_eq!(display_cost_usd(CostMode::OfficialOnly, "gpt-5", &ext), 0.0);
        assert!((display_cost_usd(CostMode::All, "gpt-5", &ext) - 9.0).abs() < 1e-9);
    }

    #[test]
    fn clamp_official_only_without_sub() {
        assert_eq!(
            clamp_mode_for_auth(CostMode::OfficialOnly, false),
            CostMode::All
        );
        assert_eq!(
            clamp_mode_for_auth(CostMode::OfficialOnly, true),
            CostMode::OfficialOnly
        );
    }

    #[test]
    fn live_display_parses_from_pricing_toml() {
        let on: PricingConfig = toml::from_str(
            r#"
mode = "all"
live_display = true
auto_sync_catalog = true
"#,
        )
        .expect("parse");
        assert!(on.live_display);
        assert_eq!(on.mode, CostMode::All);

        let off_default: PricingConfig = toml::from_str(r#"mode = "official_only""#).expect("parse");
        assert!(
            !off_default.live_display,
            "live_display defaults false when omitted"
        );
        assert_eq!(off_default.mode, CostMode::OfficialOnly);
    }

    #[test]
    fn toggle_live_display_flips_and_restores() {
        // Drive the real toggle entry point (writes pricing.toml under current
        // grok home). Always restore so we leave the host unchanged.
        let before = PricingConfig::load().live_display;
        let after = toggle_live_display();
        assert_ne!(before, after, "toggle_live_display must flip live_display");
        assert_eq!(
            PricingConfig::load().live_display,
            after,
            "load must see the value toggle_live_display saved"
        );
        let restored = toggle_live_display();
        assert_eq!(restored, before, "second toggle must restore prior value");
    }

    #[test]
    fn should_include_respects_mode() {
        assert!(!should_include_model(CostMode::Off, "grok-4.5"));
        assert!(should_include_model(CostMode::All, "claude-sonnet-4-6"));
        assert!(should_include_model(CostMode::OfficialOnly, "grok-4.5"));
        assert!(!should_include_model(
            CostMode::OfficialOnly,
            "claude-sonnet-4-6"
        ));
    }

    #[test]
    fn format_usd_compact() {
        assert_eq!(format_usd(0.12), "$0.12");
        assert_eq!(format_usd(12.34), "$12.34");
        assert_eq!(format_usd(0.0), "$0.00");
    }

    #[test]
    fn custom_wins_in_lookup_chain() {
        let mut custom = CustomPrices::default();
        let mut models = BTreeMap::new();
        models.insert(
            "grok-4.5".into(),
            ModelRates {
                input: 0.5,
                output: 0.5,
                cache_read: 0.0,
            },
        );
        custom.models = models;
        let r = lookup_rates_in(&custom, &seed_catalog(), "grok-4.5").unwrap();
        assert!((r.0 - 0.5).abs() < 1e-9);
    }

    #[test]
    fn cost_mode_serde_snake_case() {
        let j = serde_json::to_string(&CostMode::OfficialOnly).unwrap();
        assert_eq!(j, "\"official_only\"");
        let m: CostMode = serde_json::from_str("\"all\"").unwrap();
        assert_eq!(m, CostMode::All);
    }

    #[test]
    fn is_official_grok() {
        assert!(is_official_model("grok-4.5"));
        assert!(is_official_model("xai/grok-build-0.1"));
        assert!(!is_official_model("gpt-5.4"));
    }
}
