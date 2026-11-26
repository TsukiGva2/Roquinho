//! The drawing contract shared by every renderer/view, path helpers and `PixSurface`.
//!
//! Colours passed to a `Surface` are ordinary [`Color`]s (r=red); implementations convert with
//! [`Color::to_skia`], so the buffer ends up as X11 ARGB32 little-endian.

use crate::color::{swap_rb_in_place, Color};
use crate::geom::{Point, Rect};
use std::sync::Arc;
use tiny_skia::{Path, PathBuilder, Transform};

/// Font weight (Inter faces).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Weight {
    #[default]
    Regular,
    Medium,
    DemiBold,
    Bold,
}

/// Text appearance. `px` is the pixel size (Qt `font.pixelSize`).
#[derive(Clone, Debug, PartialEq)]
pub struct TextStyle {
    pub px: f32,
    pub weight: Weight,
    pub color: Color,
    /// Extra spacing after every glyph (`font.letterSpacing`), px.
    pub letter_spacing: f32,
    /// Tabular figures (`font.features tnum`) for clocks.
    pub tabular: bool,
    /// Strike-through (`font.strikeout`).
    pub strike: bool,
}

impl TextStyle {
    pub fn new(px: f32, weight: Weight, color: Color) -> TextStyle {
        TextStyle { px, weight, color, letter_spacing: 0.0, tabular: false, strike: false }
    }
    pub fn with_spacing(mut self, ls: f32) -> TextStyle {
        self.letter_spacing = ls;
        self
    }
    pub fn tabular(mut self) -> TextStyle {
        self.tabular = true;
        self
    }
    pub fn strike(mut self) -> TextStyle {
        self.strike = true;
        self
    }
}

/// Paint source: a solid colour or a gradient (in the current local coordinates).
#[derive(Clone, Debug, PartialEq)]
pub enum Fill {
    Solid(Color),
    /// Linear gradient from `p0` to `p1`; stops are `(offset 0..1, colour)`.
    Linear {
        p0: Point,
        p1: Point,
        stops: Vec<(f32, Color)>,
    },
    Radial {
        center: Point,
        radius: f32,
        stops: Vec<(f32, Color)>,
    },
}

impl From<Color> for Fill {
    fn from(c: Color) -> Fill {
        Fill::Solid(c)
    }
}

/// Everything a view may ask a renderer to do. Coordinates are logical pixels in the current
/// transform. Opacity multiplies into every later primitive (Qt `Item.opacity` without a group
/// layer), so overlapping primitives inside one item do show through each other.
pub trait Surface {
    /// Pushes transform, opacity and clip.
    fn save(&mut self);
    /// Pops what `save` pushed.
    fn restore(&mut self);
    fn translate(&mut self, dx: f32, dy: f32);
    fn scale(&mut self, sx: f32, sy: f32);
    fn rotate_deg(&mut self, deg: f32);
    /// Post-multiplies a local transform (`current = current * t`).
    fn concat(&mut self, t: Transform);
    /// Multiplies `a` into the opacity applied to every later primitive.
    fn mul_opacity(&mut self, a: f32);
    /// Intersects the clip with `r` (current transform; exact for translate/scale).
    fn clip_rect(&mut self, r: Rect);
    fn fill_path(&mut self, path: &Path, fill: &Fill);
    fn stroke_path(&mut self, path: &Path, fill: &Fill, stroke: &tiny_skia::Stroke);
    /// Rounded rectangle fill (`radius` clamped to half the short side).
    fn fill_rrect(&mut self, r: Rect, radius: f32, fill: &Fill) {
        if let Some(p) = path_rrect(r, radius) {
            self.fill_path(&p, fill);
        }
    }
    /// Rounded rectangle outline drawn centred on the rect edge.
    fn stroke_rrect(&mut self, r: Rect, radius: f32, fill: &Fill, width: f32) {
        if let Some(p) = path_rrect(r, radius) {
            let s = tiny_skia::Stroke { width, ..tiny_skia::Stroke::default() };
            self.stroke_path(&p, fill, &s);
        }
    }
    /// `RectangularShadow`: a blurred rounded rect of `r` grown by `spread` and moved by `offset`.
    fn shadow(&mut self, r: Rect, radius: f32, blur: f32, spread: f32, offset: Point, color: Color);
    /// Draws one line of text; `x`,`y` = top-left of Qt's line box. Returns the advance width.
    fn text(&mut self, s: &str, x: f32, y: f32, style: &TextStyle) -> f32;
    /// Advance width of `s`.
    fn measure(&mut self, s: &str, style: &TextStyle) -> f32;
    /// Line box height (ascent + descent, like `Text.implicitHeight`).
    fn line_height(&mut self, style: &TextStyle) -> f32;
    /// `Text.ElideRight`: shortens `s` with "…" so it fits `max_w`.
    fn elide(&mut self, s: &str, style: &TextStyle, max_w: f32) -> String;
    /// Runs `draw` (it paints in the current local coordinates, nothing outside `bounds`).
    /// `key` names the picture: two calls with the same key and bounds size must paint the same
    /// pixels, so a renderer may keep the rasterised result and blit it next time (the pet's
    /// poses repeat constantly). Without a cache this is just `draw(self)`.
    fn cached(&mut self, key: u64, bounds: Rect, draw: &mut dyn FnMut(&mut dyn Surface));
}

