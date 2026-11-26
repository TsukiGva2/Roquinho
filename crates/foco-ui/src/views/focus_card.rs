//! `FocusCard.qml` + the EM FOCO list and empty state of `MainSections.qml` (wave 3b).
//!
//! The section is `FocusSection`: 80 px cards 8 px apart (animated through a `KeyedList`), each
//! with a 48 px minute ring, a clock, a hover-revealed undo button and the green check that
//! flashes before completing. With nothing in focus it shows the dashed 72 px empty state.
//!
//! Wake/damage: clocks wake exactly when the displayed second flips (`model::next_tick`) and only
//! while the section is visible; a tick damages the clock row and the ring of that card only.

use crate::cx::{CursorShape, Cx, Handled, Msg, PaintCx};
use crate::model;
use crate::pet::{new_pet, PetLike};
use crate::view::{Section, View};
use crate::widgets::{paint_icon, ColorAnim, Hover, Icon, IconButton, KeyedList, ListSpec, MouseArea};
use foco_base::{
    path_arc, path_circle, path_rrect, theme, Action, Anim, Color, Easing, Event, Fill, Ms, PetKind, Point, Rect,
    Surface, TextStyle, Wake, Weight,
};
use std::fmt::Write;
use std::sync::OnceLock;
use tiny_skia::{LineCap, Path, Stroke, StrokeDash};

/// Card height.
pub const CARD_H: f32 = 80.0;
const GAP: f32 = 8.0;
/// Empty state height.
pub const EMPTY_H: f32 = 72.0;
/// Text column left edge inside a card (`14 + 48 + 14`).
const TEXT_X: f32 = 76.0;
/// Right group: undo 30 + 6 + check 38.
const ACTIONS_W: f32 = 74.0;
/// `Theme.green` flash opacity before completing.
const FLASH_TO: f32 = 0.35;
/// Past this the status turns into "até punk descansa".
const LONG_RUN_MS: i64 = 50 * 60_000;
/// Ratio ascent / line height of Inter, used for baseline alignment of the status text.
const ASCENT_RATIO: f32 = 0.8;

fn title_style() -> TextStyle {
    TextStyle::new(14.0, Weight::Medium, theme::TEXT)
}
fn timer_style(c: Color) -> TextStyle {
    TextStyle::new(23.0, Weight::DemiBold, c).tabular()
}
fn status_style(c: Color) -> TextStyle {
    TextStyle::new(11.0, Weight::Regular, c)
}
fn empty_sub_style() -> TextStyle {
    TextStyle::new(12.0, Weight::Regular, theme::TEXT2)
}
fn start_style() -> TextStyle {
    TextStyle::new(12.0, Weight::DemiBold, Color::WHITE)
}

/// What a card shows from the store (kept for the remove transition).
#[derive(Clone, Debug, PartialEq)]
struct CardData {
    title: String,
    acc_ms: i64,
    started_at: i64,
    running: bool,
}

/// Visual state last painted, to damage a card only on a visible change.
#[derive(Clone, Copy, PartialEq)]
struct Sig {
    bg: Color,
    border: Color,
    grad: f32,
    ring_bg: Color,
    timer_col: Color,
    undo_op: f32,
    flash: f32,
}

struct CardUi {
    tid: String,
    live: bool,
    rect: Rect,
    hover: Hover,
    ring: MouseArea,
    undo: IconButton,
    check: IconButton,
    bg: ColorAnim,
    border: ColorAnim,
    grad: Anim,
    ring_bg: ColorAnim,
    timer_col: ColorAnim,
    undo_op: Anim,
    flash: Anim,
    completing: bool,
    pending_complete: bool,
    // derived, refreshed on a model change or a clock tick
    running: bool,
    elapsed: i64,
    timer: String,
    timer_w: f32,
    status: &'static str,
    long_run: bool,
    title_src: String,
    title_el: String,
    title_w: f32,
    tick_at: Option<Ms>,
    shown: Sig,
}

impl CardUi {
    fn new(tid: &str, accent: Color, accent_text: Color, running: bool) -> CardUi {
        let border = if running { accent.with_alpha(0.38) } else { theme::STROKE };
        let green = theme::GREEN;
        CardUi {
            tid: tid.to_string(),
            live: true,
            rect: Rect::default(),
            hover: Hover::new(),
            ring: MouseArea::new(),
            undo: IconButton::new(Icon::Undo, 30.0, 15.0),
            check: IconButton::new(Icon::Check, 38.0, 19.0).colors(
                Color::WHITE,
                Color::WHITE,
                green,
                green.darker(1.12),
            ),
            bg: ColorAnim::new(theme::CARD),
            border: ColorAnim::new(border),
            grad: Anim::new(running as u8 as f32),
            ring_bg: ColorAnim::new(Color::TRANSPARENT),
            timer_col: ColorAnim::new(if running { accent_text } else { theme::TEXT2 }),
            undo_op: Anim::new(0.0),
            flash: Anim::new(0.0),
            completing: false,
            pending_complete: false,
            running,
            elapsed: 0,
            timer: String::with_capacity(8),
            timer_w: 0.0,
            status: "no intervalo",
            long_run: false,
            title_src: String::new(),
            title_el: String::new(),
            title_w: -1.0,
            tick_at: None,
            shown: Sig {
                bg: theme::CARD,
                border,
                grad: 0.0,
                ring_bg: Color::TRANSPARENT,
                timer_col: if running { accent_text } else { theme::TEXT2 },
                undo_op: 0.0,
                flash: 0.0,
            },
        }
    }

    fn sig(&self, now: Ms) -> Sig {
        Sig {
            bg: self.bg.value(now),
            border: self.border.value(now),
            grad: self.grad.value(now),
            ring_bg: self.ring_bg.value(now),
            timer_col: self.timer_col.value(now),
            undo_op: self.undo_op.value(now),
            flash: self.flash.value(now),
        }
    }

