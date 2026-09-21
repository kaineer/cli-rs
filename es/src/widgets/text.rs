use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use super::{input_cursor_style, input_style, label_style};

/// Single-line text input with collapsed / editing modes.
#[derive(Debug, Clone)]
pub struct Text {
    pub label: String,
    pub value: String,
    pub cursor: usize,
    pub editing: bool,
}

impl Text {
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        let value = value.into();
        let cursor = value.chars().count();
        Self {
            label: label.into(),
            value,
            cursor,
            editing: false,
        }
    }

    pub fn height(&self) -> u16 {
        if self.editing {
            2
        } else {
            1
        }
    }

    pub fn is_editing(&self) -> bool {
        self.editing
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        if !self.editing {
            if matches!(key.code, KeyCode::Enter) {
                self.editing = true;
                self.cursor = self.value.chars().count();
                return true;
            }
            return false;
        }
        if matches!(key.code, KeyCode::Enter | KeyCode::Esc) {
            self.editing = false;
            return true;
        }
        handle_line_edit(&mut self.value, &mut self.cursor, key)
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect, focused: bool) {
        if !focused {
            self.editing = false;
        }
        if self.editing {
            render_editing_line(f, area, &self.label, &self.value, self.cursor, focused, false);
        } else {
            render_collapsed(f, area, &self.label, &self.value, focused);
        }
    }
}

pub(super) fn handle_line_edit(value: &mut String, cursor: &mut usize, key: KeyEvent) -> bool {
    let len = value.chars().count();
    match (key.code, key.modifiers) {
        (KeyCode::Left, _) => {
            *cursor = cursor.saturating_sub(1);
            true
        }
        (KeyCode::Right, _) => {
            if *cursor < len {
                *cursor += 1;
            }
            true
        }
        (KeyCode::Home, _) => {
            *cursor = 0;
            true
        }
        (KeyCode::End, _) => {
            *cursor = len;
            true
        }
        (KeyCode::Backspace, _) => {
            if *cursor > 0 {
                let before: String = value.chars().take(*cursor - 1).collect();
                let after: String = value.chars().skip(*cursor).collect();
                *value = before + &after;
                *cursor -= 1;
            }
            true
        }
        (KeyCode::Delete, _) => {
            if *cursor < len {
                let before: String = value.chars().take(*cursor).collect();
                let after: String = value.chars().skip(*cursor + 1).collect();
                *value = before + &after;
            }
            true
        }
        (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
            let before: String = value.chars().take(*cursor).collect();
            let after: String = value.chars().skip(*cursor).collect();
            *value = format!("{before}{c}{after}");
            *cursor += 1;
            true
        }
        _ => false,
    }
}

/// Inner content width: client area minus one space on each side.
pub(super) fn inner_width(area: Rect) -> usize {
    area.width.saturating_sub(2) as usize
}

fn scroll_start(len: usize, cursor: usize, width: usize) -> usize {
    if width == 0 {
        return 0;
    }
    if cursor < width {
        0
    } else {
        cursor + 1 - width
    }
    .min(len.saturating_sub(width.min(len)))
}

/// Pad / truncate `s` to exactly `width` characters.
pub(super) fn fit_width(s: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let mut out: String = s.chars().take(width).collect();
    let n = out.chars().count();
    if n < width {
        out.push_str(&" ".repeat(width - n));
    }
    out
}

/// Collapsed: ` label: preview…` on one line to the right edge.
pub(super) fn render_collapsed(
    f: &mut Frame,
    area: Rect,
    label: &str,
    preview: &str,
    focused: bool,
) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let prefix = format!(" {label}: ");
    let prefix_len = prefix.chars().count();
    let avail = (area.width as usize).saturating_sub(prefix_len);
    let shown: String = if avail == 0 {
        String::new()
    } else {
        let chars: Vec<char> = preview.chars().collect();
        if chars.len() <= avail {
            chars.into_iter().collect()
        } else if avail <= 1 {
            "…".to_string()
        } else {
            let take = avail - 1;
            let mut s: String = chars.into_iter().take(take).collect();
            s.push('…');
            s
        }
    };
    let line = Line::from(vec![
        Span::styled(prefix, label_style(focused)),
        Span::styled(shown, input_style(false)),
    ]);
    f.render_widget(Paragraph::new(line), Rect { height: 1, ..area });
}

/// Editing: label with colon on first row, full-width input on the second.
pub(super) fn render_editing_line(
    f: &mut Frame,
    area: Rect,
    label: &str,
    value: &str,
    cursor: usize,
    focused: bool,
    mask: bool,
) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let label_line = Line::from(Span::styled(
        format!(" {label}:"),
        label_style(focused),
    ));
    f.render_widget(Paragraph::new(label_line), Rect { height: 1, ..area });

    if area.height < 2 {
        return;
    }
    let value_area = Rect {
        y: area.y + 1,
        height: 1,
        ..area
    };

    let display: String = if mask {
        "•".repeat(value.chars().count())
    } else {
        value.to_string()
    };

    let width = inner_width(area);
    let style = input_style(focused);
    let line = if width == 0 {
        Line::from("")
    } else if focused {
        let chars: Vec<char> = display.chars().collect();
        let start = scroll_start(chars.len(), cursor, width);

        let mut spans = vec![Span::raw(" ")];
        let rel_cursor = cursor.saturating_sub(start).min(width.saturating_sub(1));

        let before: String = chars.iter().skip(start).take(rel_cursor).collect();
        spans.push(Span::styled(before, style));

        let at = chars.get(cursor).copied().unwrap_or(' ');
        spans.push(Span::styled(at.to_string(), input_cursor_style()));

        let after_start = start + rel_cursor + 1;
        let after_take = width.saturating_sub(rel_cursor + 1);
        let after_chars: String = chars.iter().skip(after_start).take(after_take).collect();
        let after_len = after_chars.chars().count();
        let pad = width.saturating_sub(rel_cursor + 1 + after_len);
        let after = format!("{after_chars}{}", " ".repeat(pad));
        spans.push(Span::styled(after, style));
        spans.push(Span::raw(" "));

        Line::from(spans)
    } else {
        let fitted = fit_width(&display, width);
        Line::from(vec![
            Span::raw(" "),
            Span::styled(fitted, style),
            Span::raw(" "),
        ])
    };
    f.render_widget(Paragraph::new(line), value_area);
}
