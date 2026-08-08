//! Syntax highlighting initialization.
//!
//! Code-block colors come from a semantic [`SyntaxPalette`] (OpenCode-level
//! roles). Syntect only tokenizes; the palette paints keyword/string/comment/…
//!
//! Builtin and custom themes share this pipeline. Custom themes may set
//! `[syntax]` in their TOML; otherwise roles are derived from UI/markdown
//! colors.
//!
//! ## Minimal / terminal-native lock
//!
//! While [`crate::theme::cache::terminal_native_locked`] is set, chrome uses
//! [`Theme::terminal_default`](crate::theme::Theme::terminal_default) and
//! token colors are remapped via [`polarity_safe_syntax_fg`].

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

pub use xai_grok_markdown::Syntect;

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;

use crate::theme::syntax_palette::SyntaxPalette;
use crate::theme::{Theme, ThemeKind, custom};

/// Convert syntect style to ratatui foreground-only style, quantized for
/// terminal color support (or polarity-safe under the terminal-native lock).
pub fn syntect_to_ratatui_fg(style: syntect::highlighting::Style) -> Style {
    let fg = syntect_rgb_to_fg(style.foreground.r, style.foreground.g, style.foreground.b);
    let mut out = Style::default().fg(fg);
    use syntect::highlighting::FontStyle;
    if style.font_style.contains(FontStyle::BOLD) {
        out = out.add_modifier(Modifier::BOLD);
    }
    if style.font_style.contains(FontStyle::ITALIC) {
        out = out.add_modifier(Modifier::ITALIC);
    }
    if style.font_style.contains(FontStyle::UNDERLINE) {
        out = out.add_modifier(Modifier::UNDERLINED);
    }
    out
}

/// Map a syntect RGB triplet to a ratatui foreground color.
///
/// Under the terminal-native lock, uses [`polarity_safe_syntax_fg`]; otherwise
/// quantizes via the normal theme color pipeline.
pub fn syntect_rgb_to_fg(r: u8, g: u8, b: u8) -> Color {
    if crate::theme::cache::terminal_native_locked() {
        polarity_safe_syntax_fg(r, g, b)
    } else {
        crate::theme::quantize(Color::Rgb(r, g, b))
    }
}

/// Dual-polarity-safe ANSI mapping for syntax tokens on a transparent canvas.
pub fn polarity_safe_syntax_fg(r: u8, g: u8, b: u8) -> Color {
    let max = r.max(g).max(b) as i32;
    let min = r.min(g).min(b) as i32;
    let chroma = max - min;
    if chroma < 40 {
        return Color::Reset;
    }
    let (ri, gi, bi) = (r as i32, g as i32, b as i32);
    let h = if max == ri {
        let mut h = (gi - bi) * 60 / chroma;
        if h < 0 {
            h += 360;
        }
        h
    } else if max == gi {
        (bi - ri) * 60 / chroma + 120
    } else {
        (ri - gi) * 60 / chroma + 240
    };
    match h {
        0..30 | 330..=360 => Color::Red,
        30..90 => Color::Yellow,
        90..150 => Color::Green,
        150..210 => Color::Cyan,
        210..255 => Color::Blue,
        _ => Color::Magenta,
    }
}

/// Highlight a single line of source, falling back to plain text style.
pub fn highlight_line(
    text: &str,
    highlighter: &mut Option<syntect::easy::HighlightLines<'_>>,
    syntect: &Syntect,
    fallback: Style,
) -> Vec<Span<'static>> {
    if let Some(hl) = highlighter.as_mut()
        && let Ok(ranges) = hl.highlight_line(&format!("{text}\n"), &syntect.syntax_set)
    {
        let mut spans = Vec::new();
        for (style, segment) in ranges {
            let mut s = segment.to_owned();
            while s.ends_with('\n') || s.ends_with('\r') {
                s.pop();
            }
            if s.is_empty() {
                continue;
            }
            spans.push(Span::styled(s, syntect_to_ratatui_fg(style)));
        }
        if !spans.is_empty() {
            return spans;
        }
    }
    vec![Span::styled(text.to_string(), fallback)]
}

