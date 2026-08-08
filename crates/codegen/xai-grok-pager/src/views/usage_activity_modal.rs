//! Interactive `/usage` activity modal.
//!
//! Claude `/stats` + GitHub heat density; day/week + single-model filter;
//! WebDAV toggle / open config; mouse parity via hit regions.

use chrono::{Datelike, NaiveDate, Weekday};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};

use crate::app::actions::Action;
use crate::app::app_view::InputOutcome;
use crate::theme::Theme;
use crate::usage_activity::{
    cost_mode_help, cost_mode_ui_label, cycle_cost_mode, display_cost_usd, fmt_tokens_kb,
    format_official_estimate, format_usd, toggle_live_display, CostMode, OfficialEstimateInput,
    PricingConfig, SyncStatus, MergedActivity, ModelTotals,
};
use crate::views::credit_bar::CreditBalance;
use crate::views::modal_window::{
    self, ModalSizing, ModalWindowConfig, ModalWindowState, Shortcut,
};

// ---------------------------------------------------------------------------
// Heatmap palette
// ---------------------------------------------------------------------------

fn density_level_absolute(tokens: u64) -> u8 {
    if tokens == 0 {
        0
    } else if tokens < 50_000 {
        1
    } else if tokens < 500_000 {
        2
    } else if tokens < 5_000_000 {
        3
    } else {
        4
    }
}

fn density_level(state: &UsageActivityModalState, tokens: u64) -> u8 {
    if tokens == 0 {
        return 0;
    }
    if state.filter_model.is_some() {
        let peak = peak_day_filtered(state)
            .map(|(_, t)| t)
            .unwrap_or(1)
            .max(1);
        let r = tokens as f64 / peak as f64;
        if r < 0.18 {
            1
        } else if r < 0.40 {
            2
        } else if r < 0.70 {
            3
        } else {
            4
        }
    } else {
        density_level_absolute(tokens)
    }
}

fn color_rgb(c: Color) -> (u8, u8, u8) {
    match c {
        Color::Rgb(r, g, b) => (r, g, b),
        Color::Indexed(n) => crate::render::color::indexed_to_rgb(n),
        _ => (0x26, 0xa6, 0x41),
    }
}

fn mix_rgb(a: (u8, u8, u8), b: (u8, u8, u8), t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    Color::Rgb(
        (a.0 as f32 + (b.0 as f32 - a.0 as f32) * t).round() as u8,
        (a.1 as f32 + (b.1 as f32 - a.1 as f32) * t).round() as u8,
        (a.2 as f32 + (b.2 as f32 - a.2 as f32) * t).round() as u8,
    )
}

fn heat_stops(theme: &Theme, dark_ui: bool) -> [Color; 5] {
    let bg = color_rgb(theme.bg_base);
    let peak = color_rgb(theme.accent_success);
    let empty = if dark_ui {
        mix_rgb(bg, (255, 255, 255), 0.09)
    } else {
        mix_rgb(bg, (0, 0, 0), 0.06)
    };
    let empty_rgb = color_rgb(empty);

    if dark_ui {
        [
            empty,
            mix_rgb(empty_rgb, peak, 0.30),
            mix_rgb(empty_rgb, peak, 0.52),
            mix_rgb(empty_rgb, peak, 0.76),
            Color::Rgb(peak.0, peak.1, peak.2),
        ]
    } else {
        let deep = (
            (peak.0 as f32 * 0.50).round() as u8,
            (peak.1 as f32 * 0.66).round() as u8,
            (peak.2 as f32 * 0.50).round() as u8,
        );
        [
            empty,
            mix_rgb(empty_rgb, peak, 0.38),
            mix_rgb(empty_rgb, peak, 0.66),
            Color::Rgb(peak.0, peak.1, peak.2),
            Color::Rgb(deep.0, deep.1, deep.2),
        ]
    }
}

fn heat_glyph(level: u8) -> char {
    match level {
        0 => '·',
        1 => '░',
        2 => '▒',
        3 => '▓',
        _ => '█',
    }
}

fn heat_style(level: u8, theme: &Theme, dark_ui: bool, selected: bool) -> Style {
    let stops = heat_stops(theme, dark_ui);
    let c = stops[level.min(4) as usize];
    if selected {
        let fg = if dark_ui {
            Color::Rgb(0x0a, 0x0a, 0x0a)
        } else {
            Color::Rgb(0xff, 0xff, 0xff)
        };
        Style::default()
            .fg(fg)
            .bg(stops[4])
            .add_modifier(Modifier::BOLD)
    } else if level == 0 {
        Style::default().fg(c)
    } else {
        Style::default().fg(c)
    }
}

