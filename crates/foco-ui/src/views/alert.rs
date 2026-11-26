//! `AlertView.qml` (wave 3a): the "Dynamic Island" notice (info / done / added).
//!
//! 400x62 content: 40 px pet at x 18, a two-line text column, and a 30 px badge circle on the
//! right whose icon pops in (scale 0 -> 1 OutBack 2.6 over 480 ms after a 180 ms pause, rotation
//! -40 -> 0 over 420 ms). Opacity: 110 ms pause + 240 ms Linear in, 100 ms out.
//!
//! Damage: the whole frame only while the fade runs; during the pop only the badge circle at
//! its current scale (union with the previous one). Wakes: `Frame` while the fade or the pop
//! runs, the pet's own wake while visible, otherwise `Idle`.

use crate::cx::{Cx, Handled, PaintCx};
use crate::model;
use crate::pet::{new_pet, PetLike};
use crate::view::View;
use crate::widgets::{paint_icon, Icon};
use foco_base::{
    path_circle, theme, Anim, Color, Easing, Event, Fill, Mood, Ms, PetKind, Point, Rect, Seq, Surface, TextStyle,
    Wake, Weight,
};

const W: f32 = 400.0;
const PET_X: f32 = 18.0;
const PET_SIZE: f32 = 40.0;
const BADGE: f32 = 30.0;
const BADGE_MARGIN: f32 = 18.0;
const TEXT_X: f32 = PET_X + PET_SIZE + 12.0;
/// Width of the text column (`pet.right + 12` .. `badge.left - 12`).
const TEXT_W: f32 = W - TEXT_X - (BADGE_MARGIN + BADGE + 12.0);
const POP_PAUSE: Ms = 180.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlertKind {
    Info,
    Done,
    Added,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Alert {
    pub kind: AlertKind,
    pub title: String,
    pub sub: String,
}

fn title_style() -> TextStyle {
    TextStyle::new(14.0, Weight::DemiBold, theme::TEXT)
}
fn sub_style() -> TextStyle {
    TextStyle::new(12.0, Weight::Regular, theme::TEXT2)
}

pub struct AlertView {
    last: Alert,
    shown: bool,
    frame: Rect,
    op: Anim,
    pop_scale: Seq,
    pop_rot: Seq,
    /// What the badge was last painted with (for damage).
    pop_shown: (f32, f32),
    pet: Box<dyn PetLike>,
    pet_state: (PetKind, Mood, Color),
    was_visible: bool,
    text_dirty: bool,
    title_e: String,
    sub_e: String,
    /// Line heights of the 14 px and 12 px texts (fonts are constant, measured once).
    lh: Option<(f32, f32)>,
}

impl Default for AlertView {
    fn default() -> Self {
        Self::new()
    }
}

impl AlertView {
    pub fn new() -> Self {
        AlertView {
            last: Alert { kind: AlertKind::Info, title: String::new(), sub: String::new() },
            shown: false,
            frame: Rect::default(),
            op: Anim::new(0.0),
            pop_scale: Seq::new(0.0)
                .pause(POP_PAUSE)
                .to(1.0, 480.0, Easing::OutBack { overshoot: 2.6 }),
            pop_rot: Seq::new(-40.0).pause(POP_PAUSE).to(0.0, 420.0, Easing::OutCubic),
            pop_shown: (1.0, 0.0),
            pet: new_pet(),
            pet_state: (PetKind::Cat, Mood::Sleep, Color::TRANSPARENT),
            was_visible: false,
            text_dirty: true,
            title_e: String::new(),
            sub_e: String::new(),
            lh: None,
        }
    }

    /// `Some` => `last = a`, restart the pop animation. `None` keeps `last` for the fade-out.
    pub fn set_alert(&mut self, a: Option<&Alert>, now: Ms) {
        if let Some(a) = a {
            if self.last != *a {
                self.last.clone_from(a);
                self.text_dirty = true;
            }
            self.pop_scale.start(now);
            self.pop_rot.start(now);
        }
    }

    /// Opacity: 110 ms pause + 240 ms in, 100 ms out (both Linear).
    pub fn set_shown(&mut self, v: bool, now: Ms) {
        if v == self.shown {
            return;
        }
        self.shown = v;
        if v {
            self.op.animate_to_delayed(1.0, 110.0, 240.0, Easing::Linear, now);
        } else {
            self.op.animate_to(0.0, 100.0, Easing::Linear, now);
        }
    }

    /// `opacity > 0.01`.
    pub fn visible(&self, now: Ms) -> bool {
        self.op.value(now) > 0.01
    }

    fn cy(&self) -> f32 {
        self.frame.y + self.frame.h * 0.5
    }

    fn pet_rect(&self) -> Rect {
        Rect::new(self.frame.x + PET_X, self.cy() - PET_SIZE * 0.5, PET_SIZE, PET_SIZE)
    }

    fn badge_center(&self) -> Point {
        Point::new(self.frame.right() - BADGE_MARGIN - BADGE * 0.5, self.cy())
    }

    /// Bounding box of the badge circle at scale `k`.
    fn badge_box(&self, k: f32) -> Rect {
        let c = self.badge_center();
        let r = BADGE * 0.5 * k.abs() + 1.0;
        Rect::new(c.x - r, c.y - r, 2.0 * r, 2.0 * r)
    }

    fn tone(&self, accent: Color) -> Color {
        if self.last.kind == AlertKind::Done {
            theme::GREEN
        } else {
            accent
        }
    }

    fn icon(&self) -> Icon {
        match self.last.kind {
            AlertKind::Done => Icon::Check,
            AlertKind::Added => Icon::Plus,
            AlertKind::Info => Icon::Sparkle,
        }
    }

    fn text_top(&self) -> f32 {
        let (a, b) = self.lh.unwrap_or((17.0, 14.5));
        self.cy() - (a + 2.0 + b) * 0.5
    }
}

impl View for AlertView {
    fn update(&mut self, cx: &mut Cx) {
        let now = cx.now;
        let visible = self.visible(now);
        self.pet.set_active(visible, now);
        if self.lh.is_none() {
            self.lh = Some((cx.text.line_height(&title_style()), cx.text.line_height(&sub_style())));
            self.text_dirty = true;
        }
        if self.text_dirty {
            self.title_e = cx.text.elide(&self.last.title, &title_style(), TEXT_W);
            self.sub_e = cx.text.elide(&self.last.sub, &sub_style(), TEXT_W);
            self.text_dirty = false;
            if visible {
                cx.damage.add(self.frame);
            }
        }

        // pet
        let mood = if self.last.kind == AlertKind::Done { Mood::Happy } else { model::mood(cx.store, cx.epoch) };
        let ps = (cx.store.state().settings.kind(), mood, cx.env.accent);
        if ps != self.pet_state {
            if ps.0 != self.pet_state.0 {
                self.pet.set_kind(ps.0, now);
            }
            if ps.1 != self.pet_state.1 {
                self.pet.set_mood(ps.1, now);
            }
            if ps.2 != self.pet_state.2 {
                self.pet.set_accent(ps.2);
            }
            self.pet_state = ps;
            if visible {
                cx.damage.add(self.pet_rect().inflate(self.pet.overdraw(), self.pet.overdraw()));
            }
        }
        if self.pet.update(now) {
            cx.damage.add(self.pet_rect().inflate(self.pet.overdraw(), self.pet.overdraw()));
        }

        // badge pop
        let pop = (self.pop_scale.value(now), self.pop_rot.value(now));
        if visible && ((pop.0 - self.pop_shown.0).abs() >= 0.002 || (pop.1 - self.pop_shown.1).abs() >= 0.05) {
            let k = pop.0.abs().max(self.pop_shown.0.abs());
            cx.damage.add(self.badge_box(k));
        }
        self.pop_shown = pop;

        // fade
        if self.op.is_animating(now) || visible != self.was_visible {
            cx.damage.add(self.frame.inflate(2.0, 2.0));
        }
        self.was_visible = visible;
    }

    fn layout(&mut self, cx: &mut Cx, frame: Rect) {
        if frame != self.frame {
            if self.was_visible {
                cx.damage.add_moved(self.frame.inflate(2.0, 2.0), frame.inflate(2.0, 2.0));
            }
            self.frame = frame;
        }
    }

    fn event(&mut self, _cx: &mut Cx, _ev: &Event) -> Handled {
        Handled::No
    }

    fn paint(&mut self, s: &mut dyn Surface, pc: &PaintCx) {
        let now = pc.now;
        let op = self.op.value(now);
        if op <= 0.01 || self.frame.intersection(&pc.clip.inflate(40.0, 40.0)).is_none() {
            return;
        }
        let f = self.frame;
        s.save();
        s.mul_opacity(op);

        self.pet.paint(s, self.pet_rect(), now);

        let (lh_t, _) = self.lh.unwrap_or((17.0, 14.5));
        let ty = self.text_top();
        s.text(&self.title_e, f.x + TEXT_X, ty, &title_style());
        s.text(&self.sub_e, f.x + TEXT_X, ty + lh_t + 2.0, &sub_style());

        // badge: scale + rotation about its centre
        let k = self.pop_scale.value(now);
        let rot = self.pop_rot.value(now);
        if k.abs() > 0.001 {
            let c = self.badge_center();
            s.save();
            s.translate(c.x, c.y);
            s.rotate_deg(rot);
            s.scale(k, k);
            if let Some(p) = path_circle(0.0, 0.0, BADGE * 0.5) {
                s.fill_path(&p, &Fill::Solid(self.tone(pc.env.accent)));
            }
            paint_icon(s, self.icon(), Point::new(0.0, 0.0), 16.0, Color::WHITE, 2.2);
            s.restore();
        }
        s.restore();
    }

    fn next_wake(&self, now: Ms) -> Wake {
        let visible = self.visible(now);
        let mut w = Wake::Idle;
        if self.op.is_animating(now) {
            w = Wake::Frame;
        }
        if !visible {
            return w;
        }
        if self.pop_scale.is_running(now) {
            w = Wake::Frame;
        }
        w.merge_at(self.pet.next_wake(now), now)
    }

    fn bounds(&self) -> Rect {
        let pr = self.pet_rect();
        self.frame
            .union(&pr.inflate(self.pet.overdraw(), self.pet.overdraw()))
            .inflate(2.0, 2.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notch::{paint_notch, NotchGeom};
    use crate::testkit::Harness;
    use foco_base::{PixSurface, Task};
    use std::path::PathBuf;

    fn out_dir() -> PathBuf {
        let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/ui-test/wave3a");
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn alert(kind: AlertKind) -> Alert {
        match kind {
            AlertKind::Info => Alert {
                kind,
                title: "E aí. Eu sou o Foquinho".into(),
                sub: "Clica aqui e joga a primeira demanda na roda".into(),
            },
            AlertKind::Done => {
                Alert { kind, title: "Destruída!".into(), sub: "Escrever o relatório  ·  12 min".into() }
            }
            AlertKind::Added => Alert { kind, title: "No palco agora".into(), sub: "Responder e-mails".into() },
        }
    }

    fn step(h: &mut Harness, v: &mut AlertView, now: Ms) {
        h.with_cx(now, |cx| {
            v.update(cx);
            v.layout(cx, Rect::new(180.0, 0.0, W, 62.0));
        });
    }

    #[test]
    fn pop_timeline_and_fade() {
        let mut h = Harness::new();
        let mut v = AlertView::new();
        step(&mut h, &mut v, -1000.0);
        v.set_shown(true, 0.0);
        v.set_alert(Some(&alert(AlertKind::Done)), 0.0);
        assert!(!v.visible(100.0), "110 ms pause before the fade-in");
        assert!(v.visible(200.0));
        assert_eq!(v.pop_scale.value(0.0), 0.0);
        assert_eq!(v.pop_rot.value(0.0), -40.0);
        assert_eq!(v.pop_scale.value(179.0), 0.0, "pause");
        // OutBack 2.6 overshoots 1 before settling
        let peak = (0..48)
            .map(|i| v.pop_scale.value(180.0 + i as f64 * 10.0))
            .fold(0.0f32, f32::max);
        assert!(peak > 1.2, "overshoot {peak}");
        assert_eq!(v.pop_scale.value(700.0), 1.0);
        assert_eq!(v.pop_rot.value(600.0), 0.0);
        assert!(!v.pop_scale.is_running(661.0) && v.pop_scale.is_running(659.0));
        // out: 100 ms
        v.set_shown(false, 2000.0);
        assert!(v.visible(2050.0) && !v.visible(2100.0));
    }

    #[test]
    fn wakes_and_damage() {
        let b = 10_000.0;
        let mut h = Harness::new();
        let mut v = AlertView::new();
        step(&mut h, &mut v, b - 1000.0);
        assert_eq!(v.next_wake(b - 1000.0), Wake::Idle);
        v.set_shown(true, b);
        v.set_alert(Some(&alert(AlertKind::Added)), b);
        assert_eq!(v.next_wake(b), Wake::Frame);
        h.damage.clear();
        step(&mut h, &mut v, b + 300.0);
        assert!(!h.damage.is_empty());
        // fade done (350), pop running until 660: still Frame, but only the badge is damaged
        step(&mut h, &mut v, b + 400.0);
        h.damage.clear();
        step(&mut h, &mut v, b + 450.0);
        let r = h.damage.take();
        assert_eq!(r.len(), 1, "{r:?}");
        let bb = v.badge_box(1.6);
        assert!(r[0].w as f32 <= (bb.w + 2.0) * crate::S + 4.0, "only the badge: {r:?}");
        assert_eq!(v.next_wake(b + 450.0), Wake::Frame);
        // after the pop no view animation is left (the pet is the only possible waker)
        step(&mut h, &mut v, b + 800.0);
        assert_eq!(v.next_wake(b + 800.0), v.pet.next_wake(b + 800.0));
        // hidden: idle, pet inactive
        v.set_shown(false, b + 1000.0);
        step(&mut h, &mut v, b + 1200.0);
        assert_eq!(v.next_wake(b + 1200.0), Wake::Idle);
    }

    #[test]
    fn keeps_last_for_fade_out() {
        let mut v = AlertView::new();
        v.set_alert(Some(&alert(AlertKind::Done)), 0.0);
        v.set_alert(None, 10.0);
        assert_eq!(v.last.title, "Destruída!");
        assert_eq!(v.icon(), Icon::Check);
    }

    #[test]
    fn text_is_elided_to_the_column() {
        let mut h = Harness::new();
        let mut v = AlertView::new();
        let mut a = alert(AlertKind::Info);
        a.sub = "x".repeat(120);
        v.set_shown(true, -1000.0);
        v.set_alert(Some(&a), -1000.0);
        step(&mut h, &mut v, 0.0);
        assert!(v.sub_e.ends_with('…'));
        assert!(v.sub_e.chars().count() as f32 * 0.55 * 12.0 <= TEXT_W);
    }

    fn snap(kind: AlertKind, name: &str) {
        let mut h = Harness::new();
        h.store.state.focus.push(Task {
            tid: "a".into(),
            title: "a".into(),
            running: true,
            started_at: h.epoch as i64,
            ..Task::default()
        });
        let mut v = AlertView::new();
        step(&mut h, &mut v, -1000.0);
        v.set_shown(true, -1000.0);
        step(&mut h, &mut v, -500.0);
        v.set_alert(Some(&alert(kind)), 0.0);
        for t in [0.0, 240.0, 600.0] {
            step(&mut h, &mut v, t);
            let mut px = PixSurface::new(792, 100).unwrap();
            px.clear(Color::rgb8(0x8a, 0x8d, 0x94));
            px.scale(crate::S, crate::S);
            let g = NotchGeom::new(W, 62.0, 28.0);
            paint_notch(&mut px, &g, 0.45);
            px.save();
            px.clip_rect(g.clip_rect());
            let clip = Rect::new(0.0, 0.0, 760.0, 100.0);
            h.with_pc(t, clip, |pc| v.paint(&mut px, pc));
            px.restore();
            px.save_png(out_dir().join(format!("alert-{name}-{}ms.png", t as i32))).unwrap();
        }
    }

    #[test]
    fn snapshots() {
        snap(AlertKind::Info, "info");
        snap(AlertKind::Done, "done");
        snap(AlertKind::Added, "added");
    }
}
