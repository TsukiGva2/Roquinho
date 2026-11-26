//! The scene: a port of `qml/Main.qml` (ui-design.md §7).
//!
//! `App` owns the store, the views, the springs of the notch and the renderer, but never touches
//! X11: platform work comes out as [`PlatformReq`]s that `main.rs` executes. That keeps the
//! headless `--snapshot` mode possible.

use foco_base::event::{Button, Event, Key};
use foco_base::model::fmt_short;
use foco_base::wake::FRAME_MS;
use foco_base::{
    day_key, next_midnight_epoch, shortcut_command, Action, Anim, AppCmd, Color, Easing, Effect, IRect, Ms, Point,
    Rect, Scheduler, ScreenArg, Spring, Surface, TextStyle, TimerKey, Wake,
};
use foco_core::{autostart, rc, Rc, Store};
use foco_gfx::Renderer;
use foco_ui::notch::{paint_notch, NotchGeom};
use foco_ui::views::{Alert, AlertKind, AlertView, CollapsedView, DotView, ExpandedView, TipView};
use foco_ui::{
    CursorShape, Cx, DamageSink, Env, FocusId, Handled, Msg, PaintCx, ScreenInfo, StoreRead, TextMeasure, View,
    AUTO_ACCENT, S, WIN, WIN_DEV,
};
use foco_x11::Monitor;
use std::path::PathBuf;

/// What `main.rs` must do on the platform's behalf.
#[derive(Clone, Debug, PartialEq)]
pub enum PlatformReq {
    /// Window position in root coordinates.
    Place {
        x: i32,
        y: i32,
    },
    /// XShape input region, window-local device px.
    SetInputRegion(Vec<IRect>),
    RequestKeyboardFocus,
    ReleaseKeyboardFocus,
    RequestPaste,
    SetCursor(CursorShape),
    /// Re-query RandR (the debounce elapsed).
    RefreshMonitors,
    Exit,
}

/// Construction parameters.
pub struct AppInit {
    pub store: Store,
    pub rc: Rc,
    /// Write state.json / foco-maxrc / autostart files (false for `--snapshot`).
    pub persist: bool,
    pub state_path: PathBuf,
    pub exe: PathBuf,
    pub monitors: Vec<Monitor>,
    pub autostart: bool,
    /// Command line given when this instance was started (run 700 ms after launch, like Qt).
    pub initial_cmd: Option<String>,
}

const T_BOOT: TimerKey = TimerKey("boot", 0);
const T_WELCOME: TimerKey = TimerKey("welcome", 0);
const T_ALERT: TimerKey = TimerKey("alert", 0);
const T_QUIT: TimerKey = TimerKey("quit", 0);
const T_WAVE: TimerKey = TimerKey("wave", 0);
const T_TIP_STARTUP: TimerKey = TimerKey("tip-startup", 0);
const T_TIP_DAY: TimerKey = TimerKey("tip-day", 0);
const T_SAVE: TimerKey = TimerKey("save", 0);
const T_HEARTBEAT: TimerKey = TimerKey("heartbeat", 0);
const T_QUICK_IDLE: TimerKey = TimerKey("quick-idle", 0);
const T_SCREENS: TimerKey = TimerKey("screens", 0);
const T_INITCMD: TimerKey = TimerKey("initcmd", 0);

/// Cadence of `Wake::Ambient` in the app.
const AMBIENT_APP_MS: Ms = 100.0;
/// Period of the shared slot grid that every non-animating wake is rounded up to.
const SLOT_MS: Ms = 100.0;
/// Deadlines closer than this (30 Hz tweens, typing) keep their exact time. Every ambient step
/// (dot level, tail sway, breathing) asks for >= 50 ms, so they all land on the grid.
const SLOT_MIN_MS: Ms = 45.0;

/// Rounds `t` up to the next slot of a global grid (multiples of [`SLOT_MS`] (10 Hz) on the monotonic
/// clock). The pet, the live dot, tips and timer ticks then land on the same wake instead of
/// each costing its own: at most 10 wakes/s while nothing animates. Deadlines are only
/// ever delayed, by less than one slot.
fn ambient_slot(t: Ms) -> Ms {
    (t / SLOT_MS).ceil() * SLOT_MS
}
const SAVE_DEBOUNCE_MS: Ms = 400.0;
const HEARTBEAT_MS: Ms = 60_000.0;
const QUICK_IDLE_MS: Ms = 45_000.0;
const SCREEN_DEBOUNCE_MS: Ms = 400.0;
const FOCUS_OUT_GRACE_MS: Ms = 450.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Grab {
    Collapsed,
    Expanded,
    Tip,
    Dot,
    /// Press on the notch background (a click opens the panel).
    Background,
}

/// What the input region was computed from.
#[derive(Clone, PartialEq, Debug)]
struct RegionKey {
    w: i32,
    h: i32,
    hidden: bool,
    tip: Option<[i32; 4]>,
}

/// `Renderer` as the views' text measurer (logical px, same layout cache as painting).
struct Measure<'a>(&'a mut Renderer);

impl TextMeasure for Measure<'_> {
    fn measure(&mut self, s: &str, st: &TextStyle) -> f32 {
        self.0.measure(s, st)
    }
    fn line_height(&mut self, st: &TextStyle) -> f32 {
        self.0.line_height(st)
    }
    /// Noto Sans (the Qt default here): ascent 1069 of an em box of 1069 + 293.
    fn ascent(&mut self, st: &TextStyle) -> f32 {
        self.0.line_height(st) * (1069.0 / 1362.0)
    }
    fn elide(&mut self, s: &str, st: &TextStyle, max_w: f32) -> String {
        self.0.elide(s, st, max_w)
    }
}

/// Builds a `Cx` from disjoint fields of `$s` (so views can be borrowed alongside).
macro_rules! cx {
    ($s:ident, $m:ident, $now:expr, $epoch:expr) => {
        Cx {
            now: $now,
            epoch: $epoch,
            rev: $s.rev,
            store: &$s.store,
            env: &$s.env,
            text: &mut $m,
            damage: &mut $s.damage,
            msgs: &mut $s.msgs,
            focus: &mut $s.focus,
            window_focused: $s.window_focused,
            cursor: $s.cursor,
        }
    };
}

