use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Paragraph, Widget};

use crate::tui::theme;

/// Bottom input bar where the user composes a message.
///
/// Minimal: no title, an amber `>` prompt, a dim placeholder while empty, and
/// a solid amber caret once typing begins.
pub struct InputBar<'a> {
    input: &'a str,
}

impl<'a> InputBar<'a> {
    pub fn new(input: &'a str) -> Self {
        Self { input }
    }
}

impl Widget for InputBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let prompt = Span::styled(
            "> ",
            Style::new().fg(theme::ME).add_modifier(Modifier::BOLD),
        );

        let line = if self.input.is_empty() {
            Line::from(vec![prompt, Span::styled("type a message", theme::meta())])
        } else {
            Line::from(vec![
                prompt,
                Span::styled(self.input, Style::new().fg(theme::TEXT)),
                Span::styled("█", Style::new().fg(theme::ME)),
            ])
        };

        Paragraph::new(line)
            .block(
                Block::bordered()
                    .border_type(BorderType::Plain)
                    .border_style(theme::border()),
            )
            .render(area, buf);
    }
}
