//! `Dot.qml` (wave 3a): the hidden-mode ball (music variant removed). Click => `Act(SetHidden(false))`.
//!
//! 34x34 hit area, 14 px ball. Hover scale 1.3; appears 170 ms after being shown (scale 0 -> 1
//! OutBack 2.4 over 420 ms); while a task runs an accent ring pulses (1500 ms OutCubic to 2.3x,
//! fading 0.8 -> 0, then a 600 ms pause).
//!
//! Wakes: `Frame` while the scale / colour animates, `Ambient` (30 Hz) during the 1500 ms of the
//! ring's growth, an exact `At` for the ring's 600 ms pause and for the 170 ms appear delay,
//! `Idle` otherwise. Damage: the ball's own circle (grown to the ring's radius while it shows).

use crate::cx::{CursorShape, Cx, Handled, Msg, PaintCx};
use crate::view::View;
use crate::widgets::ColorAnim;
use foco_base::{path_circle, Action, Anim, Button, Color, Easing, Event, Fill, Ms, Point, Rect, Surface, Wake};
use tiny_skia::{Path, Stroke};

const BALL_R: f32 = 7.0;
const PULSE_MS: Ms = 1500.0;
const PULSE_PERIOD: Ms = 2100.0;
const IDLE_TONE: Color = Color::rgb8(0x8e, 0x8e, 0x93);
const OUTLINE: Color = Color::rgba8(0x00, 0x00, 0x00, 0x2e);

pub struct DotView {
    shown: bool,
    frame: Rect,
    appear: bool,
    appear_at: Option<Ms>,
    scale: Anim,
    hover: bool,
    pressed: Option<Button>,
    busy: bool,
    tone: ColorAnim,
    /// Accent while busy (the pulse ring uses the unanimated tone).
    ring_tone: Color,
    /// Start of the pulse loop; `Some` while `appear && busy`.
    ring_t0: Option<Ms>,
    /// Last painted state (for damage): core scale, tone, ring (scale, opacity).
    shown_scale: f32,
    shown_tone: Color,
    shown_ring: (f32, f32),
    ball: Option<Path>,
    outline: Option<Path>,
    ring: Option<Path>,
}

impl Default for DotView {
    fn default() -> Self {
        Self::new()
    }
}

/// `(scale, opacity)` of the pulse ring `e` ms into the loop (opacity 0 during the pause).
fn ring_at(e: Ms) -> (f32, f32) {
    let e = e.rem_euclid(PULSE_PERIOD);
    if e >= PULSE_MS {
        return (2.3, 0.0);
    }
    let u = (e / PULSE_MS) as f32;
    (1.0 + 1.3 * Easing::OutCubic.apply(u), 0.8 * (1.0 - u))
}

impl DotView {
    pub fn new() -> Self {
        DotView {
            shown: false,
            frame: Rect::default(),
            appear: false,
            appear_at: None,
            scale: Anim::new(0.0),
            hover: false,
            pressed: None,
            busy: false,
            tone: ColorAnim::new(IDLE_TONE),
            ring_tone: IDLE_TONE,
            ring_t0: None,
            shown_scale: 0.0,
            shown_tone: IDLE_TONE,
            shown_ring: (1.0, 0.0),
            ball: path_circle(0.0, 0.0, BALL_R),
            outline: path_circle(0.0, 0.0, BALL_R - 0.5),
            ring: path_circle(0.0, 0.0, BALL_R - 0.75),
        }
    }

    /// `Dot.shown`: the ball appears 170 ms later; hiding is immediate (scale back to 0).
    pub fn set_shown(&mut self, v: bool, now: Ms) {
        if v == self.shown {
            return;
        }
        self.shown = v;
        if v {
            self.appear_at = Some(now + 170.0);
        } else {
            self.appear_at = None;
            self.appear = false;
            self.hover = false;
            self.pressed = None;
            self.ring_t0 = None;
            self.retarget(now);
        }
    }

    /// Anything still on screen (shown, or the scale-out has not finished).
    pub fn visible(&self, now: Ms) -> bool {
        self.shown || self.scale.value(now).abs() > 0.001 || self.scale.is_animating(now)
    }

    fn retarget(&mut self, now: Ms) {
        let target = if self.appear {
            if self.hover {
                1.3
            } else {
                1.0
            }
        } else {
            0.0
        };
        self.scale.animate_to(target, 420.0, Easing::OutBack { overshoot: 2.4 }, now);
    }

    fn center(&self) -> Point {
        self.frame.center()
    }