    fn animating(&self, now: Ms) -> bool {
        self.bg.is_animating(now)
            || self.border.is_animating(now)
            || self.grad.is_animating(now)
            || self.ring_bg.is_animating(now)
            || self.timer_col.is_animating(now)
            || self.undo_op.is_animating(now)
            || self.flash.is_animating(now)
    }

    fn ring_rect(&self) -> Rect {
        ring_rect(self.rect)
    }

    /// The row holding the clock and the status text.
    fn clock_row(&self, lh: &Lh, w: f32) -> Rect {
        let top = self.rect.y + (CARD_H - lh.col()) * 0.5 + lh.title + 1.0;
        Rect::new(self.rect.x + TEXT_X, top, w, lh.timer)
    }
}

fn ring_rect(card: Rect) -> Rect {
    Rect::new(card.x + 14.0, card.y + 16.0, 48.0, 48.0)
}

/// Cached line heights.
#[derive(Clone, Copy, Default)]
struct Lh {
    title: f32,
    timer: f32,
    status: f32,
    sub: f32,
    start: f32,
}

impl Lh {
    /// Height of the title + clock column.
    fn col(&self) -> f32 {
        self.title + 1.0 + self.timer
    }
}

fn track_path() -> Option<&'static Path> {
    static P: OnceLock<Option<Path>> = OnceLock::new();
    P.get_or_init(|| path_circle(24.0, 24.0, 21.0)).as_ref()
}

fn dash_stroke() -> &'static Stroke {
    static S: OnceLock<Stroke> = OnceLock::new();
    S.get_or_init(|| {
        // Qt's dashPattern is in units of the stroke width
        let mut st = Stroke { width: 1.2, ..Stroke::default() };
        st.dash = StrokeDash::new(vec![4.0 * 1.2, 4.0 * 1.2], 0.0);
        st
    })
}

fn write_clock(buf: &mut String, ms: i64) {
    buf.clear();
    let s = ms.max(0) / 1000;
    let (h, m, sec) = (s / 3600, (s % 3600) / 60, s % 60);
    let _ = if h > 0 { write!(buf, "{h}:{m:02}:{sec:02}") } else { write!(buf, "{m:02}:{sec:02}") };
}

fn primed_list() -> KeyedList<CardData> {
    // the first `sync` is instant: an empty sync primes the list so the next card gets its `add`
    // transition (a model that already has cards at load time shows them at once, like QML)
    let mut l = KeyedList::new(ListSpec::focus_list());
    l.sync(std::iter::empty(), 0.0, None);
    l
}

pub struct FocusSection {
    frame: Rect,
    list: KeyedList<CardData>,
    cards: Vec<CardUi>,
    lh: Lh,
    visible: bool,
    inited: bool,
    last_rev: u64,
    last_env_rev: u64,
    n: usize,
    n_queue: usize,
    last_bounds: Rect,
    // empty state
    pet: Box<dyn PetLike>,
    pet_state: Option<(PetKind, Color)>,
    pet_active: bool,
    start_ma: MouseArea,
    start_sc: Anim,
    start_shown: (f32, bool),
    start_w: f32,
    sub_src: bool,
    sub_el: String,
    sub_w: f32,
}

impl Default for FocusSection {
    fn default() -> Self {
        Self::new()
    }
}

impl FocusSection {
    pub fn new() -> Self {
        FocusSection {
            frame: Rect::default(),
            list: KeyedList::new(ListSpec::focus_list()),
            cards: Vec::new(),
            lh: Lh::default(),
            visible: false,
            inited: false,
            last_rev: 0,
            last_env_rev: u64::MAX,
            n: 0,
            n_queue: 0,
            last_bounds: Rect::default(),
            pet: new_pet(),
            pet_state: None,
            pet_active: false,
            start_ma: MouseArea::new(),
            start_sc: Anim::new(1.0),
            start_shown: (1.0, false),
            start_w: 0.0,
            sub_src: false,
            sub_el: String::new(),
            sub_w: -1.0,
        }
    }

    /// The section is on screen (the expanded view is visible and not showing settings): pets and
    /// clocks only run while true. Becoming visible refreshes everything.
    pub fn set_visible(&mut self, v: bool, now: Ms) {
        if v == self.visible {
            return;
        }
        self.visible = v;
        if v {
            // timers may have moved while hidden
            self.inited = false;
        } else {
            self.pet.set_active(false, now);
            self.pet_active = false;
        }
    }

    /// Number of cards in the model.
    pub fn len(&self) -> usize {
        self.n
    }
    pub fn is_empty(&self) -> bool {
        self.n == 0
    }

    /// Card rect for the tests / hit-tests (live cards, in model order).
    pub fn card_rects(&self, now: Ms) -> Vec<Rect> {
        self.list
            .iter(now)
            .filter(|(e, _)| e.index.is_some())
            .filter_map(|(e, _)| self.cards.iter().find(|c| c.tid == e.key).map(|c| c.rect))
            .collect()
    }

    fn list_h(n: usize) -> f32 {
        if n == 0 {
            0.0
        } else {
            n as f32 * CARD_H + (n - 1) as f32 * GAP
        }
    }

    fn text_w(&self) -> f32 {
        (self.frame.w - TEXT_X - ACTIONS_W - 14.0 - 10.0).max(0.0)
    }

    fn list_bounds(&self, now: Ms) -> Rect {
        self.list
            .bounds(now, self.frame.x, self.frame.w, |_| CARD_H)
            .translate(0.0, self.frame.y)
    }

