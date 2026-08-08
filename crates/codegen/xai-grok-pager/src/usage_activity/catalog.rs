//! models.dev price catalog + user custom overrides.
//!
//! On-disk:
//! - `~/.grok/usage/prices/models_dev.json` — slim catalog (syncable)
//! - `~/.grok/usage/prices/custom.toml` — user add-only overrides (never touched by sync)
//!
//! Lookup order at estimate time is owned by [`super::prices::estimate_usd`]:
//! custom → disk catalog → embedded seed → placeholder.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// USD per 1M tokens.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ModelRates {
    pub input: f64,
    pub output: f64,
    #[serde(default)]
    pub cache_read: f64,
}

impl ModelRates {
    pub fn as_tuple(self) -> (f64, f64, f64) {
        (self.input, self.output, self.cache_read)
    }
}

/// Slim on-disk / in-memory catalog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Catalog {
    #[serde(default)]
    pub updated_at: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub models: BTreeMap<String, ModelRates>,
}

impl Default for Catalog {
    fn default() -> Self {
        Self {
            updated_at: String::new(),
            source: String::new(),
            models: BTreeMap::new(),
        }
    }
}

/// User custom prices (`custom.toml`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CustomPrices {
    #[serde(default)]
    pub models: BTreeMap<String, ModelRates>,
}

/// Embedded seed (~20 common models) for first run / offline fallback.
const SEED_JSON: &str = r#"{
  "updated_at": "2026-08-07T00:00:00Z",
  "source": "seed",
  "models": {
    "grok-4.5": {"input": 2.0, "output": 6.0, "cache_read": 0.3},
    "xai/grok-4.5": {"input": 2.0, "output": 6.0, "cache_read": 0.3},
    "grok-4.5-build": {"input": 2.0, "output": 6.0, "cache_read": 0.3},
    "grok-build-0.1": {"input": 1.0, "output": 2.0, "cache_read": 0.2},
    "xai/grok-build-0.1": {"input": 1.0, "output": 2.0, "cache_read": 0.2},
    "grok-4.3": {"input": 1.25, "output": 2.5, "cache_read": 0.2},
    "xai/grok-4.3": {"input": 1.25, "output": 2.5, "cache_read": 0.2},
    "grok-4.20-0309-reasoning": {"input": 1.25, "output": 2.5, "cache_read": 0.2},
    "grok-4": {"input": 3.0, "output": 15.0, "cache_read": 0.75},
    "xai/grok-4": {"input": 3.0, "output": 15.0, "cache_read": 0.75},
    "gpt-5.4": {"input": 2.5, "output": 15.0, "cache_read": 0.25},
    "openai/gpt-5.4": {"input": 2.5, "output": 15.0, "cache_read": 0.25},
    "gpt-5": {"input": 1.25, "output": 10.0, "cache_read": 0.125},
    "openai/gpt-5": {"input": 1.25, "output": 10.0, "cache_read": 0.125},
    "claude-sonnet-4-6": {"input": 3.0, "output": 15.0, "cache_read": 0.3},
    "anthropic/claude-sonnet-4-6": {"input": 3.0, "output": 15.0, "cache_read": 0.3},
    "claude-opus-4-6": {"input": 5.0, "output": 25.0, "cache_read": 0.5},
    "anthropic/claude-opus-4-6": {"input": 5.0, "output": 25.0, "cache_read": 0.5},
    "claude-haiku-4-5": {"input": 1.0, "output": 5.0, "cache_read": 0.1},
    "anthropic/claude-haiku-4-5": {"input": 1.0, "output": 5.0, "cache_read": 0.1},
    "gemini-2.5-pro": {"input": 1.25, "output": 10.0, "cache_read": 0.125},
    "google/gemini-2.5-pro": {"input": 1.25, "output": 10.0, "cache_read": 0.125},
    "deepseek-v4-pro": {"input": 1.74, "output": 3.48, "cache_read": 0.145},
    "deepseek/deepseek-v4-pro": {"input": 1.74, "output": 3.48, "cache_read": 0.145}
  }
}"#;

/// Placeholder rates for unknown external models (USD / 1M).
pub const PLACEHOLDER_RATES: (f64, f64, f64) = (1.0, 5.0, 0.25);

pub fn seed_catalog() -> Catalog {
    serde_json::from_str(SEED_JSON).unwrap_or_else(|_| Catalog {
        updated_at: "2026-08-07T00:00:00Z".into(),
        source: "seed".into(),
        models: BTreeMap::new(),
    })
}

