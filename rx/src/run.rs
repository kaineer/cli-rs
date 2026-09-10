use std::time::Duration;

use anyhow::Result;
use ratatui::{backend::Backend, Terminal};

use crate::{app::App, event, ui};

pub fn run<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    loop {
        app.tick();
        terminal.draw(|f| ui::draw(f, app))?;
        event::handle_events(app, Duration::from_millis(80))?;
        if app.should_quit {
            return Ok(());
        }
    }
}
