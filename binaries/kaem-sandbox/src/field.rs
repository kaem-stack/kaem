//! Field <-> screen coordinate mapping, plus the math for drawing the
//! expanding RF wave pulses. Pure functions only — the actual `Painter` calls
//! live in `ui::field`.
//!
//! The field is square; `inner` (the canvas rect egui hands us) usually isn't.
//! Every mapping here uses one isotropic `scale` (not separate x/y scales) so
//! a circle of `radius` field units always renders as a circle of exactly
//! `field_radius_to_screen(inner, radius, view)` screen pixels in *every*
//! direction — otherwise a range circle drawn with one scale and node
//! positions placed with another would silently disagree about where the
//! boundary actually is. [`View`] adds a zoom/pan window on top of that base
//! mapping; at its default (`zoom = 1.0`, centered on the field) the result
//! is exactly the old fixed full-field fit, letterboxed on the long axis.

use egui::{Pos2, Rect};

use kaem_sim::Pos;

/// The virtual field is a fixed `FIELD` x `FIELD` square, in meters — the
/// same unit `Medium`'s range and the grid lines are denominated in.
pub const FIELD: f32 = 100.0;

/// How many field units (meters) one wave pulse advances per virtual
/// millisecond. At `range = 35.0` this crosses the full range in ~2.8s of
/// virtual time — slow enough to actually watch a hop travel, rather than a
/// near-instant blip.
pub const WAVE_SPEED: f32 = 0.0125;

/// Zoom bounds for the canvas — `1.0` is the full-field fit; below/above
/// that the view is zoomed out/in around [`View::center`].
pub const MIN_ZOOM: f32 = 0.5;
pub const MAX_ZOOM: f32 = 8.0;

/// The visible window onto the field: how much closer than a full-field fit
/// the canvas is (`zoom`) and which field point (meters) sits at the center
/// of the canvas (`center`). `Default` reproduces the old fixed mapping
/// exactly — `zoom = 1.0`, centered on the field's own center — so any
/// caller that doesn't care about zoom/pan can just pass `View::default()`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub zoom: f32,
    pub center: Pos,
}

impl Default for View {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            center: Pos {
                x: FIELD / 2.0,
                y: FIELD / 2.0,
            },
        }
    }
}

/// The single field-units-to-pixels scale for `inner` at a given `View`,
/// plus the screen offset that puts `view.center` at `inner`'s center.
struct Projection {
    scale: f32,
    offset: Pos2,
}

impl Projection {
    fn for_rect(inner: Rect, view: View) -> Option<Self> {
        if inner.width() <= 0.0 || inner.height() <= 0.0 {
            return None;
        }
        let base_scale = (inner.width() / FIELD).min(inner.height() / FIELD);
        let scale = base_scale * view.zoom;
        let center = inner.center();
        let offset = Pos2::new(
            center.x - view.center.x * scale,
            center.y - view.center.y * scale,
        );
        Some(Self { scale, offset })
    }
}

/// Map a field position (meters) to a screen point within `inner` under
/// `view`. The field coordinate itself is clamped to `[0, FIELD]` first —
/// callers only ever hold positions inside the field (nodes, cursor), this
/// just guards against a stray out-of-domain value rather than expressing
/// anything about the visible window, which `view` already governs.
pub fn field_to_screen(inner: Rect, pos: Pos, view: View) -> Pos2 {
    let Some(proj) = Projection::for_rect(inner, view) else {
        return inner.min;
    };
    let x = pos.x.clamp(0.0, FIELD);
    let y = pos.y.clamp(0.0, FIELD);
    Pos2::new(
        proj.offset.x + x * proj.scale,
        proj.offset.y + y * proj.scale,
    )
}