pub struct App {
    gfx: Renderer,
    store: Store,
    rc: Rc,
    persist: bool,
    state_path: PathBuf,
    exe: PathBuf,
    sched: Scheduler,
    env: Env,
    rev: u64,
    damage: DamageSink,
    msgs: Vec<Msg>,
    focus: FocusId,
    cursor: CursorShape,
    sent_cursor: CursorShape,
    reqs: Vec<PlatformReq>,
    exit: bool,

    booted: bool,
    quitting: bool,
    expanded: bool,
    settings_open: bool,
    quick_mode: bool,
    expand_stamp: Ms,
    alert: Option<Alert>,
    tip_pending: bool,
    last_wave: Ms,
    notch_hover: bool,
    pointer: Option<Point>,
    /// model revision last seen by the hover replay, and until when to keep replaying
    hover_rev: u64,
    rehover_until: Ms,
    window_focused: bool,
    kb_held: bool,
    grab: Option<Grab>,
    initial_cmd: Option<String>,

    spr_w: Spring,
    spr_h: Spring,
    anim_r: Anim,
    anim_shadow: Anim,
    geom: NotchGeom,
    shadow_op: f32,
    laid_out: bool,

    collapsed: CollapsedView,
    alert_view: AlertView,
    expanded_view: ExpandedView,
    dot: DotView,
    tip: TipView,
    tip_rect: Rect,

    monitors: Vec<Monitor>,
    placed: Option<(i32, i32)>,
    region: Option<RegionKey>,
    store_deadline: Option<f64>,
    last_hidden: bool,
    last_tips: bool,
    /// Offset `epoch - mono` seen at the last tick (suspend detection).
    last_offset: f64,
    last_wake_frame: bool,
    frame_grid: Ms,
}

fn hex(c: Color) -> String {
    let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", q(c.r), q(c.g), q(c.b))
}

impl App {
    pub fn new(init: AppInit, now: Ms, epoch: f64) -> App {
        let AppInit { store, rc, persist, state_path, exe, monitors, autostart, initial_cmd } = init;
        let accent_choice = rc.accent().and_then(Color::hex);
        let first_run = store.first_run();
        let hidden = store.settings().hidden;
        let tips = store.settings().tips_enabled;
        let mut env = Env {
            rev: 1,
            accent: accent_choice.unwrap_or(AUTO_ACCENT),
            accent_choice,
            autostart,
            screen_choice: rc.screen().unwrap_or("").to_string(),
            ..Env::default()
        };
        env.screens = screens_of(&monitors);
        let mut app = App {
            gfx: Renderer::new(),
            store,
            rc,
            persist,
            state_path,
            exe,
            sched: Scheduler::new(),
            env,
            rev: 1,
            damage: DamageSink::new(),
            msgs: Vec::new(),
            focus: FocusId::None,
            cursor: CursorShape::Arrow,
            sent_cursor: CursorShape::Arrow,
            reqs: Vec::new(),
            exit: false,
            booted: false,
            quitting: false,
            expanded: false,
            settings_open: false,
            quick_mode: false,
            expand_stamp: 0.0,
            alert: None,
            tip_pending: false,
            last_wave: f64::NEG_INFINITY,
            notch_hover: false,
            pointer: None,
            hover_rev: 0,
            rehover_until: 0.0,
            window_focused: false,
            kb_held: false,
            grab: None,
            initial_cmd,
            spr_w: Spring::new(0.0, 3.8, 0.32, 0.2),
            spr_h: Spring::new(0.0, 3.4, 0.38, 0.2),
            anim_r: Anim::new(19.0),
            anim_shadow: Anim::new(0.0),
            geom: NotchGeom::new(0.0, 0.0, 19.0),
            shadow_op: 0.0,
            laid_out: false,
            collapsed: CollapsedView::new(),
            alert_view: AlertView::new(),
            expanded_view: ExpandedView::new(),
            dot: DotView::new(),
            tip: TipView::new(),
            tip_rect: Rect::default(),
            monitors,
            placed: None,
            region: None,
            store_deadline: None,
            last_hidden: hidden,
            last_tips: tips,
            last_offset: epoch - now,
            last_wake_frame: false,
            frame_grid: now,
        };
        app.sched.schedule(T_BOOT, now + 250.0);
        app.sched.schedule(T_TIP_STARTUP, now + if first_run { 7500.0 } else { 3000.0 });
        app.schedule_tip_day(now, epoch);
        if app.initial_cmd.is_some() {
            app.sched.schedule(T_INITCMD, now + 700.0);
        }
        app.place_window(now, false);
        app.store_deadline = app.store.next_change(epoch);
        if app.store.running_count() > 0 {
            app.sched.schedule(T_HEARTBEAT, now + HEARTBEAT_MS);
        }
        app.damage.add_all();
        app
    }

    // ---------------------------------------------------------------- accessors

    /// Drains the platform requests accumulated since the last call.
    pub fn take_requests(&mut self) -> Vec<PlatformReq> {
        std::mem::take(&mut self.reqs)
    }
    pub fn has_damage(&self) -> bool {
        !self.damage.is_empty()
    }
    pub fn take_damage(&mut self) -> Vec<IRect> {
        self.damage.take()
    }
    pub fn damage_all(&mut self) {
        self.damage.add_all();
    }
    pub fn first_notch_visible(&self) -> bool {
        self.geom.visible()
    }

    fn hidden(&self) -> bool {
        self.store.settings().hidden
    }
    fn hidden_mode(&self) -> bool {
        self.hidden() && !self.quitting
    }
    fn hovered(&self) -> bool {
        self.notch_hover && !self.expanded && self.alert.is_none()
    }
    fn to_mono(t_epoch: f64, now: Ms, epoch: f64) -> Ms {
        now + (t_epoch - epoch)
    }

    // ---------------------------------------------------------------- actions (Main.qml:44-82)

