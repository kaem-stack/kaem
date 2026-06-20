//! Field <-> screen coordinate mapping, plus the math for drawing the
//! expanding RF wave pulses. Pure functions only — the actual `Painter` calls
//! live in `ui::field`.
//!
//! The field is square; `inner` (the canvas rect egui hands us) usually isn't.
//! Every mapping here uses one isotropic `scale` (not separate x/y scales) so
//! a circle of `radius` field units always renders as a circle of exactly
//! `field_radius_to_screen(inner, radius)` screen pixels in *every*
//! direction — otherwise a range circle drawn with one scale and node
//! positions placed with another would silently disagree about where the
//! boundary actually is. The unused margin on the long axis is letterboxed
//! (centered, not stretched).

use egui::{Pos2, Rect};

use kaem_link::Pos;

/// The virtual field is a fixed `FIELD` x `FIELD` square (in arbitrary
/// distance units, conceptually meters).
pub const FIELD: f32 = 100.0;

/// How many field units one wave pulse advances per virtual millisecond. At
/// `range = 35.0` this crosses the full range in ~0.7s of virtual time.
pub const WAVE_SPEED: f32 = 0.05;

/// The single field-units-to-pixels scale for `inner`, plus the top-left
/// offset of the centered, letterboxed field square within it.
struct Projection {
    scale: f32,
    offset: Pos2,
}

impl Projection {
    fn for_rect(inner: Rect) -> Option<Self> {
        if inner.width() <= 0.0 || inner.height() <= 0.0 {
            return None;
        }
        let scale = (inner.width() / FIELD).min(inner.height() / FIELD);
        let used = FIELD * scale;
        let offset = Pos2::new(
            inner.min.x + (inner.width() - used) / 2.0,
            inner.min.y + (inner.height() - used) / 2.0,
        );
        Some(Self { scale, offset })
    }
}

/// Map a field position to a screen point within `inner` (the canvas's
/// drawable rect). Clamped so out-of-bounds field positions still land on the
/// nearest edge rather than escaping `inner`.
pub fn field_to_screen(inner: Rect, pos: Pos) -> Pos2 {
    let Some(proj) = Projection::for_rect(inner) else {
        return inner.min;
    };
    let x = pos.x.clamp(0.0, FIELD);
    let y = pos.y.clamp(0.0, FIELD);
    Pos2::new(
        proj.offset.x + x * proj.scale,
        proj.offset.y + y * proj.scale,
    )
}

/// Map a screen point back to a field position — the inverse of
/// [`field_to_screen`], used for click hit-testing and cursor placement.
pub fn screen_to_field(inner: Rect, point: Pos2) -> Pos {
    let Some(proj) = Projection::for_rect(inner) else {
        return Pos { x: 0.0, y: 0.0 };
    };
    Pos {
        x: ((point.x - proj.offset.x) / proj.scale).clamp(0.0, FIELD),
        y: ((point.y - proj.offset.y) / proj.scale).clamp(0.0, FIELD),
    }
}

/// Convert a radius in field units to a radius in screen pixels for `inner`,
/// using the exact same scale [`field_to_screen`] uses — so a range circle
/// drawn with this radius always lands precisely on the boundary
/// `field_to_screen`-placed nodes actually cross.
pub fn field_radius_to_screen(inner: Rect, radius: f32) -> f32 {
    let Some(proj) = Projection::for_rect(inner) else {
        return 0.0;
    };
    radius * proj.scale
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
        let p = field_to_screen(i, Pos { x: 0.0, y: 0.0 });
        let scale = i.height() / FIELD;
        let expected_x = i.min.x + (i.width() - FIELD * scale) / 2.0;
        assert!((p.x - expected_x).abs() < 0.01);
        assert!((p.y - i.min.y).abs() < 0.01);
    }

    #[test]
    fn far_corner_maps_to_letterboxed_bottom_right() {
        let i = inner();
        let p = field_to_screen(i, Pos { x: FIELD, y: FIELD });
        let scale = i.height() / FIELD;
        let expected_x = i.min.x + (i.width() - FIELD * scale) / 2.0 + FIELD * scale;
        assert!((p.x - expected_x).abs() < 0.01);
        assert!((p.y - i.max.y).abs() < 0.01);
    }

    #[test]
    fn square_rect_has_no_letterboxing() {
        let i = square_inner();
        let p = field_to_screen(i, Pos { x: FIELD, y: FIELD });
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
        );
        let center = i.center();
        assert!((p.x - center.x).abs() < 0.01);
        assert!((p.y - center.y).abs() < 0.01);
    }

    #[test]
    fn screen_to_field_roundtrips_with_field_to_screen() {
        let i = inner();
        let pos = Pos { x: 30.0, y: 70.0 };
        let p = field_to_screen(i, pos);
        let back = screen_to_field(i, p);
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
        );
        assert!(i.contains(p));
    }

    #[test]
    fn out_of_range_screen_points_clamp_into_field() {
        let i = inner();
        let pos = screen_to_field(i, Pos2::new(i.min.x - 500.0, i.max.y + 500.0));
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
        let screen_radius = field_radius_to_screen(i, range);

        let origin = Pos { x: 20.0, y: 20.0 };
        let along_x = Pos {
            x: origin.x + range,
            y: origin.y,
        };
        let along_y = Pos {
            x: origin.x,
            y: origin.y + range,
        };

        let p0 = field_to_screen(i, origin);
        let px = field_to_screen(i, along_x);
        let py = field_to_screen(i, along_y);

        let dist_x = ((px.x - p0.x).powi(2) + (px.y - p0.y).powi(2)).sqrt();
        let dist_y = ((py.x - p0.x).powi(2) + (py.y - p0.y).powi(2)).sqrt();

        assert!((dist_x - screen_radius).abs() < 0.01);
        assert!((dist_y - screen_radius).abs() < 0.01);
    }
}
