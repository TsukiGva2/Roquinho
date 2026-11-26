//! `Canvas`: the `foco_base::Surface` implementation over a tiny-skia pixmap (BGRA premultiplied
//! as seen by X11). Axis-aligned clips are integer boxes (Qt scissors the same way) and only
//! become pixel masks for rotated clips or when a primitive straddles a box edge.

use std::rc::Rc;

use foco_base::surface::{path_rrect, Fill, Surface, TextStyle};
use foco_base::{Color, IRect, Point, Rect};
use tiny_skia::{FillRule, Mask, Paint, Path, PathBuilder, Pixmap, PixmapMut, PixmapPaint, Transform};

use crate::glyphs::{blit, blit_rect_cov};
use crate::shadow::{self, Params};
use crate::Engine;

/// Identifies one rasterised `Surface::cached` picture: caller key, device scale, pixel size and
/// the 1/16-pixel sub-pixel phase of its origin.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct SpriteKey {
    key: u64,
    scale: u32,
    w: i32,
    h: i32,
    fx: u8,
    fy: u8,
}

/// Bytes of cached pictures kept before the cache is emptied (a pet's poses: tens to a couple
/// hundred of 6-34 KB each).
const SPRITE_BYTES: usize = 8 << 20;

#[derive(Clone)]
struct State {
    t: Transform,
    opacity: f32,
    /// Integer clip box in pixmap coordinates.
    clip: IRect,
    /// Exact pixel mask (rotated / skewed clips only); already includes the box at creation.
    mask: Option<Rc<Mask>>,
}

/// Drawing surface for one damaged rectangle. Created by [`crate::Renderer::render`].
pub struct Canvas<'a> {
    pm: PixmapMut<'a>,
    w: i32,
    h: i32,
    cur: State,
    stack: Vec<State>,
    eng: &'a mut Engine,
    box_masks: Vec<(IRect, Rc<Mask>)>,
    /// Something has been drawn already (otherwise the pixmap is still all transparent).
    touched: bool,
}

#[inline]
fn is_st(t: &Transform) -> bool {
    t.kx == 0.0 && t.ky == 0.0
}

/// Device-space float bounds of `r` under `t` as `(x0, y0, x1, y1)`.
fn map_rect(t: &Transform, r: Rect) -> (f32, f32, f32, f32) {
    if is_st(t) {
        let (a, b) = (t.sx * r.x + t.tx, t.sx * (r.x + r.w) + t.tx);
        let (c, d) = (t.sy * r.y + t.ty, t.sy * (r.y + r.h) + t.ty);
        (a.min(b), c.min(d), a.max(b), c.max(d))
    } else {
        let pts = [(r.x, r.y), (r.x + r.w, r.y), (r.x, r.y + r.h), (r.x + r.w, r.y + r.h)];
        let mut o = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for (x, y) in pts {
            let (dx, dy) = (t.sx * x + t.kx * y + t.tx, t.ky * x + t.sy * y + t.ty);
            o = (o.0.min(dx), o.1.min(dy), o.2.max(dx), o.3.max(dy));
        }
        o
    }
}

fn to_irect_out(b: (f32, f32, f32, f32), pad: f32) -> IRect {
    let x0 = (b.0 - pad).floor();
    let y0 = (b.1 - pad).floor();
    let x1 = (b.2 + pad).ceil();
    let y1 = (b.3 + pad).ceil();
    // clamp to a sane range to avoid i32 overflow from absurd inputs
    let c = |v: f32| v.clamp(-1.0e6, 1.0e6) as i32;
    IRect::new(c(x0), c(y0), c(x1) - c(x0), c(y1) - c(y0))
}