    /// Box covering the ball and (when it shows) the ring at the given state.
    fn extent(&self, scale: f32, ring_scale: f32, ring_visible: bool) -> Rect {
        let k = if ring_visible { ring_scale.max(1.0) } else { 1.0 };
        let r = BALL_R * scale.abs() * k + 2.0;
        let c = self.center();
        Rect::new(c.x - r, c.y - r, 2.0 * r, 2.0 * r)
    }

    fn ring_state(&self, now: Ms) -> (f32, f32) {
        match self.ring_t0 {
            Some(t0) if self.busy => ring_at(now - t0),
            _ => (1.0, 0.0),
        }
    }
}

impl View for DotView {
    fn update(&mut self, cx: &mut Cx) {
        let now = cx.now;
        // appear timer
        if let Some(t) = self.appear_at {
            if now >= t {
                self.appear_at = None;
                self.appear = true;
                // the Behavior starts when the property flips (at `t`), not when we notice
                self.retarget(t);
            }
        }
        let busy = cx.store.state().running_count() > 0;
        self.busy = busy;
        let want = if busy { cx.env.accent } else { IDLE_TONE };
        self.ring_tone = want;
        self.tone.animate_to(want, 300.0, now);
        // pulse loop runs while `appear && busy`
        if self.appear && busy {
            self.ring_t0.get_or_insert(now);
        } else {
            self.ring_t0 = None;
        }

        let scale = self.scale.value(now);
        let tone = self.tone.value(now);
        let ring = self.ring_state(now);
        let ring_vis = ring.1 > 0.0;
        let was_ring_vis = self.shown_ring.1 > 0.0;
        let changed = (scale - self.shown_scale).abs() >= 0.002
            || tone != self.shown_tone
            || (ring_vis || was_ring_vis)
                && (ring.0 != self.shown_ring.0 || (ring.1 - self.shown_ring.1).abs() >= 1.0 / 255.0);
        if changed {
            let old = self.extent(self.shown_scale, self.shown_ring.0, was_ring_vis);
            let new = self.extent(scale, ring.0, ring_vis);
            cx.damage.add(old.union(&new));
            self.shown_scale = scale;
            self.shown_tone = tone;
            self.shown_ring = ring;
        }
    }

    fn layout(&mut self, cx: &mut Cx, frame: Rect) {
        if frame != self.frame {
            if self.visible(cx.now) {
                cx.damage.add_moved(self.frame.inflate(24.0, 24.0), frame.inflate(24.0, 24.0));
            }
            self.frame = frame;
        }
    }

    fn event(&mut self, cx: &mut Cx, ev: &Event) -> Handled {
        if !self.shown {
            return Handled::No;
        }
        let now = cx.now;
        match ev {
            Event::PointerMove { pos } => {
                let h = self.frame.contains(*pos);
                if h != self.hover {
                    self.hover = h;
                    self.retarget(now);
                }
                if h {
                    cx.cursor = CursorShape::Hand;
                }
                if self.pressed.is_some() {
                    Handled::Yes
                } else {
                    Handled::No
                }
            }
            Event::PointerLeave => {
                if self.hover {
                    self.hover = false;
                    self.retarget(now);
                }
                Handled::No
            }
            Event::PointerDown { pos, button, .. } if self.frame.contains(*pos) => {
                self.pressed = Some(*button);
                if !self.hover {
                    self.hover = true;
                    self.retarget(now);
                }
                Handled::Grab
            }
            Event::PointerUp { pos, button } if self.pressed == Some(*button) => {
                self.pressed = None;
                let inside = self.frame.contains(*pos);
                if inside {
                    // `onClicked`: left and right both un-hide (right only pauses music in the Qt app)
                    cx.msgs.push(Msg::Act(Action::SetHidden(false)));
                } else if self.hover {
                    self.hover = false;
                    self.retarget(now);
                }
                Handled::Yes
            }
            _ => Handled::No,
        }
    }

