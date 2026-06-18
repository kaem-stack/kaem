use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use crate::tui::theme;

const BARS: [char; 4] = ['▂', '▄', '▆', '█'];

pub struct TopBar {
    signal: u8,
}

impl TopBar {
    pub fn new(signal: u8) -> Self {
        Self { signal: signal.min(100) }
    }
}

impl Widget for TopBar {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let (active, color) = signal_level(self.signal);

        let mut spans = vec![Span::raw(" ")];
        for (i, bar) in BARS.iter().enumerate() {
            let style = if active == 0 || i < active {
                Style::new().fg(color).add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(theme::FAINT)
            };
            spans.push(Span::styled(bar.to_string(), style));
        }
        spans.push(Span::raw(" "));

        Line::from(spans).right_aligned().render(area, buf);
    }
}

// returns (active_bar_count, color). active=0 means no connection (all bars red).
fn signal_level(signal: u8) -> (usize, ratatui::style::Color) {
    match signal {
        0       => (0, theme::WARN),
        1..=25  => (1, theme::WARN),
        26..=50 => (2, theme::ME),
        51..=75 => (3, theme::ME),
        _       => (4, theme::ME),
    }
}
