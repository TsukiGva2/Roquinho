//! `RectangularShadow` (Qt 6 QtQuick.Effects): analytic Gaussian box shadow of a rounded rect,
//! evaluated once into a corner tile and composited per frame by a pure index mapping
//! (9-slice): `alpha(x, y) = tile[min(dist_to_edge_y, th-1)][min(dist_to_edge_x, tw-1)]`.
//! Nothing is blurred per frame; tiles are cached per quantised (radius, sigma, size class).

use std::rc::Rc;
use std::sync::OnceLock;

use foco_base::IRect;

/// Empirical match to Qt's shader (fitted against `RectangularShadow` renders):
/// sigma = `SIGMA_PER_BLUR * blur`, and the shape is inset by `INSET0 + INSET_PER_BLUR * blur`.
pub const SIGMA_PER_BLUR: f32 = 0.41;
pub const INSET0: f32 = 0.7;
pub const INSET_PER_BLUR: f32 = 0.03;
/// Effective corner radius relative to the nominal one (the blur hides most of the rounding).
pub const RADIUS_FACTOR: f32 = 0.55;

pub struct Tile {
    data: Vec<u8>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Key {
    tw: u32,
    th: u32,
    wc: u32,
    hc: u32,
    rq: u32,
    sq: u32,
}

#[derive(Default)]
pub struct ShadowCache {
    tiles: Vec<(Key, Rc<Tile>)>,
    pub tiles_built: u64,
}

/// Standard normal CDF through a table (no `exp`/`erf` in the tile loop).
fn phi_table() -> &'static [f32] {
    static T: OnceLock<Vec<f32>> = OnceLock::new();
    T.get_or_init(|| {
        (0..=PHI_N)
            .map(|i| {
                let z = (i as f32 / PHI_RES) - PHI_MAX;
                0.5 + 0.5 * erf(z * std::f32::consts::FRAC_1_SQRT_2)
            })
            .collect()
    })
}

const PHI_MAX: f32 = 6.0;
const PHI_RES: f32 = 32.0;
const PHI_N: usize = (2.0 * PHI_MAX * PHI_RES) as usize;

#[inline]
fn phi(t: &[f32], z: f32) -> f32 {
    let f = (z.clamp(-PHI_MAX + 0.001, PHI_MAX - 0.001) + PHI_MAX) * PHI_RES;
    let i = f as usize;
    let fr = f - i as f32;
    t[i] + (t[i + 1] - t[i]) * fr
}

/// Abramowitz-Stegun 7.1.26 (|error| < 1.5e-7), used only to build the table.
fn erf(x: f32) -> f32 {
    let s = x.signum();
    let x = x.abs();
    let t = 1.0 / (1.0 + 0.327_591_1 * x);
    let y = 1.0
        - (((((1.061_405_4 * t - 1.453_152_1) * t) + 1.421_413_8) * t - 0.284_496_72) * t + 0.254_829_6)
            * t
            * (-x * x).exp();
    s * y
}

fn build_tile(tw: usize, th: usize, wc: f32, hc: f32, corner: f32, sigma: f32, m: f32) -> Tile {
    let table = phi_table();
    let (hx, hy) = (wc * 0.5, hc * 0.5);
    let inv_s = 1.0 / sigma;
    let norm = 1.0 / (sigma * (2.0 * std::f32::consts::PI).sqrt());
    let mut data = vec![0u8; tw * th];
    // square tile of a square (saturated) shape: the shadow is symmetric about the diagonal
    let sym = tw == th && (wc - hc).abs() < 0.5;
    for iy in 0..th {
        let py = iy as f32 + 0.5 - m - hy;
        let low = py - hy;
        let high = py + hy;
        let start = (-3.0 * sigma).clamp(low.min(high), high.max(low));
        let end = (3.0 * sigma).clamp(low.min(high), high.max(low));
        let step = (end - start) / 4.0;
        // per-sample constants for this row
        let mut curved = [0.0f32; 4];
        let mut wgt = [0.0f32; 4];
        let mut y = start + step * 0.5;
        for i in 0..4 {
            let yy = py - y;
            let delta = (hy - corner - yy.abs()).min(0.0);
            curved[i] = hx - corner + (corner * corner - delta * delta).max(0.0).sqrt();
            wgt[i] = norm * (-(y * y) * 0.5 * inv_s * inv_s).exp() * step;
            y += step;
        }
        for ix in 0..if sym { iy + 1 } else { tw } {
            let px = ix as f32 + 0.5 - m - hx;
            let mut v = 0.0;
            for i in 0..4 {
                let integral = phi(table, (px + curved[i]) * inv_s) - phi(table, (px - curved[i]) * inv_s);
                v += integral * wgt[i];
            }
            let q = (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            data[iy * tw + ix] = q;
            if sym {
                data[ix * tw + iy] = q;
            }
        }
    }
    Tile { data }
}

#[expect(clippy::inline_always, reason = "tiny per-pixel helper of the compositing inner loop; must inline")]
#[inline(always)]
fn div255(x: u32) -> u32 {
    (x + 128 + ((x + 128) >> 8)) >> 8
}

/// A shadow in device pixels: rect `[x0,x1) x [y0,y1)` (already spread/offset/inset),
/// effective corner radius and Gaussian sigma.
#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub rect: IRect,
    pub radius: f32,
    pub sigma: f32,
}