impl<'a> Canvas<'a> {
    pub(crate) fn new(pm: PixmapMut<'a>, origin: (i32, i32), eng: &'a mut Engine) -> Canvas<'a> {
        let (w, h) = (pm.width() as i32, pm.height() as i32);
        Canvas {
            pm,
            w,
            h,
            cur: State {
                t: Transform::from_translate(-origin.0 as f32, -origin.1 as f32),
                opacity: 1.0,
                clip: IRect::new(0, 0, w, h),
                mask: None,
            },
            stack: Vec::with_capacity(16),
            eng,
            box_masks: Vec::new(),
            touched: false,
        }
    }

    /// Size of the pixmap being painted (the damaged rect).
    pub fn size(&self) -> (i32, i32) {
        (self.w, self.h)
    }

    /// Current transform (local -> pixmap).
    pub fn transform(&self) -> Transform {
        self.cur.t
    }

    /// Device-space (pixmap coordinates) integer bounds of a local rect under the current transform.
    pub fn device_bounds(&self, r: Rect) -> IRect {
        to_irect_out(map_rect(&self.cur.t, r), 0.0)
    }

    /// True when anything of the local rect could be visible (cheap culling for callers).
    pub fn visible(&self, r: Rect) -> bool {
        let b = self.device_bounds(r);
        b.intersect(&self.cur.clip).is_some()
    }

    fn box_mask(&mut self, b: IRect) -> Option<Rc<Mask>> {
        if let Some((_, m)) = self.box_masks.iter().find(|(r, _)| *r == b) {
            return Some(m.clone());
        }
        let mut m = Mask::new(self.w as u32, self.h as u32)?;
        let p = PathBuilder::from_rect(tiny_skia::Rect::from_xywh(b.x as f32, b.y as f32, b.w as f32, b.h as f32)?);
        m.fill_path(&p, FillRule::Winding, false, Transform::identity());
        let m = Rc::new(m);
        if self.box_masks.len() >= 8 {
            self.box_masks.remove(0);
        }
        self.box_masks.push((b, m.clone()));
        Some(m)
    }

    /// Decides how a primitive with device bounds `b` must be clipped.
    /// `None` = fully clipped away; `Some(None)` = no mask needed; `Some(Some(m))` = use `m`.
    fn clip_for(&mut self, b: IRect) -> Option<Option<Rc<Mask>>> {
        let c = self.cur.clip;
        let vis = b.intersect(&c)?;
        if vis.is_empty() {
            return None;
        }
        if let Some(m) = &self.cur.mask {
            return Some(Some(m.clone()));
        }
        let full = c.x <= 0 && c.y <= 0 && c.right() >= self.w && c.bottom() >= self.h;
        let inside = b.x >= c.x && b.y >= c.y && b.right() <= c.right() && b.bottom() <= c.bottom();
        if full || inside {
            Some(None)
        } else {
            Some(self.box_mask(c))
        }
    }

    fn paint(&self, fill: &Fill) -> Option<Paint<'static>> {
        let op = self.cur.opacity;
        let mut paint = Paint { anti_alias: true, ..Paint::default() };
        let stops = |s: &[(f32, Color)]| -> Vec<tiny_skia::GradientStop> {
            s.iter()
                .map(|(o, c)| tiny_skia::GradientStop::new(*o, c.mul_alpha(op).to_skia()))
                .collect()
        };
        match fill {
            Fill::Solid(c) => paint.set_color(c.mul_alpha(op).to_skia()),
            Fill::Linear { p0, p1, stops: st } => {
                paint.shader = tiny_skia::LinearGradient::new(
                    tiny_skia::Point::from_xy(p0.x, p0.y),
                    tiny_skia::Point::from_xy(p1.x, p1.y),
                    stops(st),
                    tiny_skia::SpreadMode::Pad,
                    Transform::identity(),
                )?;
            }
            Fill::Radial { center, radius, stops: st } => {
                let c = tiny_skia::Point::from_xy(center.x, center.y);
                paint.shader = tiny_skia::RadialGradient::new(
                    c,
                    c,
                    *radius,
                    stops(st),
                    tiny_skia::SpreadMode::Pad,
                    Transform::identity(),
                )?;
            }
        }
        Some(paint)
    }

    fn path_bounds(&self, path: &Path, pad: f32) -> IRect {
        let b = path.bounds();
        let r = Rect::new(b.left(), b.top(), b.width(), b.height());
        // +1 px for anti-aliasing
        to_irect_out(map_rect(&self.cur.t, r), pad + 1.0)
    }

    fn scale_factor(&self) -> f32 {
        let t = &self.cur.t;
        (t.sx * t.sy - t.kx * t.ky).abs().sqrt()
    }

    fn solid_premul(&self, c: Color) -> [u8; 4] {
        let p = c.mul_alpha(self.cur.opacity).to_skia().premultiply().to_color_u8();
        [p.red(), p.green(), p.blue(), p.alpha()]
    }

    // ---------------------------------------------------------------- text helpers

    fn text_as_path(&mut self, s: &str, x: f32, y: f32, style: &TextStyle) {
        let lay = self.eng.text.layout(&mut self.eng.fonts, s, style);
        let mut pb = PathBuilder::new();
        for g in &lay.glyphs {
            let Some(face) = self.eng.fonts.get(g.slot) else { continue };
            let scale = style.px / face.upem;
            let mut b = PathOut { pb: &mut pb, ox: x + g.x, oy: y + lay.asc, scale };
            face.face.outline_glyph(ttf_parser::GlyphId(g.gid), &mut b);
        }
        if let Some(p) = pb.finish() {
            self.fill_path(&p, &Fill::Solid(style.color));
        }
    }
}

