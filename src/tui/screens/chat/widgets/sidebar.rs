use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, List, ListItem, ListState, StatefulWidget, Widget};
use unicode_width::UnicodeWidthStr;

use crate::model::Contact;
use crate::tui::theme;

pub struct Sidebar<'a> {
    contacts: &'a [Contact],
    selected: usize,
}

impl<'a> Sidebar<'a> {
    pub fn new(contacts: &'a [Contact], selected: usize) -> Self {
        Self { contacts, selected }
    }

    fn row(contact: &Contact, width: usize) -> ListItem<'static> {
        let mut spans = vec![
            Span::raw(" "),
            Span::styled(contact.name.clone(), Style::new().fg(theme::TEXT)),
        ];

        if contact.unread > 0 {
            let count = contact.unread.to_string();
            let used = 1 + contact.name.width();
            let gap = width.saturating_sub(used + count.width() + 1).max(1);
            spans.push(Span::raw(" ".repeat(gap)));
            spans.push(Span::styled(
                count,
                Style::new().fg(theme::THEM).add_modifier(Modifier::BOLD),
            ));
        }

        ListItem::new(Line::from(spans))
    }
}

impl Widget for Sidebar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let inner_width = area.width.saturating_sub(2) as usize;
        let items: Vec<ListItem> = self
            .contacts
            .iter()
            .map(|c| Self::row(c, inner_width))
            .collect();

        let list = List::new(items)
            .block(
                Block::bordered()
                    .border_type(BorderType::Plain)
                    .border_style(theme::border())
                    .title(Span::styled(" contacts ", theme::meta())),
            )
            .highlight_style(theme::selection());

        let mut state = ListState::default();
        state.select(Some(self.selected));
        StatefulWidget::render(list, area, buf, &mut state);
    }
}
