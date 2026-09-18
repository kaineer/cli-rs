use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use super::text::{fit_width, inner_width};
use super::{input_cursor_style, input_style, label_style};

/// Marker at the end of a soft-wrapped row (points left: line continues below).
const WRAP_MARK: char = '↩';

/// Multi-line text editor with soft-wrap.
#[derive(Debug, Clone)]
pub struct TextArea {
    pub label: String,
    pub lines: Vec<String>,
    pub row: usize,
    pub col: usize,
    pub view_height: u16,
    /// Inner width from the last render (for visual Up/Down).
    last_width: usize,
}

#[derive(Debug, Clone, Copy)]
struct VisualRow {
    log_row: usize,
    start: usize,
    len: usize,
    /// Soft-wrap continues after this row → show WRAP_MARK on the right.
    wraps: bool,
}

impl TextArea {
    pub fn new(label: impl Into<String>, text: impl Into<String>, view_height: u16) -> Self {
        let text = text.into();
        let lines: Vec<String> = if text.is_empty() {
            vec![String::new()]
        } else {
            text.lines().map(str::to_string).collect()
        };
        Self {
            label: label.into(),
            lines,
            row: 0,
            col: 0,
            view_height: view_height.max(1),
            last_width: 0,
        }
    }

    pub fn height(&self) -> u16 {
        1 + self.view_height
    }

    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        self.clamp_cursor();
        let width = self.last_width;
        match key.code {
            KeyCode::Up if width > 0 => {
                self.move_visual(-1, width);
                true
            }
            KeyCode::Down if width > 0 => {
                self.move_visual(1, width);
                true
            }
            KeyCode::Left => {
                if self.col > 0 {
                    self.col -= 1;
                } else if self.row > 0 {
                    self.row -= 1;
                    self.col = self.lines[self.row].chars().count();
                }
                true
            }
            KeyCode::Right => {
                let len = self.lines[self.row].chars().count();
                if self.col < len {
                    self.col += 1;
                } else if self.row + 1 < self.lines.len() {
                    self.row += 1;
                    self.col = 0;
                }
                true
            }
            KeyCode::Home => {
                // Start of current visual row if wrapped, else line start.
                if width > 0 {
                    let visual = build_visual(&self.lines, width);
                    let vi = visual_index_for_cursor(&visual, self.row, self.col);
                    self.col = visual[vi].start;
                } else {
                    self.col = 0;
                }
                true
            }
            KeyCode::End => {
                if width > 0 {
                    let visual = build_visual(&self.lines, width);
                    let vi = visual_index_for_cursor(&visual, self.row, self.col);
                    let vr = visual[vi];
                    let line_len = self.lines[self.row].chars().count();
                    self.col = (vr.start + vr.len).min(line_len);
                } else {
                    self.col = self.lines[self.row].chars().count();
                }
                true
            }
            KeyCode::Enter => {
                let line = &self.lines[self.row];
                let before: String = line.chars().take(self.col).collect();
                let after: String = line.chars().skip(self.col).collect();
                self.lines[self.row] = before;
                self.lines.insert(self.row + 1, after);
                self.row += 1;
                self.col = 0;
                true
            }
            KeyCode::Backspace => {
                if self.col > 0 {
                    let line = &mut self.lines[self.row];
                    let before: String = line.chars().take(self.col - 1).collect();
                    let after: String = line.chars().skip(self.col).collect();
                    *line = before + &after;
                    self.col -= 1;
                } else if self.row > 0 {
                    let cur = self.lines.remove(self.row);
                    self.row -= 1;
                    self.col = self.lines[self.row].chars().count();
                    self.lines[self.row].push_str(&cur);
                }
                true
            }
            KeyCode::Delete => {
                let len = self.lines[self.row].chars().count();
                if self.col < len {
                    let line = &mut self.lines[self.row];
                    let before: String = line.chars().take(self.col).collect();
                    let after: String = line.chars().skip(self.col + 1).collect();
                    *line = before + &after;
                } else if self.row + 1 < self.lines.len() {
                    let next = self.lines.remove(self.row + 1);
                    self.lines[self.row].push_str(&next);
                }
                true
            }
            KeyCode::Char(c)
                if key.modifiers == KeyModifiers::NONE || key.modifiers == KeyModifiers::SHIFT =>
            {
                let line = &mut self.lines[self.row];
                let before: String = line.chars().take(self.col).collect();
                let after: String = line.chars().skip(self.col).collect();
                *line = format!("{before}{c}{after}");
                self.col += 1;
                true
            }
            _ => false,
        }
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect, focused: bool) {
        if area.height == 0 || area.width == 0 {
            return;
        }
        let body = Rect {
            y: area.y + 1,
            height: area.height.saturating_sub(1).min(self.view_height),
            ..area
        };
        self.last_width = inner_width(body);
        self.render_with_width(f, area, focused, self.last_width);
    }

    fn render_with_width(&self, f: &mut Frame, area: Rect, focused: bool, width: usize) {
        if area.height == 0 || area.width == 0 {
            return;
        }
        let label = Line::from(Span::styled(
            format!(" {}", self.label),
            label_style(focused),
        ));
        f.render_widget(Paragraph::new(label), Rect { height: 1, ..area });

        let body = Rect {
            y: area.y + 1,
            height: area.height.saturating_sub(1).min(self.view_height),
            ..area
        };
        if body.height == 0 {
            return;
        }

        let style = input_style(focused);
        let visual = build_visual(&self.lines, width);
        let cursor_v = visual_index_for_cursor(&visual, self.row, self.col);
        let view_h = body.height as usize;
        let first = scroll_to_cursor(cursor_v, view_h, visual.len());

        let mut out = Vec::new();
        for vi in first..(first + view_h).min(visual.len()) {
            let vr = visual[vi];
            let content = &self.lines[vr.log_row];
            let show_cursor = focused && vi == cursor_v;
            out.push(render_visual_row(
                content,
                vr,
                width,
                style,
                show_cursor,
                self.col,
            ));
        }
        while out.len() < view_h {
            out.push(padded_line("", width, style, false));
        }
        f.render_widget(Paragraph::new(out), body);
    }

    fn move_visual(&mut self, delta: i32, width: usize) {
        let visual = build_visual(&self.lines, width);
        if visual.is_empty() {
            return;
        }
        let cur = visual_index_for_cursor(&visual, self.row, self.col);
        let next = (cur as i32 + delta).clamp(0, visual.len() as i32 - 1) as usize;
        let prev = visual[cur];
        let vr = visual[next];
        let rel = self.col.saturating_sub(prev.start);
        self.row = vr.log_row;
        let line_len = self.lines[self.row].chars().count();
        self.col = (vr.start + rel).clamp(vr.start, (vr.start + vr.len).min(line_len));
    }

    fn clamp_cursor(&mut self) {
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
        if self.row >= self.lines.len() {
            self.row = self.lines.len() - 1;
        }
        let len = self.lines[self.row].chars().count();
        if self.col > len {
            self.col = len;
        }
    }
}

