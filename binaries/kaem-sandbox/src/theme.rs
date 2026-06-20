//! The same brutalist amber-on-gray palette as the live `kaem` binary
//! (`binaries/kaem/src/tui/theme.rs`). Duplicated rather than shared — the
//! two binaries don't depend on each other and the palette is ~10 constants.

use egui::{Color32, FontFamily, FontId, Style, TextStyle, Visuals};

pub const ME: Color32 = Color32::from_rgb(216, 160, 64);
pub const THEM: Color32 = Color32::from_rgb(158, 152, 142);
pub const TEXT: Color32 = Color32::from_rgb(214, 207, 188);
pub const META: Color32 = Color32::from_rgb(124, 118, 104);
pub const BORDER: Color32 = Color32::from_rgb(82, 78, 70);
pub const FAINT: Color32 = Color32::from_rgb(70, 66, 60);
pub const INK: Color32 = Color32::from_rgb(28, 26, 22);

/// `color` at `alpha` (0-255) — used for low-opacity fills like a node's
/// range circle, where the full-strength color would overwhelm the canvas.
pub fn with_alpha(color: Color32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}

/// Build the brutalist dark visuals: amber accent, muted gray text/borders,
/// square (unrounded) widgets, no decoration.
pub fn visuals() -> Visuals {
    let mut visuals = Visuals::dark();

    visuals.override_text_color = Some(TEXT);
    visuals.window_fill = INK;
    visuals.panel_fill = INK;
    visuals.faint_bg_color = INK;
    visuals.extreme_bg_color = INK;
    visuals.hyperlink_color = ME;
    visuals.selection.bg_fill = ME;
    visuals.selection.stroke.color = INK;

    visuals.widgets.noninteractive.bg_fill = INK;
    visuals.widgets.noninteractive.fg_stroke.color = TEXT;
    visuals.widgets.noninteractive.bg_stroke.color = BORDER;

    visuals.widgets.inactive.bg_fill = INK;
    visuals.widgets.inactive.fg_stroke.color = META;
    visuals.widgets.inactive.bg_stroke.color = BORDER;

    visuals.widgets.hovered.bg_fill = FAINT;
    visuals.widgets.hovered.fg_stroke.color = ME;
    visuals.widgets.hovered.bg_stroke.color = ME;

    visuals.widgets.active.bg_fill = FAINT;
    visuals.widgets.active.fg_stroke.color = ME;
    visuals.widgets.active.bg_stroke.color = ME;

    visuals.window_stroke.color = BORDER;

    // Square corners everywhere — keep the brutalist edge.
    let zero = egui::CornerRadius::ZERO;
    visuals.window_corner_radius = zero;
    visuals.menu_corner_radius = zero;
    visuals.widgets.noninteractive.corner_radius = zero;
    visuals.widgets.inactive.corner_radius = zero;
    visuals.widgets.hovered.corner_radius = zero;
    visuals.widgets.active.corner_radius = zero;
    visuals.widgets.open.corner_radius = zero;

    visuals
}

/// Monospace everywhere, tight spacing, no rounded item backgrounds — a
/// terminal/protocol-analyzer feel rather than a default proportional-font
/// desktop app.
pub fn style() -> Style {
    let mut style = Style::default();

    for (text_style, size) in [
        (TextStyle::Small, 11.0),
        (TextStyle::Body, 13.0),
        (TextStyle::Button, 13.0),
        (TextStyle::Heading, 16.0),
        (TextStyle::Monospace, 13.0),
    ] {
        style
            .text_styles
            .insert(text_style, FontId::new(size, FontFamily::Monospace));
    }

    style.spacing.item_spacing = egui::vec2(6.0, 4.0);
    style.spacing.window_margin = egui::Margin::same(8);
    style.spacing.button_padding = egui::vec2(6.0, 2.0);

    // `Style` owns its own `Visuals` — fold ours in here so a single
    // `set_style` call carries both, rather than two calls racing to decide
    // which `Visuals` wins.
    style.visuals = visuals();

    style
}
