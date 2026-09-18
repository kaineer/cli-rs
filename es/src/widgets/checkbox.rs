use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use super::{field_style, label_style};

/// Classic checkbox boolean.
#[derive(Debug, Clone)]
pub struct Checkbox {
    pub label: String,
    /// `None` = undefined (not set).
    pub value: Option<bool>,
}

impl Checkbox {
    pub fn new(label: impl Into<String>, value: Option<bool>) -> Self {
        Self {
            label: label.into(),
            value,
        }
    }

    pub fn height(&self) -> u16 {
        1
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char(' ') | KeyCode::Enter => {
                self.value = match self.value {
                    None | Some(false) => Some(true),
                    Some(true) => Some(false),
                };
                true
            }
            KeyCode::Delete | KeyCode::Backspace => {
                self.value = None;
                true
            }
            _ => false,
        }
    }

    pub fn render(&self, f: &mut Frame, area: Rect, focused: bool) {
        if area.height == 0 {
            return;
        }
        let mark = match self.value {
            Some(true) => "[x]",
            Some(false) => "[ ]",
            None => "[?]",
        };
        let line = Line::from(vec![
            Span::styled(format!(" {mark} "), field_style(focused)),
            Span::styled(&self.label, label_style(focused)),
        ]);
        f.render_widget(Paragraph::new(line), Rect { height: 1, ..area });
    }
}
