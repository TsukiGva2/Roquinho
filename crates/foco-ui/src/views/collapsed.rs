//! `CollapsedView.qml` (minus music): pet, task title (+ `i/n` badge), live dot / hover controls
//! and timer. Wave 3(a) adds the 5 s cycle + swap animation and polish; this version is
//! complete enough to drive the notch width and the tip position.

use crate::cx::{Cx, Handled, PaintCx};
use crate::model;
use crate::pet::{new_pet, PetLike};
use crate::view::View;
use crate::widgets::{
    paint_icon, paint_pill, paint_text_centered, pill_style, pill_width, ColorAnim, Icon, IconButton,
};
use foco_base::model::fmt_clock;
use foco_base::{
    path_circle, pet_event, theme, Action, Anim, Easing, Event, Fill, Ms, PetKind, Rect, Seq, Surface, TextStyle, Wake,
    Weight,
};
use std::f32::consts::PI;

/// `Timer { interval: 5000 }` of the focus cycle.
const CYCLE_MS: Ms = 5000.0;
/// First half of the swap animation (fade out + slide up), after which the shown task changes.
const SWAP_OUT_MS: Ms = 140.0;
/// Half period of the live dot's opacity pulse (900 ms `InOutSine` each way).
const LIVE_HALF_MS: f32 = 900.0;
/// Quantisation of the live dot's opacity (steps of 1/8: a 6 px dot shows no finer change).
const LIVE_STEP: f32 = 1.0 / 8.0;
/// Minimum distance between two live-dot wakes (ms).
const LIVE_MIN_GAP_MS: Ms = 50.0;

/// For `v(theta) = c + a*cos(theta)` quantised to multiples of `step`: the angle (radians, > 0)
/// to advance from `theta` until the quantised level changes. `None` if it never changes.
/// Shared with the tip bubble's float.
pub(crate) fn next_level_change(c: f32, a: f32, step: f32, theta: f32) -> Option<f32> {
    if a == 0.0 || step <= 0.0 {
        return None;
    }
    let tau = 2.0 * PI;
    let th = theta.rem_euclid(tau);
    let v = c + a * th.cos();
    let k = (v / step).round();
    let mut best: Option<f32> = None;
    for b in [(k - 0.5) * step, (k + 0.5) * step] {
        let cs = (b - c) / a;
        if !(-1.0..=1.0).contains(&cs) {
            continue;
        }
        let t0 = cs.acos();
        for cand in [t0, tau - t0] {
            let d = (cand - th).rem_euclid(tau);
            if d > 1e-6 && best.is_none_or(|b| d < b) {
                best = Some(d);
            }
        }
    }
    best
}

/// Quantised opacity of the live dot at `e` ms after the pulse started (`1 -> 0.25 -> 1`).
fn live_level(e: Ms) -> f32 {
    let th = PI * (e as f32) / LIVE_HALF_MS;
    let v = 0.625 + 0.375 * th.cos();
    ((v / LIVE_STEP).round() * LIVE_STEP).clamp(0.25, 1.0)
}

const PET_SIZE: f32 = 26.0;
const PET_X: f32 = 14.0;
const TITLE_X: f32 = PET_X + PET_SIZE + 10.0;
const TITLE_MAX_W: f32 = 230.0;

fn title_style() -> TextStyle {
    TextStyle::new(13.0, Weight::Medium, theme::TEXT)
}
fn timer_style(c: foco_base::Color) -> TextStyle {
    TextStyle::new(13.0, Weight::DemiBold, c).tabular()
}
fn idle_style() -> TextStyle {
    TextStyle::new(13.0, Weight::Regular, theme::TEXT2)
}

pub struct CollapsedView {
    shown: bool,
    hovered: bool,
    op: Anim,
    sc: Anim,
    pet: Box<dyn PetLike>,
    pet_state: (PetKind, foco_base::Mood, bool, foco_base::Color),
    frame: Rect,
    // derived from the store
    inited: bool,
    was_visible: bool,
    last_rev: u64,
    last_env_rev: u64,
    has_focus: bool,
    n_focus: usize,
    n_queue: usize,
    /// `shownIdx` (raw; `idx` is the clamped one the QML calls `idx`).
    shown_idx: usize,
    idx: usize,
    /// `cycle`: the index the 5 s timer last picked.
    cycle: usize,
    cycle_at: Option<Ms>,
    swap_op: Seq,
    swap_y: Seq,
    swap_switch_at: Option<Ms>,
    swap_shown: (f32, f32),
    title: String,
    /// Title as drawn (elided when over 230 px).
    title_draw: String,
    title_w: f32,
    badge: String,
    badge_w: f32,
    timer: String,
    timer_w: f32,
    idle_text: String,
    running: bool,
    tick_at: Option<Ms>,
    implicit_w: f32,
    timer_col: ColorAnim,
    /// Start of the live dot's pulse, `Some` while it runs.
    live_t0: Option<Ms>,
    /// Quantised live-dot opacity currently painted.
    live_shown: f32,
    // hover controls
    ctl_op: Anim,
    ctl_sc: Anim,
    pause_btn: IconButton,
    done_btn: IconButton,
}