// ---------------------------------------------------------------- path helpers

const KAPPA: f32 = 0.552_284_8;

fn skia_rect(r: Rect) -> Option<tiny_skia::Rect> {
    tiny_skia::Rect::from_xywh(r.x, r.y, r.w, r.h)
}

/// Rectangle path.
pub fn path_rect(r: Rect) -> Option<Path> {
    PathBuilder::from_rect(skia_rect(r)?).into()
}

/// Rounded-rectangle path; `radius` is clamped to `min(w,h)/2`.
pub fn path_rrect(r: Rect, radius: f32) -> Option<Path> {
    if r.is_empty() {
        return None;
    }
    let rad = radius.clamp(0.0, r.w.min(r.h) * 0.5);
    if rad <= 0.0 {
        return path_rect(r);
    }
    let k = rad * (1.0 - KAPPA);
    let (x0, y0, x1, y1) = (r.x, r.y, r.right(), r.bottom());
    let mut pb = PathBuilder::new();
    pb.move_to(x0 + rad, y0);
    pb.line_to(x1 - rad, y0);
    pb.cubic_to(x1 - k, y0, x1, y0 + k, x1, y0 + rad);
    pb.line_to(x1, y1 - rad);
    pb.cubic_to(x1, y1 - k, x1 - k, y1, x1 - rad, y1);
    pb.line_to(x0 + rad, y1);
    pb.cubic_to(x0 + k, y1, x0, y1 - k, x0, y1 - rad);
    pb.line_to(x0, y0 + rad);
    pb.cubic_to(x0, y0 + k, x0 + k, y0, x0 + rad, y0);
    pb.close();
    pb.finish()
}

/// Circle path (clockwise on screen).
pub fn path_circle(cx: f32, cy: f32, r: f32) -> Option<Path> {
    path_ellipse(cx, cy, r, r)
}

/// Ellipse path.
pub fn path_ellipse(cx: f32, cy: f32, rx: f32, ry: f32) -> Option<Path> {
    if !(rx > 0.0 && ry > 0.0) {
        return None;
    }
    let mut pb = PathBuilder::new();
    pb.move_to(cx + rx, cy);
    arc_cubics(&mut pb, cx, cy, rx, ry, 0.0, 0.0, std::f32::consts::TAU);
    pb.close();
    pb.finish()
}

/// `PathAngleArc`: open arc of a circle. Angles in degrees; 0 is 3 o'clock and a positive
/// sweep runs clockwise on screen (y down). Starts with a move-to at the arc start.
pub fn path_arc(cx: f32, cy: f32, r: f32, start_deg: f32, sweep_deg: f32) -> Option<Path> {
    if r.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
        return None;
    }
    let a0 = start_deg.to_radians();
    let mut pb = PathBuilder::new();
    pb.move_to(cx + r * a0.cos(), cy + r * a0.sin());
    arc_cubics(&mut pb, cx, cy, r, r, 0.0, a0, sweep_deg.to_radians());
    pb.finish()
}

