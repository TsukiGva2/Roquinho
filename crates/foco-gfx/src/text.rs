//! Text shaping (1 glyph per char, GPOS pair kerning, fallback chain, tabular digits,
//! letter spacing) with a direct-mapped layout cache, plus elision.

use std::rc::Rc;

use foco_base::{TextStyle, Weight};

use crate::fonts::{Fonts, Slot};

#[derive(Clone, Copy, Debug)]
pub struct PGlyph {
    pub slot: Slot,
    pub gid: u16,
    /// Pen x of this glyph, logical px from the start of the string.
    pub x: f32,
}

/// A laid-out line (logical px at the style's pixel size).
#[derive(Debug)]
pub struct Layout {
    pub glyphs: Vec<PGlyph>,
    /// For every char: pen position after it (incl. letter spacing).
    pub ends: Vec<f32>,
    pub width: f32,
    /// Primary face ascent / descent in logical px.
    pub asc: f32,
    pub desc: f32,
    pub primary: Slot,
}

struct Entry {
    text: Box<str>,
    px: u32,
    ls: u32,
    w: Weight,
    tab: bool,
    style: crate::fonts::FontStyle,
    layout: Rc<Layout>,
}

const CACHE: usize = 512;

pub struct TextEngine {
    cache: Vec<Option<Entry>>,
    pub hits: u64,
    pub misses: u64,
}

impl Default for TextEngine {
    fn default() -> Self {
        TextEngine { cache: (0..CACHE).map(|_| None).collect(), hits: 0, misses: 0 }
    }
}

fn hash(s: &str, px: u32, ls: u32, w: Weight, tab: bool, style: u8) -> usize {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in s.as_bytes() {
        h = (h ^ b as u64).wrapping_mul(0x100000001b3);
    }
    for v in [px as u64, ls as u64, w as u64, tab as u64, style as u64] {
        h = (h ^ v).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        h ^= h >> 31;
    }
    (h as usize) % CACHE
}

/// Qt keeps glyph advances in 26.6 fixed point.
#[inline]
fn r64(v: f32) -> f32 {
    (v * 64.0).round() / 64.0
}

impl TextEngine {
    pub fn layout(&mut self, fonts: &mut Fonts, s: &str, st: &TextStyle) -> Rc<Layout> {
        let (px, ls) = (st.px.to_bits(), st.letter_spacing.to_bits());
        let idx = hash(s, px, ls, st.weight, st.tabular, fonts.style as u8);
        if let Some(e) = &self.cache[idx] {
            if e.px == px && e.ls == ls && e.w == st.weight && e.tab == st.tabular && e.style == fonts.style && &*e.text == s {
                self.hits += 1;
                return e.layout.clone();
            }
        }
        self.misses += 1;
        let layout = Rc::new(compute(fonts, s, st));
        self.cache[idx] = Some(Entry {
            text: s.into(),
            px,
            ls,
            w: st.weight,
            tab: st.tabular,
            style: fonts.style,
            layout: layout.clone(),
        });
        layout
    }
}

fn compute(fonts: &mut Fonts, s: &str, st: &TextStyle) -> Layout {
    let primary = fonts.primary(st.weight);
    let fallbacks = fonts.fallbacks();
    let px = st.px;
    let (asc, desc) = match fonts.get(primary).map(|f| (f.asc / f.upem * px, f.desc / f.upem * px)) {
        Some(v) => v,
        None => (px * 0.95, px * 0.25),
    };
    let mut glyphs: Vec<PGlyph> = Vec::with_capacity(s.len());
    let mut ends: Vec<f32> = Vec::with_capacity(s.len());
    let mut pen = 0.0f32;
    let mut prev: Option<(Slot, u16)> = None;
    for c in s.chars() {
        if (c as u32) < 0x20 || c == '\u{7f}' {
            ends.push(pen);
            continue;
        }
        // pick the face
        let mut chosen: Option<(Slot, u16)> = None;
        for slot in std::iter::once(primary).chain(fallbacks.iter().copied()) {
            if let Some(f) = fonts.get(slot) {
                let g = f.gid(c);
                if g != 0 {
                    chosen = Some((slot, g));
                    break;
                }
            }
        }
        let (slot, mut gid, missing) = match chosen {
            Some((sl, g)) => (sl, g, false),
            None => (primary, 0, true),
        };
        let Some(face) = fonts.get(slot) else {
            pen += r64(px * 0.5) + st.letter_spacing;
            ends.push(pen);
            prev = None;
            continue;
        };
        let scale = px / face.upem;
        // kerning with the previous glyph (same face only)
        if let Some((ps, pg)) = prev {
            if ps == slot && !missing {
                let k = face.kern(pg, gid);
                pen += k as f32 * scale;
            }
        }
        let mut gx = 0.0;
        let mut adv = face.advance(gid) * scale;
        if st.tabular && c.is_ascii_digit() && !missing {
            let d = (c as u8 - b'0') as usize;
            if let Some(tab) = face.tabular_digits() {
                gid = tab[d];
                adv = face.advance(gid) * scale;
            } else {
                // fixed cell: widest digit, glyph centred
                let mut cell = 0.0f32;
                for dd in '0'..='9' {
                    let g = face.gid(dd);
                    cell = cell.max(face.advance(g));
                }
                let a0 = adv;
                adv = cell * scale;
                gx = (adv - a0) * 0.5;
            }
        }
        let adv = r64(adv);
        if gid != 0 {
            glyphs.push(PGlyph { slot, gid, x: pen + gx });
        }
        pen += adv + st.letter_spacing;
        ends.push(pen);
        prev = if missing { None } else { Some((slot, gid)) };
    }
    Layout { glyphs, ends, width: pen, asc, desc, primary }
}

/// `Text.ElideRight`.
pub fn elide(te: &mut TextEngine, fonts: &mut Fonts, s: &str, st: &TextStyle, max_w: f32) -> String {
    let lay = te.layout(fonts, s, st);
    if lay.width <= max_w {
        return s.to_string();
    }
    let ell = te.layout(fonts, "\u{2026}", st);
    let budget = max_w - ell.width;
    // largest prefix (in chars) whose end fits
    let mut n = 0;
    for (i, &e) in lay.ends.iter().enumerate() {
        if e <= budget {
            n = i + 1;
        } else {
            break;
        }
    }
    let mut out: String = s.chars().take(n).collect();
    out.push('\u{2026}');
    out
}
