//! The notch silhouette (`NotchShape.qml`), its shadow and clip (`Main.qml:258-300`).

use crate::cx::WIN;
use foco_base::{theme, Color, Fill, Point, Rect, Surface};
use tiny_skia::{Path, PathBuilder};

/// `RectangularShadow.blur`.
pub const SHADOW_BLUR: f32 = 34.0;
/// A black shadow under a light notch needs to be much softer than under a black one.
const LIGHT_SHADOW_SCALE: f32 = 0.4;
const KAPPA: f32 = 0.552_284_8;

/// Notch geometry in window logical px for the current animated width/height/radius.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NotchGeom {
    /// Width of the body (`curW`), without ears.
    pub cur_w: f32,
    /// Height (`curH`).
    pub h: f32,
    /// Bottom corner radius (`curR`).
    pub radius: f32,
    /// `min(12, h / 2.6)`.
    pub ear: f32,
    /// Left edge of the whole shape including the left ear: `round((760 - w) / 2)`.
    pub x: f32,
}

impl NotchGeom {
    pub fn new(cur_w: f32, cur_h: f32, cur_r: f32) -> NotchGeom {
        let cur_w = cur_w.max(0.0);
        let h = cur_h.max(0.0);
        let ear = (h / 2.6).min(12.0);
        let w = cur_w + 2.0 * ear;
        NotchGeom { cur_w, h, radius: cur_r, ear, x: ((WIN - w) * 0.5).round() }
    }
    /// Width including both ears.
    pub fn width(&self) -> f32 {
        self.cur_w + 2.0 * self.ear
    }
    /// `height > 0.5 && curW > 1`.
    pub fn visible(&self) -> bool {
        self.h > 0.5 && self.cur_w > 1.0
    }
    /// The shape's bounding rect (ears included).
    pub fn rect(&self) -> Rect {
        Rect::new(self.x, 0.0, self.width(), self.h)
    }
    /// The content clipper: a **rectangle** (not the notch shape), `(x + ear, 0, curW, h)`.
    pub fn clip_rect(&self) -> Rect {
        Rect::new(self.x + self.ear, 0.0, self.cur_w, self.h)
    }
    /// Damage box: the shape, plus the shadow overhang (`x` +-36, bottom +50) when `shadow`.
    pub fn bbox(&self, shadow: bool) -> Rect {
        let r = self.rect();
        if shadow {
            Rect::new(r.x - 36.0, 0.0, r.w + 72.0, r.h + 50.0)
        } else {
            r
        }
    }
}

/// `NotchShape.qml` path in window coordinates: concave quadratic ears, convex bottom corners
/// of `r = max(0, min(radius, h/2, bw/2))`, `earY = max(0, min(ear, h - r))`.
pub fn notch_path(g: &NotchGeom) -> Option<Path> {
    if g.h <= 0.0 {
        return None;
    }
    let w = g.width();
    let h = g.h;
    let ear = g.ear;
    let bw = (w - 2.0 * ear).max(0.0);
    let r = g.radius.min(h * 0.5).min(bw * 0.5).max(0.0);
    let ear_y = (ear.min(h - r)).max(0.0);
    let k = r * KAPPA;
    let x = g.x;
    let mut pb = PathBuilder::new();
    pb.move_to(x, 0.0);
    pb.quad_to(x + ear, 0.0, x + ear, ear_y);
    pb.line_to(x + ear, h - r);
    // bottom-left corner: left wall -> bottom edge
    pb.cubic_to(x + ear, h - r + k, x + ear + r - k, h, x + ear + r, h);
    pb.line_to(x + w - ear - r, h);
    // bottom-right corner: bottom edge -> right wall
    pb.cubic_to(x + w - ear - r + k, h, x + w - ear, h - r + k, x + w - ear, h - r);
    pb.line_to(x + w - ear, ear_y);
    pb.quad_to(x + w - ear, 0.0, x + w, 0.0);
    pb.line_to(x, 0.0);
    pb.close();
    pb.finish()
}

/// Paints the shadow (when `shadow_opacity > 0`) and then the silhouette.
pub fn paint_notch(s: &mut dyn Surface, g: &NotchGeom, shadow_opacity: f32) {
    if !g.visible() {
        return;
    }
    if shadow_opacity > 0.004 {
        let body = Rect::new(g.x + g.ear, 0.0, g.cur_w, g.h);
        s.shadow(
            body,
            g.radius.min(g.h * 0.5),
            SHADOW_BLUR,
            -4.0,
            Point::new(0.0, 14.0),
            Color::BLACK.with_alpha(shadow_opacity.clamp(0.0, 1.0) * LIGHT_SHADOW_SCALE),
        );
    }
    if let Some(p) = notch_path(g) {
        s.fill_path(&p, &Fill::Solid(theme::NOTCH));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use foco_base::PixSurface;

    #[test]
    fn geometry_matches_main_qml() {
        let g = NotchGeom::new(212.0, 38.0, 19.0);
        assert_eq!(g.ear, 12.0, "38/2.6 = 14.6 is capped at 12");
        assert!((NotchGeom::new(212.0, 26.0, 13.0).ear - 10.0).abs() < 1e-5);
        assert!(g.visible());
        assert_eq!(g.x, ((760.0 - (212.0 + 2.0 * g.ear)) / 2.0).round());
        assert_eq!(g.clip_rect(), Rect::new(g.x + g.ear, 0.0, 212.0, 38.0));
        let big = NotchGeom::new(548.0, 300.0, 32.0);
        assert_eq!(big.ear, 12.0);
        assert!(!NotchGeom::new(0.5, 38.0, 19.0).visible());
        assert!(!NotchGeom::new(100.0, 0.4, 19.0).visible());
        assert!(big.bbox(true).h > big.bbox(false).h);
        // ears shrink with the height
        assert!(NotchGeom::new(100.0, 10.0, 19.0).ear < 4.0);
    }

    #[test]
    fn path_has_concave_ears() {
        let g = NotchGeom::new(200.0, 60.0, 20.0);
        let p = notch_path(&g).unwrap();
        let b = p.bounds();
        assert!((b.left() - g.x).abs() < 0.01 && (b.right() - (g.x + g.width())).abs() < 0.01);
        assert!((b.bottom() - 60.0).abs() < 0.01);
        // draw and probe: the pixel just inside the ear's curve (x=1, y=1) is empty, the body is filled
        let mut s = PixSurface::new(760, 100).unwrap();
        paint_notch(&mut s, &g, 0.0);
        let gx = g.x as u32;
        assert_eq!(s.pixel(gx + 1, 1)[3], 0, "outside the concave ear");
        assert_eq!(s.pixel(gx + g.ear as u32 + 5, 1)[3], 255, "inside the body");
        assert_eq!(s.pixel(gx + g.ear as u32 + 30, 58)[3], 255);
        assert!(s.pixel(gx + g.ear as u32 + 1, 59)[3] < 255, "rounded corner");
        // the ear is filled right at the top edge near the body
        assert!(s.pixel(gx + g.ear as u32 - 1, 0)[3] > 0);
    }

    #[test]
    fn shadow_draws_below() {
        let g = NotchGeom::new(300.0, 120.0, 32.0);
        let mut s = PixSurface::new(760, 260).unwrap();
        paint_notch(&mut s, &g, 0.65);
        assert!(s.pixel(380, 135)[3] > 0, "shadow under the notch");
        assert_eq!(s.pixel(380, 60)[3], 255);
    }
}
