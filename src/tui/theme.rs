//! Warm-vintage palette.
//!
//! Two-tone by design: amber is the primary accent, gray is the secondary.
//! Green and red are held back for status only. Every color has one fixed
//! meaning, and tones are muted on purpose — vintage terminal, not neon.

use ratatui::style::{Color, Modifier, Style};

/// You: sent bubbles, the compose caret, selection — the primary accent.
pub const ME: Color = Color::Rgb(216, 160, 64);
/// Them: received bubbles and unread counts — the secondary accent (gray).
pub const THEM: Color = Color::Rgb(158, 152, 142);
/// Body copy — warm off-white so messages stay readable over the accents.
pub const TEXT: Color = Color::Rgb(214, 207, 188);
/// Metadata: timestamps, key hints, placeholder text.
pub const META: Color = Color::Rgb(124, 118, 104);
/// Panel borders and dividers — present but quiet.
pub const BORDER: Color = Color::Rgb(82, 78, 70);
/// Barely-there rule for day separators ("super low opacity").
pub const FAINT: Color = Color::Rgb(70, 66, 60);
/// Encrypted / healthy state.
pub const OK: Color = Color::Rgb(132, 161, 100);
/// Plaintext / warning state.
pub const WARN: Color = Color::Rgb(184, 92, 80);
/// Near-black ink used as foreground on the amber selection bar.
pub const INK: Color = Color::Rgb(28, 26, 22);

/// Quiet border styling shared by every framed panel.
pub fn border() -> Style {
    Style::new().fg(BORDER)
}

/// Dim metadata styling (timestamps, hints, placeholders).
pub fn meta() -> Style {
    Style::new().fg(META)
}

/// Selected-row styling: a solid amber bar with dark ink text.
pub fn selection() -> Style {
    Style::new().bg(ME).fg(INK).add_modifier(Modifier::BOLD)
}
