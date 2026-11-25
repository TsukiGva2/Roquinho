//! Animation primitives: Qt easing curves, tweens, `Anim` (a Behavior-like animated scalar),
//! `Seq` (SequentialAnimation) and `Spring` (QQuickSpringAnimation port).

use crate::time::Ms;
use std::f64::consts::PI;

/// Qt `QEasingCurve` shapes used by the QML (formulas match qeasingcurve.cpp).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Easing {
    Linear,
    InQuad,
    OutQuad,
    InOutQuad,
    InCubic,
    OutCubic,
    InOutCubic,
    InSine,
    OutSine,
    InOutSine,
    /// `easing.overshoot`, Qt default 1.70158 (see [`Easing::OUT_BACK`]).
    OutBack { overshoot: f32 },
    InBack { overshoot: f32 },
    /// `easing.amplitude` / `easing.period`, Qt defaults 1.0 / 0.3 (see [`Easing::OUT_ELASTIC`]).
    OutElastic { amplitude: f32, period: f32 },
    OutExpo,
    InOutQuart,
}

impl Easing {
    /// `Easing.OutBack` with Qt's default overshoot.
    pub const OUT_BACK: Easing = Easing::OutBack { overshoot: 1.70158 };
    pub const IN_BACK: Easing = Easing::InBack { overshoot: 1.70158 };
    /// `Easing.OutElastic` with Qt's default amplitude/period.
    pub const OUT_ELASTIC: Easing = Easing::OutElastic { amplitude: 1.0, period: 0.3 };

    /// Maps progress `t` (clamped to 0..1) to eased progress.
    pub fn apply(self, t: f32) -> f32 {
        let t = (t as f64).clamp(0.0, 1.0);
        let r = match self {
            Easing::Linear => t,
            Easing::InQuad => t * t,
            Easing::OutQuad => -t * (t - 2.0),
            Easing::InOutQuad => {
                let t = t * 2.0;
                if t < 1.0 {
                    0.5 * t * t
                } else {
                    let t = t - 1.0;
                    -0.5 * (t * (t - 2.0) - 1.0)
                }
            }
            Easing::InCubic => t * t * t,
            Easing::OutCubic => {
                let t = t - 1.0;
                t * t * t + 1.0
            }
            Easing::InOutCubic => {
                let t = t * 2.0;
                if t < 1.0 {
                    0.5 * t * t * t
                } else {
                    let t = t - 2.0;
                    0.5 * (t * t * t + 2.0)
                }
            }
            Easing::InSine => -(t * PI / 2.0).cos() + 1.0,
            Easing::OutSine => (t * PI / 2.0).sin(),
            Easing::InOutSine => -0.5 * ((PI * t).cos() - 1.0),
            Easing::OutBack { overshoot } => {
                let s = overshoot as f64;
                let t = t - 1.0;
                t * t * ((s + 1.0) * t + s) + 1.0
            }
            Easing::InBack { overshoot } => {
                let s = overshoot as f64;
                t * t * ((s + 1.0) * t - s)
            }
            Easing::OutElastic { amplitude, period } => {
                if t == 0.0 {
                    0.0
                } else if t >= 1.0 {
                    1.0
                } else {
                    let (mut a, p) = (amplitude as f64, period as f64);
                    let s;
                    if a < 1.0 {
                        a = 1.0;
                        s = p / 4.0;
                    } else {
                        s = p / (2.0 * PI) * (1.0 / a).asin();
                    }
                    a * 2f64.powf(-10.0 * t) * ((t - s) * (2.0 * PI) / p).sin() + 1.0
                }
            }
            Easing::OutExpo => {
                if t >= 1.0 {
                    1.0
                } else {
                    -(2f64.powf(-10.0 * t)) + 1.0
                }
            }
            Easing::InOutQuart => {
                let t = t * 2.0;
                if t < 1.0 {
                    0.5 * t * t * t * t
                } else {
                    let t = t - 2.0;
                    -0.5 * (t * t * t * t - 2.0)
                }
            }
        };
        r as f32
    }
}

/// A single from→to interpolation with optional start delay.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tween {
    pub from: f32,
    pub to: f32,
    pub start: Ms,
    pub delay: Ms,
    pub dur: Ms,
    pub easing: Easing,
}

