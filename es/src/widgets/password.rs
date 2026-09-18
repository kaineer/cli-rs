use crossterm::event::KeyEvent;
use ratatui::{layout::Rect, Frame};

use super::text::{handle_line_edit, render_line_field};

/// Single-line masked text input.
#[derive(Debug, Clone)]
pub struct Password {
    pub label: String,
    pub value: String,
    pub cursor: usize,
}

impl Password {
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        let value = value.into();
        let cursor = value.chars().count();
        Self {
            label: label.into(),
            value,
            cursor,
        }
    }

    pub fn height(&self) -> u16 {
        2
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        handle_line_edit(&mut self.value, &mut self.cursor, key)
    }

    pub fn render(&self, f: &mut Frame, area: Rect, focused: bool) {
        render_line_field(
            f,
            area,
            &self.label,
            &self.value,
            self.cursor,
            focused,
            true,
        );
    }
}
