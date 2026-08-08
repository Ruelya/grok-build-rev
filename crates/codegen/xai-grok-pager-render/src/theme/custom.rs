//! User-defined theme registration (same apply path as official themes).
//!
//! Custom themes are stored by name and, when active, take precedence over
//! the builtin [`super::ThemeKind`] cache in [`super::Theme::current`].
//!
//! ## Disk layout
//!
//! `{config_home}/themes/<name>.toml` (typically `~/.grok/themes/`):
//!
//! ```toml
//! base = "groknight"   # required: builtin ThemeKind name
//! # optional hex overrides (#RGB or #RRGGBB) for any Theme color field:
//! # accent_error = "#ff3366"
//! # md_heading_h1 = "#ff6b2b"
//! # diff_insert_fg = "#87904a"
//!
//! # OpenCode-level syntax roles (optional; defaults derived from UI colors):
//! [syntax]
//! keyword = "#ff6b2b"
//! string = "#4f8a62"
//! comment = "#8b8177"
//! ```
//!
//! Call [`load_user_themes_dir`] at startup (and after config reload) so
//! `/theme` and `[ui] theme` can select them via [`Theme::apply_custom`].

use std::collections::HashMap;
use std::path::Path;
use std::sync::{LazyLock, Mutex};

use ratatui::style::Color;

use super::syntax_palette::SyntaxPalette;
use super::{Theme, ThemeKind};

#[derive(Clone, Copy)]
struct CustomEntry {
    theme: Theme,
    syntax: SyntaxPalette,
}

static CUSTOM_THEMES: LazyLock<Mutex<HashMap<String, CustomEntry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static ACTIVE_CUSTOM: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));
/// Interned names for `SettingValue::Enum(&'static str)` persistence.
static INTERNED_NAMES: LazyLock<Mutex<HashMap<String, &'static str>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn normalize(name: &str) -> String {
    name.trim().to_ascii_lowercase()
}

/// Intern a custom theme name for the life of the process (settings Enum).
pub fn intern_name(name: &str) -> &'static str {
    let key = normalize(name);
    let mut map = INTERNED_NAMES.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(s) = map.get(&key) {
        return s;
    }
    let leaked: &'static str = Box::leak(key.clone().into_boxed_str());
    map.insert(key, leaked);
    leaked
}

/// Register or replace a user theme under `name` (case-insensitive).
/// Syntax palette is derived from the UI theme.
pub fn register_custom_theme(name: impl Into<String>, theme: Theme) {
    let syntax = SyntaxPalette::from_ui_theme(&theme);
    register_custom_theme_with_syntax(name, theme, syntax);
}

/// Register a user theme with an explicit syntax palette.
pub fn register_custom_theme_with_syntax(
    name: impl Into<String>,
    theme: Theme,
    syntax: SyntaxPalette,
) {
    let key = normalize(&name.into());
    if key.is_empty() {
        return;
    }
    let _ = intern_name(&key);
    if let Ok(mut map) = CUSTOM_THEMES.lock() {
        map.insert(key.clone(), CustomEntry { theme, syntax });
    }
    // Drop any cached Syntect built for this name (colors may have changed).
    crate::syntax::invalidate_palette_cache_key(&format!("c:{key}"));
}

/// Whether a custom theme with this name is registered.
pub fn is_registered(name: &str) -> bool {
    let key = normalize(name);
    CUSTOM_THEMES
        .lock()
        .map(|m| m.contains_key(&key))
        .unwrap_or(false)
}

/// Names of all registered custom themes (sorted).
pub fn list_registered_names() -> Vec<String> {
    let mut names: Vec<String> = CUSTOM_THEMES
        .lock()
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default();
    names.sort();
    names
}

/// Activate a registered custom theme. Returns `false` if unknown.
/// Does not touch disk; mirrors [`Theme::apply_kind`] for builtins.
pub fn apply_custom_theme(name: &str) -> bool {
    let key = normalize(name);
    let ok = CUSTOM_THEMES
        .lock()
        .map(|m| m.contains_key(&key))
        .unwrap_or(false);
    if !ok {
        return false;
    }
    if let Ok(mut active) = ACTIVE_CUSTOM.lock() {
        *active = Some(key);
    }
    true
}

/// Clear custom activation so builtin [`ThemeKind`] applies again.
pub fn clear_custom_theme() {
    if let Ok(mut active) = ACTIVE_CUSTOM.lock() {
        *active = None;
    }
}