impl Tween {
    /// Value at `now` (holds `from` during the delay and `to` afterwards).
    pub fn value(&self, now: Ms) -> f32 {
        let e = now - self.start - self.delay;
        if e <= 0.0 {
            return self.from;
        }
        if e >= self.dur {
            return self.to;
        }
        let p = self.easing.apply((e / self.dur) as f32);
        self.from + (self.to - self.from) * p
    }
    /// True once `now` is past delay + duration.
    pub fn done(&self, now: Ms) -> bool {
        now - self.start >= self.delay + self.dur
    }
}

/// An animated scalar, like a QML `Behavior on x { NumberAnimation {…} }`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Anim {
    tw: Tween,
}

impl Anim {
    /// Starts at rest at `v`.
    pub fn new(v: f32) -> Anim {
        Anim { tw: Tween { from: v, to: v, start: 0.0, delay: 0.0, dur: 0.0, easing: Easing::Linear } }
    }
    /// Jumps to `v` (cancels any animation).
    pub fn set(&mut self, v: f32) {
        *self = Anim::new(v);
    }
    /// Animates from the current value to `to`. A no-op when already heading to `to`
    /// (QML Behaviors do not retrigger on an unchanged target).
    pub fn animate_to(&mut self, to: f32, dur: Ms, easing: Easing, now: Ms) {
        self.animate_to_delayed(to, 0.0, dur, easing, now);
    }
    /// Like [`animate_to`](Self::animate_to) with a start delay (value holds meanwhile).
    pub fn animate_to_delayed(&mut self, to: f32, delay: Ms, dur: Ms, easing: Easing, now: Ms) {
        if to == self.tw.to {
            return;
        }
        let from = self.value(now);
        if dur <= 0.0 && delay <= 0.0 {
            self.set(to);
            return;
        }
        self.tw = Tween { from, to, start: now, delay, dur: dur.max(0.0), easing };
    }
    pub fn value(&self, now: Ms) -> f32 {
        self.tw.value(now)
    }
    /// The value it is heading to.
    pub fn target(&self) -> f32 {
        self.tw.to
    }
    pub fn is_animating(&self, now: Ms) -> bool {
        !self.tw.done(now)
    }
    /// Time at which the animation ends (`None` if at rest).
    pub fn end_time(&self, now: Ms) -> Option<Ms> {
        self.is_animating(now).then_some(self.tw.start + self.tw.delay + self.tw.dur)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Seg {
    To { to: f32, dur: Ms, easing: Easing },
    Pause(Ms),
    Set(f32),
}

/// A SequentialAnimation: a chain of value segments, pauses and jumps, optionally looping.
///
/// Build with `Seq::new(initial).to(..).pause(..)`, then `start(now)`. In a loop the first
/// segment of iteration N>0 starts from the final value of the previous iteration (as Qt does).
#[derive(Clone, Debug, PartialEq)]
pub struct Seq {
    initial: f32,
    segs: Vec<Seg>,
    looping: bool,
    started: Option<Ms>,
}

impl Seq {
    /// A sequence whose first segment starts from `initial`.
    pub fn new(initial: f32) -> Seq {
        Seq { initial, segs: Vec::new(), looping: false, started: None }
    }
    /// Appends an eased move to `to` over `dur` ms.
    pub fn to(mut self, to: f32, dur: Ms, easing: Easing) -> Seq {
        self.segs.push(Seg::To { to, dur: dur.max(0.0), easing });
        self
    }
    /// Appends a pause (value held).
    pub fn pause(mut self, dur: Ms) -> Seq {
        self.segs.push(Seg::Pause(dur.max(0.0)));
        self
    }
    /// Appends an instantaneous jump (a `PropertyAction`).
    pub fn set(mut self, v: f32) -> Seq {
        self.segs.push(Seg::Set(v));
        self
    }
    /// Loops forever (`loops: Animation.Infinite`).
    pub fn looping(mut self) -> Seq {
        self.looping = true;
        self
    }
    /// (Re)starts at `now`.
    pub fn start(&mut self, now: Ms) {
        self.started = Some(now);
    }
    /// Stops; `value` returns the initial value again.
    pub fn stop(&mut self) {
        self.started = None;
    }
    /// Total duration of one pass.
    pub fn duration(&self) -> Ms {
        self.segs
            .iter()
            .map(|s| match s {
                Seg::To { dur, .. } | Seg::Pause(dur) => *dur,
                Seg::Set(_) => 0.0,
            })
            .sum()
    }
    fn end_value(&self) -> f32 {
        let mut v = self.initial;
        for s in &self.segs {
            match s {
                Seg::To { to, .. } => v = *to,
                Seg::Set(x) => v = *x,
                Seg::Pause(_) => {}
            }
        }
        v
    }
    /// Running = started and (looping or not yet past the end).
    pub fn is_running(&self, now: Ms) -> bool {
        match self.started {
            None => false,
            Some(s) => self.looping || now - s < self.duration(),
        }
    }
    /// Current value; holds the final value after a non-looping run ends.
    pub fn value(&self, now: Ms) -> f32 {
        let Some(start) = self.started else { return self.initial };
        let total = self.duration();
        let mut e = (now - start).max(0.0);
        let mut v = self.initial;
        if total <= 0.0 {
            return self.end_value();
        }
        if e >= total {
            if !self.looping {
                return self.end_value();
            }
            let n = (e / total).floor();
            e -= n * total;
            v = self.end_value();
        }
        for s in &self.segs {
            match *s {
                Seg::Set(x) => v = x,
                Seg::Pause(d) => {
                    if e < d {
                        return v;
                    }
                    e -= d;
                }
                Seg::To { to, dur, easing } => {
                    if e < dur {
                        let p = easing.apply((e / dur) as f32);
                        return v + (to - v) * p;
                    }
                    e -= dur;
                    v = to;
                }
            }
        }
        v
    }
}

/// Port of Qt 6 `QQuickSpringAnimation`, integrating in fixed 16 ms steps.
///
/// Leftover time (less than a step) carries to the next `step` call exactly as in Qt.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spring {
    cur: f64,
    to: f64,
    velocity: f64,
    spring: f64,
    damping: f64,
    epsilon: f64,
    mass: f64,
    last: Ms,
    settled: bool,
}

const STEP_MS: f64 = 16.0;

impl Spring {
    /// New settled spring at `value`. `epsilon` is Qt's default 0.01 in QML (pass it explicitly).
    pub fn new(value: f32, spring: f32, damping: f32, epsilon: f32) -> Spring {
        Spring {
            cur: value as f64,
            to: value as f64,
            velocity: 0.0,
            spring: spring as f64,
            damping: damping as f64,
            epsilon: epsilon as f64,
            mass: 1.0,
            last: 0.0,
            settled: true,
        }
    }
    /// Sets the mass (Qt default 1.0; only used in the integrator when != 1).
    pub fn with_mass(mut self, mass: f32) -> Spring {
        self.mass = mass as f64;
        self
    }
    /// Retargets; the spring keeps its current value and velocity. `now` starts the step clock
    /// if the spring was at rest.
    pub fn set_target(&mut self, to: f32, now: Ms) {
        let to = to as f64;
        if to == self.to && !self.settled {
            return;
        }
        self.to = to;
        if self.settled {
            self.last = now;
        }
        if (self.to - self.cur).abs() < self.epsilon && self.velocity.abs() < self.epsilon {
            self.cur = self.to;
            self.velocity = 0.0;
            self.settled = true;
        } else {
            self.settled = false;
        }
    }
    /// Jumps to `v` and rests there.
    pub fn jump(&mut self, v: f32) {
        self.cur = v as f64;
        self.to = v as f64;
        self.velocity = 0.0;
        self.settled = true;
    }
    pub fn value(&self) -> f32 {
        self.cur as f32
    }
    pub fn target(&self) -> f32 {
        self.to as f32
    }
    pub fn velocity(&self) -> f32 {
        self.velocity as f32
    }
    pub fn settled(&self) -> bool {
        self.settled
    }
    /// Advances to `now` in whole 16 ms steps and returns the value.
    pub fn step(&mut self, now: Ms) -> f32 {
        if self.settled {
            return self.cur as f32;
        }
        let elapsed = now - self.last;
        let count = (elapsed / STEP_MS).floor();
        if count >= 1.0 {
            let use_mass = self.mass != 1.0;
            for _ in 0..count as u64 {
                let diff = self.to - self.cur;
                self.velocity = if use_mass {
                    self.velocity + (self.spring * diff - self.damping * self.velocity) / self.mass
                } else {
                    self.velocity + self.spring * diff - self.damping * self.velocity
                };
                self.cur += self.velocity * STEP_MS / 1000.0;
            }
            if self.velocity.abs() < self.epsilon && (self.to - self.cur).abs() < self.epsilon {
                self.velocity = 0.0;
                self.cur = self.to;
                self.settled = true;
            }
            self.last = now - (elapsed - count * STEP_MS);
        }
        self.cur as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Easing; 15] = [
        Easing::Linear,
        Easing::InQuad,
        Easing::OutQuad,
        Easing::InOutQuad,
        Easing::InCubic,
        Easing::OutCubic,
        Easing::InOutCubic,
        Easing::InSine,
        Easing::OutSine,
        Easing::InOutSine,
        Easing::OUT_BACK,
        Easing::IN_BACK,
        Easing::OUT_ELASTIC,
        Easing::OutExpo,
        Easing::InOutQuart,
    ];

