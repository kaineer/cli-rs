use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use super::label_style;

/// Boolean shown as enabled/disabled labels.
#[derive(Debug, Clone)]
pub struct Enable {
    pub label: String,
    pub value: Option<bool>,
    pub on: String,
    pub off: String,
}

impl Enable {
    pub fn new(
        label: impl Into<String>,
        value: Option<bool>,
        on: impl Into<String>,
        off: impl Into<String>,
    ) -> Self {
        Self {
            label: label.into(),
            value,
            on: on.into(),
            off: off.into(),
        }
    }

    pub fn height(&self) -> u16 {
        1
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char(' ') | KeyCode::Enter | KeyCode::Left | KeyCode::Right => {
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
        let active = Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD);
        let inactive = Style::default().fg(Color::DarkGray);
        let unknown = Style::default().fg(Color::Yellow);

        let (on_style, off_style) = match self.value {
            Some(true) => (active, inactive),
            Some(false) => (inactive, active),
            None => (unknown, unknown),
        };

        let line = Line::from(vec![
            Span::styled(format!(" {} ", self.label), label_style(focused)),
            Span::styled(&self.on, on_style),
            Span::raw(" · "),
            Span::styled(&self.off, off_style),
        ]);
        f.render_widget(Paragraph::new(line), Rect { height: 1, ..area });
    }
}
