//! `Toggle.qml`: macOS-style switch, 38x22.

use super::mouse::MouseArea;
use crate::cx::{CursorShape, Cx, PaintCx};
use foco_base::{path_circle, theme, Anim, Easing, Event, Fill, Ms, Rect, Surface, Wake};

pub const W: f32 = 38.0;
pub const H: f32 = 22.0;

#[derive(Clone, Debug)]
pub struct Toggle {
    checked: bool,
    /// 0 = off, 1 = on (colour, 180 ms).
    on: Anim,
    /// Knob x offset (240 ms OutBack 1.6).
    knob: Anim,
    ma: MouseArea,
    shown: (f32, f32),
}

fn knob_x(checked: bool) -> f32 {
    if checked {
        W - 18.0 - 2.0
    } else {
        2.0
    }
}

impl Toggle {
    pub fn new(checked: bool) -> Toggle {
        Toggle {
            checked,
            on: Anim::new(checked as u8 as f32),
            knob: Anim::new(knob_x(checked)),
            ma: MouseArea::new(),
            shown: (checked as u8 as f32, knob_x(checked)),
        }
    }
    pub fn checked(&self) -> bool {
        self.checked
    }
    pub fn rect(&self) -> Rect {
        self.ma.rect
    }
    /// Follows the store value (the QML `checked` is a binding, not local state).
    pub fn sync(&mut self, cx: &mut Cx, checked: bool) {
        if self.checked != checked {
            self.checked = checked;
            self.on.animate_to(checked as u8 as f32, 180.0, Easing::Linear, cx.now);
            self.knob
                .animate_to(knob_x(checked), 240.0, Easing::OutBack { overshoot: 1.6 }, cx.now);
        }
        let cur = (self.on.value(cx.now), self.knob.value(cx.now));
        if (cur.0 - self.shown.0).abs() >= 1.0 / 255.0 || (cur.1 - self.shown.1).abs() >= 0.05 {
            self.shown = cur;
            cx.damage.add(self.ma.rect);
        }
    }
    /// Places the switch at `(x, y)` (the size is fixed).
    pub fn layout(&mut self, cx: &mut Cx, x: f32, y: f32) {
        let r = Rect::new(x, y, W, H);
        if r != self.ma.rect {
            cx.damage.add_moved(self.ma.rect, r);
            self.ma.rect = r;
        }
    }
    /// `Some(new value)` when clicked (`toggled(!checked)`).
    pub fn pointer(&mut self, cx: &mut Cx, ev: &Event) -> (crate::cx::Handled, Option<bool>) {
        let out = self.ma.event(ev);
        if self.ma.hovered() {
            cx.cursor = CursorShape::Hand;
        }
        (out.handled, out.clicked.then_some(!self.checked))
    }
    pub fn next_wake(&self, now: Ms) -> Wake {
        if self.on.is_animating(now) || self.knob.is_animating(now) {
            Wake::Frame
        } else {
            Wake::Idle
        }
    }
    pub fn paint(&self, s: &mut dyn Surface, pc: &PaintCx) {
        let r = self.ma.rect;
        let on = self.on.value(pc.now);
        let col = theme::FILL2.lerp(pc.env.accent, on);
        s.fill_rrect(r, H * 0.5, &Fill::Solid(col));
        let kx = r.x + self.knob.value(pc.now);
        if let Some(p) = path_circle(kx + 9.0, r.y + 2.0 + 9.0, 9.0) {
            s.fill_path(&p, &Fill::Solid(foco_base::Color::WHITE));
        }
    }
}