    #[test]
    fn easing_endpoints() {
        for e in ALL {
            assert!(e.apply(0.0).abs() < 1e-5, "{e:?} at 0");
            assert!((e.apply(1.0) - 1.0).abs() < 1e-5, "{e:?} at 1");
        }
        let e = Easing::OutElastic { amplitude: 1.2, period: 0.35 };
        assert_eq!(e.apply(0.0), 0.0);
        assert_eq!(e.apply(1.0), 1.0);
    }

    #[test]
    fn easing_monotonic_where_expected() {
        let mono = [
            Easing::Linear, Easing::InQuad, Easing::OutQuad, Easing::InOutQuad, Easing::InCubic,
            Easing::OutCubic, Easing::InOutCubic, Easing::InSine, Easing::OutSine, Easing::InOutSine,
            Easing::OutExpo, Easing::InOutQuart,
        ];
        for e in mono {
            let mut prev = -1.0;
            for i in 0..=100 {
                let v = e.apply(i as f32 / 100.0);
                assert!(v >= prev - 1e-6, "{e:?} not monotonic at {i}");
                prev = v;
            }
        }
    }

    #[test]
    fn easing_known_values() {
        assert!((Easing::OutQuad.apply(0.5) - 0.75).abs() < 1e-6);
        assert!((Easing::InCubic.apply(0.5) - 0.125).abs() < 1e-6);
        assert!((Easing::OutCubic.apply(0.5) - 0.875).abs() < 1e-6);
        assert!((Easing::InOutSine.apply(0.5) - 0.5).abs() < 1e-6);
        assert!((Easing::InOutQuad.apply(0.25) - 0.125).abs() < 1e-6);
        // OutBack overshoots above 1 mid-way
        assert!(Easing::OUT_BACK.apply(0.7) > 1.0);
        assert!((Easing::OUT_BACK.apply(0.5) - 1.0876975).abs() < 1e-4);
        assert!(Easing::OUT_ELASTIC.apply(0.2) > 1.0);
        assert_eq!(Easing::Linear.apply(2.0), 1.0);
    }

