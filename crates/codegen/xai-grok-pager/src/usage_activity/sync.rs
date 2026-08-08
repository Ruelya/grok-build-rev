//! Orchestrate scan → pull → merge → upload when `/usage` opens (or force sync).
//!
//! Multi-device model (not whole-DB overwrite):
//! 1. Scan **this** device's sessions → `devices/<me>/snapshot.json`
//! 2. WebDAV pull **other** devices only → cache under `devices/<id>/`
//! 3. PUT **this** device file only
//! 4. UI merge = sum day×model across all local device snapshots

use super::format::activity_block_text;
use super::scan::scan_local_sessions;
use super::store::{load_merged_view, save_local_snapshot, MergedActivity};
use super::webdav::{
    cache_remote_snapshots, download_all_devices, ensure_default_config_file, load_config,
    load_sync_status, record_sync_result, upload_snapshot, SyncStatus,
};

pub struct RefreshOutcome {
    pub activity_text: String,
    pub merged: MergedActivity,
    pub sync_note: Option<String>,
    pub sync_status: SyncStatus,
}

impl RefreshOutcome {
    pub fn into_modal_state(
        self,
        billing: Option<crate::views::credit_bar::CreditBalance>,
        tier: Option<String>,
    ) -> crate::views::usage_activity_modal::UsageActivityModalState {
        let mut state = crate::views::usage_activity_modal::UsageActivityModalState::new(
            self.merged,
            self.sync_note,
            self.sync_status,
            billing,
            tier,
        );
        state.phase = crate::views::usage_activity_modal::UsagePhase::Ready;
        state
    }
}

/// Best trigger: **when the user opens `/usage`**.
pub fn refresh_on_usage_open() -> RefreshOutcome {
    refresh_inner(false)
}

/// Immediate sync from the Usage modal (`s` / click).
pub fn refresh_force_sync() -> RefreshOutcome {
    refresh_inner(true)
}

/// Local-only preview for the loading modal (no WebDAV).
pub fn local_preview_merged() -> (MergedActivity, SyncStatus) {
    ensure_default_config_file();
    // Best-effort: use last on-disk snapshots without rescanning.
    let merged = load_merged_view();
    (merged, load_sync_status())
}

fn refresh_inner(force_webdav: bool) -> RefreshOutcome {
    ensure_default_config_file();

    // 1. Local scan (source of truth for this device only)
    let local = scan_local_sessions();
    if let Err(e) = save_local_snapshot(&local) {
        tracing::warn!(error = %e, "failed to save local usage snapshot");
    }

    // Best-effort models.dev price catalog refresh (add/update only; never touches custom.toml).
    let pricing = super::prices::PricingConfig::load();
    let price_note = if pricing.auto_sync_catalog {
        match super::catalog::sync_models_dev() {
            Ok(msg) => {
                tracing::info!(%msg, "usage price catalog synced");
                Some(msg)
            }
            Err(e) => {
                tracing::debug!(error = %e, "price catalog sync skipped");
                Some(format!("prices: {e}"))
            }
        }
    } else {
        None
    };

    let status_before = load_sync_status();
    let mut sync_note = None;

    match load_config() {
        Ok(None) => {
            if !status_before.enabled {
                sync_note = Some("sync off".into());
            }
        }
        Ok(Some(cfg)) => {
            let should_webdav = force_webdav || cfg.auto_sync;
            if !should_webdav {
                sync_note = Some("auto_sync off — press s or click Sync to run".into());
            } else {
                sync_note = Some(run_webdav_roundtrip(&cfg, &local));
            }
        }
        Err(e) => {
            record_sync_result(&e);
            sync_note = Some(e);
        }
    }

    if let Some(pn) = price_note {
        sync_note = Some(match sync_note {
            Some(n) => format!("{n} · {pn}"),
            None => pn,
        });
    }

    let merged = load_merged_view();
    let mut activity_text = activity_block_text(&merged);
    if let Some(note) = &sync_note {
        activity_text = format!("{activity_text}\n  Sync: {note}");
    }

    let sync_status = load_sync_status();

    RefreshOutcome {
        activity_text,
        merged,
        sync_note,
        sync_status,
    }
}

fn run_webdav_roundtrip(
    cfg: &super::webdav::WebDavConfig,
    local: &super::store::DeviceSnapshot,
) -> String {
    // Pull other devices first (errors are real now — not silent empty).
    let pull = download_all_devices(cfg);
    let (remote_n, pull_err) = match &pull {
        Ok(remote) => {
            cache_remote_snapshots(remote);
            (remote.len(), None)
        }
        Err(e) => (0usize, Some(e.clone())),
    };

    // Re-save local so merge always has our freshest scan (never replaced by remote self).
    let _ = save_local_snapshot(local);

    let up = upload_snapshot(cfg, local);
    let note = match (pull_err, up) {
        (None, Ok(())) => format!(
            "pulled {remote_n} remote · uploaded this device · merge {} devices",
            load_merged_view().devices.len()
        ),
        (None, Err(e)) => format!("pulled {remote_n} remote · upload failed: {e}"),
        (Some(pe), Ok(())) => format!("pull failed ({pe}); uploaded this device"),
        (Some(pe), Err(e2)) => format!("pull failed ({pe}); upload failed ({e2})"),
    };
    record_sync_result(&note);
    note
}
