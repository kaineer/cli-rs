mod debug;
mod normal;

use crate::app::App;
use ratatui::Frame;

pub fn render(frame: &mut Frame, app: &App) {
    if app.debug_mode {
        debug::render_debug(frame, app);
    } else {
        normal::render_normal(frame, app);
    }
}
