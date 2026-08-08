//! Scan `sessions/**/updates.jsonl` for `turn_completed.usage`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::store::{record_turn_model, device_id, DeviceSnapshot, DayTotals};

/// Rebuild this device's snapshot from local session history.
pub fn scan_local_sessions() -> DeviceSnapshot {
    let device = device_id();
    let name = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| device.clone());
    let mut snap = DeviceSnapshot::empty(&device, &name);
    let Some(home) = xai_grok_config::user_grok_home() else {
        return snap;
    };
    let sessions = home.join("sessions");
    if !sessions.is_dir() {
        return snap;
    }

    let mut sessions_scanned = 0u64;
    let mut turns = 0u64;
    for summary in walk_summary_files(&sessions) {
        let dir = match summary.parent() {
            Some(p) => p.to_path_buf(),
            None => continue,
        };
        sessions_scanned += 1;
        let updates = dir.join("updates.jsonl");
        if !updates.is_file() {
            continue;
        }
        turns += absorb_updates_file(&updates, &mut snap.by_day);
    }
    snap.sessions_scanned = sessions_scanned;
    snap.turns_recorded = turns;
    snap.updated_at = chrono::Utc::now().to_rfc3339();
    snap
}

fn walk_summary_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for ent in rd.flatten() {
            let p = ent.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.file_name().and_then(|s| s.to_str()) == Some("summary.json") {
                out.push(p);
            }
        }
    }
    out
}

fn absorb_updates_file(path: &Path, by_day: &mut BTreeMap<String, DayTotals>) -> u64 {
    let Ok(text) = std::fs::read_to_string(path) else {
        return 0;
    };
    let mut n = 0u64;
    // Dedup prompt_id within file (session/update vs _x.ai/session/update).
    let mut seen = std::collections::HashSet::<String>::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || !line.contains("turn_completed") {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let update = v
            .pointer("/params/update")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        let kind = update
            .get("sessionUpdate")
            .and_then(|x| x.as_str())
            .unwrap_or("");
        if kind != "turn_completed" {
            continue;
        }
        let usage = match update.get("usage") {
            Some(u) if u.is_object() => u,
            _ => continue,
        };
        let prompt_id = update
            .get("prompt_id")
            .or_else(|| update.get("promptId"))
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let dedup_key = if prompt_id.is_empty() {
            // Fallback: hash line
            {
                let h = blake3::hash(line.as_bytes());
                h.to_hex().to_string()
            }
        } else {
            prompt_id
        };
        if !seen.insert(dedup_key) {
            continue;
        }

        let ts = v
            .get("timestamp")
            .and_then(|t| t.as_i64().or_else(|| t.as_u64().map(|u| u as i64)));
        let day = day_from_unix(ts);

        // Prefer per-model breakdown.
        if let Some(models) = usage.get("modelUsage").and_then(|m| m.as_object()) {
            if !models.is_empty() {
                for (model, row) in models {
                    let input = json_u64(row, &["inputTokens", "input_tokens"]);
                    let output = json_u64(row, &["outputTokens", "output_tokens"]);
                    let cached = json_u64(row, &["cachedReadTokens", "cached_read_tokens"]);
                    let reasoning = json_u64(row, &["reasoningTokens", "reasoning_tokens"]);
                    let calls = json_u64(row, &["modelCalls", "model_calls"]).max(1);
                    let ticks = json_i64(row, &["costUsdTicks", "cost_usd_ticks"]);
                    record_turn_model(
                        by_day, &day, model, input, output, cached, reasoning, calls, ticks,
                    );
                }
                n += 1;
                continue;
            }
        }

        let input = json_u64(usage, &["inputTokens", "input_tokens"]);
        let output = json_u64(usage, &["outputTokens", "output_tokens"]);
        let cached = json_u64(usage, &["cachedReadTokens", "cached_read_tokens"]);
        let reasoning = json_u64(usage, &["reasoningTokens", "reasoning_tokens"]);
        let calls = json_u64(usage, &["modelCalls", "model_calls"]).max(1);
        let ticks = json_i64(usage, &["costUsdTicks", "cost_usd_ticks"]);
        let model = "unknown".to_string();
        record_turn_model(
            by_day, &day, &model, input, output, cached, reasoning, calls, ticks,
        );
        n += 1;
    }
    n
}

fn day_from_unix(ts: Option<i64>) -> String {
    use chrono::{TimeZone, Utc};
    match ts {
        Some(sec) => Utc
            .timestamp_opt(sec, 0)
            .single()
            .map(|dt| {
                dt.with_timezone(&chrono::Local)
                    .format("%Y-%m-%d")
                    .to_string()
            })
            .unwrap_or_else(|| chrono::Local::now().format("%Y-%m-%d").to_string()),
        None => chrono::Local::now().format("%Y-%m-%d").to_string(),
    }
}

fn json_u64(v: &serde_json::Value, keys: &[&str]) -> u64 {
    for k in keys {
        if let Some(n) = v.get(*k).and_then(|x| x.as_u64()) {
            return n;
        }
        if let Some(n) = v.get(*k).and_then(|x| x.as_i64()) {
            return n.max(0) as u64;
        }
    }
    0
}

fn json_i64(v: &serde_json::Value, keys: &[&str]) -> Option<i64> {
    for k in keys {
        if let Some(n) = v.get(*k).and_then(|x| x.as_i64()) {
            if n > 0 {
                return Some(n);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_turn_completed_line() {
        let mut by_day = BTreeMap::new();
        let line = r#"{"timestamp":1785408776,"method":"_x.ai/session/update","params":{"update":{"sessionUpdate":"turn_completed","prompt_id":"p1","usage":{"inputTokens":100,"outputTokens":10,"cachedReadTokens":50,"reasoningTokens":2,"modelCalls":1,"costUsdTicks":10000000000,"modelUsage":{"grok-4.5-build":{"inputTokens":100,"outputTokens":10,"cachedReadTokens":50,"reasoningTokens":2,"modelCalls":1,"costUsdTicks":10000000000}}}}}}"#;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("updates.jsonl");
        std::fs::write(&path, line).unwrap();
        let n = absorb_updates_file(&path, &mut by_day);
        assert_eq!(n, 1);
        let day = by_day.values().next().unwrap();
        assert_eq!(day.total.input, 100);
        assert_eq!(day.total.output, 10);
        assert!(day.by_model.contains_key("grok-4.5-build"));
        assert!((day.total.cost_usd - 1.0).abs() < 1e-6);
    }
}
