//! Shared context types: scale constants, the store/text/pet seams, `Env`, messages, damage sink
//! and the per-iteration `Cx` / `PaintCx`.
//!
//! Frozen by wave 2 (ui-design.md §1). The API may only grow.

use foco_base::{theme, Action, AppCmd, Color, Damage, IRect, Ms, Rect, StateFile, TextStyle};

/// Global device scale: logical px -> device px.
pub const S: f32 = 25.0 / 24.0;
/// Logical window side.
pub const WIN: f32 = 760.0;
/// Device window side (`ceil(WIN * S)`).
pub const WIN_DEV: i32 = 792;
/// Automatic accent colour (`#315bef`, what the Qt build shows without kdeglobals).
pub const AUTO_ACCENT: Color = Color::rgb8(0x31, 0x5b, 0xef);

/// Read-only view of the store (wave 2 implements it for `foco_core::Store`).
pub trait StoreRead {
    /// focus / queue / done / settings / `total_done`.
    fn state(&self) -> &StateFile;
    /// Epoch ms until which the pet celebrates.
    fn celebrate_until(&self) -> f64;
    /// True on the very first run (no state.json yet).
    fn first_run(&self) -> bool;
}

/// Text measurement outside a render pass (implemented over the foco-gfx fonts; in tests by a
/// fixed-width fake).
pub trait TextMeasure {
    fn measure(&mut self, s: &str, st: &TextStyle) -> f32;
    fn line_height(&mut self, st: &TextStyle) -> f32;
    /// Baseline distance from the top of the line box (default: 0.8 of the line height).
    fn ascent(&mut self, st: &TextStyle) -> f32 {
        self.line_height(st) * 0.8
    }
    /// `Text.ElideRight`.
    fn elide(&mut self, s: &str, st: &TextStyle, max_w: f32) -> String;
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScreenInfo {
    pub name: String,
    pub label: String,
    pub primary: bool,
}

/// Environment owned by the App; `rev` bumps on any change.
#[derive(Clone, Debug)]
pub struct Env {
    pub rev: u64,
    /// Choice or automatic accent.
    pub accent: Color,
    pub accent_choice: Option<Color>,
    pub autostart: bool,
    /// Sorted like `Backend::sortedScreens`.
    pub screens: Vec<ScreenInfo>,
    /// `""` = primary.
    pub screen_choice: String,
    pub current_screen: String,
    /// Command -> key names (`["Meta","Alt","N"]` or `["—"]`).
    pub shortcut_keys: Vec<(&'static str, Vec<String>)>,
    /// `Store.quip` (App-owned, 2600 ms).
    pub quip: Option<String>,
    /// `Store.tipTalking`.
    pub tip_talking: bool,
}

impl Default for Env {
    fn default() -> Self {
        Env {
            rev: 0,
            accent: AUTO_ACCENT,
            accent_choice: None,
            autostart: false,
            screens: Vec::new(),
            screen_choice: String::new(),
            current_screen: String::new(),
            shortcut_keys: Vec::new(),
            quip: None,
            tip_talking: false,
        }
    }
}

impl Env {
    /// `Store.keysFor(cmd, letter)`: the configured keys, else `Meta+Alt+<letter>`.
    pub fn keys_for(&self, cmd: &str, letter: &str) -> Vec<String> {
        for (c, k) in &self.shortcut_keys {
            if *c == cmd && !k.is_empty() {
                return k.clone();
            }
        }
        vec!["Meta".into(), "Alt".into(), letter.into()]
    }
    /// `Theme.accentText`.
    pub fn accent_text(&self) -> Color {
        theme::accent_text(self.accent)
    }
}

/// Who owns the keyboard focus inside the window.
#[derive(Clone, Debug, PartialEq)]
pub enum FocusId {
    None,
    NewTask,
    /// Queue rename editor of the task with this `tid`.
    Rename(String),
    PetName,
}

/// What views ask the App to do (applied after event dispatch, never mid-borrow).
#[derive(Clone, Debug, PartialEq)]
pub enum Msg {
    /// Store mutation.
    Act(Action),
    /// Shell command: Open, Collapse, Hide, Show, Quit, Open/CloseSettings, `SetAccent`,
    /// `SetAutostart`, Screen(..).
    Cmd(AppCmd),
    TipAnswer {
        never: bool,
    },
    /// A petted quip (goes through the same path as `Effect::Say`).
    Say(String),
    RequestPaste,
}

/// Pointer cursor requested by the hovered view (`cursorShape`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CursorShape {
    #[default]
    Arrow,
    Hand,
    OpenHand,
    ClosedHand,
    IBeam,
}

/// How an event was treated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handled {
    No,
    Yes,
    /// Capture the pointer until `PointerUp`.
    Grab,
}

