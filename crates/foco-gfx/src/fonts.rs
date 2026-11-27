//! Lazily loaded, memory-mapped font faces (ttf-parser over mmapped files; nothing is read
//! into a Vec and no glyph is parsed until it is needed).

use std::cell::OnceCell;
use std::collections::HashMap;
use std::ffi::CString;
use std::path::{Path, PathBuf};

use foco_base::Weight;
use fontconfig::{
    Fontconfig, Pattern, FC_FAMILY, FC_WEIGHT, FC_WEIGHT_BOLD, FC_WEIGHT_MEDIUM, FC_WEIGHT_REGULAR, FC_WEIGHT_SEMIBOLD,
};
use ttf_parser as ttf;

/// Which Latin family the UI text uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FontStyle {
    /// What the Qt app really renders on the target machine: the application font comes from
    /// qt6ct (`Noto Sans`, style name "Regular"), and because the style name is pinned, Qt
    /// renders weights 500/600 with the Regular face and only 700 with the Bold face.
    #[default]
    QtDefault,
    /// Inter (Regular / Medium / `SemiBold` / Bold resolved through fontconfig).
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

/// What fontconfig is asked for: a family and a weight (always upright).
struct SlotSpec {
    family: &'static str,
    weight: Weight,
}

fn spec(slot: Slot) -> SlotSpec {
    let (family, weight) = match slot {
        Slot::InterRegular => ("Inter", Weight::Regular),
        Slot::InterMedium => ("Inter", Weight::Medium),
        Slot::InterSemiBold => ("Inter", Weight::DemiBold),
        Slot::InterBold => ("Inter", Weight::Bold),
        Slot::NotoRegular => ("Noto Sans", Weight::Regular),
        Slot::NotoBold => ("Noto Sans", Weight::Bold),
        Slot::NotoSymbols => ("Noto Sans Symbols", Weight::Regular),
        Slot::NotoSymbols2 => ("Noto Sans Symbols2", Weight::Regular),
        Slot::NotoMath => ("Noto Sans Math", Weight::Regular),
        Slot::DejaVu => ("DejaVu Sans", Weight::Regular),
    };
    SlotSpec { family, weight }
}

/// The fontconfig weight (its own scale, not CSS) for `w`.
fn fc_weight(w: Weight) -> i32 {
    match w {
        Weight::Regular => FC_WEIGHT_REGULAR,
        Weight::Medium => FC_WEIGHT_MEDIUM,
        Weight::DemiBold => FC_WEIGHT_SEMIBOLD,
        Weight::Bold => FC_WEIGHT_BOLD,
    }
}

/// The face index to use for a fontconfig match, or `None` when the match must be ignored.
/// Fontconfig always substitutes something, so a different family is a miss (the Qt-parity
/// metrics depend on the real family); a named instance (high index bits) is a variable font
/// preset and the renderer does not apply variation axes.
fn accept_match(requested: &str, matched: &str, index: i32) -> Option<u32> {
    let index = u32::try_from(index).ok()?;
    (same_family(requested, matched) && index >> 16 == 0).then_some(index)
}

/// Family equality the way fontconfig defines it (`FcStrCmpIgnoreBlanksAndCase`): Arch's file
/// names itself "Noto Sans Symbols 2", Debian's "Noto Sans Symbols2".
fn same_family(a: &str, b: &str) -> bool {
    let key = |s: &str| {
        s.chars()
            .filter(|c| !c.is_whitespace())
            .flat_map(char::to_lowercase)
            .collect::<String>()
    };
    key(a) == key(b)
}

/// Asks fontconfig for the file and face index of `spec`.
fn resolve(fc: &Fontconfig, spec: &SlotSpec) -> Option<(PathBuf, u32)> {
    let family = CString::new(spec.family).ok()?;
    let mut pat = Pattern::new(fc).ok()?;
    pat.add_string(FC_FAMILY, &family).ok()?;
    pat.add_integer(FC_WEIGHT, fc_weight(spec.weight)).ok()?;
    let found = pat.font_match().ok()?;
    let index = accept_match(spec.family, found.get_string(FC_FAMILY).ok()?, found.face_index().ok()?)?;
    Some((PathBuf::from(found.filename().ok()?), index))
}

/// Maps a read-only file into memory for the life of the process.
pub fn map_file(path: &Path) -> Option<&'static [u8]> {
    let f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len() as usize;
    if len == 0 {
        return None;
    }
    // SAFETY: private read-only mapping of a regular file that is never unmapped; the slice
    // lives for 'static. (A concurrent truncation of a system font file would be a system
    // problem, not a memory-safety contract we can enforce.)
    let mapped = unsafe {
        rustix::mm::mmap(std::ptr::null_mut(), len, rustix::mm::ProtFlags::READ, rustix::mm::MapFlags::PRIVATE, &f, 0)
    };
    match mapped {
        // SAFETY: `p` points to a successful READ mapping of exactly `len` bytes (the file size
        // above), never unmapped and not written through, so the shared slice is valid for
        // 'static; the mapping outlives `f`, closing the fd does not invalidate it.
        Ok(p) => Some(unsafe { std::slice::from_raw_parts(p as *const u8, len) }),
        Err(_) => std::fs::read(path).ok().map(|v| &*Box::leak(v.into_boxed_slice())),
    }
}

/// One loaded face with its small per-face caches.
#[expect(
    clippy::struct_field_names,
    reason = "`face` is the parsed ttf face; the other fields are its caches and metrics"
)]
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
    /// Lazily probed tabular-figure glyphs: `None` = not probed, `Some(None)` = font has none.
    #[expect(clippy::option_option, reason = "outer = probed yet, inner = font has the glyphs")]
    tab: Option<Option<[u16; 10]>>,
}

