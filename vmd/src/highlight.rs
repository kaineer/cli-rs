//! Подсветка блоков кода через syntect + тему Nord из two-face.

use std::sync::OnceLock;

use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, Style, Theme};
use syntect::parsing::{SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;
use two_face::theme::EmbeddedThemeName;

/// Один кусок подсвеченного текста: байтовая длина и цвет.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightRun {
    pub len: usize,
    pub color: u32,
    pub bold: bool,
    pub italic: bool,
}

fn syntax_set() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    SET.get_or_init(two_face::syntax::extra_newlines)
}

fn nord_theme() -> &'static Theme {
    static THEME: OnceLock<Theme> = OnceLock::new();
    THEME.get_or_init(|| {
        two_face::theme::extra()
            .get(EmbeddedThemeName::Nord)
            .clone()
    })
}

fn find_syntax<'a>(set: &'a SyntaxSet, language: &str) -> &'a SyntaxReference {
    let key = language.trim();
    if key.is_empty() {
        return set.find_syntax_plain_text();
    }
    set.find_syntax_by_token(key)
        .or_else(|| set.find_syntax_by_extension(key))
        .or_else(|| set.find_syntax_by_name(key))
        .unwrap_or_else(|| set.find_syntax_plain_text())
}

fn style_color(style: Style) -> u32 {
    let c = style.foreground;
    (u32::from(c.r) << 16) | (u32::from(c.g) << 8) | u32::from(c.b)
}

/// Подсвечивает `code`. Без языка или при неизвестном — plain text (цвет темы).
pub fn highlight(code: &str, language: Option<&str>) -> Vec<HighlightRun> {
    if code.is_empty() {
        return Vec::new();
    }

    let set = syntax_set();
    let theme = nord_theme();
    let syntax = match language {
        Some(lang) => find_syntax(set, lang),
        None => set.find_syntax_plain_text(),
    };

    let mut highlighter = HighlightLines::new(syntax, theme);
    let mut runs: Vec<HighlightRun> = Vec::new();

    for line in LinesWithEndings::from(code) {
        let ranges = match highlighter.highlight_line(line, set) {
            Ok(ranges) => ranges,
            Err(_) => {
                let fallback = theme
                    .settings
                    .foreground
                    .unwrap_or(syntect::highlighting::Color::WHITE);
                push_run(
                    &mut runs,
                    line.len(),
                    (u32::from(fallback.r) << 16)
                        | (u32::from(fallback.g) << 8)
                        | u32::from(fallback.b),
                    false,
                    false,
                );
                continue;
            }
        };
        for (style, piece) in ranges {
            if piece.is_empty() {
                continue;
            }
            push_run(
                &mut runs,
                piece.len(),
                style_color(style),
                style.font_style.contains(FontStyle::BOLD),
                style.font_style.contains(FontStyle::ITALIC),
            );
        }
    }

    runs
}

fn push_run(runs: &mut Vec<HighlightRun>, len: usize, color: u32, bold: bool, italic: bool) {
    if let Some(last) = runs.last_mut() {
        if last.color == color && last.bold == bold && last.italic == italic {
            last.len += len;
            return;
        }
    }
    runs.push(HighlightRun {
        len,
        color,
        bold,
        italic,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_gets_multiple_colors() {
        let code = "fn main() {\n    let x = 1;\n}\n";
        let runs = highlight(code, Some("rust"));
        assert!(!runs.is_empty());
        let colors: std::collections::HashSet<_> = runs.iter().map(|r| r.color).collect();
        assert!(
            colors.len() > 1,
            "ожидали несколько цветов, получили {colors:?}"
        );
        let total: usize = runs.iter().map(|r| r.len).sum();
        assert_eq!(total, code.len());
    }

    #[test]
    fn unknown_language_is_plain() {
        let code = "hello world\n";
        let runs = highlight(code, Some("not-a-real-lang-xyz"));
        let total: usize = runs.iter().map(|r| r.len).sum();
        assert_eq!(total, code.len());
    }

    #[test]
    fn empty_input() {
        assert!(highlight("", Some("rust")).is_empty());
    }
}
