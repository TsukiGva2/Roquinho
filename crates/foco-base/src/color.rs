//! Colours. `Color` is non-premultiplied float RGBA (0..1).
//!
//! All drawing goes straight into an X11 ARGB32 little-endian buffer (bytes B,G,R,A,
//! premultiplied) that tiny-skia believes is RGBA, so every colour handed to tiny-skia must
//! go through [`Color::to_skia`], which swaps R and B.

/// Non-premultiplied RGBA colour, components in 0..1.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const TRANSPARENT: Color = Color::new(0.0, 0.0, 0.0, 0.0);
    pub const BLACK: Color = Color::new(0.0, 0.0, 0.0, 1.0);
    pub const WHITE: Color = Color::new(1.0, 1.0, 1.0, 1.0);

    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Color {
        Color { r, g, b, a }
    }
    /// Opaque colour from 8-bit channels.
    pub const fn rgb8(r: u8, g: u8, b: u8) -> Color {
        Color::rgba8(r, g, b, 255)
    }
    /// Colour from 8-bit channels.
    pub const fn rgba8(r: u8, g: u8, b: u8, a: u8) -> Color {
        Color { r: r as f32 / 255.0, g: g as f32 / 255.0, b: b as f32 / 255.0, a: a as f32 / 255.0 }
    }
    /// Parses `#rrggbb` or Qt-style `#aarrggbb` (alpha FIRST, as in Theme.qml's `#16ffffff`).
    pub fn hex(s: &str) -> Option<Color> {
        let h = s.strip_prefix('#')?;
        if !h.is_ascii() {
            return None;
        }
        let v = u32::from_str_radix(h, 16).ok()?;
        match h.len() {
            6 => Some(Color::rgb8((v >> 16) as u8, (v >> 8) as u8, v as u8)),
            8 => Some(Color::rgba8((v >> 16) as u8, (v >> 8) as u8, v as u8, (v >> 24) as u8)),
            _ => None,
        }
    }
    /// Same colour with alpha replaced.
    pub fn with_alpha(self, a: f32) -> Color {
        Color { a, ..self }
    }
    /// Alpha multiplied by `f`.
    pub fn mul_alpha(self, f: f32) -> Color {
        Color { a: self.a * f, ..self }
    }
    /// Component-wise linear interpolation (non-premultiplied).
    pub fn lerp(self, o: Color, t: f32) -> Color {
        let l = |a: f32, b: f32| a + (b - a) * t;
        Color { r: l(self.r, o.r), g: l(self.g, o.g), b: l(self.b, o.b), a: l(self.a, o.a) }
    }

    /// 16-bit channels as Qt stores them (`QColor` uses 16 bits per channel).
    fn to_u16(self) -> [u16; 3] {
        let q = |c: f32| (c.clamp(0.0, 1.0) as f64 * 65535.0).round() as u16;
        [q(self.r), q(self.g), q(self.b)]
    }
    fn from_u16(c: [u16; 3], a: f32) -> Color {
        Color { r: c[0] as f32 / 65535.0, g: c[1] as f32 / 65535.0, b: c[2] as f32 / 65535.0, a }
    }

    /// `QColor::lighter(factor_percent)`; `factor` is the Qt.lighter() factor (1.35 = 135%).
    /// Ported with Qt's 16-bit HSV integer arithmetic.
    pub fn lighter(self, factor: f32) -> Color {
        let f = (factor as f64 * 100.0).round() as i64;
        self.lighter_pct(f)
    }
    /// `QColor::darker(factor)`; `factor` as in Qt.darker() (2.0 = half brightness).
    pub fn darker(self, factor: f32) -> Color {
        let f = (factor as f64 * 100.0).round() as i64;
        self.darker_pct(f)
    }

    fn lighter_pct(self, factor: i64) -> Color {
        if factor <= 0 {
            return self;
        }
        if factor < 100 {
            return self.darker_pct(10000 / factor);
        }
        let (h, mut s, v) = rgb16_to_hsv(self.to_u16());
        let mut v = factor * v as i64 / 100;
        if v > 65535 {
            s = (s as i64 - (v - 65535)).max(0) as u16;
            v = 65535;
        }
        Color::from_u16(hsv_to_rgb16(h, s, v as u16), self.a)
    }

    fn darker_pct(self, factor: i64) -> Color {
        if factor <= 0 {
            return self;
        }
        if factor < 100 {
            return self.lighter_pct(10000 / factor);
        }
        let (h, s, v) = rgb16_to_hsv(self.to_u16());
        let v = (v as i64 * 100 / factor) as u16;
        Color::from_u16(hsv_to_rgb16(h, s, v), self.a)
    }

    /// Converts for tiny-skia, SWAPPING r and b (see the module docs).
    pub fn to_skia(self) -> tiny_skia::Color {
        let c = |v: f32| v.clamp(0.0, 1.0);
        tiny_skia::Color::from_rgba(c(self.b), c(self.g), c(self.r), c(self.a)).unwrap_or(tiny_skia::Color::TRANSPARENT)
    }
}

