//! Glyph coverage cache: unhinted grayscale bitmaps keyed by (face, glyph, size in 1/64 px,
//! sub-pixel phase in 1/4 px on both axes), plus the premultiplied source-over blitter.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::sync::OnceLock;

use ab_glyph_rasterizer::{point, Rasterizer};
use foco_base::IRect;
use ttf_parser as ttf;

use crate::fonts::{Face, Fonts, Slot};

/// Identity-ish hasher for pre-mixed u64 keys.
#[derive(Default)]
pub struct KeyHasher(u64);

impl Hasher for KeyHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 ^ b as u64).wrapping_mul(0x100000001b3);
        }
    }
    fn write_u64(&mut self, v: u64) {
        self.0 = v.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        self.0 ^= self.0 >> 29;
    }
}

pub type FastMap<K, V> = HashMap<K, V, BuildHasherDefault<KeyHasher>>;

#[derive(Clone, Copy, Default)]
pub struct GlyphMeta {
    /// Bitmap origin relative to the glyph origin (pixels; `top` is negative above the baseline).
    pub left: i32,
    pub top: i32,
    pub w: u32,
    pub h: u32,
    off: u32,
}

/// Coverage correction: Qt's distance-field text looks slightly heavier than raw box coverage.
fn lut() -> &'static [u8; 256] {
    static L: OnceLock<[u8; 256]> = OnceLock::new();
    L.get_or_init(|| {
        let mut t = [0u8; 256];
        for (i, v) in t.iter_mut().enumerate() {
            let c = i as f32 / 255.0;
            *v = (c.powf(crate::COVERAGE_GAMMA) * 255.0 + 0.5) as u8;
        }
        t
    })
}

#[derive(Default)]
pub struct GlyphCache {
    map: FastMap<u64, u32>,
    metas: Vec<GlyphMeta>,
    data: Vec<u8>,
    /// Number of rasterisations performed (diagnostics / tests).
    pub misses: u64,
}

const MAX_DATA: usize = 24 << 20;

impl GlyphCache {
    pub fn new() -> GlyphCache {
        GlyphCache::default()
    }

    pub fn clear(&mut self) {
        self.map.clear();
        self.metas.clear();
        self.data.clear();
    }

    /// Cached glyph (rasterised on first use). `size64` = pixel size * 64, `px`/`py` = phase in quarters.
    pub fn get(&mut self, fonts: &mut Fonts, slot: Slot, gid: u16, size64: u32, px: u8, py: u8) -> (GlyphMeta, u32) {
        let key =
            (slot as u64) << 40 | (gid as u64) << 24 | (size64.min(0xFFFF) as u64) << 8 | (px as u64) << 4 | py as u64;
        if let Some(&i) = self.map.get(&key) {
            return (self.metas[i as usize], i);
        }
        if self.data.len() > MAX_DATA {
            self.clear();
        }
        self.misses += 1;
        let meta = match fonts.get(slot) {
            Some(face) => self.raster(face, gid, size64 as f32 / 64.0, px as f32 * 0.25, py as f32 * 0.25),
            None => GlyphMeta::default(),
        };
        let i = self.metas.len() as u32;
        self.metas.push(meta);
        self.map.insert(key, i);
        (meta, i)
    }

    pub fn bitmap(&self, m: &GlyphMeta) -> &[u8] {
        &self.data[m.off as usize..m.off as usize + (m.w * m.h) as usize]
    }

    fn raster(&mut self, face: &Face, gid: u16, size: f32, fx: f32, fy: f32) -> GlyphMeta {
        let id = ttf::GlyphId(gid);
        let Some(bb) = face.face.glyph_bounding_box(id) else { return GlyphMeta::default() };
        let scale = size / face.upem;
        let x0 = (bb.x_min as f32 * scale + fx).floor() as i32 - 1;
        let x1 = (bb.x_max as f32 * scale + fx).ceil() as i32 + 1;
        let top = (-(bb.y_max as f32) * scale + fy).floor() as i32 - 1;
        let bot = (-(bb.y_min as f32) * scale + fy).ceil() as i32 + 1;
        let (w, h) = ((x1 - x0).max(1) as usize, (bot - top).max(1) as usize);
        if w > 2048 || h > 2048 {
            return GlyphMeta::default();
        }
        let mut b = Builder {
            r: Rasterizer::new(w, h),
            scale,
            ox: fx - x0 as f32,
            oy: fy - top as f32,
            last: (0.0, 0.0),
            start: (0.0, 0.0),
            open: false,
        };
        face.face.outline_glyph(id, &mut b);
        b.finish();
        let off = self.data.len();
        self.data.resize(off + w * h, 0);
        let dst = &mut self.data[off..];
        let l = lut();
        b.r.for_each_pixel(|i, a| {
            dst[i] = l[(a.clamp(0.0, 1.0) * 255.0 + 0.5) as usize];
        });
        GlyphMeta { left: x0, top, w: w as u32, h: h as u32, off: off as u32 }
    }
}

struct Builder {
    r: Rasterizer,
    scale: f32,
    ox: f32,
    oy: f32,
    last: (f32, f32),
    start: (f32, f32),
    open: bool,
}

impl Builder {
    #[inline]
    fn p(&self, x: f32, y: f32) -> (f32, f32) {
        (x * self.scale + self.ox, -y * self.scale + self.oy)
    }
    fn finish(&mut self) {
        if self.open {
            self.r
                .draw_line(point(self.last.0, self.last.1), point(self.start.0, self.start.1));
            self.open = false;
        }
    }
}

