use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use super::label_style;

#[derive(Debug, Clone)]
pub struct SelectOption {
    pub label: String,
    pub value: String,
}

/// Inline select in the same style as `Enable`: options separated by ` · `.
#[derive(Debug, Clone)]
pub struct Select {
    pub label: String,
    pub options: Vec<SelectOption>,
    pub selected: usize,
    /// `false` = undefined (omit on save).
    pub set: bool,
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
            set: true,
        }
    }

    pub fn unset(mut self) -> Self {
        self.set = false;
        self
    }

    pub fn height(&self) -> u16 {
        1
    }

    pub fn value(&self) -> Option<&str> {
        if !self.set {
            return None;
        }
        self.options.get(self.selected).map(|o| o.value.as_str())
    }

    /// Kept for form focus logic; inline select is never "open".
    pub fn open(&self) -> bool {
        false
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        if self.options.is_empty() {
            return false;
        }
        match key.code {
            KeyCode::Left | KeyCode::Char('h') | KeyCode::Up => {
                if self.selected == 0 {
                    self.selected = self.options.len() - 1;
                } else {
                    self.selected -= 1;
                }
                self.set = true;
                true
            }
            KeyCode::Right
            | KeyCode::Char('l')
            | KeyCode::Down
            | KeyCode::Char(' ')
            | KeyCode::Enter => {
                self.selected = (self.selected + 1) % self.options.len();
                self.set = true;
                true
            }
            KeyCode::Delete | KeyCode::Backspace => {
                self.set = false;
                true
            }
            _ => false,
        }
    }

    pub fn render(&self, f: &mut Frame, area: Rect, focused: bool) {
        if area.height == 0 {
            return;
        }

        let active = Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD);
        let inactive = Style::default().fg(Color::DarkGray);
        let unknown = Style::default().fg(Color::Yellow);

        let mut spans = vec![Span::styled(
            format!(" {}: ", self.label),
            label_style(focused),
        )];

        for (i, opt) in self.options.iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw(" · "));
            }
            let style = if !self.set {
                unknown
            } else if i == self.selected {
                active
            } else {
                inactive
            };
            spans.push(Span::styled(opt.label.clone(), style));
        }

        if self.options.is_empty() {
            spans.push(Span::styled("—", unknown));
        }

        f.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect { height: 1, ..area },
        );
    }
}
