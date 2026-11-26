//! foco-gfx: the CPU renderer of Foquinho.
//!
//! * [`Renderer`] clears each damaged rectangle, lets the caller paint it through a [`Canvas`]
//!   (the `foco_base::Surface` implementation over tiny-skia) and copies it into the X11 buffer.
//! * Text is unhinted grayscale coverage at fractional positions, rasterised lazily from
//!   memory-mapped fonts (ttf-parser + `ab_glyph_rasterizer`) and cached per glyph/size/phase.
//! * Shadows are analytic Gaussian rounded-box shadows composited from cached corner tiles.
//! * [`icons`] ports every icon of `qml/Icon.qml`.

mod canvas;
mod fonts;
mod glyphs;
pub mod icons;
mod renderer;
mod shadow;
mod text;

pub use canvas::Canvas;
pub use fonts::FontStyle;
pub use renderer::Renderer;

/// Coverage exponent applied to glyph bitmaps. Fitted so the total ink of 13 px text at the
/// 25/24 device scale matches a real Qt 6 grab (<1 makes light-on-dark text slightly heavier).
pub(crate) const COVERAGE_GAMMA: f32 = 0.85;

/// Everything that outlives a frame: fonts, glyph/layout/shadow caches.
pub(crate) struct Engine {
    pub fonts: fonts::Fonts,
    pub glyphs: glyphs::GlyphCache,
    pub text: text::TextEngine,
    pub shadows: shadow::ShadowCache,
    /// Rasterised `Surface::cached` pictures.
    pub sprites: std::collections::HashMap<canvas::SpriteKey, tiny_skia::Pixmap>,
}

#[cfg(test)]
mod tests;