fn wrap_logical_line(line: &str, width: usize) -> Vec<VisualRow> {
    if width == 0 {
        return vec![VisualRow {
            log_row: 0,
            start: 0,
            len: 0,
            wraps: false,
        }];
    }
    let chars: Vec<char> = line.chars().collect();
    if chars.is_empty() {
        return vec![VisualRow {
            log_row: 0,
            start: 0,
            len: 0,
            wraps: false,
        }];
    }

    let mut rows = Vec::new();
    let mut start = 0;
    while start < chars.len() {
        let remaining = chars.len() - start;
        if remaining <= width {
            rows.push(VisualRow {
                log_row: 0,
                start,
                len: remaining,
                wraps: false,
            });
            break;
        }
        // Reserve one cell on the right for WRAP_MARK.
        let take = if width > 1 { width - 1 } else { width };
        if take == 0 {
            break;
        }
        rows.push(VisualRow {
            log_row: 0,
            start,
            len: take,
            wraps: true,
        });
        start += take;
    }
    rows
}

fn build_visual(lines: &[String], width: usize) -> Vec<VisualRow> {
    let mut out = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        for mut seg in wrap_logical_line(line, width) {
            seg.log_row = i;
            out.push(seg);
        }
    }
    if out.is_empty() {
        out.push(VisualRow {
            log_row: 0,
            start: 0,
            len: 0,
            wraps: false,
        });
    }
    out
}