pub fn catalog_path() -> Option<PathBuf> {
    xai_grok_config::user_grok_home().map(|h| h.join("usage").join("prices").join("models_dev.json"))
}

pub fn custom_path() -> Option<PathBuf> {
    xai_grok_config::user_grok_home().map(|h| h.join("usage").join("prices").join("custom.toml"))
}

/// Load on-disk catalog, or embedded seed when missing / unreadable.
pub fn load_catalog() -> Catalog {
    if let Some(path) = catalog_path() {
        if path.is_file() {
            if let Ok(bytes) = std::fs::read(&path) {
                if let Ok(c) = serde_json::from_slice::<Catalog>(&bytes) {
                    return c;
                }
            }
        }
    }
    seed_catalog()
}

/// Load user custom prices (empty if missing).
pub fn load_custom() -> CustomPrices {
    let Some(path) = custom_path() else {
        return CustomPrices::default();
    };
    if !path.is_file() {
        return CustomPrices::default();
    }
    let Ok(text) = std::fs::read_to_string(&path) else {
        return CustomPrices::default();
    };
    toml::from_str(&text).unwrap_or_default()
}

/// Fuzzy lookup: exact → strip provider prefix → contains (case-insensitive keys).
pub fn fuzzy_lookup(map: &BTreeMap<String, ModelRates>, model: &str) -> Option<ModelRates> {
    if map.is_empty() || model.is_empty() {
        return None;
    }
    let needle = model.trim();
    if needle.is_empty() {
        return None;
    }

    // 1. Exact
    if let Some(r) = map.get(needle) {
        return Some(*r);
    }

    // Case-insensitive exact
    let needle_l = needle.to_ascii_lowercase();
    for (k, v) in map {
        if k.eq_ignore_ascii_case(needle) {
            return Some(*v);
        }
    }

    // 2. Strip provider prefix on query (provider/model → model)
    let stripped_query = needle_l
        .rsplit_once('/')
        .map(|(_, rest)| rest)
        .unwrap_or(needle_l.as_str());
    if stripped_query != needle_l.as_str() {
        for (k, v) in map {
            if k.eq_ignore_ascii_case(stripped_query) {
                return Some(*v);
            }
            let k_stripped = k
                .rsplit_once('/')
                .map(|(_, rest)| rest)
                .unwrap_or(k.as_str());
            if k_stripped.eq_ignore_ascii_case(stripped_query) {
                return Some(*v);
            }
        }
    }

    // Also match when catalog has provider/model and query is bare
    for (k, v) in map {
        let k_stripped = k
            .rsplit_once('/')
            .map(|(_, rest)| rest)
            .unwrap_or(k.as_str());
        if k_stripped.eq_ignore_ascii_case(needle) || k_stripped.eq_ignore_ascii_case(stripped_query)
        {
            return Some(*v);
        }
    }

    // 3. Contains (prefer longest key match)
    let mut best: Option<(usize, ModelRates)> = None;
    for (k, v) in map {
        let kl = k.to_ascii_lowercase();
        let k_stripped = kl
            .rsplit_once('/')
            .map(|(_, rest)| rest)
            .unwrap_or(kl.as_str());
        if needle_l.contains(k_stripped)
            || k_stripped.contains(stripped_query)
            || needle_l.contains(&kl)
            || kl.contains(&needle_l)
        {
            let score = k_stripped.len().max(kl.len());
            match best {
                Some((s, _)) if s >= score => {}
                _ => best = Some((score, *v)),
            }
        }
    }
    best.map(|(_, r)| r)
}

/// Resolve rates: custom → catalog maps (caller supplies maps). Returns None if neither hits.
pub fn lookup_rates_in(
    custom: &CustomPrices,
    catalog: &Catalog,
    model: &str,
) -> Option<(f64, f64, f64)> {
    if let Some(r) = fuzzy_lookup(&custom.models, model) {
        return Some(r.as_tuple());
    }
    fuzzy_lookup(&catalog.models, model).map(ModelRates::as_tuple)
}

/// Load custom + catalog from disk and fuzzy-lookup.
pub fn lookup_rates(model: &str) -> Option<(f64, f64, f64)> {
    lookup_rates_in(&load_custom(), &load_catalog(), model)
}

