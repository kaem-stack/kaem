use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Paragraph, Widget};
use tui_input::Input;

use crate::tui::theme;

pub struct InputBar<'a> {
    input: &'a Input,
}

impl<'a> InputBar<'a> {
    pub fn new(input: &'a Input) -> Self {
        Self { input }
    }
}

impl Widget for InputBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let prompt = Span::styled(
            "> ",
            Style::new().fg(theme::ME).add_modifier(Modifier::BOLD),
        );

        let value = self.input.value();
        let cursor = self.input.cursor();

        let line = if value.is_empty() {
            Line::from(vec![prompt, Span::styled("type a message", theme::meta())])
        } else {
            let chars: Vec<char> = value.chars().collect();
            let before: String = chars[..cursor.min(chars.len())].iter().collect();
            let after: String = chars[cursor.min(chars.len())..].iter().collect();
            Line::from(vec![
                prompt,
                Span::styled(before, Style::new().fg(theme::TEXT)),
                Span::styled("█", Style::new().fg(theme::ME)),
                Span::styled(after, Style::new().fg(theme::TEXT)),
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