/// Dirty rectangles in device px, fed with logical rects.
#[derive(Clone, Debug, Default)]
pub struct DamageSink {
    d: Damage,
}

impl DamageSink {
    pub fn new() -> DamageSink {
        DamageSink::default()
    }
    /// Logical rect -> `(r*S).round_out()` inflated by 1 px, clipped to the window.
    pub fn add(&mut self, r: Rect) {
        if r.is_empty() || !r.x.is_finite() || !r.y.is_finite() || !r.w.is_finite() || !r.h.is_finite() {
            return;
        }
        let d = Rect::new(r.x * S, r.y * S, r.w * S, r.h * S).round_out();
        let d = IRect::new(d.x - 1, d.y - 1, d.w + 2, d.h + 2);
        if let Some(c) = d.intersect(&IRect::new(0, 0, WIN_DEV, WIN_DEV)) {
            self.d.add(c);
        }
    }
    /// Damages old and new when they differ.
    pub fn add_moved(&mut self, old: Rect, new: Rect) {
        if old != new {
            self.add(old);
            self.add(new);
        }
    }
    /// Adds a rect already in device px.
    pub fn add_device(&mut self, r: IRect) {
        if let Some(c) = r.intersect(&IRect::new(0, 0, WIN_DEV, WIN_DEV)) {
            self.d.add(c);
        }
    }
    /// The whole window.
    pub fn add_all(&mut self) {
        self.d.add(IRect::new(0, 0, WIN_DEV, WIN_DEV));
    }
    pub fn is_empty(&self) -> bool {
        self.d.is_empty()
    }
    pub fn rects(&self) -> &[IRect] {
        self.d.rects()
    }
    pub fn take(&mut self) -> Vec<IRect> {
        self.d.take()
    }
    pub fn clear(&mut self) {
        self.d.clear();
    }
}

/// Mutable context of `update` / `layout` / `event`.
pub struct Cx<'a> {
    pub now: Ms,
    /// Wall clock epoch ms.
    pub epoch: f64,
    /// Model revision (bumped by App on ModelChanged/SettingsChanged).
    pub rev: u64,
    pub store: &'a dyn StoreRead,
    pub env: &'a Env,
    pub text: &'a mut dyn TextMeasure,
    pub damage: &'a mut DamageSink,
    pub msgs: &'a mut Vec<Msg>,
    pub focus: &'a mut FocusId,
    /// Active focus = `window_focused && *focus == mine`.
    pub window_focused: bool,
    /// Views set it on hover; App forwards changes to X11.
    pub cursor: CursorShape,
}

impl Cx<'_> {
    /// True when `id` owns the keyboard and the window is active.
    pub fn has_focus(&self, id: &FocusId) -> bool {
        self.window_focused && self.focus == id
    }
}

/// Read-only context of `paint`.
pub struct PaintCx<'a> {
    pub now: Ms,
    pub epoch: f64,
    /// Logical clip (the damaged rect).
    pub clip: Rect,
    pub env: &'a Env,
    pub store: &'a dyn StoreRead,
    pub focus: &'a FocusId,
    pub window_focused: bool,
}

impl PaintCx<'_> {
    pub fn has_focus(&self, id: &FocusId) -> bool {
        self.window_focused && self.focus == id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn damage_sink_scales_and_clips() {
        let mut d = DamageSink::new();
        d.add(Rect::new(0.0, 0.0, 24.0, 24.0));
        // 24 * 25/24 = 25 px, inflated by 1 and clipped at 0
        assert_eq!(d.rects(), &[IRect::new(0, 0, 26, 26)]);
        d.clear();
        d.add(Rect::new(10_000.0, 0.0, 5.0, 5.0));
        assert!(d.is_empty());
        d.add(Rect::new(0.0, 0.0, 0.0, 5.0));
        assert!(d.is_empty());
    }

    #[test]
    fn env_keys() {
        let mut e = Env::default();
        assert_eq!(e.keys_for("add", "N"), vec!["Meta", "Alt", "N"]);
        e.shortcut_keys.push(("add", vec!["Ctrl".into(), "K".into()]));
        assert_eq!(e.keys_for("add", "N"), vec!["Ctrl", "K"]);
    }
}