// ---------------------------------------------------------------------------
// Hit testing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageHit {
    Day(usize),
    Model(usize),
    SyncToggle,
    SyncConfig,
    /// Cycle cost attribution mode (off / all / official).
    CostMode,
    /// Toggle live `$` on prompt / subagent frames.
    LiveDisplay,
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeatmapGranularity {
    Day,
    Week,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Calendar,
    Models,
    SyncToggle,
    SyncConfig,
    CostMode,
    LiveDisplay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsagePhase {
    /// Local preview while scan/WebDAV runs.
    Loading,
    Ready,
}

#[derive(Debug)]
pub struct UsageActivityModalState {
    pub window: ModalWindowState,
    pub activity: MergedActivity,
    pub sync_note: Option<String>,
    pub sync_status: SyncStatus,
    pub billing: Option<CreditBalance>,
    pub tier: Option<String>,
    pub selected_day: usize,
    pub selected_model: usize,
    pub model_scroll: usize,
    pub focus: Focus,
    pub granularity: HeatmapGranularity,
    pub filter_model: Option<usize>,
    pub day_keys: Vec<String>,
    pub model_rows: Vec<(String, ModelTotals)>,
    pub phase: UsagePhase,
    /// Current cost attribution mode (from pricing.toml).
    pub cost_mode: CostMode,
    /// Whether live `$` is shown on prompt / subagent frames.
    pub live_display: bool,
    /// Hit regions recorded during last render (content coords).
    pub hits: Vec<(Rect, UsageHit)>,
    /// Content origin for mouse → local hit test.
    pub content_area: Option<Rect>,
}

impl UsageActivityModalState {
    pub fn new(
        activity: MergedActivity,
        sync_note: Option<String>,
        sync_status: SyncStatus,
        billing: Option<CreditBalance>,
        tier: Option<String>,
    ) -> Self {
        let day_keys = build_day_window(120);
        let model_rows = sorted_models(&activity);
        let selected_day = day_keys.len().saturating_sub(1);
        Self {
            window: ModalWindowState::new(),
            activity,
            sync_note,
            sync_status,
            billing,
            tier,
            selected_day,
            selected_model: 0,
            model_scroll: 0,
            focus: Focus::Calendar,
            granularity: HeatmapGranularity::Day,
            filter_model: None,
            day_keys,
            model_rows,
            phase: UsagePhase::Ready,
            cost_mode: PricingConfig::load().mode,
            live_display: PricingConfig::load().live_display,
            hits: Vec::new(),
            content_area: None,
        }
    }

    /// Whether OfficialOnly is offered (subscription / consumer billing present).
    pub fn official_mode_available(&self) -> bool {
        let tier = self.tier.as_deref().unwrap_or("");
        let is_api_key = tier.eq_ignore_ascii_case("api key")
            || tier.eq_ignore_ascii_case("apikey")
            || tier.eq_ignore_ascii_case("api_key");
        !is_api_key && (self.billing.is_some() || self.tier.is_some())
    }

    /// Cycle cost mode with `p` (Off → All → OfficialOnly, clamped for non-sub).
    /// Triggers a local rescan so stored `$` match the new mode.
    pub fn cycle_pricing_mode(&mut self) -> InputOutcome {
        let mode = cycle_cost_mode(self.official_mode_available());
        self.cost_mode = mode;
        self.phase = UsagePhase::Loading;
        self.sync_note = Some(format!(
            "cost mode → {} ({}) · rescanning…",
            cost_mode_ui_label(mode),
            cost_mode_help(mode)
        ));
        InputOutcome::Action(Action::RescanUsageLocal)
    }

    /// Toggle live `$` on the prompt info line and subagent frames (`d` / click).
    pub fn toggle_live_display(&mut self) -> InputOutcome {
        let on = toggle_live_display();
        self.live_display = on;
        self.focus = Focus::LiveDisplay;
        self.sync_note = Some(if on {
            "live $ → ON (prompt + subagent frames)".into()
        } else {
            "live $ → off".into()
        });
        InputOutcome::Changed
    }

    /// Instant loading shell: disk merge + status while worker runs.
    pub fn loading_preview(
        activity: MergedActivity,
        sync_status: SyncStatus,
        billing: Option<CreditBalance>,
        tier: Option<String>,
    ) -> Self {
        let note = if sync_status.enabled {
            "Syncing WebDAV… scanning local usage"
        } else {
            "Scanning local usage…"
        };
        let mut s = Self::new(
            activity,
            Some(note.into()),
            sync_status,
            billing,
            tier,
        );
        s.phase = UsagePhase::Loading;
        s
    }

    fn selected_day_key(&self) -> Option<&str> {
        self.day_keys.get(self.selected_day).map(|s| s.as_str())
    }

    fn filter_model_name(&self) -> Option<&str> {
        self.filter_model
            .and_then(|i| self.model_rows.get(i))
            .map(|(n, _)| n.as_str())
    }

    fn day_tokens(&self, key: &str) -> u64 {
        let Some(day) = self.activity.by_day.get(key) else {
            return 0;
        };
        match self.filter_model_name() {
            Some(model) => day
                .by_model
                .get(model)
                .map(|m| m.total_tokens())
                .unwrap_or(0),
            None => day.total.total_tokens(),
        }
    }

    fn day_cost(&self, key: &str) -> f64 {
        let Some(day) = self.activity.by_day.get(key) else {
            return 0.0;
        };
        match self.filter_model_name() {
            Some(model) => day
                .by_model
                .get(model)
                .map(|m| display_cost_usd(self.cost_mode, model, m))
                .unwrap_or(0.0),
            None => day
                .by_model
                .iter()
                .map(|(name, m)| display_cost_usd(self.cost_mode, name, m))
                .sum(),
        }
    }

    fn toggle_filter_on_selected_model(&mut self) {
        if self.model_rows.is_empty() {
            return;
        }
        match self.filter_model {
            Some(i) if i == self.selected_model => self.filter_model = None,
            _ => self.filter_model = Some(self.selected_model),
        }
    }

    pub fn toggle_sync_enabled(&mut self) -> InputOutcome {
        let new_val = !self.sync_status.enabled;
        match crate::usage_activity::set_enabled(new_val) {
            Ok(st) => {
                self.sync_status = st;
                self.sync_note = Some(if new_val {
                    "WebDAV sync ON — press s or click Sync to pull/push".into()
                } else {
                    "WebDAV sync OFF".into()
                });
                if new_val && self.sync_status.configured {
                    self.phase = UsagePhase::Loading;
                    self.sync_note = Some("Syncing WebDAV…".into());
                    return InputOutcome::Action(Action::ForceUsageSync);
                }
                InputOutcome::Changed
            }
            Err(e) => {
                self.sync_note = Some(format!("toggle failed: {e}"));
                InputOutcome::Changed
            }
        }
    }

    pub fn open_sync_config(&mut self) -> InputOutcome {
        if crate::usage_activity::open_sync_config() {
            self.sync_note = Some("opened sync.toml".into());
        } else {
            self.sync_note = Some(format!(
                "could not open {}",
                self.sync_status.config_path
            ));
        }
        InputOutcome::Changed
    }
}

fn build_day_window(days: i64) -> Vec<String> {
    let today = chrono::Local::now().date_naive();
    let mut keys = Vec::with_capacity(days as usize);
    for i in (0..days).rev() {
        let d = today
            .checked_sub_signed(chrono::Duration::days(i))
            .unwrap_or(today);
        keys.push(d.format("%Y-%m-%d").to_string());
    }
    keys
}

fn sorted_models(activity: &MergedActivity) -> Vec<(String, ModelTotals)> {
    let mut models: Vec<_> = activity.by_model_all_time().into_iter().collect();
    models.sort_by(|a, b| b.1.total_tokens().cmp(&a.1.total_tokens()));
    models
}

/// Week cell width so the grid fills `available` columns.
pub fn week_cell_width(available: usize, week_count: usize) -> usize {
    if week_count == 0 || available == 0 {
        return 1;
    }
    (available / week_count).clamp(1, 5)
}

// ---------------------------------------------------------------------------
// Shortcut ids (footer clickable)
// ---------------------------------------------------------------------------

const SC_WEEK: usize = 1;
const SC_FILTER: usize = 2;
const SC_SYNC: usize = 3;
const SC_CONFIG: usize = 4;
const SC_CLEAR: usize = 5;
const SC_PRICE: usize = 6;
const SC_LIVE: usize = 7;

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

pub fn render_usage_activity(
    state: &mut UsageActivityModalState,
    area: Rect,
    buf: &mut Buffer,
    theme: &Theme,
) {
    state.hits.clear();
    let dark_ui = theme.is_dark();
    let gran = match state.granularity {
        HeatmapGranularity::Day => "day",
        HeatmapGranularity::Week => "week",
    };
    let filt = state
        .filter_model_name()
        .map(|m| format!(" · {m}"))
        .unwrap_or_default();
    let loading = if state.phase == UsagePhase::Loading {
        " · syncing"
    } else {
        ""
    };
    let cost_tag = cost_mode_ui_label(state.cost_mode);
    let live_tag = if state.live_display { "on" } else { "off" };
    let title = format!("Usage · {gran} · cost:{cost_tag} · live:{live_tag}{filt}{loading}");

    let price_label = format!("p cost-mode");
    let live_label = if state.live_display {
        "d live$ ON"
    } else {
        "d live$ off"
    };
    let shortcuts = [
        Shortcut {
            label: "w week",
            clickable: true,
            id: SC_WEEK,
        },
        Shortcut {
            label: "f filter",
            clickable: true,
            id: SC_FILTER,
        },
        Shortcut {
            label: "s sync",
            clickable: true,
            id: SC_SYNC,
        },
        Shortcut {
            label: "config",
            clickable: true,
            id: SC_CONFIG,
        },
        Shortcut {
            label: "c clear",
            clickable: true,
            id: SC_CLEAR,
        },
        Shortcut {
            label: price_label.as_str(),
            clickable: true,
            id: SC_PRICE,
        },
        Shortcut {
            label: live_label,
            clickable: true,
            id: SC_LIVE,
        },
        Shortcut {
            label: "Esc",
            clickable: false,
            id: 0,
        },
    ];
    let config = ModalWindowConfig {
        title: &title,
        tabs: None,
        shortcuts: &shortcuts,
        sizing: ModalSizing {
            width_pct: 0.86,
            max_width: 120,
            min_width: 64,
            v_margin: 1,
            h_pad: 2,
            v_pad: 1,
            footer_lines: 2,
        },
        fold_info: None,
    };
    let Some(content) =
        modal_window::render_modal_window(buf, area, &mut state.window, &config, theme)
    else {
        return;
    };
    state.content_area = Some(content.content);
    render_activity_content(state, content.content, buf, theme, dark_ui);
}

/// Draw the activity body into an arbitrary rect (e.g. embedded as a tab
/// inside the official usage modal). Records hit regions relative to `area`.
pub fn render_activity_content(
    state: &mut UsageActivityModalState,
    area: Rect,
    buf: &mut Buffer,
    theme: &Theme,
    dark_ui: bool,
) {
    state.hits.clear();
    state.content_area = Some(area);
    if area.width < 28 || area.height < 10 {
        return;
    }

    let mut y = area.y;

    // Loading banner
    if state.phase == UsagePhase::Loading {
        put_line(
            buf,
            area.x,
            y,
            area.width,
            "Syncing... local scan + WebDAV pull/push",
            Style::default()
                .fg(theme.accent_user)
                .add_modifier(Modifier::BOLD),
        );
        y = y.saturating_add(1);
    }

    y = draw_sync_status(state, area, buf, theme, y);
    y = y.saturating_add(1);

    y = draw_cost_mode_row(state, area, buf, theme, y);
    y = y.saturating_add(1);

    // Reverse-estimate sits above heatmap/models so it is not clipped off-screen.
    y = draw_official(state, area, buf, theme, y);
    y = y.saturating_add(1);

    y = draw_kpi_cards(state, area, buf, theme, y);
    y = y.saturating_add(1);

    y = draw_legend(area, buf, theme, dark_ui, y, state.filter_model.is_some());

    let cal_focus = state.focus == Focus::Calendar;
    match state.granularity {
        HeatmapGranularity::Day => {
            y = draw_day_strip(state, area, buf, theme, dark_ui, y, cal_focus);
        }
        HeatmapGranularity::Week => {
            y = draw_week_grid(state, area, buf, theme, dark_ui, y, cal_focus);
        }
    }

    y = draw_selected_day(state, area, buf, theme, y);
    y = y.saturating_add(1);

    let _ = draw_models(state, area, buf, theme, y);
}

// ---------------------------------------------------------------------------
// Sync status (toggle + config path)
// ---------------------------------------------------------------------------

fn draw_sync_status(
    state: &mut UsageActivityModalState,
    area: Rect,
    buf: &mut Buffer,
    theme: &Theme,
    y: u16,
) -> u16 {
    let st = &state.sync_status;
    let on = st.enabled;
    let switch = if on { "[ON ]" } else { "[OFF]" };
    let focus_toggle = state.focus == Focus::SyncToggle;
    let focus_cfg = state.focus == Focus::SyncConfig;

    // Line 1: WebDAV sync [ON/OFF]
    let label = "WebDAV sync  ";
    put_line(
        buf,
        area.x,
        y,
        area.width,
        label,
        Style::default().fg(theme.gray),
    );
    let sx = area.x.saturating_add(label.len() as u16);
    let switch_style = if focus_toggle {
        Style::default()
            .fg(theme.text_primary)
            .bg(theme.bg_highlight)
            .add_modifier(Modifier::BOLD)
    } else if on {
        Style::default()
            .fg(theme.accent_success)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.gray)
    };
    put_line(buf, sx, y, 6, switch, switch_style);
    state.hits.push((
        Rect {
            x: sx,
            y,
            width: 5,
            height: 1,
        },
        UsageHit::SyncToggle,
    ));

    let last = st.last_synced_at.as_deref().unwrap_or("never");
    let tail = format!("  ·  last {last}");
    put_line(
        buf,
        sx.saturating_add(6),
        y,
        area.width.saturating_sub(sx.saturating_add(6) - area.x),
        &tail,
        Style::default().fg(theme.gray),
    );
    let mut y = y.saturating_add(1);

    // Line 2: config path (always shown; open with Enter when focused)
    let cfg_label = "Config  ";
    put_line(
        buf,
        area.x,
        y,
        area.width,
        cfg_label,
        Style::default().fg(theme.gray),
    );
    let cx = area.x.saturating_add(cfg_label.len() as u16);
    let path = if st.config_path.is_empty() {
        "~/.grok/usage/sync.toml".to_string()
    } else {
        st.config_path.clone()
    };
    let path_disp = truncate(&path, (area.width as usize).saturating_sub(16).max(12));
    let cfg_style = if focus_cfg {
        Style::default()
            .fg(theme.accent_user)
            .bg(theme.bg_highlight)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.text_secondary)
    };
    put_line(
        buf,
        cx,
        y,
        area.width.saturating_sub(cx - area.x),
        &format!("{path_disp}  ↵ open"),
        cfg_style,
    );
    state.hits.push((
        Rect {
            x: area.x,
            y,
            width: area.width,
            height: 1,
        },
        UsageHit::SyncConfig,
    ));
    y = y.saturating_add(1);

    // Line 3: remote + note
    if st.configured && on {
        let rem = if st.base_path.chars().count() > 56 {
            truncate(&st.base_path, 56)
        } else {
            st.base_path.clone()
        };
        put_line(
            buf,
            area.x,
            y,
            area.width,
            &format!("Remote  {rem}"),
            Style::default().fg(theme.gray),
        );
        y = y.saturating_add(1);
    }

    if let Some(note) = &state.sync_note {
        put_line(
            buf,
            area.x,
            y,
            area.width,
            &format!("↻ {note}"),
            Style::default().fg(theme.text_secondary),
        );
        y = y.saturating_add(1);
    } else if let Some(res) = &st.last_result {
        put_line(
            buf,
            area.x,
            y,
            area.width,
            &format!("↻ {res}"),
            Style::default().fg(theme.gray),
        );
        y = y.saturating_add(1);
    }
    y
}

