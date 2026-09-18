use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use super::{field_style, label_style};

#[derive(Debug, Clone)]
pub struct SelectOption {
    pub label: String,
    pub value: String,
}

/// Dropdown-like select: collapsed shows current; open lists options.
#[derive(Debug, Clone)]
pub struct Select {
    pub label: String,
    pub options: Vec<SelectOption>,
    pub selected: usize,
    pub open: bool,
}

impl Select {
    pub fn new(label: impl Into<String>, options: Vec<SelectOption>, selected: usize) -> Self {
        let selected = if options.is_empty() {
            0
        } else {
            selected.min(options.len() - 1)
        };
        Self {
            label: label.into(),
            options,
            selected,
            open: false,
        }
    }

    pub fn height(&self) -> u16 {
        if self.open {
            2 + self.options.len() as u16
        } else {
            2
        }
    }

    pub fn value(&self) -> Option<&str> {
        self.options.get(self.selected).map(|o| o.value.as_str())
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        if self.options.is_empty() {
            return false;
        }
        match key.code {
            KeyCode::Enter | KeyCode::Char(' ') if !self.open => {
                self.open = true;
                true
            }
            KeyCode::Esc if self.open => {
                self.open = false;
                true
            }
            KeyCode::Enter if self.open => {
                self.open = false;
                true
            }
            KeyCode::Up | KeyCode::Char('k') if self.open => {
                if self.selected == 0 {
                    self.selected = self.options.len() - 1;
                } else {
                    self.selected -= 1;
                }
                true
            }
            KeyCode::Down | KeyCode::Char('j') if self.open => {
                self.selected = (self.selected + 1) % self.options.len();
                true
            }
            _ => false,
        }
    }

    pub fn render(&self, f: &mut Frame, area: Rect, focused: bool) {
        if area.height == 0 {
            return;
        }
        let label = Line::from(Span::styled(
            format!(" {}", self.label),
            label_style(focused),
        ));
        f.render_widget(Paragraph::new(label), Rect { height: 1, ..area });

        if area.height < 2 {
            return;
        }

        let current = self
            .options
            .get(self.selected)
            .map(|o| o.label.as_str())
            .unwrap_or("—");
        let marker = if self.open { "▼" } else { "▸" };
        let head = Line::from(Span::styled(
            format!(" {marker} {current}"),
            field_style(focused),
        ));
        f.render_widget(
            Paragraph::new(head),
            Rect {
                y: area.y + 1,
                height: 1,
                ..area
            },
        );

        if !self.open || area.height < 3 {
            return;
        }

        let list_area = Rect {
            y: area.y + 2,
            height: area.height.saturating_sub(2),
            ..area
        };
        let mut lines = Vec::new();
        for (i, opt) in self.options.iter().enumerate() {
            if lines.len() as u16 >= list_area.height {
                break;
            }
            let prefix = if i == self.selected { "› " } else { "  " };
            let style = if focused && i == self.selected {
                field_style(true)
            } else {
                field_style(false)
            };
            lines.push(Line::from(Span::styled(
                format!(" {prefix}{}", opt.label),
                style,
            )));
        }
        f.render_widget(Paragraph::new(lines), list_area);
    }
}