    fn start_rect(&self) -> Rect {
        Rect::new(self.frame.right() - 14.0 - self.start_w, self.frame.y + (EMPTY_H - 32.0) * 0.5, self.start_w, 32.0)
    }

    fn pet_rect(&self) -> Rect {
        Rect::new(self.frame.x + 16.0, self.frame.y + (EMPTY_H - 38.0) * 0.5, 38.0, 38.0)
    }

    fn start_visible(&self) -> bool {
        self.n_queue > 0
    }

    /// (Re)reads a card's derived fields from the task.
    fn derive(c: &mut CardUi, t: &foco_base::Task, cx: &mut Cx, text_w: f32) {
        let elapsed = t.elapsed(cx.epoch);
        c.running = t.running;
        c.elapsed = elapsed;
        write_clock(&mut c.timer, elapsed);
        c.timer_w = cx.text.measure(&c.timer, &timer_style(theme::TEXT));
        c.long_run = elapsed > LONG_RUN_MS;
        c.status = if !c.running {
            "no intervalo"
        } else if c.long_run {
            "até punk descansa"
        } else {
            "tocando"
        };
        c.tick_at = model::next_tick(t, cx.epoch, cx.now, 1000.0);
        if c.title_src != t.title || c.title_w != text_w {
            c.title_src.clear();
            c.title_src.push_str(&t.title);
            c.title_el = cx.text.elide(&t.title, &title_style(), text_w);
            c.title_w = text_w;
        }
    }

    fn sync_model(&mut self, cx: &mut Cx) {
        let now = cx.now;
        let st = cx.store.state();
        let n = st.focus.len();
        self.n_queue = st.queue.len();
        let text_w = self.text_w();
        if n == 0 {
            {
                self.list = primed_list();
                self.cards.clear();
                cx.damage.add(self.last_bounds);
                cx.damage.add(self.frame);
            }
            self.n = 0;
        } else {
            let changed = self.list.sync(
                st.focus.iter().enumerate().map(|(i, t)| {
                    (
                        t.tid.as_str(),
                        CardData {
                            title: t.title.clone(),
                            acc_ms: t.acc_ms,
                            started_at: t.started_at,
                            running: t.running,
                        },
                        i as f32 * (CARD_H + GAP),
                    )
                }),
                now,
                None,
            );
            for c in &mut self.cards {
                c.live = false;
            }
            for t in &st.focus {
                match self.cards.iter_mut().find(|c| c.tid == t.tid) {
                    Some(c) => c.live = true,
                    None => self
                        .cards
                        .push(CardUi::new(&t.tid, cx.env.accent, cx.env.accent_text(), t.running)),
                }
            }
            for t in &st.focus {
                if let Some(c) = self.cards.iter_mut().find(|c| c.tid == t.tid) {
                    Self::derive(c, t, cx, text_w);
                }
            }
            self.n = n;
            if changed || !self.inited {
                cx.damage.add(self.last_bounds);
                self.last_bounds = self.list_bounds(now);
                cx.damage.add(self.last_bounds);
            }
        }
    }
}