/// Map a screen point back to a field position (meters) under `view` — the
/// inverse of [`field_to_screen`], used for click hit-testing, cursor
/// placement, and node dragging. Clamped to `[0, FIELD]` so e.g. a drag past
/// the field's edge still lands the node on the boundary.
pub fn screen_to_field(inner: Rect, point: Pos2, view: View) -> Pos {
    let Some(proj) = Projection::for_rect(inner, view) else {
        return Pos { x: 0.0, y: 0.0 };
    };
    Pos {
        x: ((point.x - proj.offset.x) / proj.scale).clamp(0.0, FIELD),
        y: ((point.y - proj.offset.y) / proj.scale).clamp(0.0, FIELD),
    }
}

/// Convert a radius in field units (meters) to a radius in screen pixels for
/// `inner` under `view`, using the exact same scale [`field_to_screen`]
/// uses — so a range circle drawn with this radius always lands precisely
/// on the boundary `field_to_screen`-placed nodes actually cross.
pub fn field_radius_to_screen(inner: Rect, radius: f32, view: View) -> f32 {
    let Some(proj) = Projection::for_rect(inner, view) else {
        return 0.0;
    };
    radius * proj.scale
}

/// The field point currently under `point` (screen space) stays fixed as
/// `view`'s zoom changes to `new_zoom` — the "zoom toward the mouse cursor"
/// behavior. Returns the recentered `View`; a no-op (same `center`) if
/// `inner` is degenerate.
pub fn zoom_at(inner: Rect, view: View, point: Pos2, new_zoom: f32) -> View {
    let new_zoom = new_zoom.clamp(MIN_ZOOM, MAX_ZOOM);
    let Some(proj) = Projection::for_rect(inner, view) else {
        return View {
            zoom: new_zoom,
            ..view
        };
    };
    let field_at_point = Pos {
        x: (point.x - proj.offset.x) / proj.scale,
        y: (point.y - proj.offset.y) / proj.scale,
    };
    let base_scale = proj.scale / view.zoom;
    let new_scale = base_scale * new_zoom;
    let center = Pos {
        x: field_at_point.x - (point.x - inner.center().x) / new_scale,
        y: field_at_point.y - (point.y - inner.center().y) / new_scale,
    };
    View {
        zoom: new_zoom,
        center,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inner() -> Rect {
        Rect::from_min_size(Pos2::new(10.0, 5.0), egui::vec2(400.0, 200.0))
    }

    fn square_inner() -> Rect {
        Rect::from_min_size(Pos2::new(0.0, 0.0), egui::vec2(300.0, 300.0))
    }

    #[test]
    fn origin_maps_to_letterboxed_top_left() {
        // 400x200 rect, FIELD-square content scaled by height (the limiting
        // axis), so the square is centered horizontally with margin on
        // either side rather than stretched to fill the width.
        let i = inner();
        let p = field_to_screen(i, Pos { x: 0.0, y: 0.0 }, View::default());
        let scale = i.height() / FIELD;
        let expected_x = i.min.x + (i.width() - FIELD * scale) / 2.0;
        assert!((p.x - expected_x).abs() < 0.01);
        assert!((p.y - i.min.y).abs() < 0.01);
    }

    #[test]
    fn far_corner_maps_to_letterboxed_bottom_right() {
        let i = inner();
        let p = field_to_screen(i, Pos { x: FIELD, y: FIELD }, View::default());
        let scale = i.height() / FIELD;
        let expected_x = i.min.x + (i.width() - FIELD * scale) / 2.0 + FIELD * scale;
        assert!((p.x - expected_x).abs() < 0.01);
        assert!((p.y - i.max.y).abs() < 0.01);
    }

    #[test]
    fn square_rect_has_no_letterboxing() {
        let i = square_inner();
        let p = field_to_screen(i, Pos { x: FIELD, y: FIELD }, View::default());
        assert!((p.x - i.max.x).abs() < 0.01);
        assert!((p.y - i.max.y).abs() < 0.01);
    }

    #[test]
    fn center_maps_roughly_to_center() {
        let i = inner();
        let p = field_to_screen(
            i,
            Pos {
                x: FIELD / 2.0,
                y: FIELD / 2.0,
            },
            View::default(),
        );
        let center = i.center();
        assert!((p.x - center.x).abs() < 0.01);
        assert!((p.y - center.y).abs() < 0.01);
    }

    #[test]
    fn screen_to_field_roundtrips_with_field_to_screen() {
        let i = inner();
        let pos = Pos { x: 30.0, y: 70.0 };
        let p = field_to_screen(i, pos, View::default());
        let back = screen_to_field(i, p, View::default());
        assert!((back.x - pos.x).abs() < 0.01);
        assert!((back.y - pos.y).abs() < 0.01);
    }

    #[test]
    fn out_of_range_field_positions_clamp_into_bounds() {
        let i = inner();
        let p = field_to_screen(
            i,
            Pos {
                x: -10.0,
                y: 1000.0,
            },
            View::default(),
        );
        assert!(i.contains(p));
    }

    #[test]
    fn out_of_range_screen_points_clamp_into_field() {
        let i = inner();
        let pos = screen_to_field(
            i,
            Pos2::new(i.min.x - 500.0, i.max.y + 500.0),
            View::default(),
        );
        assert!(pos.x >= 0.0 && pos.x <= FIELD);
        assert!(pos.y >= 0.0 && pos.y <= FIELD);
    }

    /// The bug this module exists to prevent: on a non-square rect, a radius
    /// converted by `field_radius_to_screen` must match the actual screen
    /// distance between two points exactly `radius` apart in field space —
    /// in *both* axes, not just the one that happens to be the limiting
    /// dimension. Before the isotropic-scale fix, `field_to_screen` stretched
    /// x/y independently while the range circle used an averaged scale, so a
    /// node could sit outside the drawn circle while still in range (or vice
    /// versa) depending on which axis it moved along.
    #[test]
    fn range_circle_radius_matches_real_distance_on_both_axes() {
        let i = inner(); // non-square: 400x200
        let range = 35.0;
        let screen_radius = field_radius_to_screen(i, range, View::default());

        let origin = Pos { x: 20.0, y: 20.0 };
        let along_x = Pos {
            x: origin.x + range,
            y: origin.y,
        };
        let along_y = Pos {
            x: origin.x,
            y: origin.y + range,
        };

        let p0 = field_to_screen(i, origin, View::default());
        let px = field_to_screen(i, along_x, View::default());
        let py = field_to_screen(i, along_y, View::default());

        let dist_x = ((px.x - p0.x).powi(2) + (px.y - p0.y).powi(2)).sqrt();
        let dist_y = ((py.x - p0.x).powi(2) + (py.y - p0.y).powi(2)).sqrt();

        assert!((dist_x - screen_radius).abs() < 0.01);
        assert!((dist_y - screen_radius).abs() < 0.01);
    }

    #[test]
    fn doubling_zoom_doubles_apparent_radius() {
        let i = square_inner();
        let view = View {
            zoom: 2.0,
            ..View::default()
        };
        let base = field_radius_to_screen(i, 10.0, View::default());
        let zoomed = field_radius_to_screen(i, 10.0, view);
        assert!((zoomed - base * 2.0).abs() < 0.01);
    }

    #[test]
    fn zoom_at_keeps_the_pointed_at_field_point_fixed_on_screen() {
        let i = square_inner();
        let view = View::default();
        let point = Pos2::new(i.min.x + 40.0, i.min.y + 40.0);
        let field_before = screen_to_field(i, point, view);

        let zoomed = zoom_at(i, view, point, 3.0);
        let field_after = screen_to_field(i, point, zoomed);

        assert!((field_after.x - field_before.x).abs() < 0.01);
        assert!((field_after.y - field_before.y).abs() < 0.01);
        assert_eq!(zoomed.zoom, 3.0);
    }

    #[test]
    fn zoom_at_clamps_to_bounds() {
        let i = square_inner();
        let zoomed = zoom_at(i, View::default(), i.center(), 100.0);
        assert_eq!(zoomed.zoom, MAX_ZOOM);
        let zoomed = zoom_at(i, View::default(), i.center(), 0.0);
        assert_eq!(zoomed.zoom, MIN_ZOOM);
    }
}
