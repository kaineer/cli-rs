use anyhow::Result;
use crossterm::event::{
    self, Event, KeyCode, KeyModifiers, MouseButton, MouseEventKind,
};
use std::time::Duration;

use crate::app::{App, Panel, RightMode};

pub fn handle_events(app: &mut App, timeout: Duration) -> Result<()> {
    if !event::poll(timeout)? {
        return Ok(());
    }
    match event::read()? {
        Event::Key(key) => handle_key(app, key.code, key.modifiers),
        Event::Mouse(mouse) => handle_mouse(app, mouse),
        _ => {}
    }
    Ok(())
}

fn handle_key(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    match (code, modifiers) {
        (KeyCode::Char('q'), _) | (KeyCode::Esc, _) => app.quit(),

        (KeyCode::Tab, _) | (KeyCode::BackTab, _) => app.toggle_focus(),

        // j / Down
        (KeyCode::Down, _) | (KeyCode::Char('j'), KeyModifiers::NONE) => match app.focus {
            Panel::Left => app.next(),
            Panel::Right => match app.right_mode() {
                RightMode::List => app.right_next(),
                RightMode::Body => app.right_scroll(1),
            },
        },
        // k / Up
        (KeyCode::Up, _) | (KeyCode::Char('k'), KeyModifiers::NONE) => match app.focus {
            Panel::Left => app.previous(),
            Panel::Right => match app.right_mode() {
                RightMode::List => app.right_previous(),
                RightMode::Body => app.right_scroll(-1),
            },
        },

        // Space / Enter / l
        (KeyCode::Char(' '), _)
        | (KeyCode::Enter, _)
        | (KeyCode::Char('l'), KeyModifiers::NONE) => match app.focus {
            Panel::Left => app.activate_selected(),
            Panel::Right => app.right_toggle_view(),
        },

        _ => {}
    }
}

fn handle_mouse(app: &mut App, mouse: crossterm::event::MouseEvent) {
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            // 1. Divider.
            let divider_x = app.left_width as i32;
            if (mouse.column as i32 - divider_x).abs() <= 1 {
                app.dragging = true;
                return;
            }

            // 2. Левая панель.
            if app
                .left_panel_rect
                .contains((mouse.column, mouse.row).into())
            {
                app.on_left_panel_click(mouse.row);
                return;
            }

            // 3. Правая панель.
            if app
                .right_panel_rect
                .contains((mouse.column, mouse.row).into())
            {
                app.on_right_panel_click(mouse.row);
                return;
            }
        }
        MouseEventKind::Down(MouseButton::Right) => {
            // Правый клик по элементу левого списка = «выделить + запустить».
            if app
                .left_panel_rect
                .contains((mouse.column, mouse.row).into())
            {
                app.on_left_panel_right_click(mouse.row);
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