    fn expand(&mut self, with_input: bool, now: Ms, epoch: f64) {
        if self.hidden() {
            self.apply(Action::SetHidden(false), now, epoch);
        }
        self.set_alert(None, now);
        let was = self.expanded;
        self.expanded = true;
        self.expand_stamp = now;
        if !was && now - self.last_wave > 90_000.0 {
            self.last_wave = now;
            self.sched.schedule(T_WAVE, now + 380.0);
        }
        if with_input {
            self.settings_open = false;
            self.quick_mode = true;
            self.request_kb();
            self.sched.schedule(T_QUICK_IDLE, now + QUICK_IDLE_MS);
            self.focus = FocusId::NewTask;
            let mut m = Measure(&mut self.gfx);
            let mut cx = cx!(self, m, now, epoch);
            self.expanded_view.focus_input(&mut cx);
            self.cursor = cx.cursor;
        }
    }

    fn collapse(&mut self) {
        self.expanded = false;
        self.settings_open = false;
        self.quick_mode = false;
        self.sched.cancel(T_QUICK_IDLE);
        self.focus = FocusId::None;
        if self.kb_held {
            self.kb_held = false;
            self.reqs.push(PlatformReq::ReleaseKeyboardFocus);
        }
    }

    fn hide_to_dot(&mut self, now: Ms, epoch: f64) {
        self.collapse();
        self.set_alert(None, now);
        self.apply(Action::SetHidden(true), now, epoch);
    }

    fn quit_app(&mut self, now: Ms) {
        self.save_now();
        self.expanded = false;
        self.quitting = true;
        self.sched.schedule(T_QUIT, now + 650.0);
    }

    fn show_alert(&mut self, a: Alert, ms: Ms, now: Ms) {
        if self.expanded || self.hidden() {
            return;
        }
        self.alert_view.set_alert(Some(&a), now);
        self.alert = Some(a);
        self.sched.schedule(T_ALERT, now + ms);
    }

    fn set_alert(&mut self, a: Option<Alert>, now: Ms) {
        match a {
            Some(a) => {
                self.alert_view.set_alert(Some(&a), now);
                self.alert = Some(a);
            }
            None => {
                if self.alert.take().is_some() {
                    self.alert_view.set_alert(None, now);
                }
                self.sched.cancel(T_ALERT);
            }
        }
    }

    fn request_kb(&mut self) {
        self.kb_held = true;
        self.reqs.push(PlatformReq::RequestKeyboardFocus);
    }

    // ---------------------------------------------------------------- store

    fn apply(&mut self, a: Action, now: Ms, epoch: f64) {
        let fx = self.store.apply(a, epoch);
        self.handle_effects(fx, now, epoch);
    }

    fn handle_effects(&mut self, fx: Vec<Effect>, now: Ms, epoch: f64) {
        for e in fx {
            match e {
                Effect::Completed { title, spent_ms } => {
                    let sub = format!("{title}  ·  {}", fmt_short(spent_ms));
                    self.show_alert(Alert { kind: AlertKind::Done, title: "Destruída!".into(), sub }, 2600.0, now);
                }
                Effect::Added { title, to_focus } => {
                    if self.quick_mode {
                        self.collapse();
                    }
                    let t = if to_focus { "No palco agora" } else { "Pra fila do gargarejo" };
                    self.show_alert(Alert { kind: AlertKind::Added, title: t.into(), sub: title }, 2000.0, now);
                }
                Effect::PetEvent(n) => {
                    self.collapsed.pet_react(&n, now);
                    self.expanded_view.pet_react(&n, now);
                }
                Effect::Say(_) | Effect::Celebrate { .. } => {
                    self.rev += 1;
                }
                Effect::ModelChanged => {
                    self.rev += 1;
                }
                Effect::SettingsChanged => {
                    self.rev += 1;
                    let tips = self.store.settings().tips_enabled;
                    if tips && !self.last_tips {
                        self.sched.schedule(T_TIP_DAY, now + 60_000.0);
                    }
                    self.last_tips = tips;
                    self.last_hidden = self.hidden();
                }
                Effect::Save => {
                    self.sched.schedule(T_SAVE, now + SAVE_DEBOUNCE_MS);
                }
            }
        }
        self.sync_quip(epoch);
        if self.store.running_count() > 0 && !self.sched.is_scheduled(T_HEARTBEAT) {
            self.sched.schedule(T_HEARTBEAT, now + HEARTBEAT_MS);
        }
        self.store_deadline = self.store.next_change(epoch);
    }

    /// `Store.quip` as the views see it (the store owns the 2600 ms expiry).
    fn sync_quip(&mut self, epoch: f64) {
        let q = if self.store.quip_active(epoch) { Some(self.store.quip()) } else { None };
        if q != self.env.quip.as_deref() {
            self.env.quip = q.map(str::to_string);
            self.env.rev += 1;
        }
    }

    fn save_now(&mut self) {
        if !self.persist {
            return;
        }
        if let Err(e) = self.store.save(&self.state_path) {
            eprintln!("foco-max: could not save {}: {e}", self.state_path.display());
        }
    }

    fn save_rc(&mut self) {
        if self.persist {
            if let Err(e) = self.rc.save() {
                eprintln!("foco-max: could not save foco-maxrc: {e}");
            }
        }
    }

    // ---------------------------------------------------------------- tips

    fn maybe_tip(&mut self, on_startup: bool, epoch: f64) {
        let s = self.store.settings();
        if !s.tips_enabled || self.tip_pending {
            return;
        }
        if on_startup || s.last_tip_day != day_key(epoch) {
            self.tip_pending = true;
        }
    }

    fn schedule_tip_day(&mut self, now: Ms, epoch: f64) {
        self.sched
            .schedule(T_TIP_DAY, Self::to_mono(next_midnight_epoch(epoch), now, epoch) + 50.0);
    }

    // ---------------------------------------------------------------- commands

    pub fn on_shortcut(&mut self, id: &str, now: Ms, epoch: f64) {
        if let Some(c) = shortcut_command(id) {
            self.on_cmd(c, now, epoch);
        }
    }