impl ttf::OutlineBuilder for Builder {
    fn move_to(&mut self, x: f32, y: f32) {
        self.finish();
        let p = self.p(x, y);
        self.last = p;
        self.start = p;
        self.open = true;
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.p(x, y);
        self.r.draw_line(point(self.last.0, self.last.1), point(p.0, p.1));
        self.last = p;
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (c, p) = (self.p(x1, y1), self.p(x, y));
        self.r
            .draw_quad(point(self.last.0, self.last.1), point(c.0, c.1), point(p.0, p.1));
        self.last = p;
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let (c1, c2, p) = (self.p(x1, y1), self.p(x2, y2), self.p(x, y));
        self.r
            .draw_cubic(point(self.last.0, self.last.1), point(c1.0, c1.1), point(c2.0, c2.1), point(p.0, p.1));
        self.last = p;
    }
    fn close(&mut self) {
        self.finish();
    }
}

#[inline(always)]
fn div255(x: u32) -> u32 {
    (x + 128 + ((x + 128) >> 8)) >> 8
}

/// Premultiplied source-over of a coverage bitmap tinted with `color` (premultiplied bytes in
/// the destination's byte order). `dst` is the whole pixmap (`dw` x `dh`); `(gx, gy)` is the
/// bitmap's top-left in pixmap coordinates; the result is limited to `clip` and, when given,
/// scaled by `mask` (one byte per pixmap pixel).
#[allow(clippy::too_many_arguments)]
pub fn blit(
    dst: &mut [u8],
    dw: i32,
    dh: i32,
    cov: &[u8],
    gw: i32,
    gh: i32,
    gx: i32,
    gy: i32,
    clip: IRect,
    mask: Option<&[u8]>,
    color: [u8; 4],
) {
    let x0 = gx.max(clip.x).max(0);
    let y0 = gy.max(clip.y).max(0);
    let x1 = (gx + gw).min(clip.right()).min(dw);
    let y1 = (gy + gh).min(clip.bottom()).min(dh);
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let [cb, cg, cr, ca] = color.map(|v| v as u32);
    for y in y0..y1 {
        let crow = &cov[((y - gy) * gw + (x0 - gx)) as usize..((y - gy) * gw + (x1 - gx)) as usize];
        let drow = &mut dst[((y * dw + x0) * 4) as usize..((y * dw + x1) * 4) as usize];
        match mask {
            None => {
                for (c, d) in crow.iter().zip(drow.chunks_exact_mut(4)) {
                    let c = *c as u32;
                    if c == 0 {
                        continue;
                    }
                    let sa = div255(ca * c);
                    let inv = 255 - sa;
                    d[0] = (div255(cb * c) + div255(d[0] as u32 * inv)).min(255) as u8;
                    d[1] = (div255(cg * c) + div255(d[1] as u32 * inv)).min(255) as u8;
                    d[2] = (div255(cr * c) + div255(d[2] as u32 * inv)).min(255) as u8;
                    d[3] = (sa + div255(d[3] as u32 * inv)).min(255) as u8;
                }
            }
            Some(m) => {
                let mrow = &m[(y * dw + x0) as usize..(y * dw + x1) as usize];
                for ((c, mk), d) in crow.iter().zip(mrow).zip(drow.chunks_exact_mut(4)) {
                    let c = div255(*c as u32 * *mk as u32);
                    if c == 0 {
                        continue;
                    }
                    let sa = div255(ca * c);
                    let inv = 255 - sa;
                    d[0] = (div255(cb * c) + div255(d[0] as u32 * inv)).min(255) as u8;
                    d[1] = (div255(cg * c) + div255(d[1] as u32 * inv)).min(255) as u8;
                    d[2] = (div255(cr * c) + div255(d[2] as u32 * inv)).min(255) as u8;
                    d[3] = (sa + div255(d[3] as u32 * inv)).min(255) as u8;
                }
            }
        }
    }
}

/// Fills a solid rectangle (used for strike-through lines) with the same blend as `blit`.
#[allow(clippy::too_many_arguments)]
pub fn blit_rect_cov(
    dst: &mut [u8],
    dw: i32,
    dh: i32,
    rect: IRect,
    covx: &dyn Fn(i32) -> u8,
    covy: &dyn Fn(i32) -> u8,
    clip: IRect,
    mask: Option<&[u8]>,
    color: [u8; 4],
) {
    let Some(r) = rect.intersect(&clip) else { return };
    let Some(r) = r.intersect(&IRect::new(0, 0, dw, dh)) else { return };
    let [cb, cg, cr, ca] = color.map(|v| v as u32);
    for y in r.y..r.bottom() {
        let cy = covy(y) as u32;
        if cy == 0 {
            continue;
        }
        for x in r.x..r.right() {
            let mut c = div255(covx(x) as u32 * cy);
            if let Some(m) = mask {
                c = div255(c * m[(y * dw + x) as usize] as u32);
            }
            if c == 0 {
                continue;
            }
            let i = ((y * dw + x) * 4) as usize;
            let d = &mut dst[i..i + 4];
            let sa = div255(ca * c);
            let inv = 255 - sa;
            d[0] = (div255(cb * c) + div255(d[0] as u32 * inv)).min(255) as u8;
            d[1] = (div255(cg * c) + div255(d[1] as u32 * inv)).min(255) as u8;
            d[2] = (div255(cr * c) + div255(d[2] as u32 * inv)).min(255) as u8;
            d[3] = (sa + div255(d[3] as u32 * inv)).min(255) as u8;
        }
    }
}