/// Appends an elliptical arc (centre parameterisation, angles in radians, `phi` = x-axis
/// rotation) as cubics of at most 90 degrees each. The pen must already be at the start point.
#[allow(clippy::too_many_arguments)]
fn arc_cubics(pb: &mut PathBuilder, cx: f32, cy: f32, rx: f32, ry: f32, phi: f32, th1: f32, dth: f32) {
    let n = ((dth.abs() / std::f32::consts::FRAC_PI_2).ceil() as usize).max(1);
    let d = dth / n as f32;
    let t = 4.0 / 3.0 * (d / 4.0).tan();
    let (sp, cp) = phi.sin_cos();
    let pt = |a: f32| {
        let (s, c) = a.sin_cos();
        (cx + cp * rx * c - sp * ry * s, cy + sp * rx * c + cp * ry * s)
    };
    let dv = |a: f32| {
        let (s, c) = a.sin_cos();
        (-cp * rx * s - sp * ry * c, -sp * rx * s + cp * ry * c)
    };
    for i in 0..n {
        let a1 = th1 + d * i as f32;
        let a2 = a1 + d;
        let (p1, p2) = (pt(a1), pt(a2));
        let (d1, d2) = (dv(a1), dv(a2));
        pb.cubic_to(p1.0 + t * d1.0, p1.1 + t * d1.1, p2.0 - t * d2.0, p2.1 - t * d2.1, p2.0, p2.1);
    }
}

/// SVG arc to endpoint (spec F.6.5) appended as cubics.
#[allow(clippy::too_many_arguments)]
fn svg_arc(
    pb: &mut PathBuilder,
    x0: f32,
    y0: f32,
    mut rx: f32,
    mut ry: f32,
    rot_deg: f32,
    large: bool,
    sweep: bool,
    x: f32,
    y: f32,
) {
    if (x0 == x && y0 == y) || rx == 0.0 || ry == 0.0 {
        if x0 != x || y0 != y {
            pb.line_to(x, y);
        }
        return;
    }
    rx = rx.abs();
    ry = ry.abs();
    let phi = rot_deg.to_radians();
    let (sp, cp) = phi.sin_cos();
    let dx2 = (x0 - x) * 0.5;
    let dy2 = (y0 - y) * 0.5;
    let x1p = cp * dx2 + sp * dy2;
    let y1p = -sp * dx2 + cp * dy2;
    let lam = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
    if lam > 1.0 {
        let s = lam.sqrt();
        rx *= s;
        ry *= s;
    }
    let num = rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p;
    let den = rx * rx * y1p * y1p + ry * ry * x1p * x1p;
    let mut co = if den == 0.0 { 0.0 } else { (num / den).max(0.0).sqrt() };
    if large == sweep {
        co = -co;
    }
    let cxp = co * rx * y1p / ry;
    let cyp = -co * ry * x1p / rx;
    let cx = cp * cxp - sp * cyp + (x0 + x) * 0.5;
    let cy = sp * cxp + cp * cyp + (y0 + y) * 0.5;
    let ang = |ux: f32, uy: f32, vx: f32, vy: f32| {
        let dot = ux * vx + uy * vy;
        let len = (ux * ux + uy * uy).sqrt() * (vx * vx + vy * vy).sqrt();
        let mut a = (dot / len).clamp(-1.0, 1.0).acos();
        if ux * vy - uy * vx < 0.0 {
            a = -a;
        }
        a
    };
    let th1 = ang(1.0, 0.0, (x1p - cxp) / rx, (y1p - cyp) / ry);
    let mut dth = ang((x1p - cxp) / rx, (y1p - cyp) / ry, (-x1p - cxp) / rx, (-y1p - cyp) / ry);
    if !sweep && dth > 0.0 {
        dth -= std::f32::consts::TAU;
    } else if sweep && dth < 0.0 {
        dth += std::f32::consts::TAU;
    }
    arc_cubics(pb, cx, cy, rx, ry, phi, th1, dth);
}

struct Scan<'a> {
    s: &'a [u8],
    i: usize,
}

