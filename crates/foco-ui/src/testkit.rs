//! Fakes and a harness for unit-testing views and widgets without a renderer or a store.
//! Text is the `PixSurface` placeholder (`0.55 * px` per char, line height `1.2 * px`).

use crate::cx::{CursorShape, Cx, DamageSink, Env, FocusId, Msg, PaintCx, StoreRead, TextMeasure, S, WIN_DEV};
use crate::pet::PetLike;
use foco_base::{Color, Mood, Ms, PetKind, PixSurface, Rect, StateFile, Surface, TextStyle, Wake};

/// A store that is just a `StateFile`.
#[derive(Default)]
pub struct FakeStore {
    pub state: StateFile,
    pub celebrate_until: f64,
    pub first_run: bool,
}

impl StoreRead for FakeStore {
    fn state(&self) -> &StateFile {
        &self.state
    }
    fn celebrate_until(&self) -> f64 {
        self.celebrate_until
    }
    fn first_run(&self) -> bool {
        self.first_run
    }
}

/// Fixed-width text measurer matching `PixSurface`.
#[derive(Default)]
pub struct FakeText;

impl TextMeasure for FakeText {
    fn measure(&mut self, s: &str, st: &TextStyle) -> f32 {
        s.chars().count() as f32 * (0.55 * st.px + st.letter_spacing)
    }
    fn line_height(&mut self, st: &TextStyle) -> f32 {
        st.px * 1.2
    }
    fn elide(&mut self, s: &str, st: &TextStyle, max_w: f32) -> String {
        if self.measure(s, st) <= max_w {
            return s.to_string();
        }
        let chars: Vec<char> = s.chars().collect();
        for n in (0..chars.len()).rev() {
            let cand: String = chars[..n].iter().collect::<String>() + "…";
            if self.measure(&cand, st) <= max_w {
                return cand;
            }
        }
        String::new()
    }
}

/// A pet that records calls and animates (breathing) while active.
#[derive(Default)]
pub struct FakePet {
    pub active: bool,
    pub mood: Mood,
    pub kind: PetKind,
    pub talking: bool,
    pub look: (f32, f32),
    pub hovered: bool,
    pub reactions: Vec<String>,
    pub updates: usize,
    last: Ms,
}

impl PetLike for FakePet {
    fn update(&mut self, now: Ms) -> bool {
        if !self.active || now - self.last < 33.0 {
            return false;
        }
        self.last = now;
        self.updates += 1;
        true
    }
    fn next_wake(&self, _now: Ms) -> Wake {
        if self.active {
            Wake::Ambient
        } else {
            Wake::Idle
        }
    }
    fn paint(&self, s: &mut dyn Surface, rect: Rect, _now: Ms) {
        s.fill_rrect(rect, rect.w * 0.5, &Color::rgb8(0xF7, 0xB2, 0x67).into());
    }
    fn react(&mut self, name: &str, _ms: Ms, _now: Ms) {
        if self.active {
            self.reactions.push(name.to_string());
        }
    }
    fn set_mood(&mut self, mood: Mood, _now: Ms) {
        self.mood = mood;
    }
    fn set_kind(&mut self, kind: PetKind, _now: Ms) {
        self.kind = kind;
    }
    fn set_active(&mut self, active: bool, _now: Ms) {
        self.active = active;
    }
    fn set_look(&mut self, x: f32, y: f32, _now: Ms) {
        self.look = (x, y);
    }
    fn set_talking(&mut self, talking: bool, _now: Ms) {
        self.talking = talking;
    }
    fn overdraw(&self) -> f32 {
        4.0
    }
    fn set_hovered(&mut self, hovered: bool, _now: Ms) {
        self.hovered = hovered;
    }
}

/// Owns everything a `Cx` borrows.
pub struct Harness {
    pub store: FakeStore,
    pub env: Env,
    pub text: FakeText,
    pub damage: DamageSink,
    pub msgs: Vec<Msg>,
    pub focus: FocusId,
    pub rev: u64,
    pub epoch: f64,
    pub window_focused: bool,
    pub cursor: CursorShape,
}

impl Default for Harness {
    fn default() -> Self {
        Harness::new()
    }
}

impl Harness {
    pub fn new() -> Harness {
        Harness {
            store: FakeStore::default(),
            env: Env::default(),
            text: FakeText,
            damage: DamageSink::new(),
            msgs: Vec::new(),
            focus: FocusId::None,
            rev: 1,
            epoch: 1_700_000_000_000.0,
            window_focused: true,
            cursor: CursorShape::Arrow,
        }
    }
    /// Runs `f` with a `Cx` at monotonic `now` (epoch advances with it from `self.epoch`).
    pub fn with_cx<R>(&mut self, now: Ms, f: impl FnOnce(&mut Cx) -> R) -> R {
        let mut cx = Cx {
            now,
            epoch: self.epoch + now,
            rev: self.rev,
            store: &self.store,
            env: &self.env,
            text: &mut self.text,
            damage: &mut self.damage,
            msgs: &mut self.msgs,
            focus: &mut self.focus,
            window_focused: self.window_focused,
            cursor: self.cursor,
        };
        let r = f(&mut cx);
        self.cursor = cx.cursor;
        r
    }
    /// Runs `f` with a `PaintCx` clipped to `clip`.
    pub fn with_pc<R>(&self, now: Ms, clip: Rect, f: impl FnOnce(&PaintCx) -> R) -> R {
        let pc = PaintCx {
            now,
            epoch: self.epoch + now,
            clip,
            env: &self.env,
            store: &self.store,
            focus: &self.focus,
            window_focused: self.window_focused,
        };
        f(&pc)
    }
    /// A 792x792 transparent surface with the global scale applied (logical coordinates).
    pub fn surface() -> PixSurface {
        let mut s = PixSurface::new(WIN_DEV as u32, WIN_DEV as u32).expect("surface");
        s.scale(S, S);
        s
    }
    pub fn take_msgs(&mut self) -> Vec<Msg> {
        std::mem::take(&mut self.msgs)
    }
}