impl Default for CollapsedView {
    fn default() -> Self {
        Self::new()
    }
}

impl CollapsedView {
    pub fn new() -> Self {
        let green = theme::GREEN;
        CollapsedView {
            shown: false,
            hovered: false,
            op: Anim::new(0.0),
            sc: Anim::new(0.92),
            pet: new_pet(),
            pet_state: (PetKind::Cat, foco_base::Mood::Sleep, false, foco_base::Color::TRANSPARENT),
            frame: Rect::default(),
            inited: false,
            was_visible: false,
            last_rev: 0,
            last_env_rev: u64::MAX,
            has_focus: false,
            n_focus: 0,
            n_queue: 0,
            shown_idx: 0,
            idx: 0,
            cycle: 0,
            cycle_at: None,
            swap_op: Seq::new(1.0)
                .to(0.0, SWAP_OUT_MS, Easing::Linear)
                .to(1.0, 220.0, Easing::Linear)
                .pause(40.0),
            swap_y: Seq::new(0.0)
                .to(-7.0, SWAP_OUT_MS, Easing::InCubic)
                .set(7.0)
                .to(0.0, 260.0, Easing::OutCubic),
            swap_switch_at: None,
            swap_shown: (1.0, 0.0),
            title: String::new(),
            title_draw: String::new(),
            title_w: 0.0,
            badge: String::new(),
            badge_w: 0.0,
            timer: String::new(),
            timer_w: 0.0,
            idle_text: String::new(),
            running: false,
            tick_at: None,
            implicit_w: 212.0,
            timer_col: ColorAnim::new(theme::TEXT2),
            live_t0: None,
            live_shown: 1.0,
            ctl_op: Anim::new(0.0),
            ctl_sc: Anim::new(0.6),
            pause_btn: IconButton::new(Icon::Pause, 26.0, 13.0).colors(
                theme::TEXT2,
                theme::TEXT,
                theme::FILL,
                crate::widgets::icon_button::HOVER_BG,
            ),
            done_btn: IconButton::new(Icon::Check, 26.0, 14.0).colors(
                foco_base::Color::WHITE,
                foco_base::Color::WHITE,
                green,
                green.darker(1.15),
            ),
        }
    }

    /// `opacity > 0.01`.
    pub fn visible(&self, now: Ms) -> bool {
        self.op.value(now) > 0.01
    }

    /// Opacity 120 ms pause + 240 `OutCubic` in / 90 ms out; scale 0.92 <-> 1 over 320 ms.
    pub fn set_shown(&mut self, v: bool, now: Ms) {
        if v == self.shown {
            return;
        }
        self.shown = v;
        if v {
            self.op.animate_to_delayed(1.0, 120.0, 240.0, Easing::OutCubic, now);
        } else {
            self.op.animate_to(0.0, 90.0, Easing::OutCubic, now);
        }
        self.sc.animate_to(if v { 1.0 } else { 0.92 }, 320.0, Easing::OutCubic, now);
    }

    /// Notch hover: reveals the controls and widens the notch (`controlsW` 0 <-> 62).
    pub fn set_hovered(&mut self, v: bool, now: Ms) {
        if v == self.hovered {
            return;
        }
        self.hovered = v;
        self.ctl_op.animate_to(v as u8 as f32, 180.0, Easing::Linear, now);
        self.ctl_sc.animate_to(if v { 1.0 } else { 0.6 }, 260.0, Easing::OUT_BACK, now);
    }

    fn controls_w(&self) -> f32 {
        if self.hovered && self.has_focus {
            62.0
        } else {
            0.0
        }
    }

    /// `implicitWidth` (feeds the notch width spring).
    pub fn implicit_width(&self) -> f32 {
        self.implicit_w
    }

    fn compute_implicit(&self) -> f32 {
        if self.has_focus {
            let badge = if self.n_focus > 1 { self.badge_w + 8.0 } else { 0.0 };
            (14.0
                + PET_SIZE
                + 9.0
                + self.title_w.min(TITLE_MAX_W)
                + badge
                + 22.0
                + self.controls_w()
                + self.timer_w
                + 16.0)
                .max(250.0)
        } else if self.hovered {
            226.0
        } else {
            212.0
        }
    }

    /// Centre of the pet in the view (`pet.x + pet.width/2`).
    pub fn pet_center(&self) -> f32 {
        PET_X + PET_SIZE * 0.5
    }

    pub fn pet_react(&mut self, name: &str, now: Ms) {
        self.pet.react(name, pet_event::duration_ms(name), now);
    }

    fn pet_rect(&self) -> Rect {
        Rect::new(self.frame.x + PET_X, self.frame.y + self.frame.h * 0.5 - PET_SIZE * 0.5, PET_SIZE, PET_SIZE)
    }

    fn timer_x(&self) -> f32 {
        self.frame.right() - 16.0 - self.timer_w
    }

    fn controls_visible(&self, now: Ms) -> bool {
        self.has_focus && self.ctl_op.value(now) > 0.01
    }

    fn damage_frame(&self, cx: &mut Cx) {
        cx.damage.add(self.frame.inflate(2.0, 2.0));
    }

