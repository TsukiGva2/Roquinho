//! Lazily loaded, memory-mapped font faces (ttf-parser over mmapped files; nothing is read
//! into a Vec and no glyph is parsed until it is needed).

use std::collections::HashMap;

use foco_base::Weight;
use ttf_parser as ttf;

/// Which Latin family the UI text uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FontStyle {
    /// What the Qt app really renders on the target machine: the application font comes from
    /// qt6ct (`Noto Sans`, style name "Regular"), and because the style name is pinned, Qt
    /// renders weights 500/600 with the Regular face and only 700 with the Bold face.
    #[default]
    QtDefault,
    /// Inter (Regular / Medium / SemiBold / Bold from `Inter.ttc`).
    Inter,
}

/// A font slot: one concrete face of one file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Slot {
    InterRegular = 0,
    InterMedium,
    InterSemiBold,
    InterBold,
    NotoRegular,
    NotoBold,
    NotoSymbols,
    NotoSymbols2,
    NotoMath,
    DejaVu,
}

pub const SLOT_COUNT: usize = 10;

const INTER_TTC: &str = "/usr/share/fonts/inter/Inter.ttc";

struct SlotSpec {
    /// Candidate files, first that exists wins.
    paths: &'static [&'static str],
    /// For collections: `(family, weight, index hint)`.
    ttc: Option<(&'static str, u16, u32)>,
}

fn spec(slot: Slot) -> SlotSpec {
    match slot {
        Slot::InterRegular => SlotSpec { paths: &[INTER_TTC], ttc: Some(("Inter", 400, 0)) },
        Slot::InterMedium => SlotSpec { paths: &[INTER_TTC], ttc: Some(("Inter", 500, 10)) },
        Slot::InterSemiBold => SlotSpec { paths: &[INTER_TTC], ttc: Some(("Inter", 600, 12)) },
        Slot::InterBold => SlotSpec { paths: &[INTER_TTC], ttc: Some(("Inter", 700, 14)) },
        Slot::NotoRegular => SlotSpec {
            paths: &["/usr/share/fonts/noto/NotoSans-Regular.ttf", "/usr/share/fonts/TTF/NotoSans-Regular.ttf"],
            ttc: None,
        },
        Slot::NotoBold => SlotSpec {
            paths: &["/usr/share/fonts/noto/NotoSans-Bold.ttf", "/usr/share/fonts/TTF/NotoSans-Bold.ttf"],
            ttc: None,
        },
        Slot::NotoSymbols => SlotSpec { paths: &["/usr/share/fonts/noto/NotoSansSymbols-Regular.ttf"], ttc: None },
        Slot::NotoSymbols2 => SlotSpec { paths: &["/usr/share/fonts/noto/NotoSansSymbols2-Regular.ttf"], ttc: None },
        Slot::NotoMath => SlotSpec { paths: &["/usr/share/fonts/noto/NotoSansMath-Regular.ttf"], ttc: None },
        Slot::DejaVu => SlotSpec {
            paths: &["/usr/share/fonts/TTF/DejaVuSans.ttf", "/usr/share/fonts/dejavu/DejaVuSans.ttf"],
            ttc: None,
        },
    }
}

/// Maps a read-only file into memory for the life of the process.
pub fn map_file(path: &str) -> Option<&'static [u8]> {
    let f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len() as usize;
    if len == 0 {
        return None;
    }
    // SAFETY: private read-only mapping of a regular file that is never unmapped; the slice
    // lives for 'static. (A concurrent truncation of a system font file would be a system
    // problem, not a memory-safety contract we can enforce.)
    let mapped = unsafe {
        rustix::mm::mmap(
            std::ptr::null_mut(),
            len,
            rustix::mm::ProtFlags::READ,
            rustix::mm::MapFlags::PRIVATE,
            &f,
            0,
        )
    };
    match mapped {
        Ok(p) => Some(unsafe { std::slice::from_raw_parts(p as *const u8, len) }),
        Err(_) => std::fs::read(path).ok().map(|v| &*Box::leak(v.into_boxed_slice())),
    }
}

/// Picks the face of `family`/`weight` (non-italic) out of a collection; tries `hint` first.
fn find_in_collection(data: &'static [u8], family: &str, weight: u16, hint: u32) -> Option<u32> {
    let matches = |idx: u32| -> bool {
        let Ok(face) = ttf::Face::parse(data, idx) else { return false };
        if face.is_italic() || face.weight().to_number() != weight {
            return false;
        }
        let mut fam = None;
        for id in [16u16, 1] {
            for n in face.names() {
                if n.name_id == id && n.is_unicode() {
                    if let Some(s) = n.to_string() {
                        fam = Some(s);
                        break;
                    }
                }
            }
            if fam.is_some() {
                break;
            }
        }
        fam.as_deref() == Some(family)
    };
    if matches(hint) {
        return Some(hint);
    }
    let n = ttf::fonts_in_collection(data)?;
    (0..n).find(|&i| matches(i))
}