struct PathOut<'p> {
    pb: &'p mut PathBuilder,
    ox: f32,
    oy: f32,
    scale: f32,
}

impl ttf_parser::OutlineBuilder for PathOut<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        self.pb.move_to(self.ox + x * self.scale, self.oy - y * self.scale);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.pb.line_to(self.ox + x * self.scale, self.oy - y * self.scale);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.pb.quad_to(
            self.ox + x1 * self.scale,
            self.oy - y1 * self.scale,
            self.ox + x * self.scale,
            self.oy - y * self.scale,
        );
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.pb.cubic_to(
            self.ox + x1 * self.scale,
            self.oy - y1 * self.scale,
            self.ox + x2 * self.scale,
            self.oy - y2 * self.scale,
            self.ox + x * self.scale,
            self.oy - y * self.scale,
        );
    }
    fn close(&mut self) {
        self.pb.close();
    }
}

impl Surface for Canvas<'_> {
    fn cached(&mut self, key: u64, bounds: Rect, draw: &mut dyn FnMut(&mut dyn Surface)) {
        let t = self.cur.t;
        let plain = is_st(&t) && (t.sx - t.sy).abs() < 1e-6 && t.sx > 0.0 && self.cur.mask.is_none();
        if plain {
            let sx = t.sx;
            let (dx, dy) = (t.sx * bounds.x + t.tx, t.sy * bounds.y + t.ty);
            let (ix, iy) = (dx.floor(), dy.floor());
            let q = |v: f32| ((v * 16.0).round() as i32).clamp(0, 15) as u8;
            let (fx, fy) = (q(dx - ix), q(dy - iy));
            let (pw, ph) = ((bounds.w * sx).ceil() as i32 + 2, (bounds.h * sx).ceil() as i32 + 2);
            let b = IRect::new(ix as i32, iy as i32, pw, ph);
            match self.clip_for(b) {
                None => return,
                Some(None) if pw > 0 && ph > 0 => {
                    let sk = SpriteKey { key, scale: sx.to_bits(), w: pw, h: ph, fx, fy };
                    if !self.eng.sprites.contains_key(&sk) {
                        let used: usize = self.eng.sprites.values().map(|p| p.data().len()).sum();
                        if used + (pw * ph * 4) as usize > SPRITE_BYTES {
                            self.eng.sprites.clear();
                        }
                        if let Some(mut pm) = Pixmap::new(pw as u32, ph as u32) {
                            {
                                let mut cv = Canvas::new(pm.as_mut(), (0, 0), &mut *self.eng);
                                cv.cur.t = Transform::from_row(
                                    sx,
                                    0.0,
                                    0.0,
                                    sx,
                                    f32::from(fx) / 16.0 - sx * bounds.x,
                                    f32::from(fy) / 16.0 - sx * bounds.y,
                                );
                                draw(&mut cv);
                            }
                            self.eng.sprites.insert(sk, pm);
                        }
                    }
                    if let Some(pm) = self.eng.sprites.get(&sk) {
                        let paint = PixmapPaint { opacity: self.cur.opacity, ..PixmapPaint::default() };
                        self.pm.draw_pixmap(b.x, b.y, pm.as_ref(), &paint, Transform::identity(), None);
                        self.touched = true;
                        return;
                    }
                }
                Some(_) => {}
            }
        }
        draw(self);
    }

    fn save(&mut self) {
        self.stack.push(self.cur.clone());
    }

    fn restore(&mut self) {
        if let Some(s) = self.stack.pop() {
            self.cur = s;
        }
    }

    fn translate(&mut self, dx: f32, dy: f32) {
        self.cur.t = self.cur.t.pre_translate(dx, dy);
    }

    fn scale(&mut self, sx: f32, sy: f32) {
        self.cur.t = self.cur.t.pre_scale(sx, sy);
    }

    fn rotate_deg(&mut self, deg: f32) {
        self.cur.t = self.cur.t.pre_concat(Transform::from_rotate(deg));
    }

    fn concat(&mut self, t: Transform) {
        self.cur.t = self.cur.t.pre_concat(t);
    }

    fn mul_opacity(&mut self, a: f32) {
        self.cur.opacity *= a.clamp(0.0, 1.0);
    }

    fn clip_rect(&mut self, r: Rect) {
        let t = self.cur.t;
        if is_st(&t) {
            let b = map_rect(&t, r);
            // Qt scissors axis-aligned clips on whole device pixels.
            let c = |v: f32| v.round().clamp(-1.0e6, 1.0e6) as i32;
            let (x0, y0, x1, y1) = (c(b.0), c(b.1), c(b.2), c(b.3));
            let nb = IRect::new(x0, y0, x1 - x0, y1 - y0);
            let cur = self.cur.clip;
            match cur.intersect(&nb) {
                Some(i) => self.cur.clip = i,
                None => self.cur.clip = IRect::new(cur.x, cur.y, 0, 0),
            }
            if let Some(m) = &mut self.cur.mask {
                // keep the exact mask consistent with the tightened box
                if let Some(p) = rect_path(self.cur.clip) {
                    Rc::make_mut(m).intersect_path(&p, FillRule::Winding, false, Transform::identity());
                }
            }
            return;
        }
        // rotated / skewed: exact anti-aliased mask
        let Some(path) = foco_base::path_rect(r) else {
            self.cur.clip = IRect::new(self.cur.clip.x, self.cur.clip.y, 0, 0);
            return;
        };
        match &mut self.cur.mask {
            Some(m) => Rc::make_mut(m).intersect_path(&path, FillRule::Winding, true, t),
            None => {
                if let Some(mut m) = Mask::new(self.w as u32, self.h as u32) {
                    m.fill_path(&path, FillRule::Winding, true, t);
                    if let Some(bp) = rect_path(self.cur.clip) {
                        m.intersect_path(&bp, FillRule::Winding, false, Transform::identity());
                    }
                    self.cur.mask = Some(Rc::new(m));
                }
            }
        }
    }

    fn fill_path(&mut self, path: &Path, fill: &Fill) {
        let b = self.path_bounds(path, 0.0);
        let Some(m) = self.clip_for(b) else { return };
        self.touched = true;
        if let Some(p) = self.paint(fill) {
            self.pm.fill_path(path, &p, FillRule::Winding, self.cur.t, m.as_deref());
        }
    }

    fn stroke_path(&mut self, path: &Path, fill: &Fill, stroke: &tiny_skia::Stroke) {
        let miter = if matches!(stroke.line_join, tiny_skia::LineJoin::Miter | tiny_skia::LineJoin::MiterClip) {
            stroke.miter_limit.max(1.0)
        } else {
            1.0
        };
        let pad = stroke.width * 0.5 * miter * self.scale_factor();
        let b = self.path_bounds(path, pad);
        let Some(m) = self.clip_for(b) else { return };
        self.touched = true;
        if let Some(p) = self.paint(fill) {
            self.pm.stroke_path(path, &p, stroke, self.cur.t, m.as_deref());
        }
    }

    fn fill_rrect(&mut self, r: Rect, radius: f32, fill: &Fill) {
        // cull before allocating the path
        let b = to_irect_out(map_rect(&self.cur.t, r), 1.0);
        if b.intersect(&self.cur.clip).is_none() {
            return;
        }
        if let Some(p) = path_rrect(r, radius) {
            self.fill_path(&p, fill);
        }
    }

    fn stroke_rrect(&mut self, r: Rect, radius: f32, fill: &Fill, width: f32) {
        let pad = width * 0.5 * self.scale_factor() + 1.0;
        let b = to_irect_out(map_rect(&self.cur.t, r), pad);
        if b.intersect(&self.cur.clip).is_none() {
            return;
        }
        if let Some(p) = path_rrect(r, radius) {
            let s = tiny_skia::Stroke { width, ..tiny_skia::Stroke::default() };
            self.stroke_path(&p, fill, &s);
        }
    }

    fn shadow(&mut self, r: Rect, radius: f32, blur: f32, spread: f32, offset: Point, color: Color) {
        let a = color.a * self.cur.opacity;
        if a < 1.0 / 512.0 || r.is_empty() || !is_st(&self.cur.t) || self.cur.t.sx <= 0.0 || self.cur.t.sy <= 0.0 {
            return;
        }
        let t = self.cur.t;
        let inset = if blur > 0.0 { shadow::INSET0 + shadow::INSET_PER_BLUR * blur } else { 0.0 };
        let shape = r.translate(offset.x, offset.y).inflate(spread - inset, spread - inset);
        if shape.is_empty() {
            return;
        }
        let s = t.sx.min(t.sy);
        let round = |v: f32| v.round().clamp(-1.0e6, 1.0e6) as i32;
        let (x0, y0) = (round(t.sx * shape.x + t.tx), round(t.sy * shape.y + t.ty));
        let (x1, y1) = (round(t.sx * shape.right() + t.tx), round(t.sy * shape.bottom() + t.ty));
        let rect = IRect::new(x0, y0, (x1 - x0).max(1), (y1 - y0).max(1));
        let sigma = shadow::SIGMA_PER_BLUR * blur * s;
        let rad = ((radius + spread).max(0.0) * s).min(rect.w.min(rect.h) as f32 * 0.5);
        if sigma < 0.3 {
            // hard shadow
            let c = color.mul_alpha(1.0);
            let sr = Rect::new(shape.x, shape.y, shape.w, shape.h);
            self.fill_rrect(sr, radius + spread, &Fill::Solid(c));
            return;
        }
        let params = Params { rect, radius: rad * shadow::RADIUS_FACTOR, sigma };
        let Some(m) = self.clip_for(params.bounds()) else { return };
        let premul = self.solid_premul(color);
        let clip = self.cur.clip;
        let (w, h) = (self.w, self.h);
        let mask = m.as_ref().map(|m| m.data());
        let fresh = !self.touched && mask.is_none();
        self.touched = true;
        self.eng
            .shadows
            .composite(self.pm.data_mut(), w, h, clip, mask, &params, premul, fresh);
    }

    fn text(&mut self, s: &str, x: f32, y: f32, style: &TextStyle) -> f32 {
        let lay = self.eng.text.layout(&mut self.eng.fonts, s, style);
        if lay.glyphs.is_empty() && !style.strike {
            return lay.width;
        }
        let t = self.cur.t;
        if !is_st(&t) || t.sx <= 0.0 || t.sy <= 0.0 || (t.sx - t.sy).abs() > 1e-3 * t.sx {
            self.text_as_path(s, x, y, style);
            return lay.width;
        }
        let lh = (lay.asc + lay.desc).ceil();
        let bounds = to_irect_out(
            map_rect(
                &t,
                Rect::new(x - style.px * 0.3, y - style.px * 0.2, lay.width + style.px * 0.6, lh + style.px * 0.4),
            ),
            1.0,
        );
        let Some(m) = self.clip_for(bounds) else { return lay.width };
        let color = self.solid_premul(style.color);
        if color[3] == 0 {
            return lay.width;
        }
        self.touched = true;
        let clip = self.cur.clip;
        let (w, h) = (self.w, self.h);
        let size64 = (style.px * t.sx * 64.0).round().max(1.0) as u32;
        let base_x = t.tx + t.sx * x;
        let base_y = t.ty + t.sy * (y + lay.asc);
        let mask = m.as_ref().map(|m| m.data());
        for g in &lay.glyphs {
            let gx = base_x + t.sx * g.x;
            let (ix, px) = quarter(gx);
            let (iy, py) = quarter(base_y);
            // quick horizontal reject before touching the cache
            if ix > clip.right() + 4 || ix < clip.x - (style.px * t.sx * 2.0) as i32 {
                continue;
            }
            let (meta, _) = self.eng.glyphs.get(&mut self.eng.fonts, g.slot, g.gid, size64, px, py);
            if meta.w == 0 {
                continue;
            }
            let cov = self.eng.glyphs.bitmap(&meta);
            blit(
                self.pm.data_mut(),
                w,
                h,
                cov,
                meta.w as i32,
                meta.h as i32,
                ix + meta.left,
                iy + meta.top,
                clip,
                mask,
                color,
            );
        }
        if style.strike {
            if let Some(face) = self.eng.fonts.get(lay.primary) {
                let k = style.px / face.upem;
                let thick = (face.strike_size * k * t.sx).max(1.0);
                let ypos = base_y - face.strike_pos * k * t.sx;
                let x0 = base_x;
                let x1 = base_x + lay.width * t.sx;
                let y0 = ypos - thick * 0.5;
                let y1 = ypos + thick * 0.5;
                let rect = IRect::new(
                    x0.floor() as i32,
                    y0.floor() as i32,
                    (x1.ceil() - x0.floor()) as i32,
                    (y1.ceil() - y0.floor()) as i32,
                );
                let covx = |px: i32| -> u8 {
                    ((x1.min(px as f32 + 1.0) - x0.max(px as f32)).clamp(0.0, 1.0) * 255.0 + 0.5) as u8
                };
                let covy = |py: i32| -> u8 {
                    ((y1.min(py as f32 + 1.0) - y0.max(py as f32)).clamp(0.0, 1.0) * 255.0 + 0.5) as u8
                };
                blit_rect_cov(self.pm.data_mut(), w, h, rect, &covx, &covy, clip, mask, color);
            }
        }
        lay.width
    }

    fn measure(&mut self, s: &str, style: &TextStyle) -> f32 {
        self.eng.text.layout(&mut self.eng.fonts, s, style).width
    }

    fn line_height(&mut self, style: &TextStyle) -> f32 {
        let lay = self.eng.text.layout(&mut self.eng.fonts, "", style);
        (lay.asc + lay.desc).ceil()
    }

    fn elide(&mut self, s: &str, style: &TextStyle, max_w: f32) -> String {
        crate::text::elide(&mut self.eng.text, &mut self.eng.fonts, s, style, max_w)
    }
}

fn rect_path(b: IRect) -> Option<Path> {
    if b.is_empty() {
        // an empty box masks everything
        return PathBuilder::from_rect(tiny_skia::Rect::from_xywh(0.0, 0.0, 0.0001, 0.0001)?).into();
    }
    PathBuilder::from_rect(tiny_skia::Rect::from_xywh(b.x as f32, b.y as f32, b.w as f32, b.h as f32)?).into()
}

/// Splits a device coordinate into whole pixel + quarter-pixel phase (0..=3).
#[inline]
fn quarter(v: f32) -> (i32, u8) {
    let q = (v * 4.0).round() as i64;
    (q.div_euclid(4) as i32, q.rem_euclid(4) as u8)
}