/// Active custom theme name, if any.
pub fn active_custom_name() -> Option<String> {
    ACTIVE_CUSTOM.lock().ok().and_then(|g| g.clone())
}

/// Lookup a registered custom theme by name.
pub fn get_custom_theme(name: &str) -> Option<Theme> {
    let key = normalize(name);
    CUSTOM_THEMES
        .lock()
        .ok()
        .and_then(|m| m.get(&key).map(|e| e.theme))
}

/// Syntax palette for a registered custom theme.
pub fn get_custom_syntax(name: &str) -> Option<SyntaxPalette> {
    let key = normalize(name);
    CUSTOM_THEMES
        .lock()
        .ok()
        .and_then(|m| m.get(&key).map(|e| e.syntax))
}

/// Theme for the currently active custom selection.
pub fn active_custom_theme() -> Option<Theme> {
    let name = active_custom_name()?;
    get_custom_theme(&name)
}

/// Syntax palette for the currently active custom selection.
pub fn active_custom_syntax() -> Option<SyntaxPalette> {
    let name = active_custom_name()?;
    get_custom_syntax(&name)
}

/// Default user themes directory: `$GROK_HOME/themes` (typically `~/.grok/themes`).
pub fn default_user_themes_dir() -> Option<std::path::PathBuf> {
    Some(xai_grok_config::grok_home().join("themes"))
}

/// Load all `*.toml` theme files from `dir`. Returns how many were registered.
///
/// Each file's stem is the theme name. Invalid files are skipped with a warn.
pub fn load_user_themes_dir(dir: &Path) -> usize {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            tracing::debug!(
                path = %dir.display(),
                error = %e,
                "user themes dir not readable; skip load"
            );
            return 0;
        }
    };
    let mut n = 0usize;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("toml") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        match load_theme_file(&path) {
            Ok((theme, syntax)) => {
                register_custom_theme_with_syntax(stem, theme, syntax);
                n += 1;
            }
            Err(e) => {
                tracing::warn!(
                    path = %path.display(),
                    error = %e,
                    "failed to load user theme"
                );
            }
        }
    }
    if n > 0 {
        tracing::info!(count = n, path = %dir.display(), "loaded user themes");
    }
    n
}

/// Load themes from the default `~/.grok/themes` directory if present.
pub fn load_default_user_themes() -> usize {
    match default_user_themes_dir() {
        Some(dir) if dir.is_dir() => load_user_themes_dir(&dir),
        _ => 0,
    }
}

fn load_theme_file(path: &Path) -> Result<(Theme, SyntaxPalette), String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let table: toml::Table =
        toml::from_str(&text).map_err(|e: toml::de::Error| e.to_string())?;
    let base_name = table
        .get("base")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing required `base` (builtin theme name)".to_string())?;
    let kind = ThemeKind::from_name(base_name)
        .filter(|k| !k.is_auto())
        .ok_or_else(|| format!("unknown base theme: {base_name}"))?;
    let mut theme = theme_from_kind(kind);
    apply_hex_overrides_table(&mut theme, &table)?;
    let mut syntax = SyntaxPalette::from_ui_theme(&theme);
    if let Some(syn_table) = table.get("syntax").and_then(|v| v.as_table()) {
        apply_syntax_overrides(&mut syntax, syn_table)?;
    }
    Ok((theme, syntax))
}

fn theme_from_kind(kind: ThemeKind) -> Theme {
    match kind {
        ThemeKind::GrokNight => Theme::groknight(),
        ThemeKind::GrokDay => Theme::grokday(),
        ThemeKind::TokyoNight => Theme::tokyonight(),
        ThemeKind::RosePineMoon => Theme::rosepine_moon(),
        ThemeKind::OscuraMidnight => Theme::oscura_midnight(),
        ThemeKind::Auto => Theme::groknight(),
    }
}

fn apply_hex_overrides_table(theme: &mut Theme, map: &toml::Table) -> Result<(), String> {
    for (key, val) in map {
        if key == "base" || key == "syntax" {
            continue;
        }
        // Nested tables other than [syntax] are rejected.
        if val.as_table().is_some() {
            return Err(format!("unknown table section: [{key}]"));
        }
        let Some(hex) = val.as_str() else {
            continue;
        };
        let color = parse_hex_color(hex)
            .ok_or_else(|| format!("invalid color for {key}: {hex}"))?;
        set_theme_color_field(theme, key, color).map_err(|e| format!("{key}: {e}"))?;
    }
    Ok(())
}

