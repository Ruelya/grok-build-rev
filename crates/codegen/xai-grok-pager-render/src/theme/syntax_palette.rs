//! Semantic syntax-highlight palette (OpenCode-level roles).
//!
//! Syntect still **tokenizes** (scope stack); **colors** come from this palette
//! so custom themes can match OpenCode `syntax*` tokens after rewrite.

use std::str::FromStr;

use ratatui::style::Color;
use syntect::highlighting::{
    Color as SynColor, FontStyle, ScopeSelectors, StyleModifier, Theme as SyntectTheme,
    ThemeItem, ThemeSettings,
};

use super::Theme;

/// OpenCode-aligned syntax roles (semantic parity, not full TextMate scopes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyntaxPalette {
    /// Default code foreground (`markdownCodeBlock` / body in fences).
    pub default: Color,
    pub comment: Color,
    pub keyword: Color,
    pub string: Color,
    pub number: Color,
    pub function: Color,
    /// Maps from OpenCode `syntaxType`.
    pub type_name: Color,
    pub variable: Color,
    pub operator: Color,
    pub punctuation: Color,
    pub constant: Color,
    pub property: Color,
}

impl SyntaxPalette {
    /// Derive a usable palette from UI / markdown theme colors (fallback).
    pub fn from_ui_theme(t: &Theme) -> Self {
        Self {
            default: t.md_text,
            comment: t.md_muted,
            keyword: t.command,
            string: t.accent_success,
            number: t.warning,
            function: t.accent_tool,
            type_name: t.accent_assistant,
            variable: t.accent_thinking,
            operator: t.gray_bright,
            punctuation: t.gray,
            constant: t.accent_skill,
            property: t.path,
        }
    }

    /// Builtin theme-kind defaults approximating the bundled `.tmTheme` look.
    pub fn for_kind(kind: super::ThemeKind) -> Self {
        match kind {
            super::ThemeKind::GrokDay => Self::from_ui_theme(&Theme::grokday()),
            super::ThemeKind::TokyoNight => Self::from_ui_theme(&Theme::tokyonight()),
            super::ThemeKind::RosePineMoon => Self::from_ui_theme(&Theme::rosepine_moon()),
            super::ThemeKind::OscuraMidnight => Self::from_ui_theme(&Theme::oscura_midnight()),
            super::ThemeKind::GrokNight | super::ThemeKind::Auto => {
                Self::from_ui_theme(&Theme::groknight())
            }
        }
    }

    /// Build a syntect [`Theme`] whose scope colors come only from this palette.
    pub fn to_syntect_theme(self) -> SyntectTheme {
        let fg = |c: Color| StyleModifier {
            foreground: Some(to_syn_color(c)),
            background: None,
            font_style: None,
        };
        let fg_italic = |c: Color| StyleModifier {
            foreground: Some(to_syn_color(c)),
            background: None,
            font_style: Some(FontStyle::ITALIC),
        };

        // More specific scopes first; broad scopes last.
        let rules: &[(&str, StyleModifier)] = &[
            (
                "comment, punctuation.definition.comment, string.quoted.docstring",
                fg_italic(self.comment),
            ),
            (
                "string, string.quoted, string.template, punctuation.definition.string",
                fg(self.string),
            ),
            (
                "constant.numeric, constant.language.boolean, constant.language.null, constant.language",
                fg(self.number),
            ),
            (
                "constant.character, constant.other, entity.name.constant, variable.other.constant",
                fg(self.constant),
            ),
            (
                "entity.name.function, support.function, meta.function-call entity.name.function",
                fg(self.function),
            ),
            (
                "entity.name.type, entity.name.class, entity.name.struct, entity.name.enum, entity.name.interface, support.type, support.class",
                fg(self.type_name),
            ),
            (
                "variable, variable.other, variable.language, variable.parameter, meta.definition.variable",
                fg(self.variable),
            ),
            (
                "entity.name.tag, entity.other.attribute-name, support.type.property-name, meta.object-literal.key, variable.other.property",
                fg(self.property),
            ),
            (
                "keyword.operator, keyword.operator.assignment, keyword.operator.comparison, keyword.operator.logical, keyword.operator.arithmetic",
                fg(self.operator),
            ),
            // Include storage.type so Rust/JS `fn`/`let`/`const` land on keyword
            // (OpenCode syntaxKeyword), not type.
            (
                "keyword, keyword.control, keyword.declaration, keyword.other, storage, storage.type, storage.modifier",
                fg(self.keyword),
            ),
            (
                "punctuation, punctuation.separator, punctuation.terminator, punctuation.accessor, meta.brace",
                fg(self.punctuation),
            ),
        ];

        let mut scopes = Vec::with_capacity(rules.len());
        for (selector, style) in rules {
            let scope = ScopeSelectors::from_str(selector).unwrap_or_default();
            scopes.push(ThemeItem { scope, style: *style });
        }

        SyntectTheme {
            name: Some("grok-semantic-palette".into()),
            author: None,
            settings: ThemeSettings {
                foreground: Some(to_syn_color(self.default)),
                background: None,
                caret: Some(to_syn_color(self.default)),
                line_highlight: None,
                selection: None,
                ..ThemeSettings::default()
            },
            scopes,
        }
    }