    fn damage_timer(&self, cx: &mut Cx, old_w: f32) {
        let w = old_w.max(self.timer_w) + 2.0;
        cx.damage
            .add(Rect::new(self.frame.right() - 16.0 - w, self.frame.y, w, self.frame.h));
    }

    /// Everything right of the pet: the part that the swap animation moves and fades.
    fn damage_content(&self, cx: &mut Cx) {
        let f = self.frame;
        let x = f.x + PET_X + PET_SIZE;
        cx.damage
            .add(Rect::new(x, f.y, (f.right() - x).max(0.0), f.h).inflate(2.0, 2.0));
    }

    fn refresh_model(&mut self, cx: &mut Cx) {
        let st = cx.store.state();
        let n_focus = st.focus.len();
        let n_queue = st.queue.len();
        self.n_focus = n_focus;
        self.n_queue = n_queue;
        self.has_focus = n_focus > 0;
        let old_timer_w = self.timer_w;
        let mut changed = false;
        if n_focus > 0 {
            // `idx = max(0, min(shownIdx, count - 1))`
            let idx = self.shown_idx.min(n_focus - 1);
            self.idx = idx;
            let task = &st.focus[idx];
            if self.title != task.title || !self.inited {
                self.title.clear();
                self.title.push_str(&task.title);
                let style = title_style();
                let raw = cx.text.measure(&self.title, &style);
                self.title_w = raw.min(TITLE_MAX_W);
                self.title_draw = if raw > TITLE_MAX_W {
                    cx.text.elide(&self.title, &style, TITLE_MAX_W)
                } else {
                    self.title.clone()
                };
                changed = true;
            }
            let badge = if n_focus > 1 { format!("{}/{}", idx + 1, n_focus) } else { String::new() };
            if badge != self.badge {
                self.badge_w =
                    if badge.is_empty() { 0.0 } else { pill_width(cx.text, &badge, &pill_style(theme::TEXT, true)) };
                self.badge = badge;
                changed = true;
            }
            let timer = fmt_clock(task.elapsed(cx.epoch));
            if timer != self.timer {
                self.timer_w = cx.text.measure(&timer, &timer_style(theme::TEXT));
                self.timer = timer;
                if self.timer_w == old_timer_w {
                    self.damage_timer(cx, old_timer_w);
                } else {
                    changed = true;
                }
            }
            self.running = task.running;
            self.tick_at = model::next_tick(task, cx.epoch, cx.now, 1000.0);
        } else {
            self.idx = 0;
            self.title.clear();
            self.title_draw.clear();
            self.badge.clear();
            self.timer.clear();
            self.running = false;
            self.tick_at = None;
            let idle = if n_queue > 0 { format!("{n_queue} na fila") } else { "Palco vazio".to_string() };
            if idle != self.idle_text {
                self.idle_text = idle;
                changed = true;
            }
        }
        if changed {
            self.damage_frame(cx);
        }
    }

    /// The 5 s `Timer` that cycles between several focus tasks, and the swap it triggers.
    fn drive_cycle(&mut self, now: Ms) {
        let want = self.n_focus > 1 && !self.hovered && self.shown;
        if !want {
            self.cycle_at = None;
            return;
        }
        match self.cycle_at {
            None => self.cycle_at = Some(now + CYCLE_MS),
            Some(t) if now >= t => {
                self.cycle = (self.cycle + 1) % self.n_focus;
                self.swap_op.start(now);
                self.swap_y.start(now);
                self.swap_switch_at = Some(now + SWAP_OUT_MS);
                self.cycle_at = Some(if t + CYCLE_MS > now { t + CYCLE_MS } else { now + CYCLE_MS });
            }
            Some(_) => {}
        }
    }
}

