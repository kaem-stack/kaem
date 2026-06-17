use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use crate::datetime;
use crate::tui::app::Author;
use crate::tui::app::Contact;
use crate::tui::theme;

/// Consecutive messages from the same sender are grouped if they arrive within this window.
const GROUP_WINDOW_SECS: i64 = 5 * 60;
/// Longest sender label the name column will reserve.
const SENDER_CAP: usize = 12;

/// Conversation history rendered as a dense operator log.
///
/// One row per message line, filling top-down. Columns: a dim time gutter, a
/// colored lane bar (amber = you, gray = them) that runs the height of each
/// speaker block, the sender name (printed once per block), then the text.
/// Direction is always readable from the lane color, even on grouped lines.
pub struct MessagePanel<'a> {
    contact: &'a Contact,
}

impl<'a> MessagePanel<'a> {
    pub fn new(contact: &'a Contact) -> Self {
        Self { contact }
    }
}

impl Widget for MessagePanel<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let field = Rect {
            x: area.x + 1,
            y: area.y,
            width: area.width.saturating_sub(2),
            height: area.height,
        };
        if field.width < 16 || field.height == 0 {
            return;
        }

        let sender_w = self.contact.name.chars().count().clamp(3, SENDER_CAP);
        // time(5) + sp + bar(1) + sp + sender + 2 spaces.
        let text_start = sender_w + 10;
        let text_w = (field.width as usize).saturating_sub(text_start).max(8);

        let lines = self.lines(sender_w, text_w, field.width as usize);

        // Anchor newest to the bottom: keep only the tail that fits, render down.
        let visible = field.height as usize;
        let start = lines.len().saturating_sub(visible);
        for (row, line) in lines.into_iter().skip(start).enumerate() {
            let rect = Rect {
                x: field.x,
                y: field.y + row as u16,
                width: field.width,
                height: 1,
            };
            line.render(rect, buf);
        }
    }
}

impl MessagePanel<'_> {
    fn lines(&self, sender_w: usize, text_w: usize, full_w: usize) -> Vec<Line<'static>> {
        let mut lines: Vec<Line> = Vec::new();
        let mut prev_author: Option<Author> = None;
        let mut prev_ts: Option<i64> = None;
        let mut prev_day: Option<i64> = None;
        let today = datetime::epoch_day(datetime::now());

        for message in &self.contact.history {
            let cur_day = datetime::epoch_day(message.timestamp);
            let (color, sender) = match message.author {
                Author::Me => (theme::ME, "you".to_string()),
                Author::Them => (theme::THEM, self.contact.name.clone()),
            };

            let day_changed = prev_day.map_or(true, |d| d != cur_day);

            if day_changed {
                if !lines.is_empty() {
                    lines.push(Line::default());
                }
                lines.push(date_separator(
                    &datetime::day_label(cur_day, today),
                    full_w,
                ));
                lines.push(Line::default());
            }

            let new_block = day_changed
                || match (prev_author, prev_ts) {
                    (Some(a), Some(p)) => {
                        a != message.author
                            || (message.timestamp - p) > GROUP_WINDOW_SECS
                    }
                    (Some(a), None) => a != message.author,
                    _ => true,
                };

            if !day_changed && new_block && !lines.is_empty() {
                lines.push(Line::default());
            }

            for (i, segment) in wrap(&message.body, text_w).into_iter().enumerate() {
                let first_in_block = i == 0 && new_block;
                let time = if first_in_block {
                    Span::styled(
                        format!("{:<5}", datetime::hhmm(message.timestamp)),
                        theme::meta(),
                    )
                } else {
                    Span::raw(" ".repeat(5))
                };
                let name = if first_in_block {
                    Span::styled(
                        format!("{sender:<sender_w$}"),
                        Style::new().fg(color).add_modifier(Modifier::BOLD),
                    )
                } else {
                    Span::raw(" ".repeat(sender_w))
                };

                lines.push(Line::from(vec![
                    time,
                    Span::raw(" "),
                    Span::styled("│", Style::new().fg(color)),
                    Span::raw(" "),
                    name,
                    Span::raw("  "),
                    Span::styled(segment, Style::new().fg(theme::TEXT)),
                ]));
            }

            prev_author = Some(message.author);
            prev_ts = Some(message.timestamp);
            prev_day = Some(cur_day);
        }
        lines
    }
}

/// Centered date separator with faint rules on each side, like WhatsApp.
fn date_separator(label: &str, width: usize) -> Line<'static> {
    let padded = format!(" {label} ");
    let pad_len = padded.chars().count();
    let fill = width.saturating_sub(pad_len);
    let left = fill / 2;
    let right = fill.saturating_sub(left);
    Line::from(vec![
        Span::styled("─".repeat(left), Style::new().fg(theme::FAINT)),
        Span::styled(padded, Style::new().fg(theme::META)),
        Span::styled("─".repeat(right), Style::new().fg(theme::FAINT)),
    ])
}

/// Greedy word-wrap to `width` columns (ASCII-width approximation).
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();

    for word in text.split_whitespace() {
        if current.is_empty() {
            current.push_str(word);
        } else if current.chars().count() + 1 + word.chars().count() <= width {
            current.push(' ');
            current.push_str(word);
        } else {
            lines.push(std::mem::take(&mut current));
            current.push_str(word);
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}