    fn paint(&mut self, s: &mut dyn Surface, pc: &PaintCx) {
        let now = pc.now;
        let k = self.scale.value(now).abs();
        if k <= 0.001 {
            return;
        }
        let c = self.center();
        s.save();
        s.translate(c.x, c.y);
        s.scale(k, k);
        let (rs, ro) = self.ring_state(now);
        if ro > 0.0 {
            if let Some(p) = &self.ring {
                s.save();
                s.scale(rs, rs);
                s.stroke_path(
                    p,
                    &Fill::Solid(self.ring_tone.mul_alpha(ro)),
                    &Stroke { width: 1.5, ..Stroke::default() },
                );
                s.restore();
            }
        }
        if let Some(p) = &self.ball {
            s.fill_path(p, &Fill::Solid(self.tone.value(now)));
        }
        if let Some(p) = &self.outline {
            s.stroke_path(p, &Fill::Solid(OUTLINE), &Stroke { width: 1.0, ..Stroke::default() });
        }
        // highlight: x 3, y 2.5, 4x3, radius 2 (clamped to 1.5), white at 45 %
        s.fill_rrect(
            Rect::new(-BALL_R + 3.0, -BALL_R + 2.5, 4.0, 3.0),
            1.5,
            &Fill::Solid(Color::WHITE.with_alpha(0.45)),
        );
        s.restore();
    }

    fn next_wake(&self, now: Ms) -> Wake {
        let mut w = Wake::Idle;
        if self.scale.is_animating(now) || self.tone.is_animating(now) {
            w = Wake::Frame;
        }
        if !self.shown {
            return w;
        }
        if let Some(t) = self.appear_at {
            w = w.merge_at(Wake::at(now, t), now);
        }
        if let (Some(t0), true) = (self.ring_t0, self.busy) {
            let e = (now - t0).rem_euclid(PULSE_PERIOD);
            if e < PULSE_MS {
                w = w.merge_at(Wake::Ambient, now);
            } else {
                // invisible pause: wake exactly when the next pulse starts
                w = w.merge_at(Wake::at(now, now + (PULSE_PERIOD - e)), now);
            }
        }
        w
    }