// ── Palette-driven Syntect cache ──────────────────────────────────────────

static SYNTECT_CACHE: OnceLock<Mutex<HashMap<String, &'static Syntect>>> = OnceLock::new();

fn syntect_cache() -> &'static Mutex<HashMap<String, &'static Syntect>> {
    SYNTECT_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Drop all palette-built Syntect instances (e.g. after theme test reset).
pub fn invalidate_palette_cache() {
    if let Ok(mut g) = syntect_cache().lock() {
        g.clear();
    }
}

/// Drop one cache entry (e.g. after re-registering a custom theme).
pub fn invalidate_palette_cache_key(key: &str) {
    if let Ok(mut g) = syntect_cache().lock() {
        g.remove(key);
    }
}

fn current_palette_key_and_palette() -> (String, SyntaxPalette) {
    if let Some(name) = custom::active_custom_name() {
        let key = format!("c:{name}");
        let palette = custom::active_custom_syntax().unwrap_or_else(|| {
            custom::active_custom_theme()
                .map(|t| SyntaxPalette::from_ui_theme(&t))
                .unwrap_or_else(|| SyntaxPalette::for_kind(ThemeKind::GrokNight))
        });
        return (key, palette);
    }
    let kind = Theme::current_kind();
    let key = format!("k:{}", kind.display_name());
    (key, SyntaxPalette::for_kind(kind))
}

/// Returns the syntect instance matching the active theme's syntax palette.
///
/// Builtin and custom themes use the same palette → syntect theme pipeline.
/// Under the terminal-native lock, token colors are still remapped in
/// [`syntect_to_ratatui_fg`].
pub fn get_syntect() -> &'static Syntect {
    let (key, palette) = current_palette_key_and_palette();
    {
        if let Ok(g) = syntect_cache().lock()
            && let Some(s) = g.get(&key)
        {
            return *s;
        }
    }
    let syn = Syntect::from_theme(palette.to_syntect_theme());
    let leaked: &'static Syntect = Box::leak(Box::new(syn));
    if let Ok(mut g) = syntect_cache().lock() {
        // Another thread may have inserted first; prefer existing to avoid
        // unbounded leaks under races (rare).
        if let Some(existing) = g.get(&key) {
            return *existing;
        }
        g.insert(key, leaked);
    }
    leaked
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::cache as theme_cache;

    /// Hold the theme test lock so we can flip the terminal-native flag.
    fn with_native_lock<R>(locked: bool, f: impl FnOnce() -> R) -> R {
        let _guard = theme_cache::test_lock()
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        theme_cache::reset_for_test();
        theme_cache::set_terminal_native_lock(locked);
        let out = f();
        theme_cache::set_terminal_native_lock(false);
        theme_cache::reset_for_test();
        out
    }

    #[test]
    fn polarity_safe_grays_are_reset() {
        assert_eq!(polarity_safe_syntax_fg(0xc8, 0xc8, 0xc8), Color::Reset);
        assert_eq!(polarity_safe_syntax_fg(0x6c, 0x6c, 0x6c), Color::Reset);
        assert_eq!(polarity_safe_syntax_fg(0xb2, 0xb2, 0xb2), Color::Reset);
        assert_eq!(polarity_safe_syntax_fg(0x44, 0x44, 0x44), Color::Reset);
    }

    #[test]
    fn polarity_safe_never_emits_white_or_black() {
        let samples = [
            (0xbb, 0x9a, 0xf7),
            (0x7d, 0xcf, 0xff),
            (0x7a, 0xa2, 0xf7),
            (0xff, 0x9e, 0x64),
            (0xf7, 0x76, 0x8e),
            (0xe0, 0xaf, 0x68),
            (0x9e, 0xce, 0x6a),
            (0xc8, 0xc8, 0xc8),
        ];
        for (r, g, b) in samples {
            let c = polarity_safe_syntax_fg(r, g, b);
            assert!(
                !matches!(
                    c,
                    Color::White
                        | Color::Black
                        | Color::Gray
                        | Color::DarkGray
                        | Color::LightRed
                        | Color::LightGreen
                        | Color::LightYellow
                        | Color::LightBlue
                        | Color::LightMagenta
                        | Color::LightCyan
                ),
                "polarity-unsafe color for #{r:02x}{g:02x}{b:02x}: {c:?}"
            );
        }
    }

    #[test]
    fn polarity_safe_chromatic_buckets() {
        assert_eq!(polarity_safe_syntax_fg(0xf7, 0x76, 0x8e), Color::Red);
        assert_eq!(polarity_safe_syntax_fg(0xe0, 0xaf, 0x68), Color::Yellow);
        assert_eq!(polarity_safe_syntax_fg(0x9e, 0xce, 0x6a), Color::Yellow);
        assert_eq!(polarity_safe_syntax_fg(0x7d, 0xcf, 0xff), Color::Cyan);
        assert_eq!(polarity_safe_syntax_fg(0x7a, 0xa2, 0xf7), Color::Blue);
        assert_eq!(polarity_safe_syntax_fg(0xbb, 0x9a, 0xf7), Color::Magenta);
    }

    #[test]
    fn syntect_rgb_to_fg_uses_polarity_safe_when_locked() {
        with_native_lock(true, || {
            assert_eq!(syntect_rgb_to_fg(0xc8, 0xc8, 0xc8), Color::Reset);
            assert_eq!(syntect_rgb_to_fg(0xbb, 0x9a, 0xf7), Color::Magenta);
        });
    }

    #[test]
    fn highlight_line_fallback_when_no_highlighter() {
        let syn = get_syntect();
        let mut hl = None;
        let fallback = Style::default().fg(Color::Reset);
        let spans = highlight_line("fn main() {}", &mut hl, syn, fallback);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content.as_ref(), "fn main() {}");
        assert_eq!(spans[0].style.fg, Some(Color::Reset));
    }

    #[test]
    fn highlight_line_under_native_lock_avoids_white_tokens() {
        with_native_lock(true, || {
            let syn = get_syntect();
            let mut hl = syn.highlight_lines_for_token("rust");
            let fallback = Style::default().fg(Color::Reset);
            let spans = highlight_line("fn main() { let x = 1; /* c */ }", &mut hl, syn, fallback);
            assert!(!spans.is_empty());
            for span in &spans {
                let fg = span.style.fg;
                assert!(
                    !matches!(fg, Some(Color::White)),
                    "token {:?} painted White under native lock",
                    span.content
                );
            }
        });
    }

    #[test]
    fn custom_syntax_palette_paints_keyword() {
        custom::reset_for_tests();
        let mut theme = Theme::groknight();
        theme.accent_error = Color::Rgb(1, 2, 3);
        let palette = SyntaxPalette {
            default: Color::Rgb(200, 200, 200),
            comment: Color::Rgb(100, 100, 100),
            keyword: Color::Rgb(0xff, 0x00, 0x11),
            string: Color::Rgb(0x00, 0xff, 0x22),
            number: Color::Rgb(0x33, 0x44, 0x55),
            function: Color::Rgb(0x66, 0x77, 0x88),
            type_name: Color::Rgb(0x99, 0xaa, 0xbb),
            variable: Color::Rgb(0xcc, 0xdd, 0xee),
            operator: Color::Rgb(0x10, 0x20, 0x30),
            punctuation: Color::Rgb(0x40, 0x50, 0x60),
            constant: Color::Rgb(0x70, 0x80, 0x90),
            property: Color::Rgb(0xa0, 0xb0, 0xc0),
        };
        custom::register_custom_theme_with_syntax("palette-test", theme, palette);
        assert!(custom::apply_custom_theme("palette-test"));
        let syn = get_syntect();
        let has_keyword_color = syn.theme.scopes.iter().any(|item| {
            item.style
                .foreground
                .is_some_and(|c| c.r == 0xff && c.g == 0x00 && c.b == 0x11)
        });
        assert!(
            has_keyword_color,
            "expected keyword #ff0011 in palette-built syntect theme"
        );
        // Smoke: highlighter accepts rust token under palette theme.
        assert!(syn.highlight_lines_for_token("rust").is_some());
        custom::reset_for_tests();
    }
}
