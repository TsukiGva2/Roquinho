//! `MouseArea` / `HoverHandler` semantics.

use crate::cx::Handled;
use foco_base::{Button, Event, Rect};

/// Result of feeding one event to a [`MouseArea`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MouseOut {
    /// Hover or pressed flag flipped (the owner damages / retargets animations).
    pub changed: bool,
    /// Left button released inside after being pressed inside.
    pub clicked: bool,
    /// What the view should return: `Grab` on press, `Yes` on the release that ends a press.
    pub handled: Handled,
}

impl MouseOut {
    pub const NONE: MouseOut = MouseOut { changed: false, clicked: false, handled: Handled::No };
}

/// A rectangular hit area with hover and press state. A click fires on release when the
/// release lands inside (Qt `MouseArea.onClicked`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MouseArea {
    pub rect: Rect,
    hovered: bool,
    pressed: bool,
}

impl MouseArea {
    pub fn new() -> MouseArea {
        MouseArea::default()
    }
    pub fn hovered(&self) -> bool {
        self.hovered
    }
    pub fn pressed(&self) -> bool {
        self.pressed
    }
    /// `pressed && inside`: what drives the press-scale of buttons.
    pub fn pressed_inside(&self) -> bool {
        self.pressed && self.hovered
    }
    pub fn contains(&self, p: foco_base::Point) -> bool {
        self.rect.contains(p)
    }
    /// Clears hover and press (view hidden, window deactivated).
    pub fn reset(&mut self) -> bool {
        let ch = self.hovered || self.pressed;
        self.hovered = false;
        self.pressed = false;
        ch
    }
    pub fn event(&mut self, ev: &Event) -> MouseOut {
        match ev {
            Event::PointerMove { pos } => {
                let h = self.rect.contains(*pos);
                let changed = h != self.hovered;
                self.hovered = h;
                MouseOut { changed, clicked: false, handled: if self.pressed { Handled::Yes } else { Handled::No } }
            }
            Event::PointerLeave => MouseOut { changed: std::mem::replace(&mut self.hovered, false), ..MouseOut::NONE },
            Event::PointerDown { pos, button: Button::Left, .. } => {
                if self.rect.contains(*pos) {
                    let changed = !self.pressed || !self.hovered;
                    self.hovered = true;
                    self.pressed = true;
                    MouseOut { changed, clicked: false, handled: Handled::Grab }
                } else {
                    MouseOut::NONE
                }
            }
            Event::PointerUp { pos, button: Button::Left } => {
                if self.pressed {
                    let inside = self.rect.contains(*pos);
                    let changed = true;
                    self.pressed = false;
                    self.hovered = inside;
                    MouseOut { changed, clicked: inside, handled: Handled::Yes }
                } else {
                    MouseOut::NONE
                }
            }
            _ => MouseOut::NONE,
        }
    }
}

/// A bare hover flag with change detection (`HoverHandler`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Hover {
    on: bool,
}

impl Hover {
    pub fn new() -> Hover {
        Hover::default()
    }
    pub fn get(&self) -> bool {
        self.on
    }
    /// Sets the flag; true when it flipped.
    pub fn set(&mut self, v: bool) -> bool {
        let ch = self.on != v;
        self.on = v;
        ch
    }
    /// Hover from a pointer event over `rect`; `PointerLeave` clears. True when it flipped.
    pub fn update(&mut self, rect: Rect, ev: &Event) -> bool {
        match ev {
            Event::PointerMove { pos } => self.set(rect.contains(*pos)),
            Event::PointerLeave => self.set(false),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use foco_base::Point;

    fn down(x: f32, y: f32) -> Event {
        Event::PointerDown { pos: Point::new(x, y), button: Button::Left, clicks: 1 }
    }
    fn up(x: f32, y: f32) -> Event {
        Event::PointerUp { pos: Point::new(x, y), button: Button::Left }
    }

    #[test]
    fn click_needs_release_inside() {
        let mut m = MouseArea { rect: Rect::new(0.0, 0.0, 10.0, 10.0), ..Default::default() };
        let o = m.event(&Event::PointerMove { pos: Point::new(5.0, 5.0) });
        assert!(o.changed && m.hovered());
        let o = m.event(&down(5.0, 5.0));
        assert_eq!(o.handled, Handled::Grab);
        assert!(m.pressed_inside());
        let o = m.event(&Event::PointerMove { pos: Point::new(50.0, 5.0) });
        assert!(o.changed && !m.pressed_inside() && m.pressed());
        let o = m.event(&up(50.0, 5.0));
        assert!(!o.clicked && !m.pressed());
        m.event(&down(5.0, 5.0));
        let o = m.event(&up(6.0, 5.0));
        assert!(o.clicked);
        assert_eq!(m.event(&down(50.0, 50.0)), MouseOut::NONE);
        assert!(m.event(&Event::PointerLeave).changed);
    }

    #[test]
    fn hover_flag() {
        let mut h = Hover::new();
        let r = Rect::new(0.0, 0.0, 4.0, 4.0);
        assert!(h.update(r, &Event::PointerMove { pos: Point::new(1.0, 1.0) }));
        assert!(!h.update(r, &Event::PointerMove { pos: Point::new(2.0, 1.0) }));
        assert!(h.update(r, &Event::PointerLeave));
    }
}