impl Scan<'_> {
    fn skip_sep(&mut self) {
        while self.i < self.s.len() && (self.s[self.i].is_ascii_whitespace() || self.s[self.i] == b',') {
            self.i += 1;
        }
    }
    fn peek(&mut self) -> Option<u8> {
        self.skip_sep();
        self.s.get(self.i).copied()
    }
    fn number(&mut self) -> Option<f32> {
        self.skip_sep();
        let st = self.i;
        let s = self.s;
        let mut i = self.i;
        if i < s.len() && (s[i] == b'+' || s[i] == b'-') {
            i += 1;
        }
        let mut digits = 0;
        while i < s.len() && s[i].is_ascii_digit() {
            i += 1;
            digits += 1;
        }
        if i < s.len() && s[i] == b'.' {
            i += 1;
            while i < s.len() && s[i].is_ascii_digit() {
                i += 1;
                digits += 1;
            }
        }
        if digits == 0 {
            return None;
        }
        if i < s.len() && (s[i] == b'e' || s[i] == b'E') {
            let mut j = i + 1;
            if j < s.len() && (s[j] == b'+' || s[j] == b'-') {
                j += 1;
            }
            if j < s.len() && s[j].is_ascii_digit() {
                while j < s.len() && s[j].is_ascii_digit() {
                    j += 1;
                }
                i = j;
            }
        }
        self.i = i;
        std::str::from_utf8(&s[st..i]).ok()?.parse().ok()
    }
    fn flag(&mut self) -> Option<bool> {
        self.skip_sep();
        let c = *self.s.get(self.i)?;
        match c {
            b'0' | b'1' => {
                self.i += 1;
                Some(c == b'1')
            }
            _ => None,
        }
    }
}

/// Parses an SVG path `d` string (M m L l H h V v C c S s Q q T t A a Z z; arcs become cubics).
/// Returns `None` for malformed or empty data.
pub fn path_from_svg(d: &str) -> Option<Path> {
    let mut sc = Scan { s: d.as_bytes(), i: 0 };
    let mut pb = PathBuilder::new();
    let (mut cx, mut cy) = (0f32, 0f32);
    let (mut sx, mut sy) = (0f32, 0f32);
    // last control point for smooth curves: (x, y, kind) kind 1 = cubic, 2 = quad
    let mut last_ctl: Option<(f32, f32, u8)> = None;
    let mut cmd = 0u8;
    let mut first = true;
    while let Some(c) = sc.peek() {
        if c.is_ascii_alphabetic() {
            cmd = c;
            sc.i += 1;
            if first && cmd != b'M' && cmd != b'm' {
                return None;
            }
            if cmd == b'Z' || cmd == b'z' {
                pb.close();
                cx = sx;
                cy = sy;
                last_ctl = None;
                continue;
            }
        } else if cmd == 0 || cmd == b'Z' || cmd == b'z' {
            return None;
        } else if cmd == b'M' {
            cmd = b'L';
        } else if cmd == b'm' {
            cmd = b'l';
        }
        first = false;
        let rel = cmd.is_ascii_lowercase();
        let (ox, oy) = if rel { (cx, cy) } else { (0.0, 0.0) };
        match cmd.to_ascii_uppercase() {
            b'M' => {
                let (x, y) = (sc.number()? + ox, sc.number()? + oy);
                pb.move_to(x, y);
                cx = x;
                cy = y;
                sx = x;
                sy = y;
                last_ctl = None;
            }
            b'L' => {
                let (x, y) = (sc.number()? + ox, sc.number()? + oy);
                pb.line_to(x, y);
                cx = x;
                cy = y;
                last_ctl = None;
            }
            b'H' => {
                let x = sc.number()? + ox;
                pb.line_to(x, cy);
                cx = x;
                last_ctl = None;
            }
            b'V' => {
                let y = sc.number()? + oy;
                pb.line_to(cx, y);
                cy = y;
                last_ctl = None;
            }
            b'C' => {
                let (x1, y1) = (sc.number()? + ox, sc.number()? + oy);
                let (x2, y2) = (sc.number()? + ox, sc.number()? + oy);
                let (x, y) = (sc.number()? + ox, sc.number()? + oy);
                pb.cubic_to(x1, y1, x2, y2, x, y);
                last_ctl = Some((x2, y2, 1));
                cx = x;
                cy = y;
            }
            b'S' => {
                let (x1, y1) = match last_ctl {
                    Some((lx, ly, 1)) => (2.0 * cx - lx, 2.0 * cy - ly),
                    _ => (cx, cy),
                };
                let (x2, y2) = (sc.number()? + ox, sc.number()? + oy);
                let (x, y) = (sc.number()? + ox, sc.number()? + oy);
                pb.cubic_to(x1, y1, x2, y2, x, y);
                last_ctl = Some((x2, y2, 1));
                cx = x;
                cy = y;
            }
            b'Q' => {
                let (x1, y1) = (sc.number()? + ox, sc.number()? + oy);
                let (x, y) = (sc.number()? + ox, sc.number()? + oy);
                pb.quad_to(x1, y1, x, y);
                last_ctl = Some((x1, y1, 2));
                cx = x;
                cy = y;
            }
            b'T' => {
                let (x1, y1) = match last_ctl {
                    Some((lx, ly, 2)) => (2.0 * cx - lx, 2.0 * cy - ly),
                    _ => (cx, cy),
                };
                let (x, y) = (sc.number()? + ox, sc.number()? + oy);
                pb.quad_to(x1, y1, x, y);
                last_ctl = Some((x1, y1, 2));
                cx = x;
                cy = y;
            }
            b'A' => {
                let rx = sc.number()?;
                let ry = sc.number()?;
                let rot = sc.number()?;
                let large = sc.flag()?;
                let sweep = sc.flag()?;
                let (x, y) = (sc.number()? + ox, sc.number()? + oy);
                svg_arc(&mut pb, cx, cy, rx, ry, rot, large, sweep, x, y);
                cx = x;
                cy = y;
                last_ctl = None;
            }
            _ => return None,
        }
    }
    pb.finish()
}