    #[test]
    fn tween_basic() {
        let t = Tween { from: 0.0, to: 10.0, start: 100.0, delay: 50.0, dur: 100.0, easing: Easing::Linear };
        assert_eq!(t.value(0.0), 0.0);
        assert_eq!(t.value(150.0), 0.0);
        assert!((t.value(200.0) - 5.0).abs() < 1e-5);
        assert_eq!(t.value(400.0), 10.0);
        assert!(!t.done(249.0) && t.done(250.0));
    }

    #[test]
    fn anim_behaviour() {
        let mut a = Anim::new(0.0);
        assert!(!a.is_animating(0.0));
        a.animate_to(1.0, 100.0, Easing::Linear, 1000.0);
        assert!(a.is_animating(1050.0));
        assert!((a.value(1050.0) - 0.5).abs() < 1e-5);
        // retarget starts from current value
        a.animate_to(0.0, 100.0, Easing::Linear, 1050.0);
        assert!((a.value(1050.0) - 0.5).abs() < 1e-5);
        assert_eq!(a.value(1150.0), 0.0);
        assert_eq!(a.target(), 0.0);
        let mut d = Anim::new(0.0);
        d.animate_to_delayed(1.0, 100.0, 100.0, Easing::Linear, 0.0);
        assert_eq!(d.value(50.0), 0.0);
        assert!((d.value(150.0) - 0.5).abs() < 1e-5);
        assert_eq!(d.end_time(0.0), Some(200.0));
        d.set(3.0);
        assert_eq!(d.value(0.0), 3.0);
    }

