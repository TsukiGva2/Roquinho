//! Palette (macOS-light inspired). Colours are `#AARRGGBB` in Qt's 8-digit hex.

use crate::color::Color;

pub const NOTCH: Color = Color::rgb8(0xec, 0xec, 0xf0);
pub const CARD: Color = Color::rgb8(0xff, 0xff, 0xff);
pub const CARD_HOVER: Color = Color::rgb8(0xf4, 0xf4, 0xf6);
pub const FILL: Color = Color::rgb8(0xe5, 0xe5, 0xea);
pub const FILL2: Color = Color::rgb8(0xd1, 0xd1, 0xd6);
/// `#14000000`
pub const STROKE: Color = Color::rgba8(0x00, 0x00, 0x00, 0x14);
/// `#0f000000`
pub const HOVER: Color = Color::rgba8(0x00, 0x00, 0x00, 0x0f);

pub const TEXT: Color = Color::rgb8(0x1d, 0x1d, 0x1f);
pub const TEXT2: Color = Color::rgb8(0x6e, 0x6e, 0x73);
pub const TEXT3: Color = Color::rgb8(0xa1, 0xa1, 0xa6);

pub const GREEN: Color = Color::rgb8(0x24, 0x8a, 0x3d);
pub const RED: Color = Color::rgb8(0xe0, 0x35, 0x2b);
pub const ORANGE: Color = Color::rgb8(0xe0, 0x86, 0x00);
pub const BLUE: Color = Color::rgb8(0x0a, 0x84, 0xff);

/// Default accent (`#926ee4`).
pub const DEFAULT_ACCENT: Color = Color::rgb8(0x92, 0x6e, 0xe4);

/// Accent used for text/icons on the light surfaces: darker so it stays readable.
pub fn accent_text(accent: Color) -> Color {
    accent.darker(1.25)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_qml() {
        assert_eq!(STROKE, Color::hex("#14000000").unwrap());
        assert_eq!(HOVER, Color::hex("#0f000000").unwrap());
        assert_eq!(DEFAULT_ACCENT, Color::hex("#926ee4").unwrap());
        assert_eq!(TEXT, Color::hex("#1d1d1f").unwrap());
        assert_eq!(BLUE, Color::hex("#0a84ff").unwrap());
        assert_eq!(accent_text(DEFAULT_ACCENT), DEFAULT_ACCENT.darker(1.25));
    }
}