// ---------------------------------------------------------------------------
// Cost mode (p to cycle) — dedicated, always-visible row
// ---------------------------------------------------------------------------

fn draw_cost_mode_row(
    state: &mut UsageActivityModalState,
    area: Rect,
    buf: &mut Buffer,
    theme: &Theme,
    y: u16,
) -> u16 {
    let focused = state.focus == Focus::CostMode;
    let live_focused = state.focus == Focus::LiveDisplay;
    let mode = state.cost_mode;
    let live = state.live_display;

    // Row 1: Cost  [off] [all] [official]  ·  help  ·  p
    let prefix = "Cost  ";
    put_line(
        buf,
        area.x,
        y,
        area.width,
        prefix,
        Style::default().fg(if focused {
            theme.accent_user
        } else {
            theme.gray
        }),
    );

    let mut x = area.x.saturating_add(prefix.len() as u16);
    let options: &[(CostMode, &str)] = &[
        (CostMode::Off, "off"),
        (CostMode::All, "all"),
        (CostMode::OfficialOnly, "official"),
    ];
    for (_i, (m, lab)) in options.iter().enumerate() {
        let active = *m == mode;
        let unavailable = *m == CostMode::OfficialOnly && !state.official_mode_available();
        let token = if active {
            format!("[{lab}]")
        } else if unavailable {
            format!("({lab})")
        } else {
            format!(" {lab} ")
        };
        let style = if active {
            Style::default()
                .fg(theme.text_primary)
                .bg(if focused {
                    theme.bg_highlight
                } else {
                    theme.bg_visual
                })
                .add_modifier(Modifier::BOLD)
        } else if unavailable {
            Style::default().fg(theme.gray_dim)
        } else if focused {
            Style::default().fg(theme.gray_bright)
        } else {
            Style::default().fg(theme.gray)
        };
        let w = token.chars().count() as u16;
        put_line(buf, x, y, w.max(1), &token, style);
        x = x.saturating_add(w.saturating_add(1));
    }

    let rest = format!(" · {} · p/click cycle", cost_mode_help(mode));
    put_line(
        buf,
        x.saturating_add(1),
        y,
        area.width.saturating_sub(x.saturating_add(1) - area.x),
        &rest,
        Style::default().fg(theme.gray),
    );

    // Cost mode hit = full first row (live has its own row below).
    state.hits.push((
        Rect {
            x: area.x,
            y,
            width: area.width,
            height: 1,
        },
        UsageHit::CostMode,
    ));
    let y = y.saturating_add(1);

    // Row 2: Live $  [ON] / off  ·  prompt + subagent frames  ·  d
    let live_prefix = "Live $ ";
    put_line(
        buf,
        area.x,
        y,
        area.width,
        live_prefix,
        Style::default().fg(if live_focused {
            theme.accent_user
        } else {
            theme.gray
        }),
    );
    let mut lx = area.x.saturating_add(live_prefix.len() as u16);
    let live_token = if live { "[ON]" } else { " off " };
    let live_style = if live {
        Style::default()
            .fg(theme.text_primary)
            .bg(if live_focused {
                theme.bg_highlight
            } else {
                theme.bg_visual
            })
            .add_modifier(Modifier::BOLD)
    } else if live_focused {
        Style::default().fg(theme.gray_bright)
    } else {
        Style::default().fg(theme.gray)
    };
    let lw = live_token.chars().count() as u16;
    put_line(buf, lx, y, lw.max(1), live_token, live_style);
    lx = lx.saturating_add(lw.saturating_add(1));
    put_line(
        buf,
        lx,
        y,
        area.width.saturating_sub(lx - area.x),
        " · prompt + subagent frames · d/click toggle",
        Style::default().fg(theme.gray),
    );
    state.hits.push((
        Rect {
            x: area.x,
            y,
            width: area.width,
            height: 1,
        },
        UsageHit::LiveDisplay,
    ));
    y.saturating_add(1)
}

