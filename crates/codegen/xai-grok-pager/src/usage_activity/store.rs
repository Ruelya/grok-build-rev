//! On-disk usage snapshots under `~/.grok/usage/`.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::prices;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelTotals {
    #[serde(default)]
    pub input: u64,
    #[serde(default)]
    pub output: u64,
    #[serde(default)]
    pub cached_read: u64,
    #[serde(default)]
    pub reasoning: u64,
    #[serde(default)]
    pub calls: u64,
    /// Estimated or reported USD (not ticks).
    #[serde(default)]
    pub cost_usd: f64,
    /// Tokens attributed to official subscription pool.
    #[serde(default)]
    pub official: bool,
}

impl ModelTotals {
    pub fn add_assign(&mut self, other: &ModelTotals) {
        self.input = self.input.saturating_add(other.input);
        self.output = self.output.saturating_add(other.output);
        self.cached_read = self.cached_read.saturating_add(other.cached_read);
        self.reasoning = self.reasoning.saturating_add(other.reasoning);
        self.calls = self.calls.saturating_add(other.calls);
        self.cost_usd += other.cost_usd;
        self.official = self.official || other.official;
    }

    pub fn total_tokens(&self) -> u64 {
        self.input.saturating_add(self.output)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DayTotals {
    #[serde(default)]
    pub total: ModelTotals,
    #[serde(default)]
    pub by_model: BTreeMap<String, ModelTotals>,
}

impl DayTotals {
    pub fn absorb_model(&mut self, model: &str, row: ModelTotals) {
        self.total.add_assign(&row);
        self.by_model
            .entry(model.to_string())
            .or_default()
            .add_assign(&row);
    }

    pub fn add_assign(&mut self, other: &DayTotals) {
        self.total.add_assign(&other.total);
        for (m, t) in &other.by_model {
            self.by_model.entry(m.clone()).or_default().add_assign(t);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceSnapshot {
    pub schema: u32,
    pub device_id: String,
    #[serde(default)]
    pub device_name: String,
    pub updated_at: String,
    #[serde(default)]
    pub timezone: String,
    #[serde(default)]
    pub by_day: BTreeMap<String, DayTotals>,
    #[serde(default)]
    pub sessions_scanned: u64,
    #[serde(default)]
    pub turns_recorded: u64,
}

impl DeviceSnapshot {
    pub fn empty(device_id: &str, device_name: &str) -> Self {
        Self {
            schema: 1,
            device_id: device_id.to_string(),
            device_name: device_name.to_string(),
            updated_at: chrono::Utc::now().to_rfc3339(),
            timezone: iana_timezone_hint(),
            by_day: BTreeMap::new(),
            sessions_scanned: 0,
            turns_recorded: 0,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct MergedActivity {
    pub by_day: BTreeMap<String, DayTotals>,
    pub devices: Vec<String>,
    pub turns_recorded: u64,
}

impl MergedActivity {
    pub fn merge_snapshot(&mut self, snap: &DeviceSnapshot) {
        if !self.devices.iter().any(|d| d == &snap.device_id) {
            self.devices.push(snap.device_id.clone());
        }
        self.turns_recorded = self.turns_recorded.saturating_add(snap.turns_recorded);
        for (day, totals) in &snap.by_day {
            self.by_day.entry(day.clone()).or_default().add_assign(totals);
        }
    }

    pub fn grand_total(&self) -> ModelTotals {
        let mut t = ModelTotals::default();
        for d in self.by_day.values() {
            t.add_assign(&d.total);
        }
        t
    }

    pub fn official_total(&self) -> ModelTotals {
        let mut t = ModelTotals::default();
        for d in self.by_day.values() {
            for row in d.by_model.values() {
                if row.official {
                    t.add_assign(row);
                }
            }
        }
        t
    }

    /// Official spend over the last `days` local calendar days (for period reverse).
    pub fn official_total_last_days(&self, days: i64) -> ModelTotals {
        let today = chrono::Local::now().date_naive();
        let mut t = ModelTotals::default();
        for i in 0..days {
            let d = today
                .checked_sub_signed(chrono::Duration::days(i))
                .unwrap_or(today);
            let key = d.format("%Y-%m-%d").to_string();
            if let Some(day) = self.by_day.get(&key) {
                for row in day.by_model.values() {
                    if row.official {
                        t.add_assign(row);
                    }
                }
            }
        }
        t
    }

    pub fn by_model_all_time(&self) -> BTreeMap<String, ModelTotals> {
        let mut m: BTreeMap<String, ModelTotals> = BTreeMap::new();
        for d in self.by_day.values() {
            for (k, v) in &d.by_model {
                m.entry(k.clone()).or_default().add_assign(v);
            }
        }
        m
    }

    pub fn peak_day(&self) -> Option<(String, u64)> {
        self.by_day
            .iter()
            .map(|(k, v)| (k.clone(), v.total.total_tokens()))
            .max_by_key(|(_, t)| *t)
    }

    /// Current streak of consecutive local days with activity ending today or yesterday.
    pub fn current_streak_days(&self) -> u64 {
        let today = chrono::Local::now().date_naive();
        let mut streak = 0u64;
        let mut cursor = today;
        // Allow streak to count if last active was yesterday (timezone edge).
        if !self.by_day.contains_key(&cursor.format("%Y-%m-%d").to_string()) {
            cursor = today.pred_opt().unwrap_or(today);
        }
        loop {
            let key = cursor.format("%Y-%m-%d").to_string();
            let tokens = self
                .by_day
                .get(&key)
                .map(|d| d.total.total_tokens())
                .unwrap_or(0);
            if tokens == 0 {
                break;
            }
            streak += 1;
            cursor = match cursor.pred_opt() {
                Some(d) => d,
                None => break,
            };
        }
        streak
    }

    pub fn longest_streak_days(&self) -> u64 {
        if self.by_day.is_empty() {
            return 0;
        }
        let mut days: Vec<_> = self
            .by_day
            .iter()
            .filter(|(_, v)| v.total.total_tokens() > 0)
            .map(|(k, _)| k.clone())
            .collect();
        days.sort();
        let mut best = 1u64;
        let mut cur = 1u64;
        for w in days.windows(2) {
            let a = chrono::NaiveDate::parse_from_str(&w[0], "%Y-%m-%d").ok();
            let b = chrono::NaiveDate::parse_from_str(&w[1], "%Y-%m-%d").ok();
            if let (Some(a), Some(b)) = (a, b) {
                if b == a.succ_opt().unwrap_or(a) {
                    cur += 1;
                    best = best.max(cur);
                } else {
                    cur = 1;
                }
            }
        }
        best
    }
}

pub fn usage_dir() -> Option<PathBuf> {
    xai_grok_config::user_grok_home().map(|h| h.join("usage"))
}

pub fn device_id() -> String {
    let Some(dir) = usage_dir() else {
        return "unknown".into();
    };
    let path = dir.join("device_id");
    if let Ok(s) = std::fs::read_to_string(&path) {
        let t = s.trim();
        if !t.is_empty() {
            return t.to_string();
        }
    }
    let id = format!(
        "{}-{}",
        hostname_fallback(),
        &uuid::Uuid::new_v4().to_string()[..8]
    );
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(&path, &id);
    id
}

fn hostname_fallback() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "device".into())
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .take(24)
        .collect()
}

fn iana_timezone_hint() -> String {
    // Local offset label; good enough for snapshot metadata.
    chrono::Local::now().format("%z").to_string()
}

pub fn local_snapshot_path() -> Option<PathBuf> {
    let dir = usage_dir()?;
    let id = device_id();
    Some(dir.join("devices").join(id).join("snapshot.json"))
}

pub fn save_local_snapshot(snap: &DeviceSnapshot) -> std::io::Result<()> {
    let path = local_snapshot_path().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "no grok home")
    })?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_vec_pretty(snap)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(path, json)
}

pub fn load_device_snapshots_from_disk() -> Vec<DeviceSnapshot> {
    let Some(dir) = usage_dir().map(|d| d.join("devices")) else {
        return vec![];
    };
    let Ok(rd) = std::fs::read_dir(dir) else {
        return vec![];
    };
    let mut out = Vec::new();
    for ent in rd.flatten() {
        let p = ent.path().join("snapshot.json");
        if let Ok(bytes) = std::fs::read(&p) {
            if let Ok(s) = serde_json::from_slice::<DeviceSnapshot>(&bytes) {
                out.push(s);
            }
        }
    }
    out
}

pub fn load_merged_view() -> MergedActivity {
    let mut merged = MergedActivity::default();
    for s in load_device_snapshots_from_disk() {
        merged.merge_snapshot(&s);
    }
    merged
}

/// Apply one model's turn usage into a day bucket.
pub fn record_turn_model(
    by_day: &mut BTreeMap<String, DayTotals>,
    day: &str,
    model: &str,
    input: u64,
    output: u64,
    cached_read: u64,
    reasoning: u64,
    calls: u64,
    cost_usd_ticks: Option<i64>,
) {
    let official = prices::is_official_model(model);
    let mode = prices::PricingConfig::load().mode;
    // Off → zero cost; OfficialOnly → skip non-official cost (tokens still recorded).
    let cost_usd = if !prices::should_include_model(mode, model) {
        0.0
    } else {
        match cost_usd_ticks {
            Some(t) if t > 0 => prices::ticks_to_usd(t),
            _ => prices::estimate_usd(model, input, output, cached_read),
        }
    };
    let row = ModelTotals {
        input,
        output,
        cached_read,
        reasoning,
        calls,
        cost_usd,
        official,
    };
    by_day
        .entry(day.to_string())
        .or_default()
        .absorb_model(model, row);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_adds_devices() {
        let mut a = DeviceSnapshot::empty("a", "A");
        record_turn_model(
            &mut a.by_day,
            "2026-04-04",
            "grok-4.5",
            100,
            10,
            0,
            0,
            1,
            None,
        );
        let mut b = DeviceSnapshot::empty("b", "B");
        record_turn_model(
            &mut b.by_day,
            "2026-04-04",
            "grok-4.5",
            50,
            5,
            0,
            0,
            1,
            None,
        );
        let mut m = MergedActivity::default();
        m.merge_snapshot(&a);
        m.merge_snapshot(&b);
        assert_eq!(m.grand_total().total_tokens(), 165);
        assert_eq!(m.devices.len(), 2);
    }
}
