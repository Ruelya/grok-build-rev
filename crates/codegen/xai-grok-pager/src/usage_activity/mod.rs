//! Local usage activity: historical scan, per-model stats, device snapshots,
//! optional WebDAV sync, and `/usage` formatting.
//!
//! **Trigger (best default):** when the user opens `/usage` we:
//! 1. Incrementally scan local `sessions/**/updates.jsonl`
//! 2. Pull other devices from WebDAV (if configured)
//! 3. Merge day×model totals
//! 4. Upload this device's snapshot
//! 5. Return text for the activity block (session + billing remain separate)

mod catalog;
mod format;
mod live_cost;
mod prices;
mod scan;
mod store;
mod sync;
mod webdav;

pub use catalog::{
    catalog_path, custom_path, load_catalog, load_custom, lookup_rates, sync_models_dev, Catalog,
    CustomPrices, ModelRates,
};
pub use format::{
    activity_block_text, fmt_tokens_kb, format_official_estimate, OfficialEstimateInput,
};
pub use live_cost::{
    cost_from_prompt_usage, estimate_from_live_tokens_by_model, estimate_from_total_tokens,
    format_live_cost, load_pricing_config, pricing_live_display, pricing_mode,
};
pub use prices::{
    clamp_mode_for_auth, cost_mode_help, cost_mode_ui_label, cycle_cost_mode, display_cost_usd,
    estimate_usd, format_usd, is_official_model, should_include_model, ticks_to_usd,
    toggle_live_display, CostMode, PricingConfig,
};
pub use store::{
    device_id, load_merged_view, save_local_snapshot, usage_dir, DeviceSnapshot, MergedActivity,
    ModelTotals, DayTotals,
};
pub use sync::{local_preview_merged, refresh_force_sync, refresh_on_usage_open};
pub use webdav::{
    load_sync_status, open_sync_config, set_enabled, sync_toml_path, SyncStatus, DEFAULT_PROFILE,
    DEFAULT_REMOTE_ROOT,
};
