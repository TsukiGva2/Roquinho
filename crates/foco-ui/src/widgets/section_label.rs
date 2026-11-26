//! `SectionLabel.qml`: small caps-like heading with an optional count.

use super::label::paint_text_centered;
use foco_base::{theme, Rect, Surface, TextStyle, Weight};

pub const SECTION_LABEL_H: f32 = 20.0;

/// Draws the label at `rect.x + 4`, centred vertically in `rect`; `count` is drawn 6 px after
/// the label with tabular digits. Returns the right edge of what was drawn.
pub fn paint_section_label(s: &mut dyn Surface, rect: Rect, label: &str, count: Option<usize>) -> f32 {
    let cy = rect.y + rect.h * 0.5;
    let st = TextStyle::new(10.0, Weight::DemiBold, theme::TEXT3).with_spacing(1.0);
    let mut x = rect.x + 4.0;
    x += paint_text_centered(s, label, x, cy, &st);
    if let Some(n) = count {
        x += 6.0;
        let st2 = TextStyle::new(10.0, Weight::DemiBold, theme::TEXT3).tabular();
        x += paint_text_centered(s, &n.to_string(), x, cy, &st2);
    }
    x
}