/// Extract slim catalog from full models.dev `api.json` Value.
///
/// For each provider → models entry with a `cost` object, insert:
/// - bare model id (first write wins for bare; later providers may also write `provider/id`)
/// - `provider/id` key always
///
/// Only models that have a `cost` object (with at least input or output) are included.
pub fn merge_catalog_from_models_dev_json(full_api: &Value) -> Catalog {
    let mut models: BTreeMap<String, ModelRates> = BTreeMap::new();
    let obj = match full_api.as_object() {
        Some(o) => o,
        None => {
            return Catalog {
                updated_at: chrono::Utc::now().to_rfc3339(),
                source: "models.dev".into(),
                models,
            };
        }
    };

    for (provider_id, provider) in obj {
        let Some(pmodels) = provider.get("models").and_then(|m| m.as_object()) else {
            continue;
        };
        for (model_id, model) in pmodels {
            let Some(cost) = model.get("cost") else {
                continue;
            };
            let Some(rates) = cost_to_rates(cost) else {
                continue;
            };
            // Prefer first bare id; do not overwrite bare with a different provider later.
            models.entry(model_id.clone()).or_insert(rates);
            let qualified = format!("{provider_id}/{model_id}");
            models.insert(qualified, rates);
        }
    }

    Catalog {
        updated_at: chrono::Utc::now().to_rfc3339(),
        source: "models.dev".into(),
        models,
    }
}

fn cost_to_rates(cost: &Value) -> Option<ModelRates> {
    let input = cost.get("input").and_then(|v| v.as_f64());
    let output = cost.get("output").and_then(|v| v.as_f64());
    if input.is_none() && output.is_none() {
        return None;
    }
    let cache_read = cost
        .get("cache_read")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    Some(ModelRates {
        input: input.unwrap_or(0.0),
        output: output.unwrap_or(0.0),
        cache_read,
    })
}

/// Merge remote slim catalog into existing: add missing keys; update rates for existing keys.
/// Never removes keys. Does not touch custom.toml.
pub fn merge_catalog_add_only(existing: &Catalog, remote: &Catalog) -> Catalog {
    let mut models = existing.models.clone();
    for (k, v) in &remote.models {
        models.insert(k.clone(), *v);
    }
    Catalog {
        updated_at: if remote.updated_at.is_empty() {
            chrono::Utc::now().to_rfc3339()
        } else {
            remote.updated_at.clone()
        },
        source: if remote.source.is_empty() {
            "models.dev".into()
        } else {
            remote.source.clone()
        },
        models,
    }
}

