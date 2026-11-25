//! The `View` and `Section` traits (ui-design.md §1).

use crate::cx::{Cx, Handled, PaintCx};
use foco_base::{Event, Ms, Rect, Surface, Wake};

/// A retained view: pull state in `update`, place in `layout`, draw in `paint`.
///
/// Per-iteration order: `update` -> (app springs) -> `layout` -> `paint` (only when `bounds()`
/// intersects the clip) -> `next_wake`. `event` runs between iterations and only pushes
/// `Msg`s; the app applies them afterwards.
pub trait View {
    /// Pull store/env changes, retarget animations, recompute strings and implicit sizes,
    /// damage what changed.
    fn update(&mut self, cx: &mut Cx);
    /// Place in window logical coordinates; when a rect moved damage old and new.
    fn layout(&mut self, cx: &mut Cx, frame: Rect);
    /// Events in logical px. Invisible views are never asked.
    fn event(&mut self, cx: &mut Cx, ev: &Event) -> Handled;
    fn paint(&mut self, s: &mut dyn Surface, pc: &PaintCx);
    fn next_wake(&self, now: Ms) -> Wake;
    /// Painted extent including pet overdraw and shadows.
    fn bounds(&self) -> Rect;
}

/// Expanded-body sections stacked in a column (invisible sections take no height and no
/// spacing).
pub trait Section: View {
    fn height(&self) -> f32;
    fn shown(&self) -> bool {
        self.height() > 0.0
    }
}
