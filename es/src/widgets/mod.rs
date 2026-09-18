//! Interactive form widgets for the TUI (not schema DSL types).

mod button;
mod checkbox;
mod enable;
mod password;
mod select;
mod text;
mod textarea;

pub use button::Button;
pub use checkbox::Checkbox;
pub use enable::Enable;
pub use password::Password;
pub use select::{Select, SelectOption};
pub use text::Text;
pub use textarea::TextArea;

use ratatui::style::{Color, Modifier, Style};

pub fn label_style(focused: bool) -> Style {
    if focused {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    }
}

pub fn field_style(focused: bool) -> Style {
    if focused {
        Style::default()
            .fg(Color::White)
            .bg(Color::Rgb(0x3b, 0x42, 0x52))
    } else {
        Style::default().fg(Color::White)
    }
}

/// Style for text / password / textarea value area.
pub fn input_style(focused: bool) -> Style {
    if focused {
        Style::default().fg(Color::Black).bg(Color::Cyan)
    } else {
        Style::default()
            .fg(Color::White)
            .bg(Color::Rgb(0x3b, 0x42, 0x52))
    }
}

/// Cursor cell inside a focused text input.
pub fn input_cursor_style() -> Style {
    Style::default()
        .fg(Color::Cyan)
        .bg(Color::Black)
        .add_modifier(Modifier::BOLD)
}
