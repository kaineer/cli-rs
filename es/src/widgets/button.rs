use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

/// Simple action button (Save / Cancel).
#[derive(Debug, Clone)]
pub struct Button {
    pub label: String,
}

impl Button {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
        }
    }

    /// Returns true if activated.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        matches!(key.code, KeyCode::Enter | KeyCode::Char(' '))
    }

    pub fn render(&self, f: &mut Frame, area: Rect, focused: bool) {
        if area.height == 0 || area.width == 0 {
            return;
        }
        let style = if focused {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(Color::White)
                .bg(Color::Rgb(0x3b, 0x42, 0x52))
        };
        let text = format!(" {} ", self.label);
        let line = Line::from(Span::styled(text, style));
        f.render_widget(Paragraph::new(line), Rect { height: 1, ..area });
    }
}