    /// Shortcut chips for the settings panel (`ids` as in foco-maxrc).
    pub fn set_shortcuts(&mut self, list: &[(String, String)]) {
        let cmd = |id: &str| match id {
            "quick-add" => Some("add"),
            "toggle-panel" => Some("toggle"),
            "toggle-hide" => Some("hide"),
            "complete" => Some("done"),
            "next-screen" => Some("screen"),
            _ => None,
        };
        let mut keys: Vec<(&'static str, Vec<String>)> = Vec::new();
        for (id, raw) in list {
            if let Some(c) = cmd(id) {
                keys.push((c, rc::key_labels(raw)));
            }
        }
        self.env.shortcut_keys = keys;
        self.env.rev += 1;
    }

    pub fn on_cmd(&mut self, cmd: AppCmd, now: Ms, epoch: f64) {
        self.exec_cmd(cmd, now, epoch);
        self.flush_msgs(now, epoch);
    }

    fn exec_cmd(&mut self, cmd: AppCmd, now: Ms, epoch: f64) {
        match cmd {
            AppCmd::Toggle => {
                if self.expanded {
                    self.collapse()
                } else {
                    self.expand(false, now, epoch)
                }
            }
            AppCmd::Open => self.expand(false, now, epoch),
            AppCmd::Collapse => self.collapse(),
            AppCmd::QuickAdd(None) => self.expand(true, now, epoch),
            AppCmd::Hide => {
                if self.hidden() {
                    self.apply(Action::SetHidden(false), now, epoch);
                } else {
                    self.hide_to_dot(now, epoch);
                }
            }
            AppCmd::Show => self.apply(Action::SetHidden(false), now, epoch),
            AppCmd::Quit => self.quit_app(now),
            AppCmd::Screen(a) => self.cmd_screen(a, now),
            AppCmd::SetAccent(c) => {
                self.env.accent_choice = c;
                self.env.accent = c.unwrap_or(AUTO_ACCENT);
                self.env.rev += 1;
                self.rc.set_accent(c.map(hex).as_deref());
                self.save_rc();
            }
            AppCmd::SetAutostart(v) => {
                self.env.autostart = v;
                self.env.rev += 1;
                if self.persist {
                    if let Err(e) = autostart::set(v, &self.exe) {
                        eprintln!("foco-max: autostart: {e}");
                    }
                }
            }
            AppCmd::OpenSettings => {
                if self.expanded {
                    self.settings_open = true;
                }
            }
            AppCmd::CloseSettings => self.settings_open = false,
            AppCmd::PetReact(n) => {
                self.collapsed.pet_react(&n, now);
                self.expanded_view.pet_react(&n, now);
            }
            AppCmd::Ignored => {}
            other => {
                if let Some(a) = other.to_action() {
                    self.apply(a, now, epoch);
                }
            }
        }
    }

    fn cmd_screen(&mut self, a: ScreenArg, now: Ms) {
        match a {
            ScreenArg::Next => {
                if self.monitors.len() < 2 {
                    return;
                }
                let cur = self.target_monitor().map(|m| m.name.clone());
                let i = self.monitors.iter().position(|m| Some(&m.name) == cur.as_ref()).unwrap_or(0);
                let next = self.monitors[(i + 1) % self.monitors.len()].name.clone();
                self.set_screen_choice(next, now);
            }
            ScreenArg::Primary => self.set_screen_choice(String::new(), now),
            ScreenArg::Named(n) => self.set_screen_choice(n, now),
        }
    }

    fn set_screen_choice(&mut self, name: String, now: Ms) {
        if name == self.env.screen_choice {
            return;
        }
        self.rc.set_screen(&name);
        self.save_rc();
        self.env.screen_choice = name;
        self.env.rev += 1;
        self.place_window(now, true);
    }

    // ---------------------------------------------------------------- monitors / placement

    /// RandR changed (call once per event; the app debounces by 400 ms).
    pub fn on_screens_changed(&mut self, now: Ms) {
        self.sched.schedule(T_SCREENS, now + SCREEN_DEBOUNCE_MS);
    }

    pub fn set_monitors(&mut self, monitors: Vec<Monitor>, now: Ms) {
        self.env.screens = screens_of(&monitors);
        self.env.rev += 1;
        self.monitors = monitors;
        self.place_window(now, true);
    }

    fn target_monitor(&self) -> Option<&Monitor> {
        let by_choice = (!self.env.screen_choice.is_empty())
            .then(|| self.monitors.iter().find(|m| m.name == self.env.screen_choice))
            .flatten();
        by_choice
            .or_else(|| self.monitors.iter().find(|m| m.primary))
            .or_else(|| self.monitors.first())
    }

    fn place_window(&mut self, now: Ms, announce: bool) {
        let Some(m) = self.target_monitor() else { return };
        let pos = (m.x + (m.w - WIN_DEV) / 2, m.y);
        let name = m.name.clone();
        let label = m.label.clone();
        if self.placed != Some(pos) {
            self.placed = Some(pos);
            self.reqs.push(PlatformReq::Place { x: pos.0, y: pos.1 });
        }
        if self.env.current_screen != name {
            let first = self.env.current_screen.is_empty();
            self.env.current_screen = name;
            self.env.rev += 1;
            if self.booted && !first && announce {
                let sub = format!("Tocando no monitor {label}");
                self.show_alert(Alert { kind: AlertKind::Info, title: "Turnê nova!".into(), sub }, 2200.0, now);
            }
        }
    }

    // ---------------------------------------------------------------- timers

    fn on_timer(&mut self, key: TimerKey, now: Ms, epoch: f64) {
        match key {
            T_BOOT => {
                self.booted = true;
                if self.store.first_run() {
                    if !self.env.autostart {
                        self.exec_cmd(AppCmd::SetAutostart(true), now, epoch);
                    }
                    self.sched.schedule(T_WELCOME, now + 900.0);
                }
            }
            T_WELCOME => {
                let a = Alert {
                    kind: AlertKind::Info,
                    title: format!("E aí. Eu sou o {}", self.store.settings().pet_name),
                    sub: "Clica aqui e joga a primeira demanda na roda".into(),
                };
                self.show_alert(a, 5200.0, now);
            }
            T_ALERT => self.set_alert(None, now),
            T_QUIT => {
                self.exit = true;
                self.reqs.push(PlatformReq::Exit);
            }
            T_WAVE => {
                self.collapsed.pet_react("wave", now);
                self.expanded_view.pet_react("wave", now);
            }
            T_TIP_STARTUP => self.maybe_tip(true, epoch),
            T_TIP_DAY => {
                self.maybe_tip(false, epoch);
                self.schedule_tip_day(now, epoch);
            }
            T_SAVE => self.save_now(),
            T_HEARTBEAT => {
                self.save_now();
                if self.store.running_count() > 0 {
                    self.sched.schedule(T_HEARTBEAT, now + HEARTBEAT_MS);
                }
            }
            T_QUICK_IDLE => {
                if self.quick_mode {
                    if self.expanded_view.input_empty() {
                        self.collapse();
                    } else {
                        self.sched.schedule(T_QUICK_IDLE, now + QUICK_IDLE_MS);
                    }
                }
            }
            T_SCREENS => self.reqs.push(PlatformReq::RefreshMonitors),
            T_INITCMD => {
                if let Some(line) = self.initial_cmd.take() {
                    if let Some((n, a)) = foco_base::split_line(&line) {
                        let c = foco_base::parse_command(n, a);
                        self.exec_cmd(c, now, epoch);
                    }
                }
            }
            _ => {}
        }
    }

    // ---------------------------------------------------------------- messages from views

    fn flush_msgs(&mut self, now: Ms, epoch: f64) {
        for _ in 0..8 {
            if self.msgs.is_empty() {
                return;
            }
            let batch = std::mem::take(&mut self.msgs);
            for m in batch {
                match m {
                    Msg::Act(a) => self.apply(a, now, epoch),
                    Msg::Cmd(c) => self.exec_cmd(c, now, epoch),
                    Msg::TipAnswer { never } => {
                        self.tip_pending = false;
                        self.env.tip_talking = false;
                        self.env.rev += 1;
                        let fx = self.store.answer_tip(never, &day_key(epoch), epoch);
                        self.handle_effects(fx, now, epoch);
                    }
                    Msg::Say(t) => {
                        let fx = self.store.say(&t, epoch);
                        self.handle_effects(fx, now, epoch);
                    }
                    Msg::RequestPaste => self.reqs.push(PlatformReq::RequestPaste),
                }
            }
        }
    }

    // ---------------------------------------------------------------- input

    /// An event in logical px (main.rs divides device coordinates by `S`).
    pub fn on_event(&mut self, ev: Event, now: Ms, epoch: f64) {
        match &ev {
            Event::FocusIn => {
                self.window_focused = true;
                self.damage_focus_dependent();
            }
            Event::FocusOut => {
                self.window_focused = false;
                self.forward_to_expanded(&ev, now, epoch);
                self.damage_focus_dependent();
                if self.expanded && now - self.expand_stamp > FOCUS_OUT_GRACE_MS {
                    self.collapse();
                }
            }
            Event::PointerMove { pos } => {
                self.pointer = Some(*pos);
                self.cursor = CursorShape::Arrow;
                self.update_notch_hover();
                self.pointer_move(&ev, now, epoch);
            }
            Event::PointerLeave => {
                self.pointer = None;
                self.notch_hover = false;
                self.cursor = CursorShape::Arrow;
                self.pointer_move(&ev, now, epoch);
            }
            Event::PointerDown { pos, button, .. } => {
                self.pointer = Some(*pos);
                self.update_notch_hover();
                if !self.window_focused {
                    self.request_kb();
                }
                if *button == Button::Left {
                    self.pointer_down(&ev, *pos, now, epoch);
                }
            }
            Event::PointerUp { pos, .. } => self.pointer_up(&ev, *pos, now, epoch),
            Event::Wheel { .. } => {
                if self.expanded {
                    self.forward_to_expanded(&ev, now, epoch);
                    // the content glides under a still pointer (200 ms): keep hover in step
                    self.rehover_until = self.rehover_until.max(now + 350.0);
                }
            }
            Event::Key(k) => {
                let handled = if self.expanded { self.forward_to_expanded(&ev, now, epoch) } else { Handled::No };
                if handled == Handled::No && k.key == Key::Escape && self.expanded {
                    self.collapse();
                }
            }
            Event::Paste(_) => {
                if self.expanded {
                    self.forward_to_expanded(&ev, now, epoch);
                }
            }
        }
        self.flush_msgs(now, epoch);
    }

    fn damage_focus_dependent(&mut self) {
        if self.expanded {
            self.damage.add(self.geom.clip_rect());
        }
    }

    fn update_notch_hover(&mut self) {
        let Some(p) = self.pointer else { return };
        let g = &self.geom;
        let h = g.visible() && p.x >= g.x && p.x < g.x + g.width() && p.y >= 0.0 && p.y < g.h;
        self.notch_hover = h;
    }

    fn forward_to_expanded(&mut self, ev: &Event, now: Ms, epoch: f64) -> Handled {
        let mut m = Measure(&mut self.gfx);
        let mut cx = cx!(self, m, now, epoch);
        let h = self.expanded_view.event(&mut cx, ev);
        self.cursor = cx.cursor;
        h
    }

    fn pointer_move(&mut self, ev: &Event, now: Ms, epoch: f64) {
        let grab = self.grab;
        let hidden = self.hidden();
        {
            let mut m = Measure(&mut self.gfx);
            let mut cx = cx!(self, m, now, epoch);
            match grab {
                Some(Grab::Collapsed) => {
                    self.collapsed.event(&mut cx, ev);
                }
                Some(Grab::Expanded) => {
                    self.expanded_view.event(&mut cx, ev);
                }
                Some(Grab::Tip) => {
                    self.tip.event(&mut cx, ev);
                }
                Some(Grab::Dot) => {
                    self.dot.event(&mut cx, ev);
                }
                Some(Grab::Background) => {}
                None => {
                    if self.tip.visible(now) {
                        self.tip.event(&mut cx, ev);
                    }
                    if hidden {
                        self.dot.event(&mut cx, ev);
                    }
                    if self.expanded {
                        self.expanded_view.event(&mut cx, ev);
                    } else {
                        self.collapsed.event(&mut cx, ev);
                    }
                }
            }
            self.cursor = cx.cursor;
        }
        if self.cursor == CursorShape::Arrow && self.notch_hover && !self.expanded {
            self.cursor = CursorShape::Hand;
        }
        if self.cursor != self.sent_cursor {
            self.sent_cursor = self.cursor;
            self.reqs.push(PlatformReq::SetCursor(self.cursor));
        }
    }

    fn pointer_down(&mut self, ev: &Event, pos: Point, now: Ms, epoch: f64) {
        let hidden = self.hidden();
        let mut m = Measure(&mut self.gfx);
        let mut cx = cx!(self, m, now, epoch);
        let mut taken: Option<(Grab, Handled)> = None;
        if self.tip.visible(now) {
            let h = self.tip.event(&mut cx, ev);
            if h != Handled::No {
                taken = Some((Grab::Tip, h));
            }
        }
        if taken.is_none() && hidden {
            let h = self.dot.event(&mut cx, ev);
            if h != Handled::No {
                taken = Some((Grab::Dot, h));
            }
        }
        if taken.is_none() && self.geom.clip_rect().contains(pos) {
            if self.expanded {
                let h = self.expanded_view.event(&mut cx, ev);
                if h != Handled::No {
                    taken = Some((Grab::Expanded, h));
                }
            } else {
                let h = self.collapsed.event(&mut cx, ev);
                if h != Handled::No {
                    taken = Some((Grab::Collapsed, h));
                }
            }
        }
        self.cursor = cx.cursor;
        match taken {
            Some((g, Handled::Grab)) => self.grab = Some(g),
            Some(_) => {}
            None => {
                let inside = self.geom.visible() && self.geom.rect().contains(pos);
                if inside && !self.expanded {
                    self.grab = Some(Grab::Background);
                }
            }
        }
    }

    fn pointer_up(&mut self, ev: &Event, pos: Point, now: Ms, epoch: f64) {
        let Some(g) = self.grab.take() else {
            // a release with nobody holding the pointer: nothing to do
            return;
        };
        let mut m = Measure(&mut self.gfx);
        let mut cx = cx!(self, m, now, epoch);
        match g {
            Grab::Collapsed => {
                self.collapsed.event(&mut cx, ev);
            }
            Grab::Expanded => {
                self.expanded_view.event(&mut cx, ev);
            }
            Grab::Tip => {
                self.tip.event(&mut cx, ev);
            }
            Grab::Dot => {
                self.dot.event(&mut cx, ev);
            }
            Grab::Background => {}
        }
        self.cursor = cx.cursor;
        if g == Grab::Background && !self.expanded && self.geom.rect().contains(pos) {
            self.expand(false, now, epoch);
        }
    }

    // ---------------------------------------------------------------- frame

    /// Advances the scene to `now`: timers, derived store changes, view updates, springs,
    /// layout, input region. Collects damage and platform requests.
    pub fn tick(&mut self, now: Ms, epoch: f64) {
        // suspend: Instant does not advance while suspended; rebuild epoch-derived deadlines
        let off = epoch - now;
        if (off - self.last_offset).abs() > 1000.0 {
            self.last_offset = off;
            self.on_clock_jump(now, epoch);
        }
        for key in self.sched.pop_due(now) {
            self.on_timer(key, now, epoch);
        }
        if self.store_deadline.is_some_and(|d| epoch >= d) {
            let fx = self.store.apply(Action::Tick, epoch);
            self.handle_effects(fx, now, epoch);
            self.rev += 1;
        }
        self.sync_quip(epoch);
        self.flush_msgs(now, epoch);
        self.update_scene(now, epoch);
        self.flush_msgs(now, epoch);
        self.update_input_region(now);
        self.last_wake_frame = false;
    }

    fn on_clock_jump(&mut self, now: Ms, epoch: f64) {
        self.rev += 1;
        self.schedule_tip_day(now, epoch);
        self.store_deadline = self.store.next_change(epoch);
        self.damage.add_all();
    }

    fn targets(&self) -> (f32, f32, f32) {
        let hidden_mode = self.hidden_mode();
        let w = if !self.booted || self.quitting {
            0.0
        } else if hidden_mode {
            36.0
        } else if self.expanded {
            548.0
        } else if self.alert.is_some() {
            400.0
        } else {
            self.collapsed.implicit_width()
        };
        let h = if !self.booted || self.quitting || hidden_mode {
            0.0
        } else if self.expanded {
            self.expanded_view.implicit_height()
        } else if self.alert.is_some() {
            62.0
        } else if self.hovered() {
            42.0
        } else {
            38.0
        };
        let r = if self.expanded {
            32.0
        } else if self.alert.is_some() {
            28.0
        } else if self.hovered() {
            21.0
        } else {
            19.0
        };
        (w, h, r)
    }

    fn update_scene(&mut self, now: Ms, epoch: f64) {
        let hidden_mode = self.hidden_mode();
        let expanded_shown = self.expanded && !self.quitting;
        let collapsed_shown = self.booted && !self.quitting && !self.expanded && self.alert.is_none() && !hidden_mode;
        let alert_shown = self.alert.is_some() && !self.expanded && !hidden_mode && !self.quitting;
        let dot_shown = hidden_mode && self.booted;
        let tip_shown =
            self.tip_pending && self.booted && !self.quitting && !self.expanded && !hidden_mode && self.alert.is_none();
        self.update_notch_hover();
        let hovered = self.hovered();

        self.collapsed.set_shown(collapsed_shown, now);
        self.collapsed.set_hovered(hovered, now);
        self.alert_view.set_shown(alert_shown, now);
        self.expanded_view.set_shown(expanded_shown, now);
        self.expanded_view.set_settings_open(self.settings_open, now);
        self.dot.set_shown(dot_shown, now);
        self.tip.set_shown(tip_shown, now);

        // 1) views pull state and report implicit sizes
        {
            let mut m = Measure(&mut self.gfx);
            let mut cx = cx!(self, m, now, epoch);
            self.collapsed.update(&mut cx);
            self.alert_view.update(&mut cx);
            self.expanded_view.update(&mut cx);
            self.dot.update(&mut cx);
            self.tip.update(&mut cx);
            self.cursor = cx.cursor;
        }
        let typing = self.tip.typing();
        if typing != self.env.tip_talking {
            self.env.tip_talking = typing;
            self.env.rev += 1;
        }

        // 2) springs toward the derived targets
        let (tw, th, tr) = self.targets();
        self.spr_w.set_target(tw, now);
        self.spr_h.set_target(th, now);
        self.anim_r.animate_to(tr, 320.0, Easing::OutCubic, now);
        let sh = if self.expanded {
            0.65
        } else if self.alert.is_some() {
            0.45
        } else {
            0.0
        };
        self.anim_shadow.animate_to(sh, 280.0, Easing::Linear, now);
        let cw = self.spr_w.step(now);
        let ch = self.spr_h.step(now);
        let cr = self.anim_r.value(now);
        let shadow = self.anim_shadow.value(now);

        // 3) notch geometry and its damage: shape + shadow overhang, old and new
        let g = NotchGeom::new(cw, ch, cr);
        if !self.laid_out || g != self.geom || (shadow - self.shadow_op).abs() >= 0.004 {
            let old = self.geom.bbox(self.shadow_op > 0.004);
            let new = g.bbox(shadow > 0.004);
            if g.visible() || self.geom.visible() {
                self.damage.add(old);
                self.damage.add(new);
            }
            self.geom = g;
            self.shadow_op = shadow;
            self.laid_out = true;
        }

        // 4) layout in window coordinates
        let clip = g.clip_rect();
        let cwid = clip.w;
        let col_w = self.collapsed.implicit_width();
        let col_h = if hovered { 42.0 } else { 38.0 };
        let col_frame = Rect::new(clip.x + (cwid - col_w) * 0.5, 0.0, col_w, col_h);
        let alert_frame = Rect::new(clip.x + (cwid - 400.0) * 0.5, 0.0, 400.0, 62.0);
        let exp_h = self.expanded_view.implicit_height();
        let exp_frame = Rect::new(clip.x + (cwid - 548.0) * 0.5, 0.0, 548.0, exp_h);
        let dot_frame = Rect::new(((WIN - 34.0) * 0.5).round(), (22.0f32 - 17.0).round(), 34.0, 34.0);
        let tsz = self.tip.size();
        let tip_frame = Rect::new(
            ((WIN - col_w) * 0.5 + self.collapsed.pet_center() - TipView::TAIL_X).round(),
            (ch.max(30.0) + 8.0).round(),
            tsz.w,
            tsz.h,
        );
        self.tip_rect = tip_frame;
        let mut m = Measure(&mut self.gfx);
        let mut cx = cx!(self, m, now, epoch);
        self.collapsed.layout(&mut cx, col_frame);
        self.alert_view.layout(&mut cx, alert_frame);
        self.expanded_view.layout(&mut cx, exp_frame);
        self.dot.layout(&mut cx, dot_frame);
        self.tip.layout(&mut cx, tip_frame);
        self.cursor = cx.cursor;
        self.replay_hover(now, epoch);
    }

    /// Qt re-evaluates hover when items move under a still pointer (a task added or removed, a
    /// list reflowing). Replays the last pointer position for a while after the model changes.
    fn replay_hover(&mut self, now: Ms, epoch: f64) {
        if self.rev != self.hover_rev {
            self.hover_rev = self.rev;
            self.rehover_until = now + 500.0;
        }
        if self.expanded && self.grab.is_none() && now < self.rehover_until {
            if let Some(pos) = self.pointer {
                self.pointer_move(&Event::PointerMove { pos }, now, epoch);
            }
        }
    }

    fn update_input_region(&mut self, now: Ms) {
        let hidden_mode = self.hidden_mode();
        let (tw, th, _) = self.targets();
        let tip_shown = self.tip.visible(now) || (self.tip_pending && self.booted && !self.expanded);
        let tip = (tip_shown && !self.tip_rect.is_empty()).then(|| {
            let r = self.tip_rect;
            [r.x.round() as i32, r.y.round() as i32, r.w.round() as i32, r.h.round() as i32]
        });
        let key = RegionKey { w: tw.round() as i32, h: th.round() as i32, hidden: hidden_mode, tip };
        if self.region.as_ref() == Some(&key) {
            return;
        }
        let mut rects: Vec<Rect> = Vec::with_capacity(2);
        if hidden_mode {
            rects.push(Rect::new(WIN * 0.5 - 23.0, 2.0, 46.0, 46.0));
        } else {
            let w = tw.max(8.0) + 32.0;
            let h = th.max(6.0) + 4.0;
            rects.push(Rect::new(((WIN - w) * 0.5).floor(), 0.0, w, h));
        }
        if tip.is_some() {
            let r = self.tip_rect;
            rects.push(Rect::new(r.x - 4.0, r.y - 2.0, r.w + 8.0, r.h + 8.0));
        }
        let dev: Vec<IRect> = rects
            .iter()
            .map(|r| Rect::new(r.x * S, r.y * S, r.w * S, r.h * S).round_out())
            .collect();
        self.reqs.push(PlatformReq::SetInputRegion(dev));
        self.region = Some(key);
    }

    /// Monotonic deadline of the next wake (`None` = sleep until an fd is readable).
    pub fn next_deadline(&mut self, now: Ms, epoch: f64) -> Option<Ms> {
        let mut w = Wake::Idle;
        if !self.spr_w.settled()
            || !self.spr_h.settled()
            || self.anim_r.is_animating(now)
            || self.anim_shadow.is_animating(now)
        {
            w = Wake::Frame;
        }
        w = w
            .merge_at(self.collapsed.next_wake(now), now)
            .merge_at(self.alert_view.next_wake(now), now)
            .merge_at(self.expanded_view.next_wake(now), now)
            .merge_at(self.dot.next_wake(now), now)
            .merge_at(self.tip.next_wake(now), now);
        if let Some(t) = self.sched.next_deadline() {
            w = w.merge_at(Wake::at(now, t), now);
        }
        if let Some(d) = self.store_deadline {
            w = w.merge_at(Wake::at(now, Self::to_mono(d, now, epoch)), now);
        }
        if !self.msgs.is_empty() {
            w = Wake::Frame;
        }
        match w {
            Wake::Idle => None,
            Wake::Frame => {
                // keep the 60 Hz grid: previous slot + one frame, not `now` + one frame
                let mut t = self.frame_grid + FRAME_MS;
                if t <= now || t > now + 2.0 * FRAME_MS {
                    t = now + FRAME_MS;
                }
                self.frame_grid = t;
                self.last_wake_frame = true;
                Some(t)
            }
            Wake::Ambient => {
                // ambient pulses (live dot, pulse ring, tip float): the pictures are a few
                // pixels in size and every extra wake costs more than the repaint
                self.frame_grid = now;
                Some(ambient_slot(now + AMBIENT_APP_MS))
            }
            other => {
                let d = other
                    .deadline(now)
                    .map(|t| if t > now + SLOT_MIN_MS { ambient_slot(t) } else { t });
                self.frame_grid = now;
                d
            }
        }
    }

    // ---------------------------------------------------------------- paint

    /// Paints `rects` (device px) into `target` (BGRA, `WIN_DEV` square).
    pub fn render(&mut self, target: &mut [u8], rects: &[IRect], now: Ms, epoch: f64) {
        let App {
            gfx,
            store,
            env,
            focus,
            window_focused,
            geom,
            shadow_op,
            collapsed,
            alert_view,
            expanded_view,
            dot,
            tip,
            ..
        } = self;
        for r in rects {
            let clip = Rect::new(r.x as f32 / S, r.y as f32 / S, r.w as f32 / S, r.h as f32 / S).inflate(1.0, 1.0);
            let st: &dyn StoreRead = &*store;
            gfx.render(target, WIN_DEV as u32, WIN_DEV as u32, &[*r], &mut |c| {
                c.scale(S, S);
                let pc = PaintCx { now, epoch, clip, env, store: st, focus, window_focused: *window_focused };
                paint_notch(c, geom, *shadow_op);
                if geom.visible() {
                    c.save();
                    c.clip_rect(geom.clip_rect());
                    if collapsed.bounds().intersects(&clip) {
                        collapsed.paint(c, &pc);
                    }
                    alert_view.paint(c, &pc);
                    expanded_view.paint(c, &pc);
                    c.restore();
                }
                tip.paint(c, &pc);
                dot.paint(c, &pc);
            });
        }
    }

    // ---------------------------------------------------------------- snapshot helpers

    /// `--snapshot alert`: shows a sample alert (bypasses the boot delay checks).
    pub fn debug_alert(&mut self, now: Ms) {
        let a =
            Alert { kind: AlertKind::Done, title: "Destruída!".into(), sub: "Escrever relatório  ·  12m".into() };
        self.show_alert(a, 60_000.0, now);
    }

    /// `--snapshot tip`: shows the shortcut tip.
    pub fn debug_tip(&mut self) {
        self.tip_pending = true;
    }

    /// Writes state.json now (SIGTERM / SIGINT).
    pub fn save_on_exit(&mut self) {
        self.save_now();
    }
}

fn screens_of(monitors: &[Monitor]) -> Vec<ScreenInfo> {
    monitors
        .iter()
        .map(|m| ScreenInfo { name: m.name.clone(), label: m.label.clone(), primary: m.primary })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const STATE: &str = r#"{"version":1,"lastSeen":1,"settings":{"tipsEnabled":true,"lastTipDay":"2000-01-01"},
        "totalDone":0,"focus":[],"queue":[],"done":[]}"#;
    const EPOCH0: f64 = 1_800_000_000_000.0;