impl View for FocusSection {
    fn update(&mut self, cx: &mut Cx) {
        let now = cx.now;
        if self.lh.title == 0.0 {
            self.lh = Lh {
                title: cx.text.line_height(&title_style()),
                timer: cx.text.line_height(&timer_style(theme::TEXT)),
                status: cx.text.line_height(&status_style(theme::TEXT3)),
                sub: cx.text.line_height(&empty_sub_style()),
                start: cx.text.line_height(&start_style()),
            };
        }
        let env_changed = cx.env.rev != self.last_env_rev;
        let ticked = self.visible && self.cards.iter().any(|c| c.live && c.tick_at.is_some_and(|t| now >= t));
        if !self.inited || cx.rev != self.last_rev || env_changed || ticked {
            let model_changed = !self.inited || cx.rev != self.last_rev;
            if model_changed {
                self.sync_model(cx);
            } else if ticked {
                let text_w = self.text_w();
                let st = cx.store.state();
                for t in &st.focus {
                    let lh = self.lh;
                    if let Some(c) = self.cards.iter_mut().find(|c| c.tid == t.tid && c.live) {
                        if c.tick_at.is_some_and(|tt| now >= tt) {
                            Self::derive(c, t, cx, text_w);
                            // damage: the clock row (clock + status) and the ring
                            cx.damage.add(c.clock_row(&lh, text_w));
                            cx.damage.add(c.ring_rect().inflate(2.0, 2.0));
                        }
                    }
                }
            }
            if env_changed {
                self.last_env_rev = cx.env.rev;
                cx.damage.add(self.frame);
            }
            self.last_rev = cx.rev;
            self.inited = true;
        }
        if self.list.prune(now) {
            let list = &self.list;
            self.cards.retain(|c| c.live || list.iter(now).any(|(e, _)| e.key == c.tid));
            cx.damage.add(self.last_bounds);
            self.last_bounds = self.list_bounds(now);
        }
        // cards: targets, completion, damage
        let accent = cx.env.accent;
        let accent_text = cx.env.accent_text();
        let lh = self.lh;
        let text_w = self.text_w();
        for c in &mut self.cards {
            if !c.live {
                continue;
            }
            c.bg.animate_to(if c.hover.get() { theme::CARD_HOVER } else { theme::CARD }, 140.0, now);
            c.border
                .animate_to(if c.running { accent.with_alpha(0.38) } else { theme::STROKE }, 240.0, now);
            c.grad.animate_to(c.running as u8 as f32, 300.0, Easing::Linear, now);
            c.ring_bg
                .animate_to(if c.ring.hovered() { theme::FILL } else { Color::TRANSPARENT }, 140.0, now);
            c.timer_col
                .animate_to(if c.running { accent_text } else { theme::TEXT2 }, 200.0, now);
            c.undo_op
                .animate_to(if c.hover.get() { 1.0 } else { 0.0 }, 160.0, Easing::Linear, now);
            c.undo.tick(cx);
            c.check.tick(cx);
            let sig = c.sig(now);
            if sig != c.shown {
                // the clock text colour only touches the clock row; the rest the whole card
                let only_timer = Sig { timer_col: c.shown.timer_col, ..sig } == c.shown;
                if only_timer {
                    cx.damage.add(c.clock_row(&lh, text_w));
                } else {
                    cx.damage.add(c.rect);
                }
                c.shown = sig;
            }
            if c.pending_complete && !c.flash.is_animating(now) {
                c.pending_complete = false;
                if let Some(i) = cx.store.state().focus.iter().position(|t| t.tid == c.tid) {
                    cx.msgs.push(Msg::Act(Action::Complete(i)));
                }
            }
        }
        // list transitions: damage the union of the list bounds before and after
        if self.list.is_animating(now) {
            let b = self.list_bounds(now);
            cx.damage.add(self.last_bounds);
            cx.damage.add(b);
            self.last_bounds = b;
        }

        // empty state
        let empty = self.n == 0;
        let active = self.visible && empty;
        if active != self.pet_active {
            self.pet_active = active;
            self.pet.set_active(active, now);
            self.pet.set_size(38.0);
            cx.damage.add(self.pet_rect().inflate(self.pet.overdraw(), self.pet.overdraw()));
        }
        if empty {
            let ps = (cx.store.state().settings.kind(), accent);
            if self.pet_state != Some(ps) {
                if self.pet_state.map(|p| p.0) != Some(ps.0) {
                    self.pet.set_kind(ps.0, now);
                }
                self.pet.set_mood(foco_base::Mood::Sleep, now);
                self.pet.set_accent(ps.1);
                self.pet_state = Some(ps);
                cx.damage.add(self.pet_rect().inflate(self.pet.overdraw(), self.pet.overdraw()));
            }
            if active && self.pet.update(now) {
                cx.damage.add(self.pet_rect().inflate(self.pet.overdraw(), self.pet.overdraw()));
            }
            // subtitle + button width
            let sub_src = self.n_queue > 0;
            if sub_src != self.sub_src || self.sub_w < 0.0 || env_changed {
                self.sub_src = sub_src;
                self.start_w = 12.0 + 6.0 + cx.text.measure("Tocar a próxima", &start_style()) + 24.0;
                self.sub_w = -1.0;
            }
            let avail = (self.frame.w - 66.0 - 12.0 - if sub_src { self.start_w + 14.0 } else { 0.0 }).max(0.0);
            if self.sub_w != avail {
                let s = if sub_src {
                    "Puxa a próxima da fila ou inventa uma nova."
                } else {
                    "Escreve aí embaixo. Faça você mesmo."
                };
                self.sub_el = cx.text.elide(s, &empty_sub_style(), avail);
                self.sub_w = avail;
                cx.damage.add(self.frame);
            }
            let hov = self.start_ma.hovered();
            self.start_sc.animate_to(
                if self.start_ma.pressed_inside() { 0.94 } else { 1.0 },
                120.0,
                Easing::Linear,
                now,
            );
            let shown = (self.start_sc.value(now), hov);
            if shown != self.start_shown {
                self.start_shown = shown;
                cx.damage.add(self.start_rect().inflate(2.0, 2.0));
            }
        }
    }

    fn layout(&mut self, cx: &mut Cx, frame: Rect) {
        let frame = Rect::new(frame.x, frame.y, frame.w, self.height());
        if frame != self.frame {
            cx.damage.add_moved(self.frame, frame);
            self.frame = frame;
            self.last_bounds = self.list_bounds(cx.now);
        }
        let now = cx.now;
        if self.n == 0 {
            self.start_ma.rect = if self.start_visible() { self.start_rect() } else { Rect::default() };
            return;
        }
        let Self { list, cards, frame, .. } = self;
        for (e, fx) in list.iter(now) {
            let Some(c) = cards.iter_mut().find(|c| c.tid == e.key) else { continue };
            let r = Rect::new(frame.x + fx.dx, frame.y + fx.y, frame.w, CARD_H);
            c.rect = r;
            c.ring.rect = ring_rect(r);
            let ar = r.right() - 14.0;
            c.undo.layout(cx, Rect::new(ar - ACTIONS_W, r.y + 25.0, 30.0, 30.0));
            c.check.layout(cx, Rect::new(ar - 38.0, r.y + 21.0, 38.0, 38.0));
        }
    }