impl View for CollapsedView {
    fn update(&mut self, cx: &mut Cx) {
        let now = cx.now;
        let visible = self.visible(now);
        let force = visible && !self.was_visible;
        self.pet.set_active(visible, now);
        let env_changed = cx.env.rev != self.last_env_rev;
        // swap: the shown task changes after the 140 ms fade-out
        let mut switched = false;
        if self.swap_switch_at.is_some_and(|t| now >= t) {
            self.swap_switch_at = None;
            self.shown_idx = self.cycle;
            switched = true;
        }
        if !self.inited
            || cx.rev != self.last_rev
            || env_changed
            || force
            || switched
            || (visible && self.tick_at.is_some_and(|t| now >= t))
        {
            self.refresh_model(cx);
            self.last_rev = cx.rev;
            self.inited = true;
            if env_changed {
                self.last_env_rev = cx.env.rev;
                self.damage_frame(cx);
            }
        }
        self.implicit_w = self.compute_implicit();
        self.drive_cycle(now);

        // swap animation (opacity + slide of everything right of the pet)
        let (so, sy) = (self.swap_op.value(now), self.swap_y.value(now));
        if (so - self.swap_shown.0).abs() >= 1.0 / 255.0
            || (sy - self.swap_shown.1).abs() >= 0.05
            || (switched && visible)
        {
            self.swap_shown = (so, sy);
            self.damage_content(cx);
        }

        // pet
        let st = cx.store.state();
        let ps = (st.settings.kind(), model::mood(cx.store, cx.epoch), cx.env.tip_talking, cx.env.accent);
        if ps != self.pet_state {
            if ps.0 != self.pet_state.0 {
                self.pet.set_kind(ps.0, now);
            }
            if ps.1 != self.pet_state.1 {
                self.pet.set_mood(ps.1, now);
            }
            if ps.2 != self.pet_state.2 {
                self.pet.set_talking(ps.2, now);
            }
            if ps.3 != self.pet_state.3 {
                self.pet.set_accent(ps.3);
            }
            self.pet_state = ps;
            cx.damage.add(self.pet_rect().inflate(self.pet.overdraw(), self.pet.overdraw()));
        }
        if self.pet.update(now) {
            cx.damage.add(self.pet_rect().inflate(self.pet.overdraw(), self.pet.overdraw()));
        }

        // timer colour
        let want = if self.running { cx.env.accent_text() } else { theme::TEXT2 };
        let before = self.timer_col.value(now);
        self.timer_col.animate_to(want, 200.0, now);
        if self.timer_col.value(now) != before {
            self.damage_timer(cx, self.timer_w);
        }

        // live dot: 900 + 900 ms InOutSine pulse, quantised so it repaints ~20x/s at most
        let live_on = self.running && visible && !self.hovered;
        if live_on {
            self.live_t0.get_or_insert(now);
        } else {
            self.live_t0 = None;
        }
        let lv = self.live_t0.map_or(1.0, |t0| live_level(now - t0));
        if lv != self.live_shown {
            self.live_shown = lv;
            let cy = self.frame.y + self.frame.h * 0.5;
            cx.damage.add(Rect::new(self.timer_x() - 13.0, cy - 3.0, 6.0, 6.0));
        }

        // hover controls
        if self.has_focus && self.controls_visible(now) {
            let icon = if self.running { Icon::Pause } else { Icon::Play };
            self.pause_btn.set_icon(cx, icon);
            self.pause_btn.tick(cx);
            self.done_btn.tick(cx);
        }
        if self.ctl_op.is_animating(now) || self.ctl_sc.is_animating(now) {
            cx.damage.add(self.controls_rect());
        }

        // fade / scale of the whole view
        if self.op.is_animating(now) || self.sc.is_animating(now) || visible != self.was_visible {
            self.damage_frame(cx);
        }
        self.was_visible = visible;
    }

    fn layout(&mut self, cx: &mut Cx, frame: Rect) {
        if frame != self.frame {
            cx.damage.add_moved(self.frame.inflate(2.0, 2.0), frame.inflate(2.0, 2.0));
            self.frame = frame;
        }
        let cy = frame.y + frame.h * 0.5;
        let right = self.timer_x() - 8.0;
        self.pause_btn.layout(cx, Rect::new(right - 56.0, cy - 13.0, 26.0, 26.0));
        self.done_btn.layout(cx, Rect::new(right - 26.0, cy - 13.0, 26.0, 26.0));
    }

    fn event(&mut self, cx: &mut Cx, ev: &Event) -> Handled {
        let now = cx.now;
        if !self.visible(now) {
            return Handled::No;
        }
        if !self.controls_visible(now) || !self.hovered {
            self.pause_btn.reset(cx);
            self.done_btn.reset(cx);
            return Handled::No;
        }
        let idx = self.idx;
        let a = self.pause_btn.pointer(cx, ev);
        if a.clicked {
            cx.msgs.push(crate::cx::Msg::Act(Action::TogglePause(idx)));
        }
        let b = self.done_btn.pointer(cx, ev);
        if b.clicked {
            cx.msgs.push(crate::cx::Msg::Act(Action::Complete(idx)));
        }
        [a.handled, b.handled]
            .into_iter()
            .max_by_key(|h| *h as u8)
            .unwrap_or(Handled::No)
    }

