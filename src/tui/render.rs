use ratatui::Frame;

use crate::tui::app::App;
use crate::tui::screens::chat;

/// Render a single frame: dispatch to the active screen.
pub fn render(app: &App, frame: &mut Frame) {
    chat::render(app, frame, frame.area());
}