// ---------------------------------------------------------------------------
// KPI
// ---------------------------------------------------------------------------

fn draw_kpi_cards(
    state: &UsageActivityModalState,
    area: Rect,
    buf: &mut Buffer,
    theme: &Theme,
    y: u16,
) -> u16 {
    let grand = filtered_grand(state);
    let peak = peak_day_filtered(state);
    let peak_tokens = peak
        .as_ref()
        .map(|(_, t)| fmt_tokens_kb(*t))
        .unwrap_or_else(|| "—".into());
    let peak_day = peak
        .as_ref()
        .map(|(d, _)| short_date(d))
        .unwrap_or_else(|| "—".into());

    let fav = state
        .model_rows
        .first()
        .map(|(n, _)| truncate(n, 18))
        .unwrap_or_else(|| "—".into());

    let col_w = (area.width / 2).max(28);
    let est_cost = display_grand_cost(state);
    let left = [
        ("Total", fmt_tokens_kb(grand.total_tokens())),
        ("Est. cost", format_usd(est_cost)),
    ];
    let right = [
        ("Peak", format!("{peak_tokens} · {peak_day}")),
        (
            "Streak",
            format!(
                "{}d (best {})",
                state.activity.current_streak_days(),
                state.activity.longest_streak_days()
            ),
        ),
    ];
    for (i, (lab, val)) in left.iter().enumerate() {
        draw_kpi_cell(buf, area.x, y + i as u16, col_w, lab, val, theme);
    }
    for (i, (lab, val)) in right.iter().enumerate() {
        draw_kpi_cell(
            buf,
            area.x + col_w,
            y + i as u16,
            area.width.saturating_sub(col_w),
            lab,
            val,
            theme,
        );
    }

    let y2 = y.saturating_add(2);
    if y2 < area.y + area.height {
        let devices = state.activity.devices.len();
        put_line(
            buf,
            area.x,
            y2,
            area.width,
            &format!("Top model  {fav}  ·  {devices} device(s)"),
            Style::default().fg(theme.gray),
        );
        return y2.saturating_add(1);
    }
    y.saturating_add(2)
}