impl Face {
    fn new(face: ttf::Face<'static>) -> Face {
        let upem = face.units_per_em() as f32;
        let (sp, ss) = face
            .strikeout_metrics()
            .map_or((upem * 0.3, upem * 0.05), |m| (m.position as f32, m.thickness as f32));
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
            let g = self.face.glyph_index(c).map_or(0, |g| g.0);
            self.ascii[cu as usize] = g;
            return g;
        }
        if let Some(&g) = self.map.get(&c) {
            return g;
        }
        let g = self.face.glyph_index(c).map_or(0, |g| g.0);
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
                "zero.tf", "one.tf", "two.tf", "three.tf", "four.tf", "five.tf", "six.tf", "seven.tf", "eight.tf",
                "nine.tf",
            ];
            let mut out = [0u16; 10];
            let mut ok = true;
            for (i, n) in NAMES.iter().enumerate() {
                if let Some(g) = self.face.glyph_index_by_name(n) {
                    out[i] = g.0;
                } else {
                    ok = false;
                    break;
                }
            }
            self.tab = Some(if ok { Some(out) } else { None });
        }
        self.tab.unwrap()
    }
}

/// All faces, loaded on first use.
pub struct Fonts {
    /// Outer `None` = not loaded yet, `Some(None)` = file missing.
    #[expect(clippy::option_option, reason = "outer = loaded yet, inner = file exists")]
    faces: [Option<Option<Box<Face>>>; SLOT_COUNT],
    /// Created on the first load (the renderer promises cheap construction); `None` inside =
    /// fontconfig failed to initialise, so every slot is missing.
    fc: OnceCell<Option<Fontconfig>>,
    pub style: FontStyle,
}

impl Fonts {
    pub fn new(style: FontStyle) -> Fonts {
        Fonts { faces: Default::default(), fc: OnceCell::new(), style }
    }

    /// Loads the slot on first use; `None` when the file is missing.
    pub fn get(&mut self, slot: Slot) -> Option<&mut Face> {
        let i = slot as usize;
        if self.faces[i].is_none() {
            let fc = self.fc.get_or_init(Fontconfig::new).as_ref();
            self.faces[i] = Some(fc.and_then(|fc| load(fc, slot)));
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
            FontStyle::QtDefault => {
                &[Slot::NotoSymbols, Slot::NotoSymbols2, Slot::NotoMath, Slot::InterRegular, Slot::DejaVu]
            }
            FontStyle::Inter => {
                &[Slot::NotoRegular, Slot::NotoSymbols, Slot::NotoSymbols2, Slot::NotoMath, Slot::DejaVu]
            }
        }
    }
}

fn load(fc: &Fontconfig, slot: Slot) -> Option<Box<Face>> {
    let (path, index) = resolve(fc, &spec(slot))?;
    let face = ttf::Face::parse(map_file(&path)?, index).ok()?;
    Some(Box::new(Face::new(face)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_SLOTS: [Slot; SLOT_COUNT] = [
        Slot::InterRegular,
        Slot::InterMedium,
        Slot::InterSemiBold,
        Slot::InterBold,
        Slot::NotoRegular,
        Slot::NotoBold,
        Slot::NotoSymbols,
        Slot::NotoSymbols2,
        Slot::NotoMath,
        Slot::DejaVu,
    ];

    #[test]
    fn weights_map_to_fontconfig_scale() {
        let w = [Weight::Regular, Weight::Medium, Weight::DemiBold, Weight::Bold].map(fc_weight);
        assert_eq!(w, [80, 100, 180, 200]);
    }

    #[test]
    fn accept_match_rejects_substitutes_and_named_instances() {
        assert_eq!(accept_match("Inter", "Inter", 0), Some(0));
        assert_eq!(accept_match("Inter", "Inter", 3), Some(3));
        assert_eq!(accept_match("Inter", "DejaVu Sans", 0), None);
        assert_eq!(accept_match("Noto Sans Symbols2", "Noto Sans Symbols 2", 0), Some(0));
        assert_eq!(accept_match("Noto Sans Symbols2", "noto sans symbols2", 0), Some(0));
        assert_eq!(accept_match("Noto Sans Symbols2", "Noto Sans Symbols", 0), None);
        assert_eq!(accept_match("Inter", "Inter", (2 << 16) | 1), None);
        assert_eq!(accept_match("Inter", "Inter", -1), None);
    }

    #[test]
    fn all_slots_resolve_to_their_family() {
        let fc = Fontconfig::new().unwrap();
        // resolve() only returns a match whose family equals the requested one
        for slot in ALL_SLOTS {
            let sp = spec(slot);
            let (path, _) = resolve(&fc, &sp).unwrap_or_else(|| panic!("{slot:?} ({}) not found", sp.family));
            assert!(path.exists(), "{slot:?}: {}", path.display());
        }
    }

    #[test]
    fn inter_metrics() {
        let mut f = Fonts::new(FontStyle::Inter);
        let r = f.get(Slot::InterRegular).unwrap();
        assert_eq!((r.asc, r.desc, r.upem), (1984.0, 494.0, 2048.0));
        assert!(r.tabular_digits().is_some(), "Inter has *.tf digits");
        let a = r.gid('A');
        let v = r.gid('V');
        assert!(r.kern(a, v) < 0, "AV pair kerning");
        for slot in [Slot::InterMedium, Slot::InterSemiBold, Slot::InterBold] {
            assert!(f.get(slot).is_some(), "{slot:?}");
        }
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