impl Params {
    /// Margin outside the rect where the shadow is visible.
    pub fn margin(&self) -> i32 {
        (3.0 * self.sigma).ceil() as i32 + 1
    }
    /// Area touched by the shadow.
    pub fn bounds(&self) -> IRect {
        let m = self.margin();
        IRect::new(self.rect.x - m, self.rect.y - m, self.rect.w + 2 * m, self.rect.h + 2 * m)
    }
}

impl ShadowCache {
    pub fn new() -> ShadowCache {
        ShadowCache::default()
    }

    fn tile(&mut self, p: &Params) -> (Rc<Tile>, usize, usize) {
        let m = p.margin();
        let r = p.radius.clamp(0.0, (p.rect.w.min(p.rect.h) as f32) * 0.5);
        let rq = ((r / 4.0).round() as u32) * 4;
        let k = (rq as f32 + 3.0 * p.sigma).ceil() as i32 + 1;
        let lx = p.rect.w + 2 * m;
        let ly = p.rect.h + 2 * m;
        let (hlx, hly) = ((lx + 1) / 2, (ly + 1) / 2);
        let (tw, sat_x) = if m + k <= hlx { ((m + k) as usize, true) } else { (hlx as usize, false) };
        let (th, sat_y) = if m + k <= hly { ((m + k) as usize, true) } else { (hly as usize, false) };
        let wc = if sat_x { 2 * k + 2 } else { p.rect.w } as u32;
        let hc = if sat_y { 2 * k + 2 } else { p.rect.h } as u32;
        let key = Key { tw: tw as u32, th: th as u32, wc, hc, rq, sq: (p.sigma * 8.0).round() as u32 };
        if let Some((_, t)) = self.tiles.iter().find(|(k, _)| *k == key) {
            return (t.clone(), tw, th);
        }
        let sigma = key.sq as f32 / 8.0;
        let tile = Rc::new(build_tile(tw, th, wc as f32, hc as f32, rq as f32, sigma, m as f32));
        self.tiles_built += 1;
        if self.tiles.len() >= 24 {
            self.tiles.remove(0);
        }
        self.tiles.push((key, tile.clone()));
        (tile, tw, th)
    }

