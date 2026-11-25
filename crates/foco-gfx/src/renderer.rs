//! `Renderer`: damage-rect driver around [`Canvas`].

use foco_base::{IRect, TextStyle};
use tiny_skia::PixmapMut;

use crate::canvas::Canvas;
use crate::fonts::{FontStyle, Fonts};
use crate::glyphs::GlyphCache;
use crate::shadow::ShadowCache;
use crate::text::TextEngine;
use crate::Engine;

/// Owns fonts, caches and the scratch pixmap.
pub struct Renderer {
    pub(crate) eng: Engine,
    scratch: Vec<u8>,
}

impl Default for Renderer {
    fn default() -> Self {
        Renderer::new()
    }
}

impl Renderer {
    /// Cheap: fonts are mapped and parsed lazily on first use. `FOCO_FONT=inter` selects Inter
    /// instead of the Qt-default Noto Sans.
    pub fn new() -> Renderer {
        let style = match std::env::var("FOCO_FONT").as_deref() {
            Ok("inter") | Ok("Inter") => FontStyle::Inter,
            _ => FontStyle::QtDefault,
        };
        Renderer::with_font_style(style)
    }

    pub fn with_font_style(style: FontStyle) -> Renderer {
        Renderer {
            eng: Engine {
                fonts: Fonts::new(style),
                glyphs: GlyphCache::new(),
                text: TextEngine::default(),
                sprites: std::collections::HashMap::new(),
                shadows: ShadowCache::new(),
            },
            scratch: Vec::new(),
        }
    }

    /// Switches the UI font family (layout caches are keyed by style, glyph caches by face).
    pub fn set_font_style(&mut self, style: FontStyle) {
        if self.eng.fonts.style != style {
            self.eng.fonts.style = style;
        }
    }

    pub fn font_style(&self) -> FontStyle {
        self.eng.fonts.style
    }

    /// Number of glyph rasterisations so far (diagnostics).
    pub fn glyph_misses(&self) -> u64 {
        self.eng.glyphs.misses
    }

    /// Advance width of `s` in LOGICAL px. Canvas measurement is independent of the current
    /// transform (layout happens at the logical pixel size), so this equals `Surface::measure`
    /// under any root scale such as `scale(25/24, 25/24)`. Shares the layout cache with Canvas.
    pub fn measure(&mut self, s: &str, style: &TextStyle) -> f32 {
        self.eng.text.layout(&mut self.eng.fonts, s, style).width
    }

    /// Line box height in logical px (`ceil(ascent + descent)`), same as `Surface::line_height`.
    pub fn line_height(&mut self, style: &TextStyle) -> f32 {
        let lay = self.eng.text.layout(&mut self.eng.fonts, "", style);
        (lay.asc + lay.desc).ceil()
    }

    /// `Text.ElideRight` in logical px, same as `Surface::elide`.
    pub fn elide(&mut self, s: &str, style: &TextStyle, max_w: f32) -> String {
        crate::text::elide(&mut self.eng.text, &mut self.eng.fonts, s, style, max_w)
    }

    /// For each damaged rect (device px): clear to transparent, call `paint` with a [`Canvas`]
    /// clipped to that rect (canvas coordinates are device pixels, origin = top-left of the whole
    /// buffer) and copy the result into `target` (BGRA premultiplied, stride = `width * 4`).
    pub fn render(&mut self, target: &mut [u8], width: u32, height: u32, rects: &[IRect], paint: &mut dyn FnMut(&mut Canvas)) {
        let stride = width as usize * 4;
        if target.len() < stride * height as usize {
            return;
        }
        let bounds = IRect::new(0, 0, width as i32, height as i32);
        for r in rects {
            let Some(r) = r.intersect(&bounds) else { continue };
            let (rw, rh) = (r.w as usize, r.h as usize);
            if r.x == 0 && r.w == width as i32 {
                // full-width band: paint straight into the target, no scratch, no copy
                let band = &mut target[r.y as usize * stride..(r.y as usize + rh) * stride];
                band.fill(0);
                let Some(pm) = PixmapMut::from_bytes(band, width, r.h as u32) else { continue };
                let mut cv = Canvas::new(pm, (0, r.y), &mut self.eng);
                paint(&mut cv);
                continue;
            }
            let need = rw * rh * 4;
            if self.scratch.len() < need {
                self.scratch.resize(need, 0);
            }
            let buf = &mut self.scratch[..need];
            buf.fill(0);
            {
                let Some(pm) = PixmapMut::from_bytes(buf, r.w as u32, r.h as u32) else { continue };
                let mut cv = Canvas::new(pm, (r.x, r.y), &mut self.eng);
                paint(&mut cv);
            }
            for y in 0..rh {
                let d = (r.y as usize + y) * stride + r.x as usize * 4;
                target[d..d + rw * 4].copy_from_slice(&self.scratch[y * rw * 4..(y + 1) * rw * 4]);
            }
        }
    }
}