    fn bounds(&self) -> Rect {
        let c = self.center();
        let r = BALL_R * 1.3 * 2.3 + 3.0;
        Rect::new(c.x - r, c.y - r, 2.0 * r, 2.0 * r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::Harness;
    use foco_base::{PixSurface, Task};
    use std::path::PathBuf;

    fn out_dir() -> PathBuf {
        let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/ui-test/wave3a");
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    const FRAME: Rect = Rect { x: 363.0, y: 5.0, w: 34.0, h: 34.0 };

    fn step(h: &mut Harness, v: &mut DotView, now: Ms) {
        h.with_cx(now, |cx| {
            v.update(cx);
            v.layout(cx, FRAME);
        });
    }

    fn running(h: &mut Harness) {
        h.store.state.focus.push(Task {
            tid: "a".into(),
            title: "a".into(),
            running: true,
            started_at: h.epoch as i64,
            ..Task::default()
        });
        h.rev += 1;
    }

    #[test]
    fn appears_after_170ms_and_goes_idle() {
        let b = 10_000.0;
        let mut h = Harness::new();
        let mut v = DotView::new();
        step(&mut h, &mut v, b);
        assert_eq!(v.next_wake(b), Wake::Idle, "hidden: no wake");
        v.set_shown(true, b);
        assert_eq!(v.next_wake(b), Wake::At(b + 170.0));
        step(&mut h, &mut v, b + 100.0);
        assert_eq!(v.scale.value(b + 100.0), 0.0);
        h.damage.clear();
        step(&mut h, &mut v, b + 171.0);
        assert!(v.appear);
        assert_eq!(v.next_wake(b + 171.0), Wake::Frame);
        step(&mut h, &mut v, b + 300.0);
        assert!(!h.damage.is_empty());
        // OutBack 2.4 overshoots
        let peak = (0..42)
            .map(|i| v.scale.value(b + 170.0 + i as f64 * 10.0))
            .fold(0.0f32, f32::max);
        assert!(peak > 1.15, "{peak}");
        step(&mut h, &mut v, b + 700.0);
        assert_eq!(v.scale.value(b + 700.0), 1.0);
        assert_eq!(v.next_wake(b + 700.0), Wake::Idle, "idle ball: zero wakeups");
        h.damage.clear();
        step(&mut h, &mut v, b + 5000.0);
        assert!(h.damage.is_empty(), "nothing moves, nothing damaged");
        // hide: scale-out, then idle
        v.set_shown(false, b + 6000.0);
        assert!(v.visible(b + 6100.0));
        assert!(!v.visible(b + 6500.0));
        assert_eq!(v.next_wake(b + 6500.0), Wake::Idle);
    }

    #[test]
    fn pulse_ring_timing() {
        let b = 10_000.0;
        let mut h = Harness::new();
        running(&mut h);
        let mut v = DotView::new();
        v.set_shown(true, b);
        step(&mut h, &mut v, b + 171.0);
        step(&mut h, &mut v, b + 700.0);
        assert!(v.busy && v.ring_t0.is_some());
        let t0 = v.ring_t0.unwrap();
        assert_eq!(v.next_wake(b + 700.0), Wake::Ambient);
        // mid pulse: ring grows and fades
        let (s0, o0) = ring_at(0.0);
        assert_eq!((s0, o0), (1.0, 0.8));
        let (s1, o1) = ring_at(750.0);
        assert!(s1 > 1.9 && (o1 - 0.4).abs() < 1e-4, "{s1} {o1}");
        assert_eq!(ring_at(1500.0), (2.3, 0.0));
        assert_eq!(ring_at(1800.0).1, 0.0);
        // inside the pause: exact At the next pulse
        let pause = t0 + 1600.0;
        step(&mut h, &mut v, pause);
        assert_eq!(v.next_wake(pause), Wake::At(t0 + 2100.0));
        // ring damage is limited to the ball + ring box
        step(&mut h, &mut v, t0 + 2100.0 + 600.0);
        h.damage.clear();
        step(&mut h, &mut v, t0 + 2100.0 + 640.0);
        let r = h.damage.take();
        assert_eq!(r.len(), 1);
        assert!(r[0].w as f32 <= 2.0 * (7.0 * 2.3 + 3.0) * crate::S + 6.0, "{r:?}");
        // stopping the task: ring gone, idle again
        h.store.state.focus[0].running = false;
        h.rev += 1;
        step(&mut h, &mut v, t0 + 3000.0);
        assert!(v.ring_t0.is_none());
        let end = t0 + 3400.0;
        step(&mut h, &mut v, end);
        assert_eq!(v.next_wake(end), Wake::Idle);
    }

    #[test]
    fn hover_click_and_cursor() {
        let b = 10_000.0;
        let mut h = Harness::new();
        let mut v = DotView::new();
        let c = FRAME.center();
        // not shown: no events
        assert_eq!(h.with_cx(b, |cx| v.event(cx, &Event::PointerMove { pos: c })), Handled::No);
        v.set_shown(true, b);
        step(&mut h, &mut v, b + 800.0);
        h.with_cx(b + 800.0, |cx| v.event(cx, &Event::PointerMove { pos: c }));
        assert!(v.hover);
        assert_eq!(h.cursor, CursorShape::Hand);
        assert_eq!(v.scale.target(), 1.3);
        let down = Event::PointerDown { pos: c, button: Button::Left, clicks: 1 };
        let up = Event::PointerUp { pos: c, button: Button::Left };
        assert_eq!(h.with_cx(b + 900.0, |cx| v.event(cx, &down)), Handled::Grab);
        assert_eq!(h.with_cx(b + 900.0, |cx| v.event(cx, &up)), Handled::Yes);
        assert_eq!(h.take_msgs(), vec![Msg::Act(Action::SetHidden(false))]);
        // release outside: no click
        h.with_cx(b + 1000.0, |cx| v.event(cx, &down));
        let far = Event::PointerUp { pos: Point::new(10.0, 10.0), button: Button::Left };
        h.with_cx(b + 1000.0, |cx| v.event(cx, &far));
        assert!(h.take_msgs().is_empty());
        h.with_cx(b + 1100.0, |cx| v.event(cx, &Event::PointerLeave));
        assert_eq!(v.scale.target(), 1.0);
    }

    fn snap(name: &str, busy: bool, hover: bool, t: Ms) {
        let b = 10_000.0;
        let mut h = Harness::new();
        if busy {
            running(&mut h);
        }
        let mut v = DotView::new();
        v.set_shown(true, b);
        step(&mut h, &mut v, b + 171.0);
        if hover {
            h.with_cx(b + 700.0, |cx| v.event(cx, &Event::PointerMove { pos: FRAME.center() }));
        }
        step(&mut h, &mut v, b + 1200.0);
        let t0 = v.ring_t0.unwrap_or(b + 1200.0);
        let now = t0 + t;
        step(&mut h, &mut v, now);
        let mut px = PixSurface::new(792, 80).unwrap();
        px.clear(Color::rgb8(0x8a, 0x8d, 0x94));
        px.scale(crate::S, crate::S);
        h.with_pc(now, Rect::new(0.0, 0.0, 760.0, 80.0), |pc| v.paint(&mut px, pc));
        px.save_png(out_dir().join(name)).unwrap();
    }

    #[test]
    fn snapshots() {
        snap("dot-idle.png", false, false, 0.0);
        snap("dot-busy.png", true, false, 600.0);
        snap("dot-busy-hover.png", true, true, 300.0);
        snap("dot-hover.png", false, true, 0.0);
    }
}