    /// Composites the shadow into `dst` (premultiplied, `dw` x `dh`), limited to `clip` and
    /// optionally multiplied by `mask`. `color` is the premultiplied tint (pixmap byte order).
    /// `fresh` = the destination is known to be fully transparent (nothing drawn yet), so pixels
    /// are stored instead of blended.
    #[expect(
        clippy::too_many_arguments,
        reason = "destination surface, clip, mask, shadow params and colour are independent inputs of one composite"
    )]
    pub fn composite(
        &mut self,
        dst: &mut [u8],
        dw: i32,
        dh: i32,
        clip: IRect,
        mask: Option<&[u8]>,
        p: &Params,
        color: [u8; 4],
        fresh: bool,
    ) {
        let b = p.bounds();
        let Some(r) = b.intersect(&clip).and_then(|r| r.intersect(&IRect::new(0, 0, dw, dh))) else { return };
        if p.rect.w <= 0 || p.rect.h <= 0 {
            return;
        }
        let (tile, tw, th) = self.tile(p);
        let (lx, ly) = (b.w, b.h);
        let [c0, c1, c2, ca] = color.map(|v| v as u32);
        if let Some(m) = mask {
            for y in r.y..r.bottom() {
                let dy = ((y - b.y).min(b.y + ly - 1 - y) as usize).min(th - 1);
                let row = &tile.data[dy * tw..dy * tw + tw];
                let drow = &mut dst[((y * dw + r.x) * 4) as usize..((y * dw + r.right()) * 4) as usize];
                let mrow = &m[(y * dw + r.x) as usize..(y * dw + r.right()) as usize];
                for (i, d) in drow.chunks_exact_mut(4).enumerate() {
                    let x = r.x + i as i32;
                    let col = ((x - b.x).min(b.x + lx - 1 - x) as usize).min(tw - 1);
                    let t = div255(row[col] as u32 * mrow[i] as u32);
                    if t != 0 {
                        blend(d, t, c0, c1, c2, ca);
                    }
                }
            }
            return;
        }
        // premultiplied source pixel and inverse alpha per tile value
        let mut lut = [[0u8; 4]; 256];
        let mut inv = [0u32; 256];
        for (t, (l, iv)) in lut.iter_mut().zip(inv.iter_mut()).enumerate() {
            let t = t as u32;
            *l = [div255(c0 * t) as u8, div255(c1 * t) as u8, div255(c2 * t) as u8, div255(ca * t) as u8];
            *iv = 255 - l[3] as u32;
        }
        // x split: left ramp | constant middle | right ramp (absolute x)
        let mid0 = (b.x + tw as i32 - 1).clamp(r.x, r.right());
        let mid1 = (b.x + lx - tw as i32 + 1).clamp(mid0, r.right());
        let (nl, nm) = ((mid0 - r.x) as usize, (mid1 - mid0) as usize);
        let lcol0 = (r.x - b.x) as usize;
        for y in r.y..r.bottom() {
            let dy = ((y - b.y).min(b.y + ly - 1 - y) as usize).min(th - 1);
            let row = &tile.data[dy * tw..dy * tw + tw];
            let drow = &mut dst[((y * dw + r.x) * 4) as usize..((y * dw + r.right()) * 4) as usize];
            let (left, rest) = drow.split_at_mut(nl * 4);
            let (mid, right) = rest.split_at_mut(nm * 4);
            // left ramp: columns rise with x
            for (d, &t) in left.chunks_exact_mut(4).zip(&row[lcol0.min(tw)..]) {
                put(d, t, &lut, &inv, fresh);
            }
            // constant middle
            let t = row[tw - 1];
            if t != 0 && nm > 0 {
                let px = lut[t as usize];
                if fresh {
                    for d in mid.chunks_exact_mut(4) {
                        d.copy_from_slice(&px);
                    }
                } else {
                    let iv = inv[t as usize];
                    for d in mid.chunks_exact_mut(4) {
                        d[0] = (px[0] as u32 + div255(d[0] as u32 * iv)).min(255) as u8;
                        d[1] = (px[1] as u32 + div255(d[1] as u32 * iv)).min(255) as u8;
                        d[2] = (px[2] as u32 + div255(d[2] as u32 * iv)).min(255) as u8;
                        d[3] = (px[3] as u32 + div255(d[3] as u32 * iv)).min(255) as u8;
                    }
                }
            }
            // right ramp: columns fall with x
            let x0 = mid1;
            for (k, d) in right.chunks_exact_mut(4).enumerate() {
                let x = x0 + k as i32;
                let col = ((b.x + lx - 1 - x) as usize).min(tw - 1);
                put(d, row[col], &lut, &inv, fresh);
            }
        }
    }
}

#[expect(clippy::inline_always, reason = "tiny per-pixel helper of the compositing inner loop; must inline")]
#[inline(always)]
fn put(d: &mut [u8], t: u8, lut: &[[u8; 4]; 256], inv: &[u32; 256], fresh: bool) {
    if t == 0 {
        return;
    }
    let s = lut[t as usize];
    if fresh {
        d.copy_from_slice(&s);
    } else {
        let iv = inv[t as usize];
        d[0] = (s[0] as u32 + div255(d[0] as u32 * iv)).min(255) as u8;
        d[1] = (s[1] as u32 + div255(d[1] as u32 * iv)).min(255) as u8;
        d[2] = (s[2] as u32 + div255(d[2] as u32 * iv)).min(255) as u8;
        d[3] = (s[3] as u32 + div255(d[3] as u32 * iv)).min(255) as u8;
    }
}

#[expect(clippy::inline_always, reason = "tiny per-pixel helper of the compositing inner loop; must inline")]
#[inline(always)]
fn blend(d: &mut [u8], t: u32, c0: u32, c1: u32, c2: u32, ca: u32) {
    let sa = div255(ca * t);
    let inv = 255 - sa;
    d[0] = (div255(c0 * t) + div255(d[0] as u32 * inv)).min(255) as u8;
    d[1] = (div255(c1 * t) + div255(d[1] as u32 * inv)).min(255) as u8;
    d[2] = (div255(c2 * t) + div255(d[2] as u32 * inv)).min(255) as u8;
    d[3] = (sa + div255(d[3] as u32 * inv)).min(255) as u8;
}