fn draw_kpi_cell(
    buf: &mut Buffer,
    x: u16,
    y: u16,
    width: u16,
    label: &str,
    value: &str,
    theme: &Theme,
) {
    put_line(
        buf,
        x,
        y,
        width,
        &format!("{label:<10}"),
        Style::default().fg(theme.gray),
    );
    let label_w = 11u16;
    if width > label_w {
        put_line(
            buf,
            x + label_w,
            y,
            width - label_w,
            value,
            Style::default()
                .fg(theme.text_primary)
                .add_modifier(Modifier::BOLD),
        );
    }
}

// ---------------------------------------------------------------------------
// Legend
// ---------------------------------------------------------------------------

fn draw_legend(
    area: Rect,
    buf: &mut Buffer,
    theme: &Theme,
    dark_ui: bool,
    y: u16,
    relative: bool,
) -> u16 {
    put_line(
        buf,
        area.x,
        y,
        area.width,
        "Less",
        Style::default().fg(theme.gray),
    );
    let mut x = area.x + 5;
    for level in 0u8..=4 {
        if let Some(cell) = buf.cell_mut((x, y)) {
            cell.set_char(heat_glyph(level));
            cell.set_style(heat_style(level, theme, dark_ui, false));
        }
        x = x.saturating_add(1);
    }
    let suffix = if relative {
        " More   relative to model peak"
    } else {
        " More"
    };
    put_line(
        buf,
        x.saturating_add(1),
        y,
        area.width.saturating_sub(x.saturating_add(1) - area.x),
        suffix,
        Style::default().fg(theme.gray),
    );
    y.saturating_add(1)
}

// ---------------------------------------------------------------------------
// Day strip
// ---------------------------------------------------------------------------

fn draw_day_strip(
    state: &mut UsageActivityModalState,
    area: Rect,
    buf: &mut Buffer,
    theme: &Theme,
    dark_ui: bool,
    y: u16,
    focused: bool,
) -> u16 {
    let max_cells = (area.width as usize)
        .min(state.day_keys.len())
        .min(120);
    let start = state.day_keys.len().saturating_sub(max_cells);
    let visible = &state.day_keys[start..];

    let mut months = vec![' '; visible.len()];
    let mut last_m = 0u32;
    for (i, key) in visible.iter().enumerate() {
        if let Ok(d) = NaiveDate::parse_from_str(key, "%Y-%m-%d") {
            let m = d.month();
            if m != last_m {
                let label: Vec<char> = d.format("%b").to_string().chars().collect();
                for (j, ch) in label.into_iter().enumerate() {
                    if i + j < months.len() {
                        months[i + j] = ch;
                    }
                }
                last_m = m;
            }
        }
    }
    let month_s: String = months.into_iter().collect();
    put_line(
        buf,
        area.x,
        y,
        area.width,
        &month_s,
        Style::default().fg(if focused {
            theme.gray_bright
        } else {
            theme.gray
        }),
    );
    let y = y.saturating_add(1);

    let mut x = area.x;
    for (i, key) in visible.iter().enumerate() {
        let global_i = start + i;
        let tokens = state.day_tokens(key);
        let level = density_level(state, tokens);
        let selected = global_i == state.selected_day;
        if let Some(cell) = buf.cell_mut((x, y)) {
            cell.set_char(if selected {
                '█'
            } else {
                heat_glyph(level)
            });
            cell.set_style(heat_style(level, theme, dark_ui, selected));
        }
        state.hits.push((
            Rect {
                x,
                y,
                width: 1,
                height: 1,
            },
            UsageHit::Day(global_i),
        ));
        x = x.saturating_add(1);
        if x >= area.x + area.width {
            break;
        }
    }
    y.saturating_add(1)
}

// ---------------------------------------------------------------------------
// Week grid — fills available width via cell_w
// ---------------------------------------------------------------------------

fn draw_week_grid(
    state: &mut UsageActivityModalState,
    area: Rect,
    buf: &mut Buffer,
    theme: &Theme,
    dark_ui: bool,
    mut y: u16,
    focused: bool,
) -> u16 {
    let mut weeks: Vec<Vec<Option<(usize, String)>>> = Vec::new();
    let mut cur: Vec<Option<(usize, String)>> = vec![None; 7];
    for (i, key) in state.day_keys.iter().enumerate() {
        let Ok(d) = NaiveDate::parse_from_str(key, "%Y-%m-%d") else {
            continue;
        };
        let wd = match d.weekday() {
            Weekday::Sun => 0,
            Weekday::Mon => 1,
            Weekday::Tue => 2,
            Weekday::Wed => 3,
            Weekday::Thu => 4,
            Weekday::Fri => 5,
            Weekday::Sat => 6,
        };
        if wd == 0 && cur.iter().any(|c| c.is_some()) {
            weeks.push(std::mem::replace(&mut cur, vec![None; 7]));
        }
        cur[wd] = Some((i, key.clone()));
    }
    if cur.iter().any(|c| c.is_some()) {
        weeks.push(cur);
    }

    let label_w = 2u16;
    let avail = (area.width as usize).saturating_sub(label_w as usize);
    // Show as many weeks as fit at min cell_w=1, then expand cell width to fill.
    let max_cols = avail.min(weeks.len()).max(1);
    let start_col = weeks.len().saturating_sub(max_cols);
    let visible_n = weeks.len().saturating_sub(start_col).max(1);
    let cell_w = week_cell_width(avail, visible_n);

    // Month labels spaced by cell_w.
    let mut month_line = vec![' '; visible_n * cell_w];
    let mut last_m = 0u32;
    for (ci, col) in (start_col..weeks.len()).enumerate() {
        for slot in &weeks[col] {
            if let Some((_, key)) = slot {
                if let Ok(d) = NaiveDate::parse_from_str(key, "%Y-%m-%d") {
                    let m = d.month();
                    if m != last_m {
                        let label: Vec<char> = d.format("%b").to_string().chars().collect();
                        let base = ci * cell_w;
                        for (j, ch) in label.into_iter().enumerate() {
                            if base + j < month_line.len() {
                                month_line[base + j] = ch;
                            }
                        }
                        last_m = m;
                    }
                    break;
                }
            }
        }
    }
    put_line(
        buf,
        area.x + label_w,
        y,
        area.width.saturating_sub(label_w),
        &month_line.into_iter().collect::<String>(),
        Style::default().fg(if focused {
            theme.gray_bright
        } else {
            theme.gray
        }),
    );
    y = y.saturating_add(1);

    let labels = ["S", "M", "T", "W", "T", "F", "S"];
    for row in 0..7 {
        put_line(
            buf,
            area.x,
            y,
            label_w,
            labels[row],
            Style::default().fg(theme.gray),
        );
        let mut x = area.x + label_w;
        for col in start_col..weeks.len() {
            match &weeks[col][row] {
                Some((idx, key)) => {
                    let tok = state.day_tokens(key);
                    let level = density_level(state, tok);
                    let selected = *idx == state.selected_day;
                    let ch = if selected {
                        '█'
                    } else {
                        heat_glyph(level)
                    };
                    let style = heat_style(level, theme, dark_ui, selected);
                    for dx in 0..cell_w as u16 {
                        if let Some(cell) = buf.cell_mut((x + dx, y)) {
                            // First col glyph, rest fill with same block when wide.
                            cell.set_char(if dx == 0 {
                                ch
                            } else if level == 0 {
                                ' '
                            } else {
                                ch
                            });
                            cell.set_style(style);
                        }
                    }
                    state.hits.push((
                        Rect {
                            x,
                            y,
                            width: cell_w as u16,
                            height: 1,
                        },
                        UsageHit::Day(*idx),
                    ));
                }
                None => {
                    for dx in 0..cell_w as u16 {
                        if let Some(cell) = buf.cell_mut((x + dx, y)) {
                            cell.set_char(' ');
                        }
                    }
                }
            }
            x = x.saturating_add(cell_w as u16);
            if x >= area.x + area.width {
                break;
            }
        }
        y = y.saturating_add(1);
    }
    y
}

