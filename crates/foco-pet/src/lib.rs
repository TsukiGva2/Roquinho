//! foco-pet: the Foquinho mascot (cat / ghost / slime), a port of `qml/Pet.qml`.
//!
//! Model
//! * The pet draws on QML's 40x40 design grid scaled into a caller rectangle ([`Pet::paint`]).
//! * All state advances in [`Pet::update`]; `paint` only reads the parameters computed there,
//!   so a repaint of the same frame is free of side effects.
//! * `update` returns `true` only when a *quantized* snapshot of the visual parameters (to
//!   1/4 device pixel at the size given by [`Pet::set_size`]) differs from the last one, so
//!   the app can skip painting.
//! * [`Pet::next_wake`] tells the event loop precisely when the next visible change happens:
//!   `Frame` while reactions/tweens move, a precise `At(t)` otherwise (the always-on
//!   breathing loop is analytic, so the next quantum crossing is found by look-ahead instead
//!   of ticking at a fixed 30 Hz), the next blink/fidget/reaction-end deadline, or `Idle`
//!   when inactive.
//!
//! Music was removed from the app; Pet.qml itself has no music-only behaviour (`hum` comes
//! from the idle fidget pool and stays).

mod defs;
mod expr;
mod paint;
mod paths;
#[cfg(test)]
mod tests;

use std::cell::Cell;

use defs::{Ch, Step, DEFS, NRUN};
use expr::{Eyes, Mouth};
use foco_base::{pet_event, theme, Anim, Color, Easing, Mood, Ms, PetKind, Rect, Surface, Wake};

pub use expr::Expr;

/// Foquinho is a black goth-punk cat (accessories are drawn in `paint.rs`).
const BODY_CAT: Color = Color::rgb8(0x23, 0x20, 0x28);
const SHADE_CAT: Color = Color::rgb8(0x14, 0x12, 0x18);
const BODY_GHOST: Color = Color::rgb8(0xEE, 0xEE, 0xF4);
const SHADE_GHOST: Color = Color::rgb8(0xC9, 0xC9, 0xD6);

/// Fraction of `size` the drawing may reach outside `rect` (z's, hearts, notes, tail, jump).
const OVERDRAW: f32 = 0.15;
/// Repaint threshold: a parameter must move by `1/QUANT_DIV` device pixel to count as a change.
/// The ambient motion is a pixel or two in total (a few at the 66 px header pet), so half-pixel
/// steps look the same as quarter-pixel ones and halve the repaints.
const QUANT_DIV: f32 = 2.0;
/// Opacity step of the sleeping z's (tiny glyphs; finer steps only cause needless repaints).
const Z_STEP: f32 = 0.2;
const Z_PERIOD: Ms = 2700.0;
const SWEAT_PERIOD: Ms = 1400.0;
/// Look-ahead horizon (ms) when searching for the next visible ambient change.
const HORIZON: Ms = 1600.0;
/// Frame period of the pet's own tweens (blink, fidget, talk): 30 Hz.
const RUN_FRAME_MS: Ms = 1000.0 / 30.0;
/// An ambient step is never scheduled sooner than this after the previous look (ms): at these
/// sizes a step is under half a pixel and the next wake would only repaint the same pose.
const MIN_AMBIENT_GAP_MS: Ms = 50.0;