    /// Quantize every role color to a terminal level.
    pub fn quantized(self, level: super::color_support::ColorLevel) -> Self {
        use super::color_support::quantize_color;
        let q = |c: Color| quantize_color(c, level);
        Self {
            default: q(self.default),
            comment: q(self.comment),
            keyword: q(self.keyword),
            string: q(self.string),
            number: q(self.number),
            function: q(self.function),
            type_name: q(self.type_name),
            variable: q(self.variable),
            operator: q(self.operator),
            punctuation: q(self.punctuation),
            constant: q(self.constant),
            property: q(self.property),
        }
    }
}

fn to_syn_color(c: Color) -> SynColor {
    match c {
        Color::Rgb(r, g, b) => SynColor { r, g, b, a: 0xFF },
        Color::Reset => SynColor {
            r: 0xc8,
            g: 0xc8,
            b: 0xc8,
            a: 0xFF,
        },
        Color::Black => SynColor {
            r: 0,
            g: 0,
            b: 0,
            a: 0xFF,
        },
        Color::Red => SynColor {
            r: 205,
            g: 49,
            b: 49,
            a: 0xFF,
        },
        Color::Green => SynColor {
            r: 13,
            g: 188,
            b: 121,
            a: 0xFF,
        },
        Color::Yellow => SynColor {
            r: 229,
            g: 229,
            b: 16,
            a: 0xFF,
        },
        Color::Blue => SynColor {
            r: 36,
            g: 114,
            b: 200,
            a: 0xFF,
        },
        Color::Magenta => SynColor {
            r: 188,
            g: 63,
            b: 188,
            a: 0xFF,
        },
        Color::Cyan => SynColor {
            r: 17,
            g: 168,
            b: 205,
            a: 0xFF,
        },
        Color::Gray => SynColor {
            r: 229,
            g: 229,
            b: 229,
            a: 0xFF,
        },
        Color::DarkGray => SynColor {
            r: 102,
            g: 102,
            b: 102,
            a: 0xFF,
        },
        Color::LightRed => SynColor {
            r: 241,
            g: 76,
            b: 76,
            a: 0xFF,
        },
        Color::LightGreen => SynColor {
            r: 35,
            g: 209,
            b: 139,
            a: 0xFF,
        },
        Color::LightYellow => SynColor {
            r: 245,
            g: 245,
            b: 67,
            a: 0xFF,
        },
        Color::LightBlue => SynColor {
            r: 59,
            g: 142,
            b: 234,
            a: 0xFF,
        },
        Color::LightMagenta => SynColor {
            r: 214,
            g: 112,
            b: 214,
            a: 0xFF,
        },
        Color::LightCyan => SynColor {
            r: 41,
            g: 184,
            b: 219,
            a: 0xFF,
        },
        Color::White => SynColor {
            r: 229,
            g: 229,
            b: 229,
            a: 0xFF,
        },
        Color::Indexed(i) => {
            // Coarse fallback: treat as gray ramp.
            let v = i.saturating_mul(8);
            SynColor {
                r: v,
                g: v,
                b: v,
                a: 0xFF,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_builds_nonempty_syntect_theme() {
        let p = SyntaxPalette::from_ui_theme(&Theme::groknight());
        let t = p.to_syntect_theme();
        assert!(t.scopes.len() >= 8);
        assert!(t.settings.foreground.is_some());
    }

    #[test]
    fn role_colors_roundtrip_rgb() {
        let p = SyntaxPalette {
            default: Color::Rgb(1, 2, 3),
            comment: Color::Rgb(4, 5, 6),
            keyword: Color::Rgb(7, 8, 9),
            string: Color::Rgb(10, 11, 12),
            number: Color::Rgb(13, 14, 15),
            function: Color::Rgb(16, 17, 18),
            type_name: Color::Rgb(19, 20, 21),
            variable: Color::Rgb(22, 23, 24),
            operator: Color::Rgb(25, 26, 27),
            punctuation: Color::Rgb(28, 29, 30),
            constant: Color::Rgb(31, 32, 33),
            property: Color::Rgb(34, 35, 36),
        };
        let t = p.to_syntect_theme();
        let fg = t.settings.foreground.unwrap();
        assert_eq!((fg.r, fg.g, fg.b), (1, 2, 3));
    }
}
