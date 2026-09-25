//! Нормализация вывода процесса для TUI и разбор SGR в spans.

use ratatui::{
    style::{Color, Modifier, Style},
    text::Span,
};

/// Нормализует вывод процесса для безопасного показа в TUI.
///
/// - `\r` сбрасывает текущую строку (всё после последнего `\n`).
/// - CSI EL (`…K`) тоже сбрасывает текущую строку.
/// - CSI SGR (`…m`) сохраняются (цвета/стили).
/// - Остальные CSI, OSC и прочие ESC отбрасываются.
/// - Прочие control (кроме `\t` и `\n`) отбрасываются.
pub fn sanitize_output(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut line_start = 0usize;
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                out.truncate(line_start);
            }
            '\n' => {
                out.push('\n');
                line_start = out.len();
            }
            '\t' => {
                out.push('\t');
            }
            '\x1b' => {
                handle_escape(&mut out, &mut line_start, &mut chars);
            }
            c if c.is_control() => {}
            c => {
                out.push(c);
            }
        }
    }

    out
}

/// Разбирает одну строку (без `\n`) с SGR в spans; добавляет префикс `"  "`.
pub fn ansi_line_to_spans(line: &str) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    spans.push(Span::raw("  "));

    let mut style = Style::default();
    let mut text = String::new();
    let mut chars = line.chars().peekable();

    let flush = |text: &mut String, style: Style, spans: &mut Vec<Span<'static>>| {
        if !text.is_empty() {
            spans.push(Span::styled(std::mem::take(text), style));
        }
    };

    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.peek() == Some(&'[') {
                chars.next();
                let (params, final_byte) = read_csi(&mut chars);
                if final_byte == Some('m') {
                    flush(&mut text, style, &mut spans);
                    apply_sgr(&mut style, &params);
                }
                // non-m CSI: ignore (should already be gone after sanitize)
            } else {
                // skip other escapes defensively
                skip_non_csi_escape(&mut chars);
            }
        } else if c == '\n' || c == '\r' {
            // line should not contain these after sanitize + lines()
        } else {
            text.push(c);
        }
    }
    flush(&mut text, style, &mut spans);
    spans
}

fn handle_escape(
    out: &mut String,
    line_start: &mut usize,
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
) {
    match chars.next() {
        Some('[') => {
            let csi_start = out.len();
            out.push('\x1b');
            out.push('[');
            let mut final_byte = None;
            while let Some(&ch) = chars.peek() {
                chars.next();
                out.push(ch);
                if ('\u{40}'..='\u{7E}').contains(&ch) {
                    final_byte = Some(ch);
                    break;
                }
            }
            match final_byte {
                Some('m') => {
                    // SGR — оставляем как есть
                }
                Some('K') => {
                    // Erase in Line: в логе = очистить текущую строку
                    out.truncate(*line_start);
                }
                _ => {
                    out.truncate(csi_start);
                }
            }
        }
        Some(']') => {
            while let Some(ch) = chars.next() {
                if ch == '\x07' {
                    break;
                }
                if ch == '\x1b' {
                    let _ = chars.next();
                    break;
                }
            }
        }
        Some('(') | Some(')') | Some('%') | Some('#') => {
            let _ = chars.next();
        }
        Some(_) | None => {}
    }
}
fn skip_non_csi_escape(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    match chars.next() {
        Some(']') => {
            while let Some(ch) = chars.next() {
                if ch == '\x07' {
                    break;
                }
                if ch == '\x1b' {
                    let _ = chars.next();
                    break;
                }
            }
        }
        Some('(') | Some(')') | Some('%') | Some('#') => {
            let _ = chars.next();
        }
        Some(_) | None => {}
    }
}