// ------------------------------------------------------------------ rng

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Rng {
        // splitmix64 scramble so that neighbouring seeds give unrelated streams
        let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        Rng(if z == 0 { 0x9E37_79B9_7F4A_7C15 } else { z })
    }
    fn next_u64(&mut self) -> u64 {
        // xorshift64*
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    /// Uniform in [0, 1).
    fn f(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

// ------------------------------------------------------------------ state

/// Animated parameters (the QML properties) plus values derived from the ambient loops.
#[derive(Clone, Copy, Debug)]
pub(crate) struct P {
    pub squash: f32,
    pub jump: f32,
    pub hand_ry: f32,
    pub hand_rrot: f32,
    pub hand_ly: f32,
    pub face_dy: f32,
    pub ear_l: f32,
    pub ear_r: f32,
    pub tail_a: f32,
    pub talk_h: f32,
    pub hearts_t: f32,
    pub spark_t: f32,
    pub pop_s: f32,
    pub notes_t: f32,
    pub blink: f32,
    pub breathe: f32,
    pub bob: f32,
    pub tilt: f32,
    pub look_x: f32,
    pub look_y: f32,
    /// Opacity of the three sleeping "z".
    pub z: [f32; 3],
    /// Sweat drop phase 0..1.
    pub sweat: f32,
}

impl P {
    const REST: P = P {
        squash: 0.0,
        jump: 0.0,
        hand_ry: 0.0,
        hand_rrot: 0.0,
        hand_ly: 0.0,
        face_dy: 0.0,
        ear_l: 0.0,
        ear_r: 0.0,
        tail_a: 0.0,
        talk_h: 1.0,
        hearts_t: 1.0,
        spark_t: 1.0,
        pop_s: 0.0,
        notes_t: 0.0,
        blink: 1.0,
        breathe: 1.0,
        bob: 0.0,
        tilt: 0.0,
        look_x: 0.0,
        look_y: 0.0,
        z: [0.0; 3],
        sweat: 0.0,
    };
}

/// Running instance of a [`defs::Def`].
#[derive(Clone, Copy, Debug)]
struct Run {
    loop_i: u8,
    step_i: u8,
    step_start: Ms,
    entered: bool,
    from: [f32; 3],
}

/// Analytic sine ping-pong: half period `d`, phase origin `t0`.
#[derive(Clone, Copy, Debug)]
struct Lp {
    t0: Ms,
    d: Ms,
}

impl Lp {
    /// 0 -> 1 -> 0 over `2 d` (`InOutSine` up, `InOutSine` down).
    fn s(&self, t: Ms) -> f32 {
        let p = (t - self.t0).rem_euclid(2.0 * self.d);
        (0.5 * (1.0 - (std::f64::consts::PI * p / self.d).cos())) as f32
    }
}

fn rephase(lp: Option<Lp>, d: Ms, now: Ms) -> Lp {
    match lp {
        None => Lp { t0: now, d },
        Some(l) if l.d == d => l,
        Some(l) => {
            let frac = (now - l.t0).rem_euclid(2.0 * l.d) / (2.0 * l.d);
            Lp { t0: now - frac * 2.0 * d, d }
        }
    }
}

/// Raw ambient (always-running loop) values at one instant.
#[derive(Clone, Copy, Debug)]
struct Amb {
    breathe: f32,
    bob: f32,
    tail: f32,
    z: [f32; 3],
    sweat: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct AmbQ {
    breathe: i32,
    bob: i32,
    tail: i32,
    z: [i32; 3],
    sweat: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Snap {
    kind: PetKind,
    mood: Mood,
    expr: Expr,
    squash: i32,
    jump: i32,
    hand_ry: i32,
    hand_rrot: i32,
    hand_ly: i32,
    face_dy: i32,
    ear_l: i32,
    ear_r: i32,
    talk_h: i32,
    hearts: i32,
    spark: i32,
    pop: i32,
    notes: i32,
    blink: i32,
    tilt: i32,
    look_x: i32,
    look_y: i32,
    amb: AmbQ,
}

fn q(v: f32, unit: f32) -> i32 {
    (v / unit).round() as i32
}

#[derive(Clone, Copy)]
enum Fidget {
    Yawn,
    Hum,
    Curious,
    Wink,
    Sigh,
    Look,
    Nod,
    Ear,
    Tail,
    Hop,
    Blink2,
    Turn,
    Nothing,
}

/// A look-ahead result: with the painted ambient values `cur`, nothing visible changes in
/// `from..until` (so `next_wake` need not search again while `now` is inside).
#[derive(Clone, Copy)]
struct AmbCache {
    cur: AmbQ,
    from: Ms,
    until: Ms,
}

/// The mascot.
pub struct Pet {
    kind: PetKind,
    mood: Mood,
    accent: Color,
    size: f32,
    active: bool,
    talking: bool,
    hovered: bool,
    reaction: Option<Expr>,
    reaction_until: Ms,
    rng: Rng,
    p: P,
    tilt_a: Anim,
    look_x_a: Anim,
    look_y_a: Anim,
    runs: [Option<Run>; NRUN],
    breathe_lp: Option<Lp>,
    bob_lp: Option<Lp>,
    tail_lp: Option<Lp>,
    z_t0: Option<Ms>,
    sweat_t0: Option<Ms>,
    blink_at: Option<Ms>,
    blink_interval: Ms,
    fidget_at: Option<Ms>,
    fidget_interval: Ms,
    needs_sync: bool,
    dirty: bool,
    snap: Option<Snap>,
    /// Result of the last ambient look-ahead, reused until it expires or something changes.
    amb_cache: Cell<Option<AmbCache>>,
    // colours derived from kind/accent
    body: Color,
    shade: Color,
    foot: Color,
}

impl Pet {
    /// A new, active pet (QML defaults: mood `focus`). `seed` drives blink/fidget timing.
    pub fn new(kind: PetKind, seed: u64) -> Self {
        let mut pet = Pet {
            kind,
            mood: Mood::Focus,
            accent: theme::DEFAULT_ACCENT,
            size: 32.0,
            active: true,
            talking: false,
            hovered: false,
            reaction: None,
            reaction_until: 0.0,
            rng: Rng::new(seed),
            p: P::REST,
            tilt_a: Anim::new(0.0),
            look_x_a: Anim::new(0.0),
            look_y_a: Anim::new(0.0),
            runs: [None; NRUN],
            breathe_lp: None,
            bob_lp: None,
            tail_lp: None,
            z_t0: None,
            sweat_t0: None,
            blink_at: None,
            blink_interval: 3200.0,
            fidget_at: None,
            fidget_interval: 5000.0,
            needs_sync: true,
            dirty: true,
            snap: None,
            amb_cache: Cell::new(None),
            body: BODY_CAT,
            shade: SHADE_CAT,
            foot: BODY_CAT.darker(1.06),
        };
        pet.recolor();
        pet
    }

    // -------------------------------------------------------------- setters

    /// Changes the mascot kind (`settings.petKind`).
    pub fn set_kind(&mut self, kind: PetKind, now: Ms) {
        if kind == self.kind {
            return;
        }
        self.kind = kind;
        self.recolor();
        self.dirty = true;
        self.sync(now);
    }

    /// Changes the continuous mood. Entering `happy` without a reaction plays the celebration.
    pub fn set_mood(&mut self, mood: Mood, now: Ms) {
        if mood == self.mood {
            return;
        }
        self.mood = mood;
        if mood == Mood::Happy && self.reaction.is_none() && self.active {
            self.play(Expr::Happy, now);
        }
        self.sync(now);
    }

    /// QML `animated && visible`. `false` stops every loop, timer and animation.
    pub fn set_active(&mut self, active: bool, now: Ms) {
        if active == self.active {
            return;
        }
        self.active = active;
        self.dirty = true;
        if active {
            self.needs_sync = true;
            self.sync(now);
        } else {
            self.runs = [None; NRUN];
            self.reaction = None;
            self.breathe_lp = None;
            self.bob_lp = None;
            self.tail_lp = None;
            self.z_t0 = None;
            self.sweat_t0 = None;
            self.blink_at = None;
            self.fidget_at = None;
            self.p = P::REST;
            self.tilt_a.set(0.0);
            self.look_x_a.set(0.0);
            self.look_y_a.set(0.0);
        }
    }

    /// `Pet.react(name, ms)` with the default duration of the event (`Store.petEvent`).
    pub fn react(&mut self, name: &str, now: Ms) {
        self.react_for(name, pet_event::duration_ms(name), now);
    }

    /// `Pet.react(name, ms)`: shows the reaction for `ms` and plays its animation.
    /// Ignored while inactive (QML `if (!animated) return`).
    pub fn react_for(&mut self, name: &str, ms: Ms, now: Ms) {
        if !self.active {
            return;
        }
        self.reaction = Some(Expr::from_name(name));
        self.reaction_until = now + ms;
        self.play(Expr::from_name(name), now);
        self.sync(now);
    }

    /// A click/tap on an interactive pet: `react("love", 1900)`.
    pub fn tap(&mut self, now: Ms) {
        self.react_for("love", 1900.0, now);
    }

    /// The `talking` flag (speech bubble showing): talk mouth, no fidgets.
    pub fn set_talking(&mut self, on: bool, now: Ms) {
        if on == self.talking {
            return;
        }
        self.talking = on;
        self.sync(now);
    }

    /// Direct look offsets (QML `lookX`/`lookY`; the hover handler clamps to +-1.8 / +-1.2).
    pub fn set_look(&mut self, dx: f32, dy: f32, now: Ms) {
        let (dx, dy) = (dx.clamp(-1.8, 1.8), dy.clamp(-1.2, 1.2));
        self.look_x_a.animate_to(dx, 240.0, Easing::OutCubic, now);
        self.look_y_a.animate_to(dy, 240.0, Easing::OutCubic, now);
    }

    /// Pointer over an interactive pet: `fx`,`fy` are the pointer position as a fraction (0..1)
    /// of the pet's box. Sets the look like the QML `HoverHandler.onPointChanged`.
    pub fn pointer(&mut self, fx: f32, fy: f32, now: Ms) {
        if !self.hovered {
            self.set_hover(true, now);
        }
        self.set_look((fx - 0.5) * 4.0, (fy - 0.5) * 3.0, now);
    }

    /// Hover state of an interactive pet (`expr` becomes "content" while hovered).
    pub fn set_hover(&mut self, hovered: bool, now: Ms) {
        if hovered == self.hovered {
            return;
        }
        self.hovered = hovered;
        if !hovered {
            self.set_look(0.0, 0.0, now);
        }
        self.sync(now);
    }

    /// Accent colour (the slime body uses `lighter(accent, 1.22)` / `accent`).
    pub fn set_accent(&mut self, c: Color) {
        if c == self.accent {
            return;
        }
        self.accent = c;
        self.recolor();
        self.dirty = true;
    }

    /// Device size (px) the pet will be painted at; only used to quantize repaint decisions
    /// (1/4 device pixel). Defaults to 32.
    pub fn set_size(&mut self, size: f32) {
        if size > 0.0 && size != self.size {
            self.size = size;
            self.snap = None;
            self.amb_cache.set(None);
        }
    }

    // -------------------------------------------------------------- queries

    pub fn kind(&self) -> PetKind {
        self.kind
    }
    pub fn mood(&self) -> Mood {
        self.mood
    }
    pub fn is_active(&self) -> bool {
        self.active
    }
    /// A reaction is currently showing.
    pub fn reacting(&self) -> bool {
        self.reaction.is_some()
    }
    /// The current expression (`pet.expr`).
    pub fn expr(&self) -> Expr {
        if self.talking {
            Expr::Talk
        } else if let Some(r) = self.reaction {
            r
        } else if self.hovered {
            Expr::Content
        } else {
            Expr::from_mood(self.mood)
        }
    }

    /// How far outside `rect` (as a fraction of its width) [`paint`](Self::paint) may draw.
    pub fn overdraw(&self) -> f32 {
        OVERDRAW
    }

    // -------------------------------------------------------------- internals

    fn recolor(&mut self) {
        let (b, s) = match self.kind {
            PetKind::Cat => (BODY_CAT, SHADE_CAT),
            PetKind::Ghost => (BODY_GHOST, SHADE_GHOST),
            PetKind::Slime => (self.accent.lighter(1.22), self.accent),
        };
        self.body = b;
        self.shade = s;
        self.foot = if self.kind == PetKind::Cat { s } else { b.darker(1.06) };
    }

    fn set_ch(&mut self, ch: Ch, v: f32, t: Ms) {
        let p = &mut self.p;
        match ch {
            Ch::Squash => p.squash = v,
            Ch::Jump => p.jump = v,
            Ch::HandRY => p.hand_ry = v,
            Ch::HandRRot => p.hand_rrot = v,
            Ch::HandLY => p.hand_ly = v,
            Ch::FaceDy => p.face_dy = v,
            Ch::EarL => p.ear_l = v,
            Ch::EarR => p.ear_r = v,
            Ch::TailA => p.tail_a = v,
            Ch::TalkH => p.talk_h = v,
            Ch::HeartsT => p.hearts_t = v,
            Ch::SparkT => p.spark_t = v,
            Ch::PopS => p.pop_s = v,
            Ch::NotesT => p.notes_t = v,
            Ch::Blink => p.blink = v,
            Ch::Tilt => self.tilt_a.animate_to(v, 320.0, Easing::OUT_BACK, t),
            Ch::LookX => self.look_x_a.animate_to(v, 240.0, Easing::OutCubic, t),
            Ch::LookY => self.look_y_a.animate_to(v, 240.0, Easing::OutCubic, t),
        }
    }

    fn get_ch(&self, ch: Ch, t: Ms) -> f32 {
        let p = &self.p;
        match ch {
            Ch::Squash => p.squash,
            Ch::Jump => p.jump,
            Ch::HandRY => p.hand_ry,
            Ch::HandRRot => p.hand_rrot,
            Ch::HandLY => p.hand_ly,
            Ch::FaceDy => p.face_dy,
            Ch::EarL => p.ear_l,
            Ch::EarR => p.ear_r,
            Ch::TailA => p.tail_a,
            Ch::TalkH => p.talk_h,
            Ch::HeartsT => p.hearts_t,
            Ch::SparkT => p.spark_t,
            Ch::PopS => p.pop_s,
            Ch::NotesT => p.notes_t,
            Ch::Blink => p.blink,
            Ch::Tilt => self.tilt_a.value(t),
            Ch::LookX => self.look_x_a.value(t),
            Ch::LookY => self.look_y_a.value(t),
        }
    }

    fn start_run(&mut self, id: usize, now: Ms) {
        self.runs[id] = Some(Run { loop_i: 0, step_i: 0, step_start: now, entered: false, from: [0.0; 3] });
        self.tick_run(id, now);
    }

    /// Advances one animation to `now`, completing every step that ended in between.
    fn tick_run(&mut self, id: usize, now: Ms) {
        let Some(mut r) = self.runs[id] else { return };
        let def = &DEFS[id];
        loop {
            let step = &def.steps[r.step_i as usize];
            if !r.entered {
                r.entered = true;
                match step {
                    Step::Par(mvs) => {
                        for (i, m) in mvs.iter().enumerate() {
                            if m.from.is_nan() {
                                r.from[i] = self.get_ch(m.ch, r.step_start);
                            } else {
                                r.from[i] = m.from;
                                self.set_ch(m.ch, m.from, r.step_start);
                            }
                        }
                    }
                    Step::Set(ch, v) => self.set_ch(*ch, *v, r.step_start),
                    Step::Pause(_) => {}
                }
            }
            let dur = step.dur() as f64;
            let e = now - r.step_start;
            if e < dur {
                if let Step::Par(mvs) = step {
                    for (i, m) in mvs.iter().enumerate() {
                        let u = if m.dur <= 0.0 { 1.0 } else { (e as f32 / m.dur).min(1.0) };
                        let v = r.from[i] + (m.to - r.from[i]) * m.ease.apply(u);
                        self.set_ch(m.ch, v, now);
                    }
                }
                break;
            }
            let end = r.step_start + dur;
            if let Step::Par(mvs) = step {
                for m in *mvs {
                    self.set_ch(m.ch, m.to, end);
                }
            }
            r.step_start = end;
            r.entered = false;
            r.step_i += 1;
            if r.step_i as usize >= def.steps.len() {
                r.step_i = 0;
                r.loop_i = r.loop_i.saturating_add(1);
                if def.loops != 0 && r.loop_i >= def.loops {
                    for (c, v) in def.fin {
                        self.set_ch(*c, *v, end);
                    }
                    self.runs[id] = None;
                    return;
                }
                if def.loops == 0 {
                    let cyc = def.cycle();
                    let n = ((now - r.step_start) / cyc).floor();
                    if n >= 1.0 {
                        r.step_start += n * cyc;
                    }
                }
            }
        }
        self.runs[id] = Some(r);
    }

    /// `Pet.play(name)`: starts the animations of a reaction.
    fn play(&mut self, e: Expr, now: Ms) {
        use defs::{Ch, FIST, HEARTS, HOP, JUMP, NOTES, POP, SIGH, SPARK, SQUISH, STRETCH, SWAY, WAVE};
        match e {
            Expr::Happy | Expr::Celebrate => {
                self.start_run(JUMP, now);
                self.start_run(HEARTS, now);
                self.start_run(SPARK, now);
            }
            Expr::Wave => self.start_run(WAVE, now),
            Expr::Surprised => {
                self.start_run(HOP, now);
                self.start_run(POP, now);
            }
            Expr::Love => {
                self.start_run(SQUISH, now);
                self.start_run(HEARTS, now);
            }
            Expr::Wink => {
                self.start_run(SPARK, now);
                self.set_ch(Ch::Tilt, -6.0, now);
            }
            Expr::Ready => {
                self.start_run(FIST, now);
                self.start_run(POP, now);
            }
            Expr::Yawn => self.start_run(STRETCH, now),
            Expr::Curious => {
                self.set_ch(Ch::Tilt, 9.0, now);
                self.start_run(POP, now);
            }
            Expr::Hum => {
                self.start_run(NOTES, now);
                self.start_run(SWAY, now);
            }
            Expr::Sigh => self.start_run(SIGH, now),
            _ => {}
        }
    }

    fn fidget(&mut self, now: Ms) {
        use Fidget::{Blink2, Curious, Ear, Hop, Hum, Look, Nod, Nothing, Sigh, Tail, Turn, Wink, Yawn};
        let pool: &[Fidget] = match self.mood {
            Mood::Sleep => &[Yawn, Ear, Turn, Nothing],
            Mood::Idle => &[Look, Hum, Curious, Hop, Yawn, Tail, Blink2],
            Mood::Tired => &[Sigh, Look, Yawn],
            Mood::Focus => &[Look, Nod, Ear, Tail, Blink2, Wink, Nod],
            Mood::Happy => &[Hop],
        };
        let f = pool[(self.rng.f() * pool.len() as f64) as usize % pool.len()];
        let cat = self.kind == PetKind::Cat;
        match f {
            Yawn => self.react_for("yawn", 1500.0, now),
            Hum => self.react_for("hum", 2600.0, now),
            Curious => self.react_for("curious", 1700.0, now),
            Wink => self.react_for("wink", 900.0, now),
            Sigh => self.react_for("sigh", 1800.0, now),
            Look => self.start_run(defs::LOOK, now),
            Nod => self.start_run(defs::NOD, now),
            // ears and tail only exist on the cat
            Ear => {
                if cat {
                    self.start_run(defs::EAR, now);
                }
            }
            Tail => {
                if cat {
                    self.start_run(defs::TAIL_FLICK, now);
                }
            }
            Hop => self.start_run(defs::HOP, now),
            Blink2 => self.start_run(defs::BLINK2, now),
            Turn => self.start_run(defs::TURN, now),
            Nothing => {}
        }
    }

    /// Re-derives loops and timers from the current state (the QML `running:` bindings).
    fn sync(&mut self, now: Ms) {
        self.needs_sync = false;
        self.amb_cache.set(None);
        if !self.active {
            return;
        }
        let expr = self.expr();
        let cat = self.kind == PetKind::Cat;
        // breathing
        let d = match self.mood {
            Mood::Sleep => 1700.0,
            Mood::Tired => 1500.0,
            _ => 1150.0,
        };
        self.breathe_lp = Some(rephase(self.breathe_lp, d, now));
        // ghost bob
        if self.kind == PetKind::Ghost {
            self.bob_lp = Some(rephase(self.bob_lp, 1400.0, now));
        } else {
            self.bob_lp = None;
            self.p.bob = 0.0;
        }
        // cat tail
        if cat {
            let sleep = self.mood == Mood::Sleep;
            let d = if sleep { 2200.0 } else { 1100.0 };
            self.tail_lp = Some(match self.tail_lp {
                None => {
                    // start where the sway passes through the current angle (rest = 0)
                    let (lo, hi) = Self::tail_range(sleep);
                    let f = (-lo / (hi - lo)).clamp(0.0, 1.0) as f64;
                    let u0 = (1.0 - 2.0 * f).acos() / std::f64::consts::PI;
                    Lp { t0: now - u0 * d, d }
                }
                Some(l) => rephase(Some(l), d, now),
            });
        } else {
            self.tail_lp = None;
        }
        // talk loop
        if self.talking {
            if self.runs[defs::TALK].is_none() {
                self.start_run(defs::TALK, now);
            }
        } else if self.runs[defs::TALK].take().is_some() {
            self.p.talk_h = 1.0;
        }
        // sleeping z's / sweat drop
        if expr == Expr::Sleep {
            self.z_t0.get_or_insert(now);
        } else {
            self.z_t0 = None;
        }
        if matches!(expr, Expr::Tired | Expr::Sigh) {
            self.sweat_t0.get_or_insert(now);
        } else {
            self.sweat_t0 = None;
        }
        // blink timer: running while eyes are not closed
        if expr.eyes() == Eyes::Closed {
            self.blink_at = None;
        } else if self.blink_at.is_none() {
            self.blink_at = Some(now + self.blink_interval);
        }
        // fidget timer
        if self.reaction.is_none() && !self.talking && !self.hovered {
            if self.fidget_at.is_none() {
                self.fidget_at = Some(now + self.fidget_interval);
            }
        } else {
            self.fidget_at = None;
        }
    }

    fn tail_range(sleep: bool) -> (f32, f32) {
        if sleep {
            (-3.0, 3.0)
        } else {
            (-7.0, 9.0)
        }
    }

    /// Ambient loop values at `t` (pure).
    fn amb_raw(&self, t: Ms) -> Amb {
        let breathe = match self.breathe_lp {
            Some(l) => {
                let to = if self.kind == PetKind::Slime { 0.93 } else { 0.965 };
                1.0 + (to - 1.0) * l.s(t)
            }
            None => 1.0,
        };
        let bob = self.bob_lp.map_or(0.0, |l| -2.6 * l.s(t));
        let tail = self.tail_lp.map_or(0.0, |l| {
            let (lo, hi) = Self::tail_range(self.mood == Mood::Sleep);
            lo + (hi - lo) * l.s(t)
        });
        let mut z = [0.0; 3];
        if let Some(z0) = self.z_t0 {
            let p = (t - z0).rem_euclid(Z_PERIOD);
            for (i, zi) in z.iter_mut().enumerate() {
                let qq = p - i as f64 * 500.0;
                *zi = if qq < 0.0 {
                    0.0
                } else if qq < 500.0 {
                    (0.9 * qq / 500.0) as f32
                } else if qq < 1400.0 {
                    (0.9 * (1.0 - (qq - 500.0) / 900.0)) as f32
                } else {
                    0.0
                };
            }
        }
        let sweat = self.sweat_t0.map_or(0.0, |w0| {
            let u = ((t - w0).rem_euclid(SWEAT_PERIOD) / SWEAT_PERIOD) as f32;
            u * u
        });
        Amb { breathe, bob, tail, z, sweat }
    }

    /// Quantum (design units) corresponding to 1/4 device pixel.
    fn qd(&self) -> f32 {
        1.0 / (QUANT_DIV * (self.size / 40.0))
    }

    fn amb_q(&self, a: &Amb) -> AmbQ {
        let qd = self.qd();
        let deg = |r: f32| (qd / r).to_degrees();
        AmbQ {
            breathe: q(a.breathe, qd / 30.0),
            bob: q(a.bob, qd),
            tail: if self.kind == PetKind::Cat { q(a.tail, deg(12.0)) } else { 0 },
            z: [q(a.z[0], Z_STEP), q(a.z[1], Z_STEP), q(a.z[2], Z_STEP)],
            sweat: q(a.sweat, qd / 2.0),
        }
    }

    fn snapshot(&self, amb: AmbQ, expr: Expr) -> Snap {
        let qd = self.qd();
        let deg = |r: f32| (qd / r).to_degrees();
        let p = &self.p;
        let cat = self.kind == PetKind::Cat;
        let eyes = expr.eyes();
        let blinking = matches!(eyes, Eyes::Open | Eyes::Half | Eyes::Wide | Eyes::Wink);
        let pop_on = matches!(expr, Expr::Surprised | Expr::Curious | Expr::Ready);
        Snap {
            kind: self.kind,
            mood: self.mood,
            expr,
            squash: q(p.squash, qd / 30.0),
            jump: q(p.jump, qd),
            hand_ry: q(p.hand_ry, qd),
            hand_rrot: q(p.hand_rrot, deg(4.0)),
            hand_ly: q(p.hand_ly, qd),
            face_dy: q(p.face_dy, qd),
            ear_l: if cat { q(p.ear_l, deg(14.0)) } else { 0 },
            ear_r: if cat { q(p.ear_r, deg(14.0)) } else { 0 },
            talk_h: if expr.mouth(self.kind) == Mouth::Talk { q(p.talk_h, qd) } else { 0 },
            hearts: if p.hearts_t < 1.0 { q(p.hearts_t, qd / 16.0) } else { i32::MAX },
            spark: if p.spark_t < 1.0 { q(p.spark_t, qd / 6.0) } else { i32::MAX },
            pop: if pop_on { q(p.pop_s, qd / 11.0) } else { 0 },
            notes: if expr == Expr::Hum { q(p.notes_t, qd / 13.0) } else { 0 },
            blink: if blinking { q(p.blink, qd / 5.4) } else { 0 },
            tilt: q(p.tilt, deg(20.0)),
            look_x: q(p.look_x, qd),
            look_y: q(p.look_y, qd),
            amb,
        }
    }

    // -------------------------------------------------------------- frame logic

    /// Advances timers, loops and animations to `now`. Returns `true` when something visible
    /// changed since the previous `true` (compared on a quantized snapshot), i.e. when the pet
    /// must be repainted.
    pub fn update(&mut self, now: Ms) -> bool {
        if !self.active {
            return std::mem::take(&mut self.dirty);
        }
        if self.needs_sync {
            self.sync(now);
        }
        // reaction timer
        if self.reaction.is_some() && now >= self.reaction_until {
            self.reaction = None;
            self.set_ch(Ch::Tilt, 0.0, self.reaction_until);
            self.p.pop_s = 0.0;
            self.sync(now);
        }
        // blink timer
        if self.blink_at.is_some_and(|t| now >= t) {
            self.blink_interval = 2400.0 + self.rng.f() * 3200.0;
            self.blink_at = Some(now + self.blink_interval);
            self.start_run(defs::BLINK, now);
        }
        // fidget timer
        if self.fidget_at.is_some_and(|t| now >= t) {
            self.fidget_interval = 4200.0 + self.rng.f() * 6500.0;
            self.fidget_at = Some(now + self.fidget_interval);
            self.fidget(now);
        }
        // ambient loops first, running animations override them (they were started later)
        let a = self.amb_raw(now);
        self.p.breathe = a.breathe;
        self.p.bob = a.bob;
        self.p.tail_a = a.tail;
        self.p.z = a.z;
        self.p.sweat = a.sweat;
        for id in 0..NRUN {
            if self.runs[id].is_some() {
                self.tick_run(id, now);
            }
        }
        self.p.tilt = self.tilt_a.value(now);
        self.p.look_x = self.look_x_a.value(now);
        self.p.look_y = self.look_y_a.value(now);

        let amb = self.amb_q(&Amb { tail: self.p.tail_a, ..a });
        let snap = self.snapshot(amb, self.expr());
        let changed = self.snap != Some(snap);
        self.snap = Some(snap);
        std::mem::take(&mut self.dirty) || changed
    }

    /// When the pet next needs `update` + repaint.
    ///
    /// * `Idle` when inactive.
    /// * `Frame` while a reaction/tween moves (or an `update` is already due).
    /// * Otherwise `At(t)`: the earliest of the next visible quantum crossing of the ambient
    ///   loops (breathing, bob, tail, z, sweat; found by look-ahead), the next blink / fidget
    ///   deadline and the end of the current reaction.
    pub fn next_wake(&self, now: Ms) -> Wake {
        if !self.active {
            return Wake::Idle;
        }
        if self.dirty || self.needs_sync || self.snap.is_none() {
            return Wake::Frame;
        }
        let mut w = Wake::Idle;
        let mut moving = false;
        for (id, r) in self.runs.iter().enumerate() {
            let Some(r) = r else { continue };
            match &DEFS[id].steps[r.step_i as usize] {
                Step::Pause(d) => w = w.merge_at(Wake::at(now, r.step_start + *d as f64), now),
                _ => moving = true,
            }
        }
        if self.tilt_a.is_animating(now) || self.look_x_a.is_animating(now) || self.look_y_a.is_animating(now) {
            return Wake::Frame;
        }
        if moving {
            // blink / fidget / talk tweens: a pet this small reads as smooth at 30 Hz, and every
            // frame is a wake of the whole process
            return w.merge_at(Wake::At(now + RUN_FRAME_MS), now);
        }
        if self.reaction.is_some() {
            w = w.merge_at(Wake::at(now, self.reaction_until), now);
        }
        if let Some(t) = self.blink_at {
            w = w.merge_at(Wake::at(now, t), now);
        }
        if let Some(t) = self.fidget_at {
            w = w.merge_at(Wake::at(now, t), now);
        }
        w.merge_at(self.ambient_wake(now), now)
    }

    /// Look-ahead for the next time the quantized ambient values differ from the last painted.
    fn ambient_wake(&self, now: Ms) -> Wake {
        let Some(snap) = self.snap else { return Wake::Frame };
        if self.breathe_lp.is_none() && self.bob_lp.is_none() && self.tail_lp.is_none() {
            return Wake::Idle;
        }
        let cur = snap.amb;
        if let Some(c) = self.amb_cache.get() {
            if c.cur == cur && c.from <= now && now < c.until {
                return Wake::At(c.until);
            }
        }
        let at = |t: Ms| self.amb_q(&self.amb_raw(t));
        if at(now) != cur {
            return Wake::Frame;
        }
        let mut t = now;
        let mut step = 4.0;
        let mut until = now + HORIZON;
        while t < now + HORIZON {
            t += step;
            if t - now >= 64.0 {
                step = 16.0;
            }
            if at(t) != cur {
                until = t;
                break;
            }
        }
        let until = until.max(now + MIN_AMBIENT_GAP_MS);
        self.amb_cache.set(Some(AmbCache { cur, from: now, until }));
        Wake::At(until)
    }

    /// Names the picture the next [`paint`](Self::paint) produces (equal keys, equal pixels at
    /// the same size): the quantized pose plus the colours. `None` before the first `update`.
    pub fn pose_key(&self) -> Option<u64> {
        use std::hash::{Hash, Hasher};
        let snap = self.snap?;
        let mut h = std::collections::hash_map::DefaultHasher::new();
        snap.hash(&mut h);
        for v in [self.accent.r, self.accent.g, self.accent.b, self.accent.a] {
            v.to_bits().hash(&mut h);
        }
        Some(h.finish())
    }

    /// Draws the pet on QML's 40x40 grid scaled so that 40 units = `rect.w`, with the grid
    /// origin at `rect`'s top-left. Uses the parameters computed by the last
    /// [`update`](Self::update); `now` is accepted for API symmetry only.
    pub fn paint(&self, s: &mut dyn Surface, rect: Rect, _now: Ms) {
        paint::paint(self, s, rect);
    }
}
