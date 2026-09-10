use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, MouseButton, MouseEventKind};
use std::time::Duration;

use crate::app::{App, Panel};

pub fn handle_events(app: &mut App, timeout: Duration) -> Result<()> {
    if !event::poll(timeout)? {
        return Ok(());
    }
    match event::read()? {
        Event::Key(key) => handle_key(app, key.code),
        Event::Mouse(mouse) => handle_mouse(app, mouse),
        _ => {}
    }
    Ok(())
}

fn handle_key(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Char('q') | KeyCode::Esc => app.quit(),
        KeyCode::Tab | KeyCode::BackTab => app.toggle_focus(),
        KeyCode::Down | KeyCode::Char('j') => match app.focus {
            Panel::Left => app.next(),
            Panel::Right => app.right_next(),
        },
        KeyCode::Up | KeyCode::Char('k') => match app.focus {
            Panel::Left => app.previous(),
            Panel::Right => app.right_previous(),
        },
        KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Char('l') => match app.focus {
            Panel::Left => app.activate_selected(),
            Panel::Right => { /* следующий шаг: показать stdout выбранного запуска */ }
        },
        _ => {}
    }
}

fn handle_mouse(app: &mut App, mouse: crossterm::event::MouseEvent) {
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            let divider_x = app.left_width as i32;
            if (mouse.column as i32 - divider_x).abs() <= 1 {
                app.dragging = true;
            }
        }
        MouseEventKind::Drag(MouseButton::Left) if app.dragging => {
            let new_width = mouse.column.min(app.area_width.saturating_sub(1));
            app.left_width = new_width.max(4);
        }
        MouseEventKind::Up(MouseButton::Left) => {
            app.dragging = false;
        }
        _ => {}
    }
}