/// One loaded face with its small per-face caches.
pub struct Face {
    pub face: ttf::Face<'static>,
    pub upem: f32,
    /// Ascent / descent (hhea) in font units, both positive.
    pub asc: f32,
    pub desc: f32,
    /// OS/2 strikeout position (above baseline) and thickness, font units.
    pub strike_pos: f32,
    pub strike_size: f32,
    ascii: [u16; 128],
    map: HashMap<char, u16>,
    kern_lookups: Option<Vec<u16>>,
    kern_cache: HashMap<u32, i16>,
    tab: Option<Option<[u16; 10]>>,
}

impl Face {
    fn new(face: ttf::Face<'static>) -> Face {
        let upem = face.units_per_em() as f32;
        let (sp, ss) = face
            .strikeout_metrics()
            .map(|m| (m.position as f32, m.thickness as f32))
            .unwrap_or((upem * 0.3, upem * 0.05));
        let asc = face.ascender() as f32;
        let desc = -(face.descender() as f32);
        Face {
            face,
            upem,
            asc,
            desc,
            strike_pos: sp,
            strike_size: ss,
            ascii: [0xFFFF; 128],
            map: HashMap::new(),
            kern_lookups: None,
            kern_cache: HashMap::new(),
            tab: None,
        }
    }

    /// Glyph id of `c`, 0 when the face has no glyph for it.
    pub fn gid(&mut self, c: char) -> u16 {
        let cu = c as u32;
        if cu < 128 {
            let v = self.ascii[cu as usize];
            if v != 0xFFFF {
                return v;
            }
            let g = self.face.glyph_index(c).map(|g| g.0).unwrap_or(0);
            self.ascii[cu as usize] = g;
            return g;
        }
        if let Some(&g) = self.map.get(&c) {
            return g;
        }
        let g = self.face.glyph_index(c).map(|g| g.0).unwrap_or(0);
        self.map.insert(c, g);
        g
    }

    /// Horizontal advance in font units.
    #[inline]
    pub fn advance(&self, gid: u16) -> f32 {
        self.face.glyph_hor_advance(ttf::GlyphId(gid)).unwrap_or(0) as f32
    }

    /// GPOS `kern` pair adjustment (font units) between two glyphs.
    pub fn kern(&mut self, l: u16, r: u16) -> i16 {
        let key = (l as u32) << 16 | r as u32;
        if let Some(&k) = self.kern_cache.get(&key) {
            return k;
        }
        let k = self.kern_uncached(l, r);
        if self.kern_cache.len() > 20_000 {
            self.kern_cache.clear();
        }
        self.kern_cache.insert(key, k);
        k
    }

    fn kern_uncached(&mut self, l: u16, r: u16) -> i16 {
        let Some(gpos) = self.face.tables().gpos else { return 0 };
        if self.kern_lookups.is_none() {
            let mut v: Vec<u16> = Vec::new();
            let tag = ttf::Tag::from_bytes(b"kern");
            for i in 0..gpos.features.len() {
                if let Some(f) = gpos.features.get(i) {
                    if f.tag == tag {
                        for li in f.lookup_indices {
                            if !v.contains(&li) {
                                v.push(li);
                            }
                        }
                    }
                }
            }
            v.sort_unstable();
            self.kern_lookups = Some(v);
        }
        let (gl, gr) = (ttf::GlyphId(l), ttf::GlyphId(r));
        for &li in self.kern_lookups.as_ref().unwrap() {
            let Some(lookup) = gpos.lookups.get(li) else { continue };
            for st in lookup.subtables.into_iter::<ttf::gpos::PositioningSubtable>() {
                let ttf::gpos::PositioningSubtable::Pair(pair) = st else { continue };
                match pair {
                    ttf::gpos::PairAdjustment::Format1 { coverage, sets } => {
                        let Some(ci) = coverage.get(gl) else { continue };
                        let Some(set) = sets.get(ci) else { continue };
                        if let Some((v1, _)) = set.get(gr) {
                            return v1.x_advance;
                        }
                    }
                    ttf::gpos::PairAdjustment::Format2 { coverage, classes, matrix } => {
                        if coverage.get(gl).is_none() {
                            continue;
                        }
                        let c1 = classes.0.get(gl);
                        let c2 = classes.1.get(gr);
                        if let Some((v1, _)) = matrix.get((c1, c2)) {
                            return v1.x_advance;
                        }
                    }
                }
            }
        }
        0
    }