/// Читает тело CSI после `ESC [`: параметры и final byte.
fn read_csi(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> (Vec<u8>, Option<char>) {
    let mut raw = String::new();
    let mut final_byte = None;
    while let Some(&ch) = chars.peek() {
        chars.next();
        if ('\u{40}'..='\u{7E}').contains(&ch) {
            final_byte = Some(ch);
            break;
        }
        raw.push(ch);
    }
    let params = parse_sgr_params(&raw);
    (params, final_byte)
}

fn parse_sgr_params(raw: &str) -> Vec<u8> {
    if raw.is_empty() {
        return vec![0];
    }
    raw.split(';')
        .map(|p| {
            if p.is_empty() {
                0
            } else {
                p.parse::<u8>().unwrap_or(0)
            }
        })
        .collect()
}

fn apply_sgr(style: &mut Style, params: &[u8]) {
    let mut i = 0;
    while i < params.len() {
        match params[i] {
            0 => *style = Style::default(),
            1 => *style = style.add_modifier(Modifier::BOLD),
            2 => *style = style.add_modifier(Modifier::DIM),
            3 => *style = style.add_modifier(Modifier::ITALIC),
            4 => *style = style.add_modifier(Modifier::UNDERLINED),
            7 => *style = style.add_modifier(Modifier::REVERSED),
            22 => {
                *style = style.remove_modifier(Modifier::BOLD | Modifier::DIM);
            }
            23 => *style = style.remove_modifier(Modifier::ITALIC),
            24 => *style = style.remove_modifier(Modifier::UNDERLINED),
            27 => *style = style.remove_modifier(Modifier::REVERSED),
            39 => *style = style.fg(Color::Reset),
            49 => *style = style.bg(Color::Reset),
            n @ 30..=37 => *style = style.fg(ansi_16_fg(n - 30)),
            n @ 40..=47 => *style = style.bg(ansi_16_fg(n - 40)),
            n @ 90..=97 => *style = style.fg(ansi_16_bright(n - 90)),
            n @ 100..=107 => *style = style.bg(ansi_16_bright(n - 100)),
            38 => {
                if let Some((color, consumed)) = parse_extended_color(params, i + 1) {
                    *style = style.fg(color);
                    i += consumed;
                }
            }
            48 => {
                if let Some((color, consumed)) = parse_extended_color(params, i + 1) {
                    *style = style.bg(color);
                    i += consumed;
                }
            }
            _ => {}
        }
        i += 1;
    }
}

/// Returns (color, number of params consumed after 38/48).
fn parse_extended_color(params: &[u8], start: usize) -> Option<(Color, usize)> {
    let mode = *params.get(start)?;
    match mode {
        5 => {
            let idx = *params.get(start + 1)?;
            Some((Color::Indexed(idx), 2))
        }
        2 => {
            let r = *params.get(start + 1)?;
            let g = *params.get(start + 2)?;
            let b = *params.get(start + 3)?;
            Some((Color::Rgb(r, g, b), 4))
        }
        _ => None,
    }
}

fn ansi_16_fg(n: u8) -> Color {
    match n {
        0 => Color::Black,
        1 => Color::Red,
        2 => Color::Green,
        3 => Color::Yellow,
        4 => Color::Blue,
        5 => Color::Magenta,
        6 => Color::Cyan,
        _ => Color::White,
    }
}

fn ansi_16_bright(n: u8) -> Color {
    match n {
        0 => Color::DarkGray,
        1 => Color::LightRed,
        2 => Color::LightGreen,
        3 => Color::LightYellow,
        4 => Color::LightBlue,
        5 => Color::LightMagenta,
        6 => Color::LightCyan,
        _ => Color::White,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cr_rewrites_current_line() {
        assert_eq!(sanitize_output("aaa\rbbb"), "bbb");
        assert_eq!(
            sanitize_output("Downloading 10%\rDownloading 100%\nDone\n"),
            "Downloading 100%\nDone\n"
        );
    }

    #[test]
    fn cr_does_not_affect_previous_lines() {
        assert_eq!(
            sanitize_output("keep\noverwrite me\rfinal\n"),
            "keep\nfinal\n"
        );
    }

    #[test]
    fn keeps_sgr_colors() {
        assert_eq!(
            sanitize_output("\x1b[31mred\x1b[0m plain"),
            "\x1b[31mred\x1b[0m plain"
        );
    }

    #[test]
    fn strips_csi_erase_line_with_cr() {
        assert_eq!(sanitize_output("old\r\x1b[Knew"), "new");
    }

    #[test]
    fn progress_cr_esc_k_stream() {
        let raw = "progress 0\r\x1b[Kprogress 1\r\x1b[Kprogress 2\r\x1b[Kdone\n";
        assert_eq!(sanitize_output(raw), "done\n");
    }

    #[test]
    fn erase_line_clears_without_cr() {
        // ESC[2K alone should clear the current line for log purposes
        assert_eq!(sanitize_output("old\x1b[2Knew"), "new");
        assert_eq!(sanitize_output("old\x1b[0Knew"), "new");
        assert_eq!(sanitize_output("old\x1b[Knew"), "new");
    }

    #[test]
    fn cursor_col1_then_overwrite_via_el() {
        assert_eq!(sanitize_output("old\x1b[2K\x1b[1Gnew"), "new");
    }


    #[test]
    fn shed_style_progress_collapse() {
        let raw = concat!(
            "[cred] ok\n",
            "[task] POST#1\n",
            "\r\x1b[K[    ] GET #1\r\x1b[K[x   ] GET #2\r\x1b[K[dl] 6/6\r\x1b[K file:///tmp/out\n",
            " |-./a.jpg\n",
        );
        let got = sanitize_output(raw);
        assert_eq!(
            got,
            "[cred] ok\n[task] POST#1\n file:///tmp/out\n |-./a.jpg\n",
            "got: {:?}",
            got
        );
    }

    #[test]
    fn strips_osc() {
        assert_eq!(sanitize_output("a\x1b]0;title\x07b"), "ab");
    }

    #[test]
    fn keeps_tab_and_newline() {
        assert_eq!(sanitize_output("a\tb\nc"), "a\tb\nc");
    }

    #[test]
    fn ansi_line_produces_styled_spans() {
        let spans = ansi_line_to_spans("\x1b[31mred\x1b[0m plain");
        assert_eq!(spans[0].content, "  ");
        assert_eq!(spans[1].content, "red");
        assert_eq!(spans[1].style.fg, Some(Color::Red));
        assert_eq!(spans[2].content, " plain");
        assert!(spans[2].style.fg.is_none() || spans[2].style.fg == Some(Color::Reset));
    }
}