fn save_catalog(catalog: &Catalog) -> Result<(), String> {
    let path = catalog_path().ok_or_else(|| "no grok home".to_string())?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_vec_pretty(catalog).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

/// HTTP GET https://models.dev/api.json, merge into on-disk catalog (add/update only).
/// Never writes custom.toml. Returns a short status note.
pub fn sync_models_dev() -> Result<String, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .user_agent("grok-usage-pricing/1.0")
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .get("https://models.dev/api.json")
        .send()
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        return Err(format!("models.dev HTTP {}", resp.status()));
    }

    let full: Value = resp.json().map_err(|e| e.to_string())?;
    let remote = merge_catalog_from_models_dev_json(&full);
    let remote_n = remote.models.len();

    let existing = if catalog_path().map(|p| p.is_file()).unwrap_or(false) {
        load_catalog()
    } else {
        // Start from seed so first sync keeps seed keys that remote might omit.
        seed_catalog()
    };
    let before = existing.models.len();
    let merged = merge_catalog_add_only(&existing, &remote);
    let after = merged.models.len();
    let added = after.saturating_sub(before);
    save_catalog(&merged)?;

    Ok(format!(
        "prices catalog: {remote_n} remote keys · +{added} new · {after} total"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rates(i: f64, o: f64, c: f64) -> ModelRates {
        ModelRates {
            input: i,
            output: o,
            cache_read: c,
        }
    }

    #[test]
    fn seed_has_common_models() {
        let c = seed_catalog();
        assert!(c.models.contains_key("grok-4.5"));
        assert!(c.models.contains_key("grok-build-0.1"));
        assert!(c.models.contains_key("gpt-5.4"));
        assert!(c.models.contains_key("claude-sonnet-4-6"));
        let g = c.models.get("grok-4.5").unwrap();
        assert!((g.input - 2.0).abs() < 1e-9);
        assert!((g.output - 6.0).abs() < 1e-9);
    }

    #[test]
    fn fuzzy_exact_and_prefix() {
        let mut map = BTreeMap::new();
        map.insert("grok-4.5".into(), rates(2.0, 6.0, 0.3));
        map.insert("xai/grok-build-0.1".into(), rates(1.0, 2.0, 0.2));

        assert_eq!(
            fuzzy_lookup(&map, "grok-4.5").map(|r| r.as_tuple()),
            Some((2.0, 6.0, 0.3))
        );
        assert_eq!(
            fuzzy_lookup(&map, "xai/grok-4.5").map(|r| r.as_tuple()),
            Some((2.0, 6.0, 0.3))
        );
        assert_eq!(
            fuzzy_lookup(&map, "grok-build-0.1").map(|r| r.as_tuple()),
            Some((1.0, 2.0, 0.2))
        );
    }

    #[test]
    fn custom_wins_over_catalog() {
        let mut custom = CustomPrices::default();
        custom
            .models
            .insert("my-proxy-model".into(), rates(9.0, 9.0, 1.0));
        custom
            .models
            .insert("grok-4.5".into(), rates(0.01, 0.02, 0.0));

        let catalog = seed_catalog();
        let r = lookup_rates_in(&custom, &catalog, "grok-4.5").unwrap();
        assert!((r.0 - 0.01).abs() < 1e-9);
        assert!((r.1 - 0.02).abs() < 1e-9);

        let r2 = lookup_rates_in(&custom, &catalog, "my-proxy-model").unwrap();
        assert!((r2.0 - 9.0).abs() < 1e-9);
    }

    #[test]
    fn merge_from_models_dev_extracts_cost() {
        let api = json!({
            "xai": {
                "id": "xai",
                "models": {
                    "grok-4.5": {
                        "id": "grok-4.5",
                        "cost": {"input": 2.0, "output": 6.0, "cache_read": 0.3}
                    },
                    "no-cost-model": {
                        "id": "no-cost-model"
                    }
                }
            },
            "openai": {
                "id": "openai",
                "models": {
                    "gpt-5.4": {
                        "id": "gpt-5.4",
                        "cost": {"input": 2.5, "output": 15.0, "cache_read": 0.25}
                    }
                }
            }
        });
        let cat = merge_catalog_from_models_dev_json(&api);
        assert!(cat.models.contains_key("grok-4.5"));
        assert!(cat.models.contains_key("xai/grok-4.5"));
        assert!(cat.models.contains_key("gpt-5.4"));
        assert!(cat.models.contains_key("openai/gpt-5.4"));
        assert!(!cat.models.contains_key("no-cost-model"));
        let g = cat.models.get("xai/grok-4.5").unwrap();
        assert!((g.input - 2.0).abs() < 1e-9);
        assert!((g.output - 6.0).abs() < 1e-9);
    }

    #[test]
    fn merge_add_only_updates_rates_keeps_old_keys() {
        let mut existing = Catalog::default();
        existing
            .models
            .insert("old-model".into(), rates(1.0, 1.0, 0.1));
        existing
            .models
            .insert("grok-4.5".into(), rates(1.0, 1.0, 0.1));

        let mut remote = Catalog {
            updated_at: "t".into(),
            source: "models.dev".into(),
            models: BTreeMap::new(),
        };
        remote
            .models
            .insert("grok-4.5".into(), rates(2.0, 6.0, 0.3));
        remote
            .models
            .insert("new-model".into(), rates(3.0, 4.0, 0.5));

        let merged = merge_catalog_add_only(&existing, &remote);
        // old key retained
        assert!(merged.models.contains_key("old-model"));
        // existing key rates updated from remote
        let g = merged.models.get("grok-4.5").unwrap();
        assert!((g.input - 2.0).abs() < 1e-9);
        assert!((g.output - 6.0).abs() < 1e-9);
        // new key added
        assert!(merged.models.contains_key("new-model"));
        assert_eq!(merged.models.len(), 3);
    }

    #[test]
    fn lookup_priority_custom_then_catalog() {
        let mut custom = CustomPrices::default();
        custom
            .models
            .insert("proxy".into(), rates(7.0, 8.0, 0.5));
        let catalog = seed_catalog();

        // custom only
        assert_eq!(
            lookup_rates_in(&custom, &catalog, "proxy"),
            Some((7.0, 8.0, 0.5))
        );
        // catalog / seed
        let r = lookup_rates_in(&CustomPrices::default(), &catalog, "grok-build-0.1").unwrap();
        assert!((r.0 - 1.0).abs() < 1e-9);
        assert!((r.1 - 2.0).abs() < 1e-9);
        // miss
        assert!(lookup_rates_in(&CustomPrices::default(), &Catalog::default(), "zzz").is_none());
    }
}