fn apply_syntax_overrides(palette: &mut SyntaxPalette, map: &toml::Table) -> Result<(), String> {
    for (key, val) in map {
        let Some(hex) = val.as_str() else {
            continue;
        };
        let color = parse_hex_color(hex)
            .ok_or_else(|| format!("invalid syntax color for {key}: {hex}"))?;
        set_syntax_field(palette, key, color).map_err(|e| format!("syntax.{key}: {e}"))?;
    }
    Ok(())
}

fn parse_hex_color(s: &str) -> Option<Color> {
    let s = s.trim().strip_prefix('#').unwrap_or(s.trim());
    match s.len() {
        3 => {
            let r = u8::from_str_radix(&s[0..1].repeat(2), 16).ok()?;
            let g = u8::from_str_radix(&s[1..2].repeat(2), 16).ok()?;
            let b = u8::from_str_radix(&s[2..3].repeat(2), 16).ok()?;
            Some(Color::Rgb(r, g, b))
        }
        6 => {
            let r = u8::from_str_radix(&s[0..2], 16).ok()?;
            let g = u8::from_str_radix(&s[2..4], 16).ok()?;
            let b = u8::from_str_radix(&s[4..6], 16).ok()?;
            Some(Color::Rgb(r, g, b))
        }
        _ => None,
    }
}

fn set_syntax_field(p: &mut SyntaxPalette, key: &str, color: Color) -> Result<(), String> {
    // Accept short names and OpenCode-style syntax* aliases.
    match key {
        "default" | "foreground" | "markdownCodeBlock" | "syntaxDefault" => p.default = color,
        "comment" | "syntaxComment" => p.comment = color,
        "keyword" | "syntaxKeyword" => p.keyword = color,
        "string" | "syntaxString" => p.string = color,
        "number" | "syntaxNumber" => p.number = color,
        "function" | "syntaxFunction" => p.function = color,
        "type" | "type_name" | "syntaxType" => p.type_name = color,
        "variable" | "syntaxVariable" => p.variable = color,
        "operator" | "syntaxOperator" => p.operator = color,
        "punctuation" | "syntaxPunctuation" => p.punctuation = color,
        "constant" | "syntaxConstant" | "syntaxPrimitive" => p.constant = color,
        "property" | "syntaxProperty" => p.property = color,
        other => return Err(format!("unknown syntax field: {other}")),
    }
    Ok(())
}

fn set_theme_color_field(theme: &mut Theme, key: &str, color: Color) -> Result<(), String> {
    match key {
        // Backgrounds
        "bg_base" => theme.bg_base = color,
        "bg_light" => theme.bg_light = color,
        "bg_dark" => theme.bg_dark = color,
        "bg_highlight" => theme.bg_highlight = color,
        "bg_hover" => theme.bg_hover = color,
        "bg_terminal" => theme.bg_terminal = color,
        "bg_visual" => theme.bg_visual = color,
        // Accents
        "accent_user" => theme.accent_user = color,
        "accent_assistant" => theme.accent_assistant = color,
        "accent_thinking" => theme.accent_thinking = color,
        "accent_tool" => theme.accent_tool = color,
        "accent_system" => theme.accent_system = color,
        "accent_error" => theme.accent_error = color,
        "accent_success" => theme.accent_success = color,
        "accent_running" => theme.accent_running = color,
        "accent_skill" => theme.accent_skill = color,
        "accent_plan" => theme.accent_plan = color,
        "accent_verify" => theme.accent_verify = color,
        // Upstream removed accent_feedback; accept legacy TOML keys as no-op.
        "accent_feedback" => (),
        "accent_remember" => theme.accent_remember = color,
        "accent_model" => theme.accent_model = color,
        // Text / gray
        "text_primary" => theme.text_primary = color,
        "text_secondary" => theme.text_secondary = color,
        "gray_dim" => theme.gray_dim = color,
        "gray" => theme.gray = color,
        "gray_bright" => theme.gray_bright = color,
        // Semantic
        "command" => theme.command = color,
        "path" => theme.path = color,
        "running" => theme.running = color,
        "warning" => theme.warning = color,
        "fuzzy_accent" => theme.fuzzy_accent = color,
        // Borders
        "selection_border" => theme.selection_border = color,
        "hover_border" => theme.hover_border = color,
        "prompt_border" => theme.prompt_border = color,
        "prompt_border_active" => theme.prompt_border_active = color,
        // Scrollbar
        "scrollbar_bg" => theme.scrollbar_bg = color,
        "scrollbar_fg" => theme.scrollbar_fg = color,
        // Diff
        "diff_delete_bg" => theme.diff_delete_bg = color,
        "diff_delete_fg" => theme.diff_delete_fg = color,
        "diff_insert_bg" => theme.diff_insert_bg = color,
        "diff_insert_fg" => theme.diff_insert_fg = color,
        "diff_equal_fg" => theme.diff_equal_fg = color,
        "diff_gutter_fg" => theme.diff_gutter_fg = color,
        // Paste
        "paste_bg" => theme.paste_bg = color,
        "paste_fg" => theme.paste_fg = color,
        "paste_dim" => theme.paste_dim = color,
        // Markdown structure colors (OpenCode markdown* parity)
        "md_heading_h1" => theme.md_heading_h1 = color,
        // OpenCode has a single markdownHeading token — paint all levels.
        "markdownHeading" => {
            theme.md_heading_h1 = color;
            theme.md_heading_h2 = color;
            theme.md_heading_h3 = color;
            theme.md_heading_h4 = color;
            theme.md_heading_h5 = color;
            theme.md_heading_h6 = color;
        }
        "md_heading_h2" => theme.md_heading_h2 = color,
        "md_heading_h3" => theme.md_heading_h3 = color,
        "md_heading_h4" => theme.md_heading_h4 = color,
        "md_heading_h5" => theme.md_heading_h5 = color,
        "md_heading_h6" => theme.md_heading_h6 = color,
        "md_code" | "markdownCode" => theme.md_code = color,
        "md_task_checked" => theme.md_task_checked = color,
        "md_task_unchecked" => theme.md_task_unchecked = color,
        "md_muted" | "markdownBlockQuote" => theme.md_muted = color,
        "md_code_bg" => theme.md_code_bg = color,
        "md_text" | "markdownText" => theme.md_text = color,
        "link_fg" | "markdownLink" => theme.link_fg = color,
        other => return Err(format!("unknown color field: {other}")),
    }
    Ok(())
}

