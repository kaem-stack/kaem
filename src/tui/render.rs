use ratatui::Frame;

use crate::app::App;
use crate::tui::screens::chat;

pub fn render(app: &App, frame: &mut Frame) {
    chat::render(app, frame, frame.area());
}