// ---------------------------------------------------------------- PixSurface

#[derive(Clone)]
struct State {
    transform: Transform,
    opacity: f32,
    clip: Option<Arc<tiny_skia::Mask>>,
}

/// A working [`Surface`] over an owned `tiny_skia::Pixmap` (premultiplied; byte order B,G,R,A
/// as seen by X11, which tiny-skia calls RGBA). Text is a placeholder: a filled rectangle of
/// `0.55 * px` per char wide and `px` tall. Used by tests and snapshots.
pub struct PixSurface {
    pm: tiny_skia::Pixmap,
    cur: State,
    stack: Vec<State>,
}

impl PixSurface {
    /// New transparent surface; `None` if a dimension is zero.
    pub fn new(w: u32, h: u32) -> Option<PixSurface> {
        Some(PixSurface::from_pixmap(tiny_skia::Pixmap::new(w, h)?))
    }
    pub fn from_pixmap(pm: tiny_skia::Pixmap) -> PixSurface {
        PixSurface { pm, cur: State { transform: Transform::identity(), opacity: 1.0, clip: None }, stack: Vec::new() }
    }
    pub fn width(&self) -> u32 {
        self.pm.width()
    }
    pub fn height(&self) -> u32 {
        self.pm.height()
    }
    pub fn pixmap(&self) -> &tiny_skia::Pixmap {
        &self.pm
    }
    pub fn pixmap_mut(&mut self) -> &mut tiny_skia::Pixmap {
        &mut self.pm
    }
    pub fn into_pixmap(self) -> tiny_skia::Pixmap {
        self.pm
    }
    /// Raw premultiplied bytes (B,G,R,A order for X11).
    pub fn data(&self) -> &[u8] {
        self.pm.data()
    }
    /// Raw premultiplied pixel `[B,G,R,A]`; `[0;4]` outside the surface.
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        if x >= self.pm.width() || y >= self.pm.height() {
            return [0; 4];
        }
        let i = ((y * self.pm.width() + x) * 4) as usize;
        let d = self.pm.data();
        [d[i], d[i + 1], d[i + 2], d[i + 3]]
    }
    /// Clears to a colour (transform, clip and opacity are ignored).
    pub fn clear(&mut self, c: Color) {
        self.pm.fill(c.to_skia());
    }
    /// Writes the surface as a PNG (R/B swapped back to real RGBA).
    #[cfg(feature = "png")]
    pub fn save_png(&self, path: impl AsRef<std::path::Path>) -> Result<(), String> {
        let mut copy = self.pm.clone();
        swap_rb_in_place(copy.data_mut());
        copy.save_png(path).map_err(|e| e.to_string())
    }
    /// The pixels as real RGBA (R/B swapped), still premultiplied.
    pub fn to_rgba_premul(&self) -> Vec<u8> {
        let mut v = self.pm.data().to_vec();
        swap_rb_in_place(&mut v);
        v
    }

    fn paint(&self, fill: &Fill) -> Option<tiny_skia::Paint<'static>> {
        let op = self.cur.opacity;
        let mut paint = tiny_skia::Paint { anti_alias: true, ..tiny_skia::Paint::default() };
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
}