fn visual_index_for_cursor(visual: &[VisualRow], row: usize, col: usize) -> usize {
    let mut last_for_row = 0;
    for (i, vr) in visual.iter().enumerate() {
        if vr.log_row != row {
            continue;
        }
        last_for_row = i;
        let end = vr.start + vr.len;
        if col >= vr.start && col < end {
            return i;
        }
        if col == end {
            // At wrap boundary: belong to next continuation if any.
            if visual.get(i + 1).is_some_and(|n| n.log_row == row) {
                continue;
            }
            return i;
        }
    }
    last_for_row
}

fn scroll_to_cursor(cursor_v: usize, view_h: usize, total: usize) -> usize {
    if view_h == 0 || total == 0 {
        return 0;
    }
    let mut first = 0;
    if cursor_v >= view_h {
        first = cursor_v + 1 - view_h;
    }
    first.min(total.saturating_sub(view_h))
}

fn padded_line(content: &str, width: usize, style: Style, wraps: bool) -> Line<'static> {
    if width == 0 {
        return Line::from("");
    }
    if wraps && width >= 1 {
        let fitted = fit_width(content, width - 1);
        Line::from(vec![
            Span::raw(" "),
            Span::styled(fitted, style),
            Span::styled(WRAP_MARK.to_string(), style),
            Span::raw(" "),
        ])
    } else {
        let fitted = fit_width(content, width);
        Line::from(vec![
            Span::raw(" "),
            Span::styled(fitted, style),
            Span::raw(" "),
        ])
    }
}

fn render_visual_row(
    logical: &str,
    vr: VisualRow,
    width: usize,
    style: Style,
    show_cursor: bool,
    cursor_col: usize,
) -> Line<'static> {
    let chars: Vec<char> = logical.chars().collect();
    let chunk: String = chars.iter().skip(vr.start).take(vr.len).collect();

    if !show_cursor || width == 0 {
        return padded_line(&chunk, width, style, vr.wraps);
    }

    let mark_w = usize::from(vr.wraps && width >= 1);
    let content_w = width.saturating_sub(mark_w);
    if content_w == 0 {
        return padded_line(&chunk, width, style, vr.wraps);
    }

    let mut rel = cursor_col.saturating_sub(vr.start);
    if rel > vr.len {
        rel = vr.len;
    }
    if rel >= content_w {
        rel = content_w - 1;
    }

    let mut spans = vec![Span::raw(" ")];

    let before: String = chunk.chars().take(rel).collect();
    spans.push(Span::styled(before.clone(), style));

    let at = chunk.chars().nth(rel).unwrap_or(' ');
    spans.push(Span::styled(at.to_string(), input_cursor_style()));

    let after_chars: String = chunk.chars().skip(rel + 1).collect();
    let used_content = before.chars().count() + 1 + after_chars.chars().count();
    let pad = content_w.saturating_sub(used_content);
    spans.push(Span::styled(
        format!("{after_chars}{}", " ".repeat(pad)),
        style,
    ));

    if vr.wraps {
        spans.push(Span::styled(WRAP_MARK.to_string(), style));
    }
    spans.push(Span::raw(" "));
    Line::from(spans)
}