/// Test/helpers: drop all custom themes and clear activation.
#[cfg(any(test, feature = "test-support"))]
pub fn reset_for_tests() {
    if let Ok(mut m) = CUSTOM_THEMES.lock() {
        m.clear();
    }
    clear_custom_theme();
    crate::syntax::invalidate_palette_cache();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;

    #[test]
    fn register_apply_and_restore_builtin_path() {
        reset_for_tests();
        let base = Theme::groknight();
        let mut custom = base;
        custom.accent_error = Color::Rgb(1, 2, 3);
        register_custom_theme("user-mint", custom);
        assert!(is_registered("user-mint"));
        assert!(apply_custom_theme("user-mint"));
        assert_eq!(active_custom_name().as_deref(), Some("user-mint"));
        let got = active_custom_theme().expect("active");
        assert_eq!(got.accent_error, Color::Rgb(1, 2, 3));
        clear_custom_theme();
        assert!(active_custom_name().is_none());
        assert!(!apply_custom_theme("missing-theme"));
        reset_for_tests();
    }

    #[test]
    fn load_theme_file_from_base_and_hex() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ocean.toml");
        std::fs::write(
            &path,
            "base = \"tokyonight\"\naccent_error = \"#010203\"\nmd_heading_h1 = \"#0a0b0c\"\n\n[syntax]\nkeyword = \"#112233\"\n",
        )
        .unwrap();
        let (theme, syntax) = load_theme_file(&path).expect("parse");
        assert_eq!(theme.accent_error, Color::Rgb(1, 2, 3));
        assert_eq!(theme.md_heading_h1, Color::Rgb(10, 11, 12));
        assert_eq!(syntax.keyword, Color::Rgb(0x11, 0x22, 0x33));
        let n = load_user_themes_dir(dir.path());
        assert_eq!(n, 1);
        assert!(is_registered("ocean"));
        assert!(apply_custom_theme("ocean"));
        assert_eq!(
            active_custom_theme().unwrap().accent_error,
            Color::Rgb(1, 2, 3)
        );
        assert_eq!(
            active_custom_syntax().unwrap().keyword,
            Color::Rgb(0x11, 0x22, 0x33)
        );
        reset_for_tests();
    }

    #[test]
    fn intern_name_stable() {
        let a = intern_name("My-Theme");
        let b = intern_name("my-theme");
        assert_eq!(a, b);
        assert_eq!(a, "my-theme");
    }
}