    #[test]
    fn seq_pulse_loops() {
        let mut s = Seq::new(1.0).to(0.25, 900.0, Easing::InOutSine).to(1.0, 900.0, Easing::InOutSine).looping();
        assert_eq!(s.value(0.0), 1.0);
        s.start(0.0);
        assert!((s.value(900.0) - 0.25).abs() < 1e-5);
        assert!((s.value(1800.0) - 1.0).abs() < 1e-5);
        // second iteration: starts from the end value (1.0) going to 0.25
        assert!((s.value(1800.0 + 900.0) - 0.25).abs() < 1e-5);
        assert!((s.value(1800.0 + 450.0) - 0.625).abs() < 1e-5);
        assert!(s.is_running(1e9));
    }

    #[test]
    fn seq_shake() {
        let mut s = Seq::new(0.0)
            .to(-6.0, 50.0, Easing::OutQuad)
            .to(6.0, 70.0, Easing::InOutQuad)
            .to(-3.0, 60.0, Easing::InOutQuad)
            .to(0.0, 60.0, Easing::InOutQuad);
        s.start(10.0);
        assert!((s.value(60.0) + 6.0).abs() < 1e-5);
        assert!((s.value(130.0) - 6.0).abs() < 1e-5);
        assert_eq!(s.duration(), 240.0);
        assert!(s.is_running(200.0));
        assert!(!s.is_running(250.0));
        assert_eq!(s.value(1000.0), 0.0);
        s.stop();
        assert!(!s.is_running(20.0));
    }

    #[test]
    fn seq_pause_and_set() {
        let mut s = Seq::new(0.0).pause(100.0).set(5.0).to(10.0, 100.0, Easing::Linear);
        s.start(0.0);
        assert_eq!(s.value(50.0), 0.0);
        assert!((s.value(150.0) - 7.5).abs() < 1e-5);
        assert_eq!(s.value(500.0), 10.0);
    }

    fn run_width_spring() -> (Vec<f32>, f64) {
        let mut sp = Spring::new(212.0, 3.8, 0.32, 0.2);
        sp.set_target(548.0, 0.0);
        let mut vals = Vec::new();
        let mut t = 0.0;
        while !sp.settled() && t < 10_000.0 {
            t += 16.0;
            vals.push(sp.step(t));
        }
        (vals, t)
    }

    #[test]
    fn spring_width_settles_overshoots_deterministic() {
        let (v1, t1) = run_width_spring();
        let (v2, t2) = run_width_spring();
        assert_eq!(v1, v2);
        assert_eq!(t1, t2);
        assert!(t1 < 5000.0, "settle time {t1}");
        let max = v1.iter().cloned().fold(f32::MIN, f32::max);
        assert!(max > 548.0 && max < 548.0 + 60.0, "overshoot max {max}");
        assert_eq!(*v1.last().unwrap(), 548.0);
    }

    #[test]
    fn spring_carries_leftover_time() {
        let mut a = Spring::new(0.0, 3.8, 0.32, 0.01);
        let mut b = a;
        a.set_target(100.0, 0.0);
        b.set_target(100.0, 0.0);
        // a: 10 ms then 22 ms (=32 total, 2 steps); b: one call at 32 ms
        a.step(10.0);
        a.step(32.0);
        b.step(32.0);
        assert_eq!(a.value(), b.value());
        assert!(a.value() > 0.0);
        // fewer than 16 ms: nothing moves
        let mut c = Spring::new(0.0, 3.8, 0.32, 0.01);
        c.set_target(100.0, 0.0);
        assert_eq!(c.step(15.0), 0.0);
    }

    #[test]
    fn spring_mass_and_jump() {
        let mut s = Spring::new(0.0, 3.4, 0.38, 0.2).with_mass(2.0);
        s.set_target(10.0, 0.0);
        let mut t = 0.0;
        while !s.settled() && t < 20_000.0 {
            t += 16.0;
            s.step(t);
        }
        assert!(s.settled());
        s.jump(3.0);
        assert_eq!(s.value(), 3.0);
        assert!(s.settled());
        // tiny retarget snaps immediately
        s.set_target(3.05, 100.0);
        assert!(s.settled());
        assert_eq!(s.value(), 3.05);
    }
}
