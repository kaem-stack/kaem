use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};

use crate::app::App;
use crate::tui::screens::chat;
use crate::tui::widgets::topbar::TopBar;

pub fn render(app: &App, frame: &mut Frame) {
    let [topbar, body] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(frame.area());

    frame.render_widget(TopBar::new(app.signal), topbar);
    chat::render(&app.ui.chat, frame, body);
}