    fn paint(&mut self, s: &mut dyn Surface, pc: &PaintCx) {
        let now = pc.now;
        let op = self.op.value(now);
        if op <= 0.01 || self.frame.intersection(&pc.clip.inflate(40.0, 40.0)).is_none() {
            return;
        }
        let f = self.frame;
        let c = f.center();
        let cy = c.y;
        s.save();
        s.mul_opacity(op);
        let sc = self.sc.value(now);
        s.translate(c.x, c.y);
        s.scale(sc, sc);
        s.translate(-c.x, -c.y);

        // Under an identity transform a repaint of a few pixels (the pulsing live dot, the pet)
        // only needs the elements that touch the clip; any running animation paints everything.
        let steady = sc == 1.0 && self.swap_y.value(now) == 0.0 && self.swap_op.value(now) == 1.0;
        let touches = |r: Rect| !steady || r.intersects(&pc.clip);
        let od = self.pet.overdraw();
        if touches(self.pet_rect().inflate(od, od)) {
            self.pet.paint(s, self.pet_rect(), now);
        }

        if self.has_focus {
            // `swapBox`: opacity + `Translate` of title, badge, timer, dot and controls
            let so = self.swap_op.value(now);
            let sy = self.swap_y.value(now);
            if so > 0.0 {
                s.save();
                if so < 1.0 {
                    s.mul_opacity(so);
                }
                if sy != 0.0 {
                    s.translate(0.0, sy);
                }
                let tx = f.x + TITLE_X;
                if touches(Rect::new(tx - 2.0, cy - 14.0, self.title_w + 4.0, 28.0)) {
                    paint_text_centered(s, &self.title_draw, tx, cy, &title_style());
                }
                if !self.badge.is_empty() {
                    let bx = tx + self.title_w + 8.0;
                    let br = Rect::new(bx, cy - 8.5, self.badge_w, 17.0);
                    if touches(br.inflate(2.0, 2.0)) {
                        paint_pill(s, pc, br, &self.badge, 0.25, true);
                    }
                }
                let timer_col = self.timer_col.value(now);
                if touches(Rect::new(self.timer_x() - 2.0, cy - 14.0, self.timer_w + 4.0, 28.0)) {
                    paint_text_centered(s, &self.timer, self.timer_x(), cy, &timer_style(timer_col));
                }
                if !self.hovered {
                    let (col, a) = if self.running { (pc.env.accent, self.live_shown) } else { (theme::TEXT3, 1.0) };
                    if let Some(p) = path_circle(self.timer_x() - 7.0 - 3.0, cy, 3.0) {
                        s.fill_path(&p, &Fill::Solid(col.mul_alpha(a)));
                    }
                }
                let cop = self.ctl_op.value(now);
                if cop > 0.01 {
                    s.save();
                    s.mul_opacity(cop);
                    let r = self.controls_rect();
                    let cc = r.center();
                    let k = self.ctl_sc.value(now);
                    s.translate(cc.x, cc.y);
                    s.scale(k, k);
                    s.translate(-cc.x, -cc.y);
                    self.pause_btn.paint(s, now);
                    self.done_btn.paint(s, now);
                    s.restore();
                }
                s.restore();
            }
        } else {
            paint_text_centered(s, &self.idle_text, f.x + TITLE_X, cy, &idle_style());
            let col = if self.hovered { theme::TEXT } else { theme::TEXT3 };
            paint_icon(s, Icon::Plus, foco_base::Point::new(f.right() - 16.0 - 7.5, cy), 15.0, col, 1.8);
        }
        s.restore();
    }

    fn next_wake(&self, now: Ms) -> Wake {
        let mut w = Wake::Idle;
        let visible = self.visible(now);
        if self.op.is_animating(now)
            || self.sc.is_animating(now)
            || self.ctl_op.is_animating(now)
            || self.ctl_sc.is_animating(now)
            || self.timer_col.is_animating(now)
            || self.swap_op.is_running(now)
        {
            w = Wake::Frame;
        }
        if !visible {
            return w;
        }
        w = w.merge_at(self.pet.next_wake(now), now);
        // live dot: wake exactly when its quantised opacity next changes
        if let Some(t0) = self.live_t0 {
            let th = PI * ((now - t0) as f32) / LIVE_HALF_MS;
            if let Some(d) = next_level_change(0.625, 0.375, LIVE_STEP, th) {
                // never sooner than 50 ms: after a late wake the phase may already be past the
                // next level, and a skipped step of a 6 px dot is invisible
                let t = (now + (d / PI * LIVE_HALF_MS) as Ms + 0.5).max(now + LIVE_MIN_GAP_MS);
                w = w.merge_at(Wake::at(now, t), now);
            }
        }
        if let Some(t) = self.tick_at {
            w = w.merge_at(Wake::at(now, t), now);
        }
        if let Some(t) = self.cycle_at {
            w = w.merge_at(Wake::at(now, t), now);
        }
        if self.controls_visible(now) {
            w = w
                .merge_at(self.pause_btn.next_wake(now), now)
                .merge_at(self.done_btn.next_wake(now), now);
        }
        w
    }

    fn bounds(&self) -> Rect {
        let pr = self.pet_rect();
        self.frame
            .union(&pr.inflate(self.pet.overdraw(), self.pet.overdraw()))
            .inflate(2.0, 2.0)
    }
}