// ---------------------------------------------------------------------------
// Selected day
// ---------------------------------------------------------------------------

fn draw_selected_day(
    state: &UsageActivityModalState,
    area: Rect,
    buf: &mut Buffer,
    theme: &Theme,
    y: u16,
) -> u16 {
    let Some(key) = state.selected_day_key().map(|s| s.to_string()) else {
        return y;
    };
    let tok = state.day_tokens(&key);
    let cost = state.day_cost(&key);
    let filt = state
        .filter_model_name()
        .map(|m| format!(" · {m}"))
        .unwrap_or_default();

    put_line(
        buf,
        area.x,
        y,
        area.width,
        &format!(
            "▶ {}{}   {}   {}",
            short_date(&key),
            filt,
            fmt_tokens_kb(tok),
            format_usd(cost)
        ),
        Style::default()
            .fg(theme.text_primary)
            .add_modifier(Modifier::BOLD),
    );
    let mut y = y.saturating_add(1);

    if let Some(day) = state.activity.by_day.get(&key) {
        let mut parts: Vec<(u64, String)> = day
            .by_model
            .iter()
            .filter(|(m, _)| {
                state
                    .filter_model_name()
                    .map(|f| f == m.as_str())
                    .unwrap_or(true)
            })
            .map(|(m, t)| {
                (
                    t.total_tokens(),
                    format!("{} {}", m, fmt_tokens_kb(t.total_tokens())),
                )
            })
            .collect();
        parts.sort_by(|a, b| b.0.cmp(&a.0));
        if !parts.is_empty() {
            let line = parts
                .into_iter()
                .take(4)
                .map(|(_, s)| s)
                .collect::<Vec<_>>()
                .join("  ·  ");
            put_line(
                buf,
                area.x.saturating_add(2),
                y,
                area.width.saturating_sub(2),
                &line,
                Style::default().fg(theme.text_secondary),
            );
            y = y.saturating_add(1);
        }
    }
    y
}

// ---------------------------------------------------------------------------
// Models
// ---------------------------------------------------------------------------

fn draw_models(
    state: &mut UsageActivityModalState,
    area: Rect,
    buf: &mut Buffer,
    theme: &Theme,
    y: u16,
) -> u16 {
    let model_focus = state.focus == Focus::Models;
    put_line(
        buf,
        area.x,
        y,
        area.width,
        "Models",
        Style::default()
            .fg(if model_focus {
                theme.accent_user
            } else {
                theme.gray_bright
            })
            .add_modifier(if model_focus {
                Modifier::BOLD
            } else {
                Modifier::empty()
            }),
    );
    let y = y.saturating_add(1);

    let list_h = area
        .height
        .saturating_sub(y.saturating_sub(area.y))
        .saturating_sub(5)
        .max(3) as usize;
    ensure_model_visible(state, list_h);

    let total_tok = state.activity.grand_total().total_tokens().max(1);
    let end = (state.model_scroll + list_h).min(state.model_rows.len());
    let bar_w = 10usize;
    let name_w = 20usize;

    for (row_i, idx) in (state.model_scroll..end).enumerate() {
        let (name, m) = &state.model_rows[idx];
        let pct = m.total_tokens() as f64 / total_tok as f64 * 100.0;
        let bar = share_bar(m.total_tokens(), total_tok, bar_w);
        let pool = if m.official { "off" } else { "ext" };
        let mark = if state.filter_model == Some(idx) {
            "●"
        } else {
            " "
        };
        let row_cost = display_cost_usd(state.cost_mode, name, m);
        let line = format!(
            "{mark} {:<name_w$} {bar} {:>7} {:>5.1}%  ${:>7.2}  {pool}",
            truncate(name, name_w),
            fmt_tokens_kb(m.total_tokens()),
            pct,
            row_cost,
        );
        let selected = idx == state.selected_model;
        let filtered = state.filter_model == Some(idx);
        let style = if selected {
            Style::default()
                .fg(theme.text_primary)
                .bg(theme.bg_highlight)
                .add_modifier(if model_focus {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                })
        } else if filtered {
            Style::default().fg(theme.accent_success)
        } else {
            Style::default().fg(theme.text_secondary)
        };

        let row_y = y + row_i as u16;
        put_line(buf, area.x, row_y, area.width, &line, style);
        state.hits.push((
            Rect {
                x: area.x,
                y: row_y,
                width: area.width,
                height: 1,
            },
            UsageHit::Model(idx),
        ));

        if !selected {
            let bar_x = area.x.saturating_add(23);
            for i in 0..bar_w as u16 {
                if let Some(cell) = buf.cell_mut((bar_x + i, row_y)) {
                    let ch = cell.symbol().chars().next().unwrap_or(' ');
                    if ch == '█' {
                        cell.set_style(Style::default().fg(theme.accent_success));
                    } else if ch == '░' {
                        cell.set_style(Style::default().fg(theme.gray_dim));
                    }
                }
            }
        }
        if let Some(pos) = line.rfind(pool) {
            let px = area.x.saturating_add(pos as u16);
            let badge_style = if m.official {
                Style::default().fg(theme.accent_success)
            } else {
                Style::default().fg(theme.accent_model)
            };
            if !selected {
                for (i, ch) in pool.chars().enumerate() {
                    if let Some(cell) = buf.cell_mut((px + i as u16, row_y)) {
                        cell.set_char(ch);
                        cell.set_style(badge_style);
                    }
                }
            }
        }
    }

    y.saturating_add(list_h as u16).saturating_add(1)
}