    fn event(&mut self, cx: &mut Cx, ev: &Event) -> Handled {
        let mut out = Handled::No;
        let now = cx.now;
        match ev {
            Event::PointerMove { .. } | Event::PointerLeave | Event::PointerDown { .. } | Event::PointerUp { .. } => {}
            _ => return Handled::No,
        }
        if self.n == 0 {
            if self.start_visible() {
                let o = self.start_ma.event(ev);
                if o.changed {
                    cx.damage.add(self.start_rect().inflate(2.0, 2.0));
                    self.start_sc.animate_to(
                        if self.start_ma.pressed_inside() { 0.94 } else { 1.0 },
                        120.0,
                        Easing::Linear,
                        now,
                    );
                }
                if self.start_ma.hovered() {
                    cx.cursor = CursorShape::Hand;
                }
                if o.clicked {
                    cx.msgs.push(Msg::Act(Action::FocusFromQueue { index: 0, silent: false }));
                }
                out = merge(out, o.handled);
            }
            return out;
        }
        for i in 0..self.cards.len() {
            if !self.cards[i].live {
                continue;
            }
            let (clicked_ring, clicked_undo, clicked_check);
            {
                let c = &mut self.cards[i];
                if c.hover.update(c.rect, ev) {
                    cx.damage.add(c.rect);
                }
                let o = c.ring.event(ev);
                if o.changed {
                    cx.damage.add(c.ring_rect());
                }
                if c.ring.hovered() {
                    cx.cursor = CursorShape::Hand;
                }
                out = merge(out, o.handled);
                clicked_ring = o.clicked;
                let u = c.undo.pointer(cx, ev);
                out = merge(out, u.handled);
                clicked_undo = u.clicked;
                let k = c.check.pointer(cx, ev);
                out = merge(out, k.handled);
                clicked_check = k.clicked;
            }
            let tid = self.cards[i].tid.clone();
            let idx = || cx.store.state().focus.iter().position(|t| t.tid == tid);
            if clicked_ring {
                if let Some(ix) = idx() {
                    cx.msgs.push(Msg::Act(Action::TogglePause(ix)));
                }
            }
            if clicked_undo {
                if let Some(ix) = idx() {
                    cx.msgs.push(Msg::Act(Action::Unfocus(ix)));
                }
            }
            if clicked_check {
                let c = &mut self.cards[i];
                if !c.completing {
                    c.completing = true;
                    c.pending_complete = true;
                    c.flash.animate_to(FLASH_TO, 130.0, Easing::Linear, now);
                }
            }
        }
        out
    }

    fn paint(&mut self, s: &mut dyn Surface, pc: &PaintCx) {
        let now = pc.now;
        if self.n == 0 {
            if self.frame.intersects(&pc.clip) {
                self.paint_empty(s, pc);
            }
            return;
        }
        for (e, fx) in self.list.iter(now) {
            let Some(c) = self.cards.iter().find(|c| c.tid == e.key) else { continue };
            let r = Rect::new(self.frame.x + fx.dx, self.frame.y + fx.y, self.frame.w, CARD_H);
            if !r.inflate(r.w * 0.06, r.h * 0.06).intersects(&pc.clip) || fx.opacity <= 0.0 {
                continue;
            }
            s.save();
            s.mul_opacity(fx.opacity);
            if fx.scale != 1.0 {
                let ctr = r.center();
                s.translate(ctr.x, ctr.y);
                s.scale(fx.scale, fx.scale);
                s.translate(-ctr.x, -ctr.y);
            }
            paint_card(s, pc, c, r, &self.lh);
            s.restore();
        }
    }

    fn next_wake(&self, now: Ms) -> Wake {
        if !self.visible {
            return Wake::Idle;
        }
        let mut w = Wake::Idle;
        if self.list.is_animating(now) {
            return Wake::Frame;
        }
        if self.n == 0 {
            w = w.merge_at(self.pet.next_wake(now), now);
            if self.start_sc.is_animating(now) {
                return Wake::Frame;
            }
            return w;
        }
        for c in &self.cards {
            if !c.live {
                continue;
            }
            if c.animating(now) || c.undo.next_wake(now) != Wake::Idle || c.check.next_wake(now) != Wake::Idle {
                return Wake::Frame;
            }
            if let Some(t) = c.tick_at {
                w = w.merge_at(Wake::at(now, t), now);
            }
        }
        w
    }

    fn bounds(&self) -> Rect {
        if self.n == 0 {
            self.frame
                .union(&self.pet_rect().inflate(self.pet.overdraw(), self.pet.overdraw()))
        } else {
            self.list_bounds_now()
        }
    }
}

impl FocusSection {
    fn list_bounds_now(&self) -> Rect {
        // cards may sit 80 px to the right while they leave and scale up to 1.06
        let b = self.last_bounds.union(&self.frame);
        b.inflate(4.0, 4.0)
    }

    fn paint_empty(&self, s: &mut dyn Surface, pc: &PaintCx) {
        let f = self.frame;
        let accent = pc.env.accent;
        // dashed outline (`PathRectangle` 0.5,0.5, w-1 x 71, r 18)
        if let Some(p) = path_rrect(Rect::new(f.x + 0.5, f.y + 0.5, f.w - 1.0, EMPTY_H - 1.0), 18.0) {
            s.stroke_path(&p, &Fill::Solid(theme::FILL2), dash_stroke());
        }
        let pr = self.pet_rect();
        self.pet.paint(s, pr, pc.now);
        // text column
        let tx = f.x + 16.0 + 38.0 + 12.0;
        let col_h = self.lh.title + 2.0 + self.lh.sub;
        let top = f.y + (EMPTY_H - col_h) * 0.5;
        s.text("Palco vazio", tx, top, &title_style());
        s.text(&self.sub_el, tx, top + self.lh.title + 2.0, &empty_sub_style());
        if self.start_visible() {
            let r = self.start_rect();
            s.save();
            let sc = self.start_sc.value(pc.now);
            if sc != 1.0 {
                let c = r.center();
                s.translate(c.x, c.y);
                s.scale(sc, sc);
                s.translate(-c.x, -c.y);
            }
            let col = if self.start_ma.hovered() { accent.darker(1.1) } else { accent };
            s.fill_rrect(r, 16.0, &Fill::Solid(col));
            let x0 = r.x + 12.0;
            paint_icon(s, Icon::Play, Point::new(x0 + 6.0, r.y + 16.0), 12.0, Color::WHITE, 1.8);
            let cy = r.y + 16.0;
            let st = start_style();
            s.text("Tocar a próxima", x0 + 18.0, cy - self.lh.start * 0.5, &st);
            s.restore();
        }
    }
}