impl CollapsedView {
    fn controls_rect(&self) -> Rect {
        self.pause_btn.rect().union(&self.done_btn.rect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cx::Msg;
    use crate::testkit::Harness;
    use foco_base::{Button, Point, Task};

    fn task(h: &Harness, title: &str, running: bool, elapsed_ms: i64) -> Task {
        Task {
            tid: title.into(),
            title: title.into(),
            running,
            started_at: if running { h.epoch as i64 - elapsed_ms } else { 0 },
            acc_ms: if running { 0 } else { elapsed_ms },
            ..Task::default()
        }
    }

    fn step(h: &mut Harness, v: &mut CollapsedView, now: Ms) {
        h.with_cx(now, |cx| {
            v.update(cx);
            let w = v.implicit_width();
            let hh = if v.hovered { 42.0 } else { 38.0 };
            v.layout(cx, Rect::new((760.0 - w) / 2.0, 0.0, w, hh));
        });
    }

    #[test]
    fn idle_widths_and_fade() {
        let mut h = Harness::new();
        let mut v = CollapsedView::new();
        step(&mut h, &mut v, 0.0);
        assert_eq!(v.implicit_width(), 212.0);
        v.set_hovered(true, 0.0);
        step(&mut h, &mut v, 0.0);
        assert_eq!(v.implicit_width(), 226.0);
        v.set_hovered(false, 0.0);
        assert!(!v.visible(0.0));
        v.set_shown(true, 0.0);
        assert!(!v.visible(100.0), "120 ms pause before the fade-in");
        assert!(v.visible(200.0));
        assert!(matches!(v.next_wake(200.0), Wake::Frame));
        v.set_shown(false, 1000.0);
        assert!(!v.visible(1100.0), "90 ms fade-out");
    }

    #[test]
    fn width_follows_the_formula() {
        let mut h = Harness::new();
        let t = task(&h, "Escrever relatório", true, 5_000);
        h.store.state.focus.push(t);
        let mut v = CollapsedView::new();
        v.set_shown(true, 0.0);
        step(&mut h, &mut v, 1000.0);
        // FakeText: 13 px => 7.15 per char (+ no spacing)
        let title_w: f32 = 18.0 * 7.15;
        let timer_w = 5.0 * 7.15; // "00:06"
        let expect = (14.0 + 26.0 + 9.0 + title_w + 22.0 + timer_w + 16.0).max(250.0);
        assert!((v.implicit_width() - expect).abs() < 0.01, "{} vs {expect}", v.implicit_width());
        // hovered with focus: +62
        v.set_hovered(true, 1000.0);
        step(&mut h, &mut v, 1000.0);
        assert!((v.implicit_width() - (expect + 62.0).max(250.0)).abs() < 0.01);
        // a second focus task adds the badge
        v.set_hovered(false, 1000.0);
        let t2 = task(&h, "Outra", false, 0);
        h.store.state.focus.push(t2);
        h.rev += 1;
        step(&mut h, &mut v, 1001.0);
        let badge_w = 3.0 * 5.5 + 12.0; // "1/2" at 10 px
        assert!(
            (v.implicit_width() - (14.0 + 26.0 + 9.0 + title_w + badge_w + 8.0 + 22.0 + timer_w + 16.0)).abs() < 0.01
        );
    }

    #[test]
    fn long_titles_are_capped_at_230() {
        let mut h = Harness::new();
        let t = task(&h, &"x".repeat(80), false, 0);
        h.store.state.focus.push(t);
        let mut v = CollapsedView::new();
        step(&mut h, &mut v, 0.0);
        let expect = 14.0 + 26.0 + 9.0 + 230.0 + 22.0 + 5.0 * 7.15 + 16.0;
        assert!((v.implicit_width() - expect).abs() < 0.01);
    }

    #[test]
    fn clock_ticks_once_per_second_and_idles_otherwise() {
        let mut h = Harness::new();
        let t = task(&h, "T", true, 0);
        h.store.state.focus.push(t);
        let mut v = CollapsedView::new();
        v.set_shown(true, 0.0);
        step(&mut h, &mut v, 1000.0); // fully faded in by now
        step(&mut h, &mut v, 1500.0);
        h.damage.clear();
        // between ticks: no damage from the clock (live dot excluded: damage only on visible change)
        let wake = v.next_wake(1500.0);
        assert!(matches!(wake, Wake::Ambient | Wake::At(_)), "{wake:?}");
        let t = v.tick_at.expect("running task schedules a tick");
        step(&mut h, &mut v, t + 1.0);
        assert!(!h.damage.is_empty(), "timer text damaged on the tick");
        assert!(v.timer.starts_with("00:0"));
        // paused task: no tick, no wake besides the pet
        h.store.state.focus[0].running = false;
        h.store.state.focus[0].acc_ms = 3000;
        h.rev += 1;
        step(&mut h, &mut v, t + 100.0);
        assert!(v.tick_at.is_none());
        assert_eq!(v.next_wake(t + 5000.0), Wake::Idle, "idle = zero wakeups with the fake pet inactive");
    }

    #[test]
    fn no_focus_shows_queue_text() {
        let mut h = Harness::new();
        let mut v = CollapsedView::new();
        step(&mut h, &mut v, 0.0);
        assert_eq!(v.idle_text, "Palco vazio");
        h.store.state.queue.push(task(&h, "a", false, 0));
        h.store.state.queue.push(task(&h, "b", false, 0));
        h.rev += 1;
        step(&mut h, &mut v, 1.0);
        assert_eq!(v.idle_text, "2 na fila");
    }

    #[test]
    fn hover_controls_send_actions() {
        let mut h = Harness::new();
        let t = task(&h, "T", true, 0);
        h.store.state.focus.push(t);
        let mut v = CollapsedView::new();
        v.set_shown(true, 0.0);
        v.set_hovered(true, 100.0);
        step(&mut h, &mut v, 1000.0);
        step(&mut h, &mut v, 1000.0);
        let r = v.done_btn.rect();
        let c = r.center();
        let down = Event::PointerDown { pos: Point::new(c.x, c.y), button: Button::Left, clicks: 1 };
        let up = Event::PointerUp { pos: Point::new(c.x, c.y), button: Button::Left };
        let r1 = h.with_cx(1000.0, |cx| v.event(cx, &down));
        assert_eq!(r1, Handled::Grab);
        h.with_cx(1000.0, |cx| v.event(cx, &up));
        assert_eq!(h.take_msgs(), vec![Msg::Act(Action::Complete(0))]);
        let c = v.pause_btn.rect().center();
        let down = Event::PointerDown { pos: Point::new(c.x, c.y), button: Button::Left, clicks: 1 };
        let up = Event::PointerUp { pos: Point::new(c.x, c.y), button: Button::Left };
        h.with_cx(1000.0, |cx| v.event(cx, &down));
        h.with_cx(1000.0, |cx| v.event(cx, &up));
        assert_eq!(h.take_msgs(), vec![Msg::Act(Action::TogglePause(0))]);
        // no controls without hover
        v.set_hovered(false, 1100.0);
        h.with_cx(2000.0, |cx| assert_eq!(v.event(cx, &down), Handled::No));
    }

    fn shown_view(h: &mut Harness, hovered: bool) -> CollapsedView {
        let mut v = CollapsedView::new();
        v.set_shown(true, 0.0);
        if hovered {
            v.set_hovered(true, 0.0);
        }
        step(h, &mut v, 1000.0);
        v
    }

    #[test]
    fn cycles_every_5s_with_swap_animation() {
        let mut h = Harness::new();
        for t in ["Um", "Dois", "Tres"] {
            let tk = task(&h, t, false, 0);
            h.store.state.focus.push(tk);
        }
        let mut v = shown_view(&mut h, false);
        assert_eq!(v.title, "Um");
        assert_eq!(v.badge, "1/3");
        let t0 = v.cycle_at.expect("timer runs with >1 tasks, not hovered, shown");
        assert!((t0 - 6000.0).abs() < 1.0, "5 s after it started running: {t0}");
        assert_eq!(v.next_wake(1100.0).merge(Wake::Idle), v.next_wake(1100.0));
        // before the deadline nothing swaps
        step(&mut h, &mut v, t0 - 1.0);
        assert_eq!(v.title, "Um");
        assert!(!v.swap_op.is_running(t0 - 1.0));
        // trigger: fade-out starts, text still the old one
        step(&mut h, &mut v, t0 + 1.0);
        assert_eq!(v.cycle, 1);
        assert!(v.swap_op.is_running(t0 + 2.0));
        assert!(matches!(v.next_wake(t0 + 2.0), Wake::Frame));
        assert_eq!(v.title, "Um");
        let t0 = t0 + 1.0; // the swap started on that update
        let (o, y) = (v.swap_op.value(t0 + 70.0), v.swap_y.value(t0 + 70.0));
        assert!(o < 0.6 && o > 0.4, "linear fade {o}");
        assert!(y < 0.0 && y > -3.0, "InCubic slide up {y}");
        assert!((v.swap_y.value(t0 + 140.0) - 7.0).abs() < 1e-3, "jumps to +7 after the switch");
        // 140 ms later the shown task changes and slides in from y = +7
        step(&mut h, &mut v, t0 + 150.0);
        assert_eq!(v.title, "Dois");
        let t0 = t0 - 1.0;
        assert_eq!(v.badge, "2/3");
        assert!(v.swap_y.value(t0 + 150.0) > 5.0);
        assert!((v.swap_op.value(t0 + 140.0 + 110.0) - 0.5).abs() < 0.01, "220 ms fade-in");
        assert!(v.swap_y.value(t0 + 140.0 + 260.0).abs() < 1e-3, "260 ms OutCubic");
        step(&mut h, &mut v, t0 + 600.0);
        assert!(!v.swap_op.is_running(t0 + 600.0));
        assert_eq!(v.swap_op.value(t0 + 600.0), 1.0);
        // buttons act on the task being shown
        assert_eq!(v.idx, 1);
        // next cycle at +5 s, wraps around
        let t1 = v.cycle_at.unwrap();
        assert!((t1 - (t0 + 5000.0)).abs() < 1.0);
        step(&mut h, &mut v, t1 + 1.0);
        step(&mut h, &mut v, t1 + 200.0);
        step(&mut h, &mut v, t1 + 5000.0 + 200.0);
        assert_eq!(v.cycle, 0, "1 -> 2 -> 0");
    }

    #[test]
    fn cycle_stops_on_hover_single_task_or_hidden() {
        let mut h = Harness::new();
        let tk = task(&h, "Um", false, 0);
        h.store.state.focus.push(tk);
        let mut v = shown_view(&mut h, false);
        assert!(v.cycle_at.is_none(), "one task: no cycle");
        assert_eq!(v.next_wake(2000.0), Wake::Idle);
        let tk = task(&h, "Dois", false, 0);
        h.store.state.focus.push(tk);
        h.rev += 1;
        step(&mut h, &mut v, 1100.0);
        assert!(v.cycle_at.is_some());
        v.set_hovered(true, 1200.0);
        step(&mut h, &mut v, 1200.0);
        assert!(v.cycle_at.is_none(), "hovered: paused");
        v.set_hovered(false, 1300.0);
        step(&mut h, &mut v, 1300.0);
        assert!((v.cycle_at.unwrap() - 6300.0).abs() < 1.0, "timer restarts at 5 s");
        v.set_shown(false, 1400.0);
        step(&mut h, &mut v, 1400.0);
        assert!(v.cycle_at.is_none(), "not shown: stopped");
    }

    #[test]
    fn swap_damage_is_limited_to_the_content_right_of_the_pet() {
        let mut h = Harness::new();
        for t in ["Um", "Dois"] {
            let tk = task(&h, t, false, 0);
            h.store.state.focus.push(tk);
        }
        let mut v = shown_view(&mut h, false);
        let t0 = v.cycle_at.unwrap();
        step(&mut h, &mut v, t0 - 5.0);
        h.damage.clear();
        step(&mut h, &mut v, t0 + 30.0); // the swap starts here
        h.damage.clear();
        step(&mut h, &mut v, t0 + 60.0);
        let r = h.damage.take();
        assert!(!r.is_empty());
        let pet_right = ((v.frame.x + PET_X + PET_SIZE) * crate::S) as i32;
        assert!(r.iter().all(|d| d.x >= pet_right - 6), "pet untouched: {r:?}");
    }

    #[test]
    fn live_dot_wakes_are_exact_and_sparse() {
        let mut h = Harness::new();
        let tk = task(&h, "T", true, 0);
        h.store.state.focus.push(tk);
        let mut v = shown_view(&mut h, false);
        step(&mut h, &mut v, 1500.0);
        // walk one full 1800 ms pulse following only the requested wakes
        let start = 1500.0;
        let mut now = start;
        let mut n = 0;
        let mut levels = vec![v.live_shown];
        while now < start + 1800.0 && n < 400 {
            let next = match v.next_wake(now) {
                Wake::At(t) => t,
                Wake::Frame => now + 16.7,
                Wake::Ambient => now + 33.3,
                Wake::Idle => break,
            };
            // the clock tick (1/s) is also in the mix
            now = next;
            step(&mut h, &mut v, now);
            if levels.last() != Some(&v.live_shown) {
                levels.push(v.live_shown);
            }
            n += 1;
        }
        assert!(n <= 70, "{n} wakes in one pulse (incl. the 1 Hz tick and the fake 30 Hz pet)");
        assert!(levels.iter().all(|l| (0.25..=1.0).contains(l)));
        assert!(levels.iter().any(|l| *l < 0.4), "reaches the dim end");
    }

    #[test]
    fn next_level_change_finds_the_next_quantisation_step() {
        use super::next_level_change;
        // v = 0.625 + 0.375 cos(theta), step 1/24: at theta 0 (v=1) the next step is below 1 - 1/48
        let d = next_level_change(0.625, 0.375, 1.0 / 24.0, 0.0).unwrap();
        let v = 0.625 + 0.375 * (d + 1e-3).cos();
        let q = |x: f32| (x * 24.0).round() / 24.0;
        assert!(q(v) < q(1.0), "{v}");
        let v0 = 0.625 + 0.375 * (d - 1e-3).cos();
        assert_eq!(q(v0), q(1.0));
        assert_eq!(next_level_change(0.0, 0.0, 0.1, 0.0), None);
    }

    fn snapshot(name: &str, tasks: &[&str], mid_swap: bool, hovered: bool) {
        use crate::notch::{paint_notch, NotchGeom};
        let mut h = Harness::new();
        for (i, t) in tasks.iter().enumerate() {
            let tk = task(&h, t, i == 0, 754_000);
            h.store.state.focus.push(tk);
        }
        let mut v = shown_view(&mut h, hovered);
        let mut now = 1500.0;
        if mid_swap {
            let t0 = v.cycle_at.unwrap();
            now = t0 + 190.0; // new title sliding in, ~mid fade
            step(&mut h, &mut v, t0 + 1.0);
            step(&mut h, &mut v, t0 + 150.0);
        }
        step(&mut h, &mut v, now);
        step(&mut h, &mut v, now);
        let mut px = foco_base::PixSurface::new(792, 70).unwrap();
        px.clear(foco_base::Color::rgb8(0x8a, 0x8d, 0x94));
        px.scale(crate::S, crate::S);
        let g = NotchGeom::new(v.implicit_width(), v.frame.h, 19.0);
        paint_notch(&mut px, &g, 0.0);
        px.save();
        px.clip_rect(g.clip_rect());
        h.with_pc(now, Rect::new(0.0, 0.0, 760.0, 70.0), |pc| v.paint(&mut px, pc));
        px.restore();
        let d = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/ui-test/wave3a");
        std::fs::create_dir_all(&d).unwrap();
        px.save_png(d.join(name)).unwrap();
    }

    #[test]
    fn snapshots_wave3a() {
        snapshot("collapsed-1task.png", &["Escrever o relatório trimestral"], false, false);
        snapshot("collapsed-2tasks.png", &["Escrever o relatório trimestral", "Responder e-mails"], false, false);
        snapshot(
            "collapsed-2tasks-midswap.png",
            &["Escrever o relatório trimestral", "Responder e-mails"],
            true,
            false,
        );
        snapshot("collapsed-2tasks-hover.png", &["Escrever o relatório trimestral", "Responder e-mails"], false, true);
    }
}