impl Surface for PixSurface {
    fn cached(&mut self, _key: u64, _bounds: Rect, draw: &mut dyn FnMut(&mut dyn Surface)) {
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
        self.cur.transform = self.cur.transform.pre_translate(dx, dy);
    }
    fn scale(&mut self, sx: f32, sy: f32) {
        self.cur.transform = self.cur.transform.pre_scale(sx, sy);
    }
    fn rotate_deg(&mut self, deg: f32) {
        self.cur.transform = self.cur.transform.pre_concat(Transform::from_rotate(deg));
    }
    fn concat(&mut self, t: Transform) {
        self.cur.transform = self.cur.transform.pre_concat(t);
    }
    fn mul_opacity(&mut self, a: f32) {
        self.cur.opacity *= a.clamp(0.0, 1.0);
    }
    fn clip_rect(&mut self, r: Rect) {
        let Some(path) = path_rect(r) else {
            // empty clip: everything is clipped away
            if let Some(m) = tiny_skia::Mask::new(self.pm.width(), self.pm.height()) {
                self.cur.clip = Some(Arc::new(m));
            }
            return;
        };
        let t = self.cur.transform;
        match &mut self.cur.clip {
            Some(m) => Arc::make_mut(m).intersect_path(&path, tiny_skia::FillRule::Winding, true, t),
            None => {
                if let Some(mut m) = tiny_skia::Mask::new(self.pm.width(), self.pm.height()) {
                    m.fill_path(&path, tiny_skia::FillRule::Winding, true, t);
                    self.cur.clip = Some(Arc::new(m));
                }
            }
        }
    }
    fn fill_path(&mut self, path: &Path, fill: &Fill) {
        if let Some(p) = self.paint(fill) {
            self.pm
                .fill_path(path, &p, tiny_skia::FillRule::Winding, self.cur.transform, self.cur.clip.as_deref());
        }
    }
    fn stroke_path(&mut self, path: &Path, fill: &Fill, stroke: &tiny_skia::Stroke) {
        if let Some(p) = self.paint(fill) {
            self.pm
                .stroke_path(path, &p, stroke, self.cur.transform, self.cur.clip.as_deref());
        }
    }
    fn shadow(&mut self, r: Rect, radius: f32, blur: f32, spread: f32, offset: Point, color: Color) {
        // Approximation: stacked rounded rects, grown from -blur/2 to +blur/2, alpha chosen so the centre composites to `color.a`.
        const N: usize = 10;
        let base = r.translate(offset.x, offset.y).inflate(spread, spread);
        // per-layer alpha such that N stacked layers composite to `color.a` at the centre
        let c = color.with_alpha(1.0 - (1.0 - color.a.clamp(0.0, 1.0)).powf(1.0 / N as f32));
        for i in 0..N {
            let e = blur * (i as f32 / (N - 1) as f32 - 0.5);
            let rr = base.inflate(e, e);
            if let Some(p) = path_rrect(rr, (radius + spread + e).max(0.0)) {
                self.fill_path(&p, &Fill::Solid(c));
            }
        }
    }
    fn text(&mut self, s: &str, x: f32, y: f32, style: &TextStyle) -> f32 {
        let w = self.measure(s, style);
        if let (Some(r), Some(p)) =
            (skia_rect(Rect::new(x, y, w.max(0.01), style.px)), self.paint(&Fill::Solid(style.color)))
        {
            self.pm.fill_rect(r, &p, self.cur.transform, self.cur.clip.as_deref());
        }
        w
    }
    fn measure(&mut self, s: &str, style: &TextStyle) -> f32 {
        s.chars().count() as f32 * (0.55 * style.px + style.letter_spacing)
    }
    fn line_height(&mut self, style: &TextStyle) -> f32 {
        style.px * 1.2
    }
    fn elide(&mut self, s: &str, style: &TextStyle, max_w: f32) -> String {
        if self.measure(s, style) <= max_w {
            return s.to_string();
        }
        let chars: Vec<char> = s.chars().collect();
        let mut n = chars.len();
        while n > 0 {
            n -= 1;
            let cand: String = chars[..n].iter().collect::<String>() + "…";
            if self.measure(&cand, style) <= max_w {
                return cand;
            }
        }
        String::from("…")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds(p: &Path) -> (f32, f32, f32, f32) {
        let b = p.bounds();
        (b.left(), b.top(), b.right(), b.bottom())
    }

    #[test]
    fn svg_basic_shapes() {
        let p = path_from_svg("M10 10 L20 10 L20 20 Z").unwrap();
        assert_eq!(bounds(&p), (10.0, 10.0, 20.0, 20.0));
        let p = path_from_svg("m1,1 h10 v5 H0 V0 z").unwrap();
        assert_eq!(bounds(&p), (0.0, 0.0, 11.0, 6.0));
        // implicit lineto after moveto, compact numbers
        let p = path_from_svg("M0 0 10 0 10 10").unwrap();
        assert_eq!(bounds(&p), (0.0, 0.0, 10.0, 10.0));
        let p = path_from_svg("M0,0L1.5.5-2-3").unwrap();
        assert_eq!(bounds(&p), (-2.0, -3.0, 1.5, 0.5));
        assert!(path_from_svg("").is_none());
        assert!(path_from_svg("L 1 1").is_none());
        assert!(path_from_svg("M 1").is_none());
        assert!(path_from_svg("M0 0 X 1 1").is_none());
    }

    #[test]
    fn svg_curves_and_smooth() {
        let p = path_from_svg("M0 0 C 0 10 10 10 10 0 S 20 -10 20 0").unwrap();
        let b = bounds(&p);
        assert_eq!((b.0, b.2), (0.0, 20.0));
        let p = path_from_svg("M0 0 Q 5 10 10 0 T 20 0").unwrap();
        assert_eq!(bounds(&p).2, 20.0);
        // relative variants
        let p = path_from_svg("m0 0 c0 5 5 5 5 0 s5 -5 5 0 q2 4 4 0 t4 0").unwrap();
        assert!((bounds(&p).2 - 18.0).abs() < 1e-4);
    }

    #[test]
    fn svg_arcs() {
        // sweep-flag 1 = clockwise on screen (y down): left->right goes over the top (y<0)
        let p = path_from_svg("M0 0 A5 5 0 0 1 10 0").unwrap();
        let b = bounds(&p);
        assert!((b.2 - 10.0).abs() < 1e-3 && b.0.abs() < 1e-3);
        assert!((b.1 + 5.0).abs() < 0.05 && b.3.abs() < 0.05, "{b:?}");
        // other sweep goes under
        let p = path_from_svg("M0 0 A5 5 0 0 0 10 0").unwrap();
        assert!((bounds(&p).3 - 5.0).abs() < 0.05);
        // packed flags, radii auto-scaled, zero radius = line
        assert!(path_from_svg("M0 0a1 1 0 0110 0").is_some());
        let p = path_from_svg("M0 0 A0 5 0 0 1 10 0").unwrap();
        assert_eq!(bounds(&p), (0.0, 0.0, 10.0, 0.0));
    }

    #[test]
    fn helper_paths() {
        let p = path_rrect(Rect::new(0.0, 0.0, 100.0, 40.0), 999.0).unwrap();
        assert_eq!(bounds(&p), (0.0, 0.0, 100.0, 40.0));
        assert!(path_rrect(Rect::new(0.0, 0.0, 0.0, 40.0), 3.0).is_none());
        let c = path_circle(10.0, 10.0, 5.0).unwrap();
        let b = bounds(&c);
        assert!((b.0 - 5.0).abs() < 1e-3 && (b.2 - 15.0).abs() < 1e-3 && (b.1 - 5.0).abs() < 1e-3);
        // quarter arc from 3 o'clock clockwise ends at 6 o'clock (y down)
        let a = path_arc(0.0, 0.0, 10.0, 0.0, 90.0).unwrap();
        let b = bounds(&a);
        assert!((b.2 - 10.0).abs() < 1e-3 && (b.3 - 10.0).abs() < 1e-3 && b.0.abs() < 1e-3 && b.1.abs() < 1e-3);
        let a = path_arc(0.0, 0.0, 10.0, 0.0, -90.0).unwrap();
        assert!((bounds(&a).1 + 10.0).abs() < 1e-3);
        assert!(path_arc(0.0, 0.0, 0.0, 0.0, 90.0).is_none());
    }

    #[test]
    fn pix_surface_smoke() {
        let mut s = PixSurface::new(40, 40).unwrap();
        // red fill: X11 buffer bytes are B,G,R,A -> [0,0,255,255]
        s.fill_rrect(Rect::new(0.0, 0.0, 20.0, 20.0), 0.0, &Fill::Solid(Color::rgb8(255, 0, 0)));
        assert_eq!(s.pixel(5, 5), [0, 0, 255, 255]);
        assert_eq!(s.pixel(30, 30), [0; 4]);
        // transforms + opacity
        s.save();
        s.translate(20.0, 20.0);
        s.mul_opacity(0.5);
        s.fill_rrect(Rect::new(0.0, 0.0, 10.0, 10.0), 0.0, &Fill::Solid(Color::rgb8(0, 0, 255)));
        s.restore();
        let px = s.pixel(25, 25);
        assert!(px[0] >= 126 && px[0] <= 129 && px[3] >= 126 && px[3] <= 129, "{px:?}");
        // clip
        s.save();
        s.clip_rect(Rect::new(0.0, 30.0, 5.0, 5.0));
        s.fill_rrect(Rect::new(0.0, 0.0, 40.0, 40.0), 0.0, &Fill::Solid(Color::WHITE));
        s.restore();
        assert_eq!(s.pixel(2, 32), [255; 4]);
        assert_eq!(s.pixel(10, 32), [0; 4]);
        // clips nest and restore
        s.save();
        s.clip_rect(Rect::new(0.0, 0.0, 10.0, 10.0));
        s.clip_rect(Rect::new(5.0, 5.0, 10.0, 10.0));
        s.fill_rrect(Rect::new(0.0, 0.0, 40.0, 40.0), 0.0, &Fill::Solid(Color::rgb8(0, 255, 0)));
        s.restore();
        assert_eq!(s.pixel(7, 7), [0, 255, 0, 255]);
        assert_ne!(s.pixel(12, 12), [0, 255, 0, 255]);
        // gradients
        let mut g = PixSurface::new(10, 1).unwrap();
        g.fill_rrect(
            Rect::new(0.0, 0.0, 10.0, 1.0),
            0.0,
            &Fill::Linear {
                p0: Point::new(0.0, 0.0),
                p1: Point::new(10.0, 0.0),
                stops: vec![(0.0, Color::BLACK), (1.0, Color::WHITE)],
            },
        );
        assert!(g.pixel(1, 0)[0] < g.pixel(8, 0)[0]);
        let mut r = PixSurface::new(20, 20).unwrap();
        r.fill_rrect(
            Rect::new(0.0, 0.0, 20.0, 20.0),
            0.0,
            &Fill::Radial {
                center: Point::new(10.0, 10.0),
                radius: 10.0,
                stops: vec![(0.0, Color::WHITE), (1.0, Color::BLACK)],
            },
        );
        assert!(r.pixel(10, 10)[0] > r.pixel(1, 10)[0]);
    }

    #[test]
    fn pix_surface_text_shadow_stroke() {
        let mut s = PixSurface::new(100, 60).unwrap();
        let st = TextStyle::new(10.0, Weight::Medium, Color::WHITE);
        let w = s.text("abcd", 5.0, 5.0, &st);
        assert!((w - 22.0).abs() < 1e-4);
        assert_eq!(s.pixel(10, 8), [255; 4]);
        assert_eq!(s.pixel(40, 8), [0; 4]);
        assert!((s.line_height(&st) - 12.0).abs() < 1e-4);
        assert_eq!(s.elide("abcdefghij", &st, 20.0), "ab…");
        assert_eq!(s.elide("ab", &st, 25.0), "ab");
        s.shadow(Rect::new(30.0, 30.0, 40.0, 20.0), 6.0, 10.0, 0.0, Point::new(0.0, 2.0), Color::BLACK.with_alpha(0.8));
        assert!(s.pixel(50, 40)[3] > 150);
        assert!(s.pixel(50, 56)[3] < s.pixel(50, 40)[3]);
        s.stroke_rrect(Rect::new(1.0, 20.0, 20.0, 20.0), 4.0, &Fill::Solid(Color::rgb8(0, 255, 0)), 2.0);
        assert!(s.pixel(11, 20)[3] > 200);
        s.clear(Color::BLACK);
        assert_eq!(s.pixel(0, 0), [0, 0, 0, 255]);
        assert_eq!(s.to_rgba_premul().len(), 100 * 60 * 4);
        assert!(PixSurface::new(0, 5).is_none());
    }

    #[cfg(feature = "png")]
    #[test]
    fn png_writes() {
        let mut s = PixSurface::new(8, 8).unwrap();
        s.clear(Color::rgb8(255, 0, 0));
        let p = std::env::temp_dir().join("foco-base-test.png");
        s.save_png(&p).unwrap();
        let bytes = std::fs::read(&p).unwrap();
        assert_eq!(&bytes[1..4], b"PNG");
        let _ = std::fs::remove_file(&p);
    }
}
