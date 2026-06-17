use chrono::{NaiveDate, Utc};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;
use unicode_width::UnicodeWidthStr;

use crate::model::{Author, Contact, Message};
use crate::tui::theme;

const GROUP_WINDOW_SECS: i64 = 5 * 60;
const SENDER_CAP: usize = 12;

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

        let sender_w = self.contact.name.width().clamp(3, SENDER_CAP);
        let text_start = sender_w + 10;
        let text_w = (field.width as usize).saturating_sub(text_start).max(8);

        let lines = self.build_lines(sender_w, text_w, field.width as usize);

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

struct PrevMsg<'a> {
    message: &'a Message,
    date: NaiveDate,
}

impl MessagePanel<'_> {
    fn build_lines(&self, sender_w: usize, text_w: usize, full_w: usize) -> Vec<Line<'static>> {
        let mut out: Vec<Line> = Vec::new();
        let mut prev: Option<PrevMsg> = None;
        let today = Utc::now().date_naive();

        for message in &self.contact.history {
            let cur_date = message.timestamp.date_naive();
            let (color, sender) = match message.author {
                Author::Me => (theme::ME, "you".to_string()),
                Author::Them => (theme::THEM, self.contact.name.clone()),
            };

            let day_changed = prev.as_ref().is_none_or(|p| p.date != cur_date);

            if day_changed {
                if !out.is_empty() {
                    out.push(Line::default());
                }
                out.push(date_separator(&day_label(cur_date, today), full_w));
                out.push(Line::default());
            }

            let new_block = day_changed
                || prev.as_ref().is_none_or(|p| {
                    p.message.author != message.author
                        || (message.timestamp - p.message.timestamp).num_seconds()
                            > GROUP_WINDOW_SECS
                });

            if !day_changed && new_block && !out.is_empty() {
                out.push(Line::default());
            }

            for (i, segment) in wrap(&message.body, text_w).into_iter().enumerate() {
                let first = i == 0 && new_block;
                let time = if first {
                    Span::styled(
                        format!("{:<5}", message.timestamp.format("%H:%M")),
                        theme::meta(),
                    )
                } else {
                    Span::raw(" ".repeat(5))
                };
                let name = if first {
                    Span::styled(
                        format!("{sender:<sender_w$}"),
                        Style::new().fg(color).add_modifier(Modifier::BOLD),
                    )
                } else {
                    Span::raw(" ".repeat(sender_w))
                };

                out.push(Line::from(vec![
                    time,
                    Span::raw(" "),
                    Span::styled("│", Style::new().fg(color)),
                    Span::raw(" "),
                    name,
                    Span::raw("  "),
                    Span::styled(segment, Style::new().fg(theme::TEXT)),
                ]));
            }

            prev = Some(PrevMsg { message, date: cur_date });
        }
        out
    }
}

fn day_label(date: NaiveDate, today: NaiveDate) -> String {
    match (today - date).num_days() {
        0 => "Today".to_string(),
        1 => "Yesterday".to_string(),
        _ => date.format("%b %-d, %Y").to_string(),
    }
}

fn date_separator(label: &str, width: usize) -> Line<'static> {
    let padded = format!(" {label} ");
    let fill = width.saturating_sub(padded.width());
    let left = fill / 2;
    let right = fill.saturating_sub(left);
    Line::from(vec![
        Span::styled("─".repeat(left), Style::new().fg(theme::FAINT)),
        Span::styled(padded, Style::new().fg(theme::META)),
        Span::styled("─".repeat(right), Style::new().fg(theme::FAINT)),
    ])
}

fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();

    for word in text.split_whitespace() {
        if current.is_empty() {
            current.push_str(word);
        } else if current.width() + 1 + word.width() <= width {
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