fn paint_card(s: &mut dyn Surface, pc: &PaintCx, c: &CardUi, r: Rect, lh: &Lh) {
    let now = pc.now;
    let accent = pc.env.accent;
    // body + border, then the running gradient and the completion flash on top (children of the
    // QML Rectangle are drawn over its border)
    s.fill_rrect(r, 18.0, &Fill::Solid(c.bg.value(now)));
    s.stroke_rrect(r.inflate(-0.5, -0.5), 17.5, &Fill::Solid(c.border.value(now)), 1.0);
    let g = c.grad.value(now);
    if g > 0.003 {
        s.save();
        s.mul_opacity(g);
        let fill = Fill::Linear {
            p0: Point::new(r.x, r.y),
            p1: Point::new(r.right(), r.y),
            stops: vec![(0.0, accent.with_alpha(0.18)), (0.55, accent.with_alpha(0.0))],
        };
        s.fill_rrect(r, 18.0, &fill);
        s.restore();
    }
    let fl = c.flash.value(now);
    if fl > 0.003 {
        s.fill_rrect(r, 18.0, &Fill::Solid(theme::GREEN.with_alpha(fl)));
    }

    // ring: track, minute arc, hover disc, play/pause
    let ring = ring_rect(r);
    s.save();
    s.translate(ring.x, ring.y);
    if let Some(p) = track_path() {
        let st = Stroke { width: 3.5, ..Stroke::default() };
        s.stroke_path(p, &Fill::Solid(theme::FILL), &st);
    }
    let frac = (c.elapsed % 60_000) as f32 / 60_000.0;
    let sweep = (360.0 * frac).max(2.0);
    if let Some(p) = path_arc(24.0, 24.0, 21.0, -90.0, sweep) {
        let st = Stroke { width: 3.5, line_cap: LineCap::Round, ..Stroke::default() };
        let col = if c.running { accent } else { theme::TEXT3 };
        s.stroke_path(&p, &Fill::Solid(col), &st);
    }
    let bg = c.ring_bg.value(now);
    if bg.a > 0.0 {
        s.fill_rrect(Rect::new(7.0, 7.0, 34.0, 34.0), 17.0, &Fill::Solid(bg));
    }
    s.restore();
    let icon = if c.running { Icon::Pause } else { Icon::Play };
    let icol = if c.ring.hovered() { theme::TEXT } else { theme::TEXT2 };
    paint_icon(s, icon, ring.center(), 16.0, icol, 2.2);

    // text column
    let tx = r.x + TEXT_X;
    let col_h = lh.col();
    let top = r.y + (CARD_H - col_h) * 0.5;
    s.text(&c.title_el, tx, top, &title_style());
    let row_top = top + lh.title + 1.0;
    let tcol = c.timer_col.value(now);
    s.text(&c.timer, tx, row_top, &timer_style(tcol));
    let scol = if c.running && c.long_run { theme::ORANGE } else { theme::TEXT3 };
    // baseline aligned: status baseline = clock baseline
    let sy = row_top + (lh.timer - lh.status) * ASCENT_RATIO;
    s.text(c.status, tx + c.timer_w + 8.0, sy, &status_style(scol));

    // actions
    let op = c.undo_op.value(now);
    if op > 0.003 {
        s.save();
        s.mul_opacity(op);
        c.undo.paint(s, now);
        s.restore();
    }
    c.check.paint(s, now);
}

impl Section for FocusSection {
    fn height(&self) -> f32 {
        if self.n == 0 {
            EMPTY_H
        } else {
            Self::list_h(self.n)
        }
    }
}