fn share_bar(tokens: u64, total: u64, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let filled = if total == 0 {
        0
    } else {
        ((tokens as f64 / total as f64) * width as f64).round() as usize
    }
    .min(width);
    let mut s = String::with_capacity(width);
    for i in 0..width {
        s.push(if i < filled { '█' } else { '░' });
    }
    s
}

// ---------------------------------------------------------------------------
// Official estimate
// ---------------------------------------------------------------------------

/// Draw reverse-estimate block; returns the next free row `y`.
fn draw_official(
    state: &UsageActivityModalState,
    area: Rect,
    buf: &mut Buffer,
    theme: &Theme,
    y: u16,
) -> u16 {
    if y >= area.y + area.height {
        return y;
    }
    let days = state
        .billing
        .as_ref()
        .and_then(|b| b.period_type.as_deref())
        .map(|t| if t.contains("WEEKLY") { 7 } else { 31 })
        .unwrap_or(31);
    let official = state.activity.official_total_last_days(days);
    let Some(bal) = &state.billing else {
        put_line(
            buf,
            area.x,
            y,
            area.width,
            "Reverse $: waiting for billing (open Usage limit once, or wait for fetch)…",
            Style::default().fg(theme.gray),
        );
        return y.saturating_add(1);
    };
    let est = format_official_estimate(
        &official,
        &OfficialEstimateInput {
            usage_pct: bal.usage_pct,
            period_label: bal.usage_label().to_string(),
            period_end: bal.period_end_display.clone(),
            prepaid_usd: bal
                .prepaid_balance_cents
                .map(|c| c.unsigned_abs() as f64 / 100.0),
            on_demand_used_usd: bal
                .on_demand_used_cents
                .map(|c| c.unsigned_abs() as f64 / 100.0),
            on_demand_cap_usd: bal
                .on_demand_cap_cents
                .map(|c| c.unsigned_abs() as f64 / 100.0),
            tier: state.tier.clone(),
        },
    );
    let mut yy = y;
    for line in est.lines().take(6) {
        if yy >= area.y + area.height {
            break;
        }
        put_line(
            buf,
            area.x,
            yy,
            area.width,
            line,
            Style::default().fg(theme.gray),
        );
        yy = yy.saturating_add(1);
    }
    yy
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn filtered_grand(state: &UsageActivityModalState) -> ModelTotals {
    match state.filter_model_name() {
        Some(model) => state
            .model_rows
            .iter()
            .find(|(n, _)| n == model)
            .map(|(_, m)| m.clone())
            .unwrap_or_default(),
        None => state.activity.grand_total(),
    }
}

/// Est. $ for KPI under current cost mode (not raw stored grand total).
fn display_grand_cost(state: &UsageActivityModalState) -> f64 {
    match state.filter_model_name() {
        Some(model) => state
            .model_rows
            .iter()
            .find(|(n, _)| n == model)
            .map(|(_, m)| display_cost_usd(state.cost_mode, model, m))
            .unwrap_or(0.0),
        None => state
            .model_rows
            .iter()
            .map(|(n, m)| display_cost_usd(state.cost_mode, n, m))
            .sum(),
    }
}

fn peak_day_filtered(state: &UsageActivityModalState) -> Option<(String, u64)> {
    state
        .day_keys
        .iter()
        .map(|k| (k.clone(), state.day_tokens(k)))
        .filter(|(_, t)| *t > 0)
        .max_by_key(|(_, t)| *t)
}

fn ensure_model_visible(state: &mut UsageActivityModalState, view_h: usize) {
    if state.model_rows.is_empty() {
        return;
    }
    if state.selected_model < state.model_scroll {
        state.model_scroll = state.selected_model;
    } else if state.selected_model >= state.model_scroll + view_h {
        state.model_scroll = state.selected_model + 1 - view_h;
    }
}

fn short_date(ymd: &str) -> String {
    NaiveDate::parse_from_str(ymd, "%Y-%m-%d")
        .map(|d| d.format("%b %d").to_string())
        .unwrap_or_else(|_| ymd.to_string())
}

fn put_line(buf: &mut Buffer, x: u16, y: u16, width: u16, text: &str, style: Style) {
    use unicode_width::UnicodeWidthStr;
    let mut col = x;
    let end = x.saturating_add(width);
    for ch in text.chars() {
        let w = UnicodeWidthStr::width(ch.encode_utf8(&mut [0; 4])) as u16;
        if col + w > end {
            break;
        }
        if let Some(cell) = buf.cell_mut((col, y)) {
            cell.set_char(ch);
            cell.set_style(style);
        }
        col = col.saturating_add(w.max(1));
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn hit_at(state: &UsageActivityModalState, col: u16, row: u16) -> Option<UsageHit> {
    for (rect, hit) in state.hits.iter().rev() {
        if col >= rect.x
            && col < rect.x.saturating_add(rect.width)
            && row >= rect.y
            && row < rect.y.saturating_add(rect.height)
        {
            return Some(*hit);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Input
// ---------------------------------------------------------------------------

pub fn handle_usage_key(state: &mut UsageActivityModalState, key: &KeyEvent) -> InputOutcome {
    if key.kind != KeyEventKind::Press && key.kind != KeyEventKind::Repeat {
        return InputOutcome::Unchanged;
    }
    match key.code {
        KeyCode::Tab => {
            state.focus = match state.focus {
                Focus::Calendar => Focus::Models,
                Focus::Models => Focus::SyncToggle,
                Focus::SyncToggle => Focus::SyncConfig,
                Focus::SyncConfig => Focus::CostMode,
                Focus::CostMode => Focus::LiveDisplay,
                Focus::LiveDisplay => Focus::Calendar,
            };
            InputOutcome::Changed
        }
        KeyCode::Char('w') | KeyCode::Char('W') => {
            state.granularity = match state.granularity {
                HeatmapGranularity::Day => HeatmapGranularity::Week,
                HeatmapGranularity::Week => HeatmapGranularity::Day,
            };
            InputOutcome::Changed
        }
        KeyCode::Char('f') | KeyCode::Char('F') => {
            state.toggle_filter_on_selected_model();
            InputOutcome::Changed
        }
        KeyCode::Char('c') | KeyCode::Char('C') => {
            state.filter_model = None;
            InputOutcome::Changed
        }
        KeyCode::Char('s') | KeyCode::Char('S') => {
            state.phase = UsagePhase::Loading;
            state.sync_note = Some("Syncing WebDAV…".into());
            InputOutcome::Action(Action::ForceUsageSync)
        }
        KeyCode::Char('p') | KeyCode::Char('P') => state.cycle_pricing_mode(),
        KeyCode::Char('d') | KeyCode::Char('D') => state.toggle_live_display(),
        KeyCode::Left | KeyCode::Char('h') => {
            if state.selected_day > 0 {
                state.selected_day -= 1;
            }
            state.focus = Focus::Calendar;
            InputOutcome::Changed
        }
        KeyCode::Right | KeyCode::Char('l') => {
            if state.selected_day + 1 < state.day_keys.len() {
                state.selected_day += 1;
            }
            state.focus = Focus::Calendar;
            InputOutcome::Changed
        }
        KeyCode::Up | KeyCode::Char('k') => match state.focus {
            Focus::LiveDisplay => {
                state.focus = Focus::CostMode;
                InputOutcome::Changed
            }
            Focus::CostMode => {
                state.focus = Focus::SyncConfig;
                InputOutcome::Changed
            }
            Focus::SyncConfig => {
                state.focus = Focus::SyncToggle;
                InputOutcome::Changed
            }
            Focus::SyncToggle => {
                state.focus = Focus::Models;
                InputOutcome::Changed
            }
            _ => {
                state.focus = Focus::Models;
                if state.selected_model > 0 {
                    state.selected_model -= 1;
                }
                InputOutcome::Changed
            }
        },
        KeyCode::Down | KeyCode::Char('j') => match state.focus {
            Focus::SyncToggle => {
                state.focus = Focus::SyncConfig;
                InputOutcome::Changed
            }
            Focus::SyncConfig => {
                state.focus = Focus::CostMode;
                InputOutcome::Changed
            }
            Focus::CostMode => {
                state.focus = Focus::LiveDisplay;
                InputOutcome::Changed
            }
            Focus::LiveDisplay => InputOutcome::Changed,
            _ => {
                state.focus = Focus::Models;
                if state.selected_model + 1 < state.model_rows.len() {
                    state.selected_model += 1;
                }
                InputOutcome::Changed
            }
        },
        KeyCode::Enter => match state.focus {
            Focus::Models => {
                state.toggle_filter_on_selected_model();
                InputOutcome::Changed
            }
            Focus::SyncToggle => state.toggle_sync_enabled(),
            Focus::SyncConfig => state.open_sync_config(),
            Focus::CostMode => state.cycle_pricing_mode(),
            Focus::LiveDisplay => state.toggle_live_display(),
            Focus::Calendar => InputOutcome::Unchanged,
        },
        KeyCode::Home => {
            state.selected_day = 0;
            InputOutcome::Changed
        }
        KeyCode::End => {
            state.selected_day = state.day_keys.len().saturating_sub(1);
            InputOutcome::Changed
        }
        _ => InputOutcome::Unchanged,
    }
}

/// Footer shortcut id activation (mouse click on chrome shortcuts).
pub fn handle_usage_shortcut(state: &mut UsageActivityModalState, id: usize) -> InputOutcome {
    match id {
        SC_WEEK => {
            state.granularity = match state.granularity {
                HeatmapGranularity::Day => HeatmapGranularity::Week,
                HeatmapGranularity::Week => HeatmapGranularity::Day,
            };
            InputOutcome::Changed
        }
        SC_FILTER => {
            state.toggle_filter_on_selected_model();
            InputOutcome::Changed
        }
        SC_SYNC => {
            state.phase = UsagePhase::Loading;
            state.sync_note = Some("Syncing WebDAV…".into());
            InputOutcome::Action(Action::ForceUsageSync)
        }
        SC_CONFIG => state.open_sync_config(),
        SC_CLEAR => {
            state.filter_model = None;
            InputOutcome::Changed
        }
        SC_PRICE => state.cycle_pricing_mode(),
        SC_LIVE => state.toggle_live_display(),
        _ => InputOutcome::Unchanged,
    }
}

pub fn handle_usage_mouse(
    state: &mut UsageActivityModalState,
    kind: MouseEventKind,
    column: u16,
    row: u16,
) -> InputOutcome {
    match kind {
        MouseEventKind::Down(MouseButton::Left) => {
            if let Some(hit) = hit_at(state, column, row) {
                match hit {
                    UsageHit::Day(i) => {
                        state.selected_day = i;
                        state.focus = Focus::Calendar;
                        InputOutcome::Changed
                    }
                    UsageHit::Model(i) => {
                        if state.selected_model == i && state.focus == Focus::Models {
                            state.toggle_filter_on_selected_model();
                        } else {
                            state.selected_model = i;
                            state.focus = Focus::Models;
                        }
                        InputOutcome::Changed
                    }
                    UsageHit::SyncToggle => {
                        state.focus = Focus::SyncToggle;
                        state.toggle_sync_enabled()
                    }
                    UsageHit::SyncConfig => {
                        state.focus = Focus::SyncConfig;
                        state.open_sync_config()
                    }
                    UsageHit::CostMode => {
                        state.focus = Focus::CostMode;
                        state.cycle_pricing_mode()
                    }
                    UsageHit::LiveDisplay => {
                        state.focus = Focus::LiveDisplay;
                        state.toggle_live_display()
                    }
                }
            } else {
                InputOutcome::Unchanged
            }
        }
        MouseEventKind::ScrollUp => {
            if state.focus == Focus::Models {
                if state.selected_model > 0 {
                    state.selected_model -= 1;
                }
            } else if state.selected_day > 0 {
                state.selected_day -= 1;
            }
            InputOutcome::Changed
        }
        MouseEventKind::ScrollDown => {
            if state.focus == Focus::Models {
                if state.selected_model + 1 < state.model_rows.len() {
                    state.selected_model += 1;
                }
            } else if state.selected_day + 1 < state.day_keys.len() {
                state.selected_day += 1;
            }
            InputOutcome::Changed
        }
        _ => InputOutcome::Unchanged,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn week_cell_width_fills() {
        assert_eq!(week_cell_width(50, 17), 2); // 50/17 = 2
        assert_eq!(week_cell_width(100, 17), 5); // clamp max 5
        assert_eq!(week_cell_width(10, 17), 1);
        assert_eq!(week_cell_width(0, 5), 1);
    }
}
