//! Format activity + official estimate for `/usage` scrollback.

use super::store::{MergedActivity, ModelTotals};

/// Compact token formatting with K / M / B.
pub fn fmt_tokens_kb(n: u64) -> String {
    const K: f64 = 1_000.0;
    const M: f64 = 1_000_000.0;
    const B: f64 = 1_000_000_000.0;
    if n < 1_000 {
        n.to_string()
    } else if (n as f64) < 10.0 * K {
        format!("{:.1}K", n as f64 / K)
    } else if (n as f64) < M {
        format!("{:.0}K", n as f64 / K)
    } else if (n as f64) < 10.0 * M {
        format!("{:.1}M", n as f64 / M)
    } else if (n as f64) < B {
        format!("{:.0}M", n as f64 / M)
    } else if (n as f64) < 10.0 * B {
        format!("{:.1}B", n as f64 / B)
    } else {
        format!("{:.0}B", n as f64 / B)
    }
}

pub struct OfficialEstimateInput {
    /// 0–100 from billing API.
    pub usage_pct: f64,
    pub period_label: String,
    pub period_end: Option<String>,
    pub prepaid_usd: Option<f64>,
    pub on_demand_used_usd: Option<f64>,
    pub on_demand_cap_usd: Option<f64>,
    pub tier: Option<String>,
}

/// Compact one-line summary (fallback if modal cannot open).
pub fn activity_block_text(activity: &MergedActivity) -> String {
    let grand = activity.grand_total();
    if grand.total_tokens() == 0 && activity.by_day.is_empty() {
        return "Usage activity: no historical turns found yet (scanned local sessions)."
            .to_string();
    }
    let peak = activity
        .peak_day()
        .map(|(d, t)| format!("{} · {}", d, fmt_tokens_kb(t)))
        .unwrap_or_else(|| "—".into());
    format!(
        "Usage activity: {} total · peak {} · {} devices · open interactive panel for calendar",
        fmt_tokens_kb(grand.total_tokens()),
        peak,
        activity.devices.len().max(1),
    )
}

pub fn format_official_estimate(
    official_spent: &ModelTotals,
    bill: &OfficialEstimateInput,
) -> String {
    let mut lines = Vec::new();
    let tier = bill.tier.as_deref().unwrap_or("subscription");
    lines.push(format!("Official subscription ({tier}):"));
    lines.push(format!(
        "  {} used: {:.0}%",
        bill.period_label,
        bill.usage_pct.floor()
    ));
    if let Some(end) = &bill.period_end {
        lines.push(format!("  Next reset:     {end}"));
    }

    let p = bill.usage_pct.clamp(0.0, 100.0);
    let spent = official_spent.cost_usd;
    // Reverse uses local official-model $ over the billing window (server
    // costUsdTicks when present, else list-price estimate). Not the same as
    // Usage limit "Session usage Cost" (this-session ticks only).
    if p > 0.5 && spent > 0.0 {
        let allowance = spent / (p / 100.0);
        let remaining = (allowance - spent).max(0.0);
        lines.push(format!(
            "  Est. allowance: ~${:.2} / period  (spent ÷ {p:.0}%)",
            allowance
        ));
        lines.push(format!(
            "  Est. remaining: ~${:.2}  · period official $ ~${:.2}",
            remaining, spent
        ));
    } else if spent > 0.0 {
        lines.push(format!(
            "  Period official $: ~${:.2}  (usage % too low to reverse)",
            spent
        ));
    } else {
        lines.push(
            "  Period official $: none yet — need local grok* turns with cost."
                .to_string(),
        );
    }

    if let Some(pre) = bill.prepaid_usd {
        if pre > 0.0 {
            lines.push(format!("  Prepaid balance: ${pre:.2}"));
        }
    }
    if let (Some(u), Some(c)) = (bill.on_demand_used_usd, bill.on_demand_cap_usd) {
        if c > 0.0 {
            lines.push(format!("  On-demand:       ${u:.2} / ${c:.2} cap"));
        }
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage_activity::store::ModelTotals;

    #[test]
    fn reverse_estimate_from_pct_and_spent() {
        let spent = ModelTotals {
            cost_usd: 24.0,
            ..Default::default()
        };
        let text = format_official_estimate(
            &spent,
            &OfficialEstimateInput {
                usage_pct: 24.0,
                period_label: "Weekly limit".into(),
                period_end: Some("August 11, 22:30".into()),
                prepaid_usd: None,
                on_demand_used_usd: None,
                on_demand_cap_usd: None,
                tier: Some("SuperGrok Heavy".into()),
            },
        );
        assert!(text.contains("Est. allowance: ~$100.00"), "{text}");
        assert!(text.contains("Est. remaining: ~$76.00"), "{text}");
        assert!(text.contains("period official $ ~$24.00"), "{text}");
    }
}