    /// Tabular-figure glyph ids (`zero.tf`..`nine.tf`), if the font has them.
    pub fn tabular_digits(&mut self) -> Option<[u16; 10]> {
        if self.tab.is_none() {
            const NAMES: [&str; 10] = [
                "zero.tf", "one.tf", "two.tf", "three.tf", "four.tf", "five.tf", "six.tf", "seven.tf", "eight.tf", "nine.tf",
            ];
            let mut out = [0u16; 10];
            let mut ok = true;
            for (i, n) in NAMES.iter().enumerate() {
                match self.face.glyph_index_by_name(n) {
                    Some(g) => out[i] = g.0,
                    None => {
                        ok = false;
                        break;
                    }
                }
            }
            self.tab = Some(if ok { Some(out) } else { None });
        }
        self.tab.unwrap()
    }
}

/// All faces, loaded on first use.
pub struct Fonts {
    faces: [Option<Option<Box<Face>>>; SLOT_COUNT],
    pub style: FontStyle,
}

impl Fonts {
    pub fn new(style: FontStyle) -> Fonts {
        Fonts { faces: Default::default(), style }
    }

    /// Loads the slot on first use; `None` when the file is missing.
    pub fn get(&mut self, slot: Slot) -> Option<&mut Face> {
        let i = slot as usize;
        if self.faces[i].is_none() {
            self.faces[i] = Some(load(slot));
        }
        self.faces[i].as_mut().unwrap().as_deref_mut()
    }

    /// The primary face for a weight.
    pub fn primary(&self, w: Weight) -> Slot {
        match (self.style, w) {
            (FontStyle::QtDefault, Weight::Bold) => Slot::NotoBold,
            (FontStyle::QtDefault, _) => Slot::NotoRegular,
            (FontStyle::Inter, Weight::Regular) => Slot::InterRegular,
            (FontStyle::Inter, Weight::Medium) => Slot::InterMedium,
            (FontStyle::Inter, Weight::DemiBold) => Slot::InterSemiBold,
            (FontStyle::Inter, Weight::Bold) => Slot::InterBold,
        }
    }

    /// Fallback faces tried (in order) after the primary one.
    pub fn fallbacks(&self) -> &'static [Slot] {
        match self.style {
            FontStyle::QtDefault => &[Slot::NotoSymbols, Slot::NotoSymbols2, Slot::NotoMath, Slot::InterRegular, Slot::DejaVu],
            FontStyle::Inter => &[Slot::NotoRegular, Slot::NotoSymbols, Slot::NotoSymbols2, Slot::NotoMath, Slot::DejaVu],
        }
    }
}

fn load(slot: Slot) -> Option<Box<Face>> {
    let sp = spec(slot);
    for p in sp.paths {
        let Some(data) = map_file(p) else { continue };
        let index = match sp.ttc {
            Some((fam, w, hint)) => match find_in_collection(data, fam, w, hint) {
                Some(i) => i,
                None => continue,
            },
            None => 0,
        };
        if let Ok(face) = ttf::Face::parse(data, index) {
            return Some(Box::new(Face::new(face)));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inter_collection_indices() {
        let data = map_file(INTER_TTC).expect("Inter.ttc");
        assert_eq!(find_in_collection(data, "Inter", 400, 99), Some(0));
        assert_eq!(find_in_collection(data, "Inter", 500, 99), Some(10));
        assert_eq!(find_in_collection(data, "Inter", 600, 99), Some(12));
        assert_eq!(find_in_collection(data, "Inter", 700, 99), Some(14));
        let mut f = Fonts::new(FontStyle::Inter);
        let r = f.get(Slot::InterRegular).unwrap();
        assert_eq!((r.asc, r.desc, r.upem), (1984.0, 494.0, 2048.0));
        assert!(r.tabular_digits().is_some(), "Inter has *.tf digits");
        let a = r.gid('A');
        let v = r.gid('V');
        assert!(r.kern(a, v) < 0, "AV pair kerning");
    }

    #[test]
    fn noto_metrics() {
        let mut f = Fonts::new(FontStyle::QtDefault);
        let r = f.get(Slot::NotoRegular).unwrap();
        assert_eq!((r.asc, r.desc, r.upem), (1069.0, 293.0, 1000.0));
        let a = r.gid('A');
        let v = r.gid('V');
        assert!(r.kern(a, v) < 0);
        assert!(f.get(Slot::NotoBold).is_some());
    }
}
