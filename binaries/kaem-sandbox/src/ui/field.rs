//! The field canvas: draws nodes, their range circles, range-links, RF wave
//! pulses, and the placement cursor with `egui::Painter`, and turns clicks
//! into either a "open this node's chat" or "move the cursor" action. Also
//! reports raw drag info so `app.rs` can let the operator drag nodes around.

use egui::{Align2, FontId, Painter, Rect, Response, Sense, Stroke, Ui};

use kaem_sim::{Medium, Pos};

use crate::field::{WAVE_SPEED, field_radius_to_screen, field_to_screen, screen_to_field};
use crate::sandbox::Pulse;
use crate::theme;

/// How close (in field units) a click or drag-start must land to a node to
/// hit it instead of the bare field.
pub const HIT_THRESHOLD: f32 = 6.0;

const NODE_RADIUS: f32 = 5.0;
const CURSOR_RADIUS: f32 = 3.0;
const RANGE_FILL_ALPHA: u8 = 14;
const RANGE_STROKE_ALPHA: u8 = 50;

/// A labeled node drawn on the canvas.
pub struct CanvasNode<'a> {
    pub name: &'a str,
    pub pos: Pos,
    pub emphasized: bool,
}

/// What a click on the canvas resolved to.
pub enum CanvasClick {
    Node(usize),
    Field(Pos),
}

/// Everything `app.rs` needs after a frame: the resolved click (if any) and
/// the raw interaction response/rect, so the caller can additionally hit-test
/// drag gestures against `nodes` itself (`Canvas` doesn't own drag state —
/// that lives in `SandboxApp` across frames).
pub struct CanvasOutput {
    pub click: Option<CanvasClick>,
    pub response: Response,
    pub inner: Rect,
}

pub struct Canvas<'a> {
    pub medium: &'a Medium,
    pub nodes: &'a [CanvasNode<'a>],
    pub pulses: &'a [Pulse],
    pub now: u64,
    pub cursor: Pos,
}

impl Canvas<'_> {
    /// Draw the canvas into the rect egui allocates for `ui`.
    pub fn show(&self, ui: &mut Ui) -> CanvasOutput {
        let desired = ui.available_size();
        let (response, painter) = ui.allocate_painter(desired, Sense::click_and_drag());
        let inner = response.rect;

        painter.rect_filled(inner, 0.0, theme::INK);
        painter.rect_stroke(
            inner,
            0.0,
            Stroke::new(1.0, theme::BORDER),
            egui::StrokeKind::Inside,
        );

        if inner.width() <= 0.0 || inner.height() <= 0.0 {
            return CanvasOutput {
                click: None,
                response,
                inner,
            };
        }

        self.draw_range_circles(&painter, inner);
        self.draw_links(&painter, inner);
        self.draw_waves(&painter, inner);
        self.draw_nodes(&painter, inner);
        self.draw_cursor(&painter, inner);

        let click = self.resolve_click(&response, inner);
        CanvasOutput {
            click,
            response,
            inner,
        }
    }

    fn resolve_click(&self, response: &Response, inner: Rect) -> Option<CanvasClick> {
        let point = response.interact_pointer_pos()?;
        if !response.clicked() {
            return None;
        }
        let pos = screen_to_field(inner, point);
        match nearest_node(self.nodes, pos, HIT_THRESHOLD) {
            Some(idx) => Some(CanvasClick::Node(idx)),
            None => Some(CanvasClick::Field(pos)),
        }
    }

    fn draw_range_circles(&self, painter: &Painter, inner: Rect) {
        let range = self.medium.range();
        let screen_radius = field_radius_to_screen(inner, range);
        for node in self.nodes {
            let center = field_to_screen(inner, node.pos);
            painter.circle_filled(
                center,
                screen_radius,
                theme::with_alpha(theme::ME, RANGE_FILL_ALPHA),
            );
            painter.circle_stroke(
                center,
                screen_radius,
                Stroke::new(1.0, theme::with_alpha(theme::ME, RANGE_STROKE_ALPHA)),
            );
        }
    }

    fn draw_links(&self, painter: &Painter, inner: Rect) {
        for (a, b) in self.medium.reachable() {
            let Some(pa) = self.medium.position(a) else {
                continue;
            };
            let Some(pb) = self.medium.position(b) else {
                continue;
            };
            let pa = field_to_screen(inner, pa);
            let pb = field_to_screen(inner, pb);
            painter.line_segment([pa, pb], Stroke::new(1.0, theme::FAINT));
        }
    }

    fn draw_waves(&self, painter: &Painter, inner: Rect) {
        let range = self.medium.range();
        for pulse in self.pulses {
            let age = self.now.saturating_sub(pulse.start) as f32;
            let radius = age * WAVE_SPEED;
            if radius <= 0.0 || radius > range {
                continue;
            }
            let center = field_to_screen(inner, pulse.origin);
            let screen_radius = field_radius_to_screen(inner, radius);
            painter.circle_stroke(center, screen_radius, Stroke::new(1.0, theme::META));
        }
    }

    fn draw_nodes(&self, painter: &Painter, inner: Rect) {
        for node in self.nodes {
            let center = field_to_screen(inner, node.pos);
            let color = theme::ME;
            painter.circle_filled(center, NODE_RADIUS, color);
            if node.emphasized {
                painter.circle_stroke(center, NODE_RADIUS + 2.0, Stroke::new(1.5, theme::ME));
            }
            painter.text(
                center + egui::vec2(0.0, NODE_RADIUS + 4.0),
                Align2::CENTER_TOP,
                node.name,
                FontId::monospace(13.0),
                theme::ME,
            );
        }
    }

    fn draw_cursor(&self, painter: &Painter, inner: Rect) {
        let center = field_to_screen(inner, self.cursor);
        painter.circle_stroke(center, CURSOR_RADIUS, Stroke::new(1.0, theme::META));
    }
}

/// The nearest node to `pos` within `threshold` field units, if any. Exposed
/// beyond this module so `app.rs` can hit-test a drag's starting point
/// against the same node list used for click resolution. Compares squared
/// distances throughout — no `sqrt` needed for a nearest/threshold query.
pub(crate) fn nearest_node(nodes: &[CanvasNode<'_>], pos: Pos, threshold: f32) -> Option<usize> {
    let threshold_sq = threshold * threshold;
    nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (i, distance_sq(n.pos, pos)))
        .filter(|&(_, d)| d <= threshold_sq)
        .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
        .map(|(i, _)| i)
}

fn distance_sq(a: Pos, b: Pos) -> f32 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    dx * dx + dy * dy
}
