use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use crate::tui::theme;

pub struct StatusBar {
    encrypted: bool,
}

impl StatusBar {
    pub fn new(encrypted: bool) -> Self {
        Self { encrypted }
    }
}

impl Widget for StatusBar {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let key = Style::new().fg(theme::ME).add_modifier(Modifier::BOLD);

        let mut spans = Vec::new();
        for (chord, action) in [("up/dn", "navigate"), ("enter", "send"), ("esc", "quit")] {
            spans.push(Span::styled(format!(" {chord} "), key));
            spans.push(Span::styled(format!("{action}   "), theme::meta()));
        }
        Line::from(spans).render(area, buf);

        let indicator = if self.encrypted {
            Span::styled(" encrypted ", Style::new().fg(theme::OK))
        } else {
            Span::styled(
                " plaintext ",
                Style::new().fg(theme::WARN).add_modifier(Modifier::BOLD),
            )
        };
        Line::from(indicator).right_aligned().render(area, buf);
    }
}
