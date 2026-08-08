//! Live session cost display helpers (prompt info line, goal detail).
//!
//! Cost mode / config live in [`super::prices`]. This module turns turn usage
//! into USD for the chrome UI and re-exports the names call sites expect.

use super::prices::{self, CostMode, PricingConfig};

/// Format USD for the prompt info line / compact UI.
pub fn format_live_cost(usd: f64) -> String {
    prices::format_usd(usd)
}

/// Load pricing config (same as [`PricingConfig::load`], short-lived cache).
pub fn load_pricing_config() -> PricingConfig {
    // PricingConfig::load already creates defaults; no need for a second cache
    // here — render path is not hot enough to warrant Mutex.
    PricingConfig::load()
}

/// `live_display` from pricing.toml (default false).
pub fn pricing_live_display() -> bool {
    load_pricing_config().live_display
}

pub fn pricing_mode() -> CostMode {
    load_pricing_config().mode
}

/// USD for one turn's [`PromptUsage`], respecting cost mode.
///
/// Prefer ticks when present; otherwise estimate. `OfficialOnly` skips non-grok
/// models. `Off` always returns 0.
pub fn cost_from_prompt_usage(
    usage: &xai_grok_shell::extensions::notification::PromptUsage,
    mode: CostMode,
) -> f64 {
    if matches!(mode, CostMode::Off) {
        return 0.0;
    }

    if !usage.model_usage.is_empty() {
        let mut sum = 0.0;
        for (model, row) in &usage.model_usage {
            if !prices::should_include_model(mode, model) {
                continue;
            }
            sum += cost_from_model_row(model, row, mode);
        }
        return sum;
    }

    cost_from_model_row("unknown", &usage.totals, mode)
}

fn cost_from_model_row(
    model: &str,
    row: &xai_grok_shell::extensions::notification::PromptUsageModel,
    mode: CostMode,
) -> f64 {
    if matches!(mode, CostMode::Off) || !prices::should_include_model(mode, model) {
        return 0.0;
    }
    match row.cost_usd_ticks {
        Some(t) if t > 0 => prices::ticks_to_usd(t),
        _ => prices::estimate_usd(
            model,
            row.input_tokens,
            row.output_tokens,
            row.cached_read_tokens,
        ),
    }
}

/// Estimate cost from per-model live token totals that lack in/out split.
///
/// **Heuristic:** treat each model total as 50% input / 50% output, zero cache.
/// Rough for the goal-detail subagent line only.
pub fn estimate_from_live_tokens_by_model(by_model: &[(String, u64)], mode: CostMode) -> f64 {
    if matches!(mode, CostMode::Off) {
        return 0.0;
    }
    let mut sum = 0.0;
    for (model, tokens) in by_model {
        if !prices::should_include_model(mode, model) {
            continue;
        }
        let half = *tokens / 2;
        let rest = tokens.saturating_sub(half);
        sum += prices::estimate_usd(model, half, rest, 0);
    }
    sum
}

/// When only a single total token count is known (no per-model rows).
pub fn estimate_from_total_tokens(tokens: u64, model: &str, mode: CostMode) -> f64 {
    if matches!(mode, CostMode::Off) || !prices::should_include_model(mode, model) {
        return 0.0;
    }
    let half = tokens / 2;
    prices::estimate_usd(model, half, tokens.saturating_sub(half), 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_live_cost_scales() {
        assert_eq!(format_live_cost(0.0), "$0.00");
        assert!((format_live_cost(1.0).contains('1')));
    }

    #[test]
    fn cost_mode_off_zero() {
        use xai_grok_shell::extensions::notification::{PromptUsage, PromptUsageModel};
        let usage = PromptUsage {
            totals: PromptUsageModel {
                input_tokens: 1_000_000,
                output_tokens: 1_000_000,
                cost_usd_ticks: Some(10_000_000_000),
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(cost_from_prompt_usage(&usage, CostMode::Off), 0.0);
    }

    #[test]
    fn prefers_ticks() {
        use xai_grok_shell::extensions::notification::{PromptUsage, PromptUsageModel};
        let usage = PromptUsage {
            totals: PromptUsageModel {
                input_tokens: 1_000_000,
                output_tokens: 0,
                cost_usd_ticks: Some(10_000_000_000),
                ..Default::default()
            },
            ..Default::default()
        };
        let c = cost_from_prompt_usage(&usage, CostMode::All);
        assert!((c - 1.0).abs() < 1e-9);
    }

    #[test]
    fn official_only_skips_external() {
        use xai_grok_shell::extensions::notification::{PromptUsage, PromptUsageModel};
        let mut usage = PromptUsage::default();
        usage.model_usage.insert(
            "gpt-5.4".into(),
            PromptUsageModel {
                input_tokens: 1_000_000,
                output_tokens: 0,
                cost_usd_ticks: Some(10_000_000_000),
                ..Default::default()
            },
        );
        assert_eq!(cost_from_prompt_usage(&usage, CostMode::OfficialOnly), 0.0);
    }
}
