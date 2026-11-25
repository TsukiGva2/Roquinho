//! Text helpers: vertical centring, centred text and `WordWrap`.

use crate::cx::TextMeasure;
use foco_base::{Rect, Surface, TextStyle};

/// Top of a line box so that its centre lands on `cy` (`anchors.verticalCenter`).
pub fn text_top_for_center(s: &mut dyn Surface, cy: f32, st: &TextStyle) -> f32 {
    cy - s.line_height(st) * 0.5
}

/// Draws `text` starting at `x`, vertically centred on `cy`. Returns the advance width.
pub fn paint_text_centered(s: &mut dyn Surface, text: &str, x: f32, cy: f32, st: &TextStyle) -> f32 {
    let y = text_top_for_center(s, cy, st);
    s.text(text, x, y, st)
}

/// Draws `text` centred in `r` (`anchors.centerIn`).
pub fn paint_text_centered_in(s: &mut dyn Surface, text: &str, r: Rect, st: &TextStyle) -> f32 {
    let w = s.measure(text, st);
    let c = r.center();
    paint_text_centered(s, text, c.x - w * 0.5, c.y, st);
    w
}

/// `Text.WordWrap`: greedy line breaking at spaces (and at `\n`). A word wider than `w` stays
/// on its own line.
pub fn wrap(m: &mut dyn TextMeasure, text: &str, st: &TextStyle, w: f32) -> Vec<String> {
    let mut lines = Vec::new();
    for para in text.split('\n') {
        let mut cur = String::new();
        for word in para.split(' ') {
            if cur.is_empty() {
                cur.push_str(word);
                continue;
            }
            let cand_len = cur.len() + 1 + word.len();
            let mut cand = String::with_capacity(cand_len);
            cand.push_str(&cur);
            cand.push(' ');
            cand.push_str(word);
            if m.measure(&cand, st) <= w {
                cur = cand;
            } else {
                lines.push(std::mem::take(&mut cur));
                cur.push_str(word);
            }
        }
        lines.push(cur);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::FakeText;
    use foco_base::{theme, Weight};

    #[test]
    fn wraps_at_spaces() {
        let mut m = FakeText;
        let st = TextStyle::new(10.0, Weight::Regular, theme::TEXT); // 5.5 px per char
        let l = wrap(&mut m, "aaa bbb ccc ddd", &st, 5.5 * 8.0);
        assert_eq!(l, vec!["aaa bbb", "ccc ddd"]);
        let l = wrap(&mut m, "supercalifragilistic x", &st, 20.0);
        assert_eq!(l, vec!["supercalifragilistic", "x"]);
        assert_eq!(wrap(&mut m, "a\nb", &st, 100.0), vec!["a", "b"]);
        assert_eq!(wrap(&mut m, "", &st, 100.0), vec![""]);
    }
}
