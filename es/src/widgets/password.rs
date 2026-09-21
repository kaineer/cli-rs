use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{layout::Rect, Frame};

use super::text::{handle_line_edit, render_collapsed, render_editing_line};

/// Single-line masked text input with collapsed / editing modes.
#[derive(Debug, Clone)]
pub struct Password {
    pub label: String,
    pub value: String,
    pub cursor: usize,
    pub editing: bool,
    /// Collapsed preview when value is empty (default: "empty").
    pub empty: String,
    /// Collapsed preview when value is set (default: "provided").
    pub filled: String,
}

impl Password {
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self::with_hints(label, value, None, None)
    }

    pub fn with_hints(
        label: impl Into<String>,
        value: impl Into<String>,
        empty: Option<String>,
        filled: Option<String>,
    ) -> Self {
        let value = value.into();
        let cursor = value.chars().count();
        Self {
            label: label.into(),
            value,
            cursor,
            editing: false,
            empty: empty.unwrap_or_else(|| "empty".into()),
            filled: filled.unwrap_or_else(|| "provided".into()),
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
            render_editing_line(
                f,
                area,
                &self.label,
                &self.value,
                self.cursor,
                focused,
                true,
            );
        } else {
            let preview = if self.value.is_empty() {
                self.empty.as_str()
            } else {
                self.filled.as_str()
            };
            render_collapsed(f, area, &self.label, preview, focused);
        }
    }
}