    fn mk(state: &str) -> App {
        let store = Store::from_json_at(state, EPOCH0);
        let mut app = App::new(
            AppInit {
                store,
                rc: Rc::empty("/nonexistent/foco-maxrc"),
                persist: false,
                state_path: PathBuf::from("/nonexistent/state.json"),
                exe: PathBuf::from("foco-max"),
                monitors: Vec::new(),
                autostart: false,
                initial_cmd: None,
            },
            0.0,
            EPOCH0,
        );
        app.on_cmd(AppCmd::Show, 0.0, EPOCH0);
        app
    }

    fn run_for(app: &mut App, t: &mut Ms, ms: Ms) {
        let until = *t + ms;
        run(app, t, until);
    }

    fn run(app: &mut App, t: &mut Ms, until: Ms) {
        while *t < until {
            app.tick(*t, EPOCH0 + *t);
            let _ = app.take_requests();
            let _ = app.take_damage();
            *t += 16.0;
        }
    }

    /// A press+release on the middle of the collapsed notch at `at` ms after the start.
    fn click_at(state: &str, at: Ms, y: f32) -> (bool, bool) {
        click_xy(state, at, 380.0, y)
    }

    fn click_xy(state: &str, at: Ms, x: f32, y: f32) -> (bool, bool) {
        let mut app = mk(state);
        let mut t = 0.0;
        run(&mut app, &mut t, at);
        let p = Point::new(x, y);
        app.on_event(Event::PointerMove { pos: p }, t, EPOCH0 + t);
        run_for(&mut app, &mut t, 32.0);
        let tip_pending = app.tip_pending;
        app.on_event(Event::PointerDown { pos: p, button: Button::Left, clicks: 1 }, t, EPOCH0 + t);
        run_for(&mut app, &mut t, 48.0);
        app.on_event(Event::PointerUp { pos: p, button: Button::Left }, t, EPOCH0 + t);
        run_for(&mut app, &mut t, 32.0);
        (app.expanded, tip_pending)
    }

    #[test]
    fn click_on_the_notch_expands_it_at_any_time_around_the_tip() {
        // QA report: "with the tip showing, one early click on the notch did not expand it"
        let mut t = 400.0;
        while t < 4800.0 {
            let (expanded, _) = click_at(STATE, t, 12.0);
            assert!(expanded, "click at {t} ms did not expand");
            t += 40.0;
        }
    }

    const STATE_RUNNING: &str = r#"{"version":1,"lastSeen":1800000000000,"settings":{"tipsEnabled":true,"lastTipDay":"2000-01-01"},
        "totalDone":0,"focus":[{"tid":"a","title":"estudar","accMs":0,"startedAt":1800000000000,"running":true,
        "createdAt":1,"doneAt":0,"spentMs":0}],"queue":[],"done":[]}"#;

    #[test]
    fn click_matrix_with_a_running_task() {
        for x in [275.0, 300.0, 335.0, 360.0, 380.0] {
            for y in [3.0, 18.0, 33.0] {
                let mut t = 2800.0;
                while t < 4600.0 {
                    let (expanded, _) = click_xy(STATE_RUNNING, t, x, y);
                    assert!(expanded, "click at ({x},{y}) {t} ms did not expand");
                    t += 120.0;
                }
            }
        }
    }
}