/// Swaps the R and B bytes of every 4-byte pixel in place (BGRA <-> RGBA).
pub fn swap_rb_in_place(pixels: &mut [u8]) {
    for px in pixels.chunks_exact_mut(4) {
        px.swap(0, 2);
    }
}

// Qt: hue in 0..35999 (hundredths of a degree), 0xffff when achromatic; s,v in 0..65535.
fn rgb16_to_hsv(c: [u16; 3]) -> (u16, u16, u16) {
    let [r, g, b] = c.map(|x| x as f64 / 65535.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let v = (max * 65535.0).round() as u16;
    if delta.abs() < 1e-12 {
        return (0xffff, 0, v);
    }
    let s = (delta / max * 65535.0).round() as u16;
    let mut hue = if (r - max).abs() < 1e-12 {
        (g - b) / delta
    } else if g == max {
        2.0 + (b - r) / delta
    } else {
        4.0 + (r - g) / delta
    };
    hue *= 60.0;
    if hue < 0.0 {
        hue += 360.0;
    }
    ((hue * 100.0).round() as u16, s, v)
}

fn hsv_to_rgb16(hue: u16, s: u16, v: u16) -> [u16; 3] {
    if s == 0 || hue == 0xffff {
        return [v, v, v];
    }
    let h = if hue == 36000 { 0.0 } else { hue as f64 / 6000.0 };
    let s = s as f64 / 65535.0;
    let v = v as f64 / 65535.0;
    let i = h as i32;
    let f = h - i as f64;
    let p = v * (1.0 - s);
    let (r, g, b) = if i & 1 == 1 {
        let q = v * (1.0 - s * f);
        match i {
            1 => (q, v, p),
            3 => (p, q, v),
            _ => (v, p, q),
        }
    } else {
        let t = v * (1.0 - s * (1.0 - f));
        match i {
            0 => (v, t, p),
            2 => (p, v, t),
            _ => (t, p, v),
        }
    };
    [r, g, b].map(|x| (x * 65535.0).round() as u16)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q8(c: Color) -> [u8; 3] {
        [c.r, c.g, c.b].map(|x| (x * 255.0).round() as u8)
    }

    #[test]
    fn hex_forms() {
        assert_eq!(Color::hex("#ff8000"), Some(Color::rgb8(255, 128, 0)));
        let c = Color::hex("#16ffffff").unwrap();
        assert_eq!(c, Color::rgba8(255, 255, 255, 0x16));
        assert!((c.a - 22.0 / 255.0).abs() < 1e-6);
        assert_eq!(Color::hex("ff8000"), None);
        assert_eq!(Color::hex("#fff"), None);
        assert_eq!(Color::hex("#zzzzzz"), None);
        assert_eq!(Color::hex("#é23456"), None);
    }

    #[test]
    fn skia_swaps_rb() {
        let c = Color::rgb8(255, 0, 0).to_skia();
        assert_eq!((c.red(), c.green(), c.blue()), (0.0, 0.0, 1.0));
        let mut px = [1u8, 2, 3, 4, 5, 6, 7, 8];
        swap_rb_in_place(&mut px);
        assert_eq!(px, [3, 2, 1, 4, 7, 6, 5, 8]);
    }

    #[test]
    fn lighter_matches_qt() {
        // Hand-run of QColor::lighter(135) on #926ee4 with Qt's 16-bit HSV:
        // v=58596 -> 79104 (> 65535): s = 33917 - 13569 = 20348, v = 65535.
        let c = Color::hex("#926ee4").unwrap().lighter(1.35);
        assert_eq!(q8(c), [200, 176, 255]);
        // grey: only value scales and saturates
        assert_eq!(q8(Color::rgb8(100, 100, 100).lighter(1.5)), [150, 150, 150]);
        // factor < 1 delegates to darker
        assert_eq!(q8(Color::rgb8(200, 200, 200).lighter(0.5)), q8(Color::rgb8(200, 200, 200).darker(2.0)));
        assert_eq!(q8(Color::rgb8(200, 100, 0).darker(2.0)), [100, 50, 0]);
        assert_eq!(Color::hex("#926ee4").unwrap().lighter(1.0), Color::hex("#926ee4").unwrap().lighter(1.0));
    }

    #[test]
    fn lerp_alpha() {
        let c = Color::BLACK.lerp(Color::WHITE, 0.5);
        assert_eq!(c, Color::new(0.5, 0.5, 0.5, 1.0));
        assert_eq!(Color::WHITE.mul_alpha(0.5).a, 0.5);
        assert_eq!(Color::WHITE.with_alpha(0.25).a, 0.25);
    }
}
