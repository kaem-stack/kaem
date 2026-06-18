use ratatui::style::{Color, Modifier, Style};

pub const ME: Color = Color::Rgb(216, 160, 64);
pub const THEM: Color = Color::Rgb(158, 152, 142);
pub const TEXT: Color = Color::Rgb(214, 207, 188);
pub const META: Color = Color::Rgb(124, 118, 104);
pub const BORDER: Color = Color::Rgb(82, 78, 70);
pub const FAINT: Color = Color::Rgb(70, 66, 60);
pub const OK: Color = Color::Rgb(132, 161, 100);
pub const WARN: Color = Color::Rgb(184, 92, 80);
pub const INK: Color = Color::Rgb(28, 26, 22);

pub fn border() -> Style {
    Style::new().fg(BORDER)
}

pub fn meta() -> Style {
    Style::new().fg(META)
}

pub fn selection() -> Style {
    Style::new().bg(ME).fg(INK).add_modifier(Modifier::BOLD)
}
