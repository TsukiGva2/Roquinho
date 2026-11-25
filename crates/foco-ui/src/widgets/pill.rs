//! Badge pill (accent tint) and key chips.

use super::label::paint_text_centered_in;
use crate::cx::{PaintCx, TextMeasure};
use foco_base::{theme, Color, Fill, Rect, Surface, TextStyle, Weight};

/// Pill height.
pub const PILL_H: f32 = 17.0;

/// Pill text: 10 px DemiBold in `Theme.accentText`; `tabular` for counters (`1/3`).
pub fn pill_style(accent_text: Color, tabular: bool) -> TextStyle {
    let st = TextStyle::new(10.0, Weight::DemiBold, accent_text);
    if tabular {
        st.tabular()
    } else {
        st
    }
}

/// Pill width: text + 12.
pub fn pill_width(m: &mut dyn TextMeasure, text: &str, st: &TextStyle) -> f32 {
    m.measure(text, st) + 12.0
}

/// Draws an accent pill into `rect` (use `rect.h == PILL_H`): `accent.with_alpha(alpha)` fill.
pub fn paint_pill(s: &mut dyn Surface, pc: &PaintCx, rect: Rect, text: &str, alpha: f32, tabular: bool) {
    s.fill_rrect(rect, rect.h * 0.5, &Fill::Solid(pc.env.accent.with_alpha(alpha)));
    paint_text_centered_in(s, text, rect, &pill_style(pc.env.accent_text(), tabular));
}

/// Key-chip width: `max(min_w, text + pad)`.
pub fn chip_width(m: &mut dyn TextMeasure, text: &str, st: &TextStyle, pad: f32, min_w: f32) -> f32 {
    (m.measure(text, st) + pad).max(min_w)
}

/// Draws a key chip (`Theme.fill` background, optional 1 px stroke) with centred `text`.
/// Settings chips: h 20, r 6, pad 12, min 20, 11 px text, stroked. Input hints: h 17, r 5, pad 10.
pub fn paint_chip(s: &mut dyn Surface, rect: Rect, radius: f32, text: &str, st: &TextStyle, stroke: bool) {
    s.fill_rrect(rect, radius, &Fill::Solid(theme::FILL));
    if stroke {
        s.stroke_rrect(rect.inflate(-0.5, -0.5), (radius - 0.5).max(0.0), &Fill::Solid(theme::STROKE), 1.0);
    }
    paint_text_centered_in(s, text, rect, st);
}