fn merge(a: Handled, b: Handled) -> Handled {
    match (a, b) {
        (Handled::Grab, _) | (_, Handled::Grab) => Handled::Grab,
        (Handled::Yes, _) | (_, Handled::Yes) => Handled::Yes,
        _ => Handled::No,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cx::{FocusId, S};
    use crate::testkit::Harness;
    use foco_base::{Button, Task};

    const FRAME: Rect = Rect { x: 32.0, y: 100.0, w: 484.0, h: 0.0 };

    fn task(h: &Harness, tid: &str, running: bool, elapsed_ms: i64) -> Task {
        // elapsed at `now == 0`
        Task {
            tid: tid.into(),
            title: format!("Tarefa {tid}"),
            acc_ms: if running { 0 } else { elapsed_ms },
            started_at: if running { h.epoch as i64 - elapsed_ms } else { 0 },
            running,
            ..Task::default()
        }
    }

    fn step(h: &mut Harness, v: &mut FocusSection, now: Ms) {
        h.with_cx(now, |cx| {
            v.update(cx);
            v.layout(cx, FRAME);
        });
    }

    fn visible_section() -> FocusSection {
        let mut v = FocusSection::new();
        v.set_visible(true, 0.0);
        v
    }

    fn mv(x: f32, y: f32) -> Event {
        Event::PointerMove { pos: Point::new(x, y) }
    }
    fn down(x: f32, y: f32) -> Event {
        Event::PointerDown { pos: Point::new(x, y), button: Button::Left, clicks: 1 }
    }
    fn up(x: f32, y: f32) -> Event {
        Event::PointerUp { pos: Point::new(x, y), button: Button::Left }
    }
    fn click(h: &mut Harness, v: &mut FocusSection, now: Ms, x: f32, y: f32) {
        h.with_cx(now, |cx| {
            v.event(cx, &mv(x, y));
            v.event(cx, &down(x, y));
            v.event(cx, &up(x, y));
        });
    }

    #[test]
    fn heights_follow_the_count() {
        let mut h = Harness::new();
        let mut v = visible_section();
        step(&mut h, &mut v, 0.0);
        assert_eq!(v.height(), EMPTY_H);
        h.store.state.focus.push(task(&h, "a", true, 1000));
        h.rev += 1;
        step(&mut h, &mut v, 10.0);
        assert_eq!(v.height(), 80.0);
        h.store.state.focus.push(task(&h, "b", false, 5000));
        h.rev += 1;
        step(&mut h, &mut v, 20.0);
        assert_eq!(v.height(), 168.0);
        // final height at once even though the removal is still animating
        h.store.state.focus.remove(0);
        h.rev += 1;
        step(&mut h, &mut v, 30.0);
        assert_eq!(v.height(), 80.0);
        assert!(v.bounds().h >= 80.0);
    }

    #[test]
    fn clock_wakes_on_the_second_boundary_and_damages_little() {
        let mut h = Harness::new();
        let mut v = visible_section();
        h.store.state.focus.push(task(&h, "a", true, 1250));
        step(&mut h, &mut v, 0.0);
        // 750 ms to the next whole second (+2 ms slack of model::next_tick)
        assert_eq!(v.next_wake(0.0), Wake::At(752.0));
        assert_eq!(v.cards[0].timer, "00:01");
        // settle initial damage
        h.damage.clear();
        // nothing changes before the boundary
        step(&mut h, &mut v, 300.0);
        assert!(h.damage.is_empty());
        assert_eq!(v.next_wake(300.0), Wake::At(752.0));
        // the tick
        step(&mut h, &mut v, 752.0);
        assert_eq!(v.cards[0].timer, "00:02");
        assert_eq!(v.next_wake(752.0), Wake::At(1752.0));
        // only the clock row and the ring: a small fraction of the card
        let area: i64 = h.damage.rects().iter().map(foco_base::IRect::area).sum();
        let card = (484.0f32 * S * 80.0 * S) as i64;
        assert!(!h.damage.is_empty() && area < card / 2 + 4000, "area {area} card {card}");
        assert!(h.damage.rects().len() <= 2);
    }

    #[test]
    fn paused_and_invisible_sections_do_not_wake() {
        let mut h = Harness::new();
        let mut v = visible_section();
        h.store.state.focus.push(task(&h, "a", false, 61_000));
        step(&mut h, &mut v, 0.0);
        assert_eq!(v.cards[0].timer, "01:01");
        assert_eq!(v.cards[0].status, "no intervalo");
        assert_eq!(v.next_wake(0.0), Wake::Idle);
        h.store.state.focus[0] = task(&h, "a", true, 61_000);
        h.rev += 1;
        step(&mut h, &mut v, 5.0);
        assert!(matches!(v.next_wake(5.0), Wake::At(_)) || v.next_wake(5.0) == Wake::Frame);
        v.set_visible(false, 6.0);
        assert_eq!(v.next_wake(6.0), Wake::Idle);
    }

    #[test]
    fn long_run_shows_the_break_hint() {
        let mut h = Harness::new();
        let mut v = visible_section();
        h.store.state.focus.push(task(&h, "a", true, 50 * 60_000 + 1000));
        step(&mut h, &mut v, 0.0);
        assert_eq!(v.cards[0].status, "até punk descansa");
        assert!(v.cards[0].long_run);
        h.store.state.focus[0] = task(&h, "a", true, 49 * 60_000);
        h.rev += 1;
        step(&mut h, &mut v, 10.0);
        assert_eq!(v.cards[0].status, "tocando");
    }

    #[test]
    fn add_transition_runs_then_idles() {
        let mut h = Harness::new();
        let mut v = visible_section();
        step(&mut h, &mut v, 0.0);
        h.store.state.focus.push(task(&h, "a", false, 0));
        h.rev += 1;
        h.damage.clear();
        step(&mut h, &mut v, 100.0);
        assert!(!h.damage.is_empty());
        assert_eq!(v.next_wake(100.0), Wake::Frame);
        step(&mut h, &mut v, 300.0);
        let (e, fx) = v.list.iter(300.0).next().map(|(e, fx)| (e.key.clone(), fx)).unwrap();
        assert_eq!(e, "a");
        assert!(fx.opacity > 0.0 && fx.opacity < 1.0 && fx.scale > 0.9);
        // 460 ms OutBack is the longest track
        step(&mut h, &mut v, 100.0 + 470.0);
        assert_eq!(v.next_wake(570.0), Wake::Idle);
        let (_, fx) = v.list.iter(570.0).next().unwrap();
        assert_eq!((fx.opacity, fx.scale, fx.dx), (1.0, 1.0, 0.0));
    }

    #[test]
    fn check_flashes_then_completes_by_tid() {
        let mut h = Harness::new();
        let mut v = visible_section();
        h.store.state.focus.push(task(&h, "a", true, 1000));
        h.store.state.focus.push(task(&h, "b", true, 2000));
        step(&mut h, &mut v, 0.0);
        let r = v.cards[1].rect;
        // check button of card "b" (38 px, 14 px from the right edge)
        let (x, y) = (r.right() - 14.0 - 19.0, r.y + 40.0);
        click(&mut h, &mut v, 1000.0, x, y);
        assert!(h.take_msgs().is_empty(), "nothing before the flash ends");
        step(&mut h, &mut v, 1065.0);
        let fl = v.cards.iter().find(|c| c.tid == "b").unwrap().flash.value(1065.0);
        assert!(fl > 0.1 && fl < FLASH_TO, "{fl}");
        assert!(h.take_msgs().is_empty());
        // the first card is completed elsewhere meanwhile: index must be resolved at the end
        h.store.state.focus.remove(0);
        h.rev += 1;
        step(&mut h, &mut v, 1131.0);
        let b = v.cards.iter().find(|c| c.tid == "b").unwrap();
        assert_eq!(b.flash.value(1131.0), FLASH_TO);
        assert_eq!(h.take_msgs(), vec![Msg::Act(Action::Complete(0))]);
        // a second click is ignored
        click(&mut h, &mut v, 1200.0, x, y);
        step(&mut h, &mut v, 1400.0);
        assert!(h.take_msgs().is_empty());
    }

    #[test]
    fn hover_reveals_undo_and_buttons_send_actions() {
        let mut h = Harness::new();
        let mut v = visible_section();
        h.store.state.focus.push(task(&h, "a", true, 1000));
        step(&mut h, &mut v, 0.0);
        assert_eq!(v.cards[0].undo_op.value(0.0), 0.0);
        let r = v.cards[0].rect;
        h.with_cx(10.0, |cx| v.event(cx, &mv(r.x + 150.0, r.y + 40.0)));
        step(&mut h, &mut v, 10.0);
        assert!(v.cards[0].hover.get());
        step(&mut h, &mut v, 170.0);
        assert_eq!(v.cards[0].undo_op.value(170.0), 1.0);
        // undo
        click(&mut h, &mut v, 200.0, r.right() - 14.0 - 74.0 + 15.0, r.y + 40.0);
        assert_eq!(h.take_msgs(), vec![Msg::Act(Action::Unfocus(0))]);
        // ring toggles pause
        click(&mut h, &mut v, 210.0, r.x + 14.0 + 24.0, r.y + 40.0);
        assert_eq!(h.take_msgs(), vec![Msg::Act(Action::TogglePause(0))]);
        // leaving the card hides undo again
        h.with_cx(300.0, |cx| v.event(cx, &Event::PointerLeave));
        step(&mut h, &mut v, 300.0);
        assert_eq!(v.cards[0].undo_op.target(), 0.0);
    }

    #[test]
    fn empty_state_start_button() {
        let mut h = Harness::new();
        let mut v = visible_section();
        step(&mut h, &mut v, 0.0);
        assert!(!v.start_visible());
        // no queue: clicking where the button would be does nothing
        click(&mut h, &mut v, 5.0, FRAME.right() - 40.0, FRAME.y + 36.0);
        assert!(h.take_msgs().is_empty());
        h.store.state.queue.push(task(&h, "q", false, 0));
        h.rev += 1;
        step(&mut h, &mut v, 10.0);
        assert!(v.start_visible());
        assert_eq!(v.height(), 72.0);
        click(&mut h, &mut v, 20.0, FRAME.right() - 40.0, FRAME.y + 36.0);
        assert_eq!(h.take_msgs(), vec![Msg::Act(Action::FocusFromQueue { index: 0, silent: false })]);
        assert!(v.sub_el.starts_with("Puxa a próxima"), "{}", v.sub_el);
    }

    #[test]
    fn empty_state_pet_follows_visibility() {
        let mut h = Harness::new();
        let mut v = FocusSection::new();
        step(&mut h, &mut v, 0.0);
        assert_eq!(v.next_wake(0.0), Wake::Idle);
        v.set_visible(true, 1.0);
        step(&mut h, &mut v, 1.0);
        assert!(v.pet_active);
        v.set_visible(false, 2.0);
        step(&mut h, &mut v, 2.0);
        assert!(!v.pet_active);
        assert_eq!(v.next_wake(2.0), Wake::Idle);
        let _ = FocusId::None;
    }

    fn save(name: &str, s: &foco_base::PixSurface) {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/ui-test/wave3b");
        std::fs::create_dir_all(&dir).unwrap();
        s.save_png(dir.join(name)).unwrap();
    }

    fn draw(h: &Harness, v: &mut FocusSection, now: Ms, name: &str) {
        let mut s = Harness::surface();
        let bg = Fill::Solid(theme::NOTCH);
        s.fill_rrect(Rect::new(0.0, 0.0, 760.0, 760.0), 0.0, &bg);
        h.with_pc(now, Rect::new(0.0, 0.0, 760.0, 760.0), |pc| v.paint(&mut s, pc));
        save(name, &s);
    }

    #[test]
    fn snapshots() {
        let mut h = Harness::new();
        h.store.state.settings.pet_name = "Foquinho".into();
        let mut v = visible_section();
        step(&mut h, &mut v, 0.0);
        draw(&h, &mut v, 0.0, "focus-empty.png");
        h.store.state.queue.push(task(&h, "q", false, 0));
        h.rev += 1;
        step(&mut h, &mut v, 10.0);
        draw(&h, &mut v, 10.0, "focus-empty-queue.png");
        h.store.state.focus.push(task(&h, "a", true, 61_000));
        h.store.state.focus.push(task(&h, "b", false, 5 * 60_000));
        h.rev += 1;
        step(&mut h, &mut v, 20.0);
        // mid add transition
        step(&mut h, &mut v, 180.0);
        draw(&h, &mut v, 180.0, "focus-mid-add.png");
        step(&mut h, &mut v, 700.0);
        draw(&h, &mut v, 700.0, "focus-two.png");
        // hover first card + a long run
        h.store.state.focus[0] = task(&h, "a", true, 51 * 60_000);
        h.rev += 1;
        step(&mut h, &mut v, 710.0);
        let r = v.cards[0].rect;
        h.with_cx(720.0, |cx| v.event(cx, &mv(r.x + 150.0, r.y + 40.0)));
        step(&mut h, &mut v, 1000.0);
        draw(&h, &mut v, 1000.0, "focus-hover-long.png");
        // mid complete flash on card b
        let r = v.cards[1].rect;
        click(&mut h, &mut v, 1100.0, r.right() - 33.0, r.y + 40.0);
        step(&mut h, &mut v, 1170.0);
        draw(&h, &mut v, 1170.0, "focus-mid-flash.png");
        step(&mut h, &mut v, 1240.0);
        // removal mid-way
        h.store.state.focus.remove(1);
        h.rev += 1;
        step(&mut h, &mut v, 1240.0);
        step(&mut h, &mut v, 1340.0);
        draw(&h, &mut v, 1340.0, "focus-mid-remove.png");
    }
}
