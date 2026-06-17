pub mod widgets;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};

use crate::app::Chat;
use widgets::input::InputBar;
use widgets::messages::MessagePanel;
use widgets::sidebar::Sidebar;
use widgets::statusbar::StatusBar;

pub fn render(chat: &Chat, frame: &mut Frame, area: Rect) {
    let [body, status] = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(area);
    let [sidebar, conversation] =
        Layout::horizontal([Constraint::Length(24), Constraint::Min(0)]).areas(body);
    let [messages, input] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(3)]).areas(conversation);

    frame.render_widget(Sidebar::new(&chat.contacts, chat.selected), sidebar);
    frame.render_widget(MessagePanel::new(chat.selected_contact()), messages);
    frame.render_widget(InputBar::new(&chat.input), input);
    frame.render_widget(StatusBar::new(chat.encrypted), status);
}
