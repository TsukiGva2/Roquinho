//! `ExpandedView.qml` + `MainSections.qml` (wave 3b): the open panel. A 70 px header (interactive
//! mascot, name, level pill, status line, XP bar, today pill, three buttons) over a clipped,
//! wheel-scrollable body that holds either the main sections (EM FOCO, input, A SEGUIR, CONCLUIDAS)
//! or the settings panel, crossfading over 200 ms.
//!
//! Heights drive the notch height spring: `implicit_height()` is
//! `min(700, 12 + 70 + 12 + body + 20)` and is refreshed on every `update`, also while the panel
//! is invisible (QML keeps computing it).
//!
//! Wake/damage: nothing animating and nothing running means `Idle`. While open the only recurring
//! deadlines are the running clocks (second boundary, `FocusSection`) and the minute of the
//! today pill. Section damage is whatever the sections report; scrolling damages the viewport.

use crate::cx::{CursorShape, Cx, DamageSink, FocusId, Handled, Msg, PaintCx};
use crate::model;
use crate::pet::{new_pet, PetLike};
use crate::view::{Section, View};
use crate::views::done::DoneSection;
use crate::views::focus_card::FocusSection;
use crate::views::input_field::InputField;
use crate::views::queue::QueueSection;
use crate::views::settings::SettingsPanel;
use crate::widgets::{paint_icon, paint_section_label, Icon, IconButton, Scroll, SECTION_LABEL_H};
use foco_base::model::fmt_short;
use foco_base::{
    pet_event, theme, Anim, AppCmd, Color, Easing, Event, Fill, Mood, Ms, PetKind, Point, Rect, Surface, TextStyle,
    Wake, Weight,
};
use foco_core::store::{QUIPS_PETTED_CAT, QUIPS_PETTED_OTHER};

const MAX_H: f32 = 700.0;
/// Panel width (`Main.qml`: `width: 548`).
pub const WIDTH: f32 = 548.0;
const PAD: f32 = 18.0;
const HEADER_Y: f32 = 12.0;
const HEADER_H: f32 = 70.0;
/// Top of the body inside the panel (`header.y + header.height + 12`).
const BODY_Y: f32 = HEADER_Y + HEADER_H + 12.0;
const BOTTOM_PAD: f32 = 20.0;
const PET_SIZE: f32 = 66.0;
const XP_W: f32 = 110.0;
const COL_SPACING: f32 = 10.0;
const HIDE_DY: f32 = -14.0;

fn name_style() -> TextStyle {
    TextStyle::new(15.0, Weight::DemiBold, theme::TEXT)
}
fn lvl_style(c: Color) -> TextStyle {
    TextStyle::new(10.0, Weight::DemiBold, c)
}
fn line_style() -> TextStyle {
    TextStyle::new(12.0, Weight::Regular, theme::TEXT2)
}
fn stat_style() -> TextStyle {
    TextStyle::new(11.0, Weight::Medium, theme::TEXT2).tabular()
}

/// Positions of the main column children, relative to the body top (all `Column` rules applied:
/// invisible children take no height and no spacing).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct MainLayout {
    label_focus: f32,
    focus: f32,
    input: f32,
    label_queue: Option<f32>,
    queue: f32,
    done: Option<f32>,
    total: f32,
}

impl MainLayout {
    fn compute(h_focus: f32, h_input: f32, queue_visible: bool, h_queue: f32, h_done: f32) -> MainLayout {
        let mut y = 0.0f32;
        let mut first = true;
        let mut place = |h: f32, y: &mut f32| -> f32 {
            if !first {
                *y += COL_SPACING;
            }
            first = false;
            let at = *y;
            *y += h;
            at
        };
        let label_focus = place(SECTION_LABEL_H, &mut y);
        let focus = place(h_focus, &mut y);
        let input = place(h_input, &mut y);
        let (label_queue, queue) = if queue_visible {
            place(2.0, &mut y); // the 2 px spacer item
            let l = place(SECTION_LABEL_H, &mut y);
            let q = place(h_queue, &mut y);
            (Some(l), q)
        } else {
            (None, 0.0)
        };
        let done = if h_done > 0.0 { Some(place(h_done, &mut y)) } else { None };
        MainLayout { label_focus, focus, input, label_queue, queue, done, total: y }
    }
}

/// Header geometry derived in `layout`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct HeaderLayout {
    pet: Rect,
    name: Point,
    pill: Rect,
    line_y: f32,
    xp: Rect,
    stat: Rect,
    /// Whole text column box, for damage.
    col: Rect,
    sett: Rect,
    hide: Rect,
    chevron: Rect,
}

#[derive(Clone, Copy, Default)]
struct Lh {
    name: f32,
    line: f32,
}

pub struct ExpandedView {
    shown: bool,
    settings_open: bool,
    op: Anim,
    ty: Anim,
    main_op: Anim,
    shown_fx: (f32, f32),
    shown_group: (f32, f32),
    was_visible: bool,
    frame: Rect,
    // header
    pet: Box<dyn PetLike>,
    pet_state: Option<(PetKind, Mood, Color)>,
    pet_down: bool,
    pet_hover: bool,
    btn_settings: IconButton,
    btn_hide: IconButton,
    btn_collapse: IconButton,
    lh: Lh,
    hl: HeaderLayout,
    name: String,
    name_w: f32,
    lvl: String,
    lvl_w: f32,
    line_key: Option<(Mood, usize, usize, Option<String>)>,
    /// Inputs of the last update pass done while invisible (see `View::update`).
    hidden_key: Option<(u64, u64, bool, bool, Mood, bool)>,
    line: String,
    xp: Anim,
    xp_shown: f32,
    stat: String,
    stat_w: f32,
    today_at: Option<Ms>,
    // body
    scroll: Scroll,
    scroll_shown: f32,
    focus: FocusSection,
    input: InputField,
    queue: Box<dyn Section>,
    done: Box<dyn Section>,
    settings: SettingsPanel,
    n_focus: usize,
    n_queue: usize,
    count_shown: (usize, usize),
    ml: MainLayout,
    implicit_h: f32,
    grab_sections: bool,
    scratch: DamageSink,
    rng: u64,
    inited: bool,
    last_rev: u64,
    last_env_rev: u64,
}

impl Default for ExpandedView {
    fn default() -> Self {
        Self::new()
    }
}

impl ExpandedView {
    pub fn new() -> Self {
        Self::with_sections(Box::new(QueueSection::new()), Box::new(DoneSection::new()))
    }

    /// Same as `new` with custom queue / done sections (tests use stubs).
    pub fn with_sections(queue: Box<dyn Section>, done: Box<dyn Section>) -> Self {
        ExpandedView {
            shown: false,
            settings_open: false,
            op: Anim::new(0.0),
            ty: Anim::new(HIDE_DY),
            main_op: Anim::new(1.0),
            shown_fx: (0.0, HIDE_DY),
            shown_group: (1.0, 0.0),
            was_visible: false,
            frame: Rect::default(),
            pet: new_pet(),
            pet_state: None,
            pet_down: false,
            pet_hover: false,
            btn_settings: IconButton::new(Icon::Sliders, 28.0, 15.0),
            btn_hide: IconButton::new(Icon::EyeOff, 28.0, 15.0),
            btn_collapse: IconButton::new(Icon::ChevronUp, 28.0, 15.0),
            lh: Lh::default(),
            hl: HeaderLayout::default(),
            name: String::new(),
            name_w: 0.0,
            lvl: String::new(),
            lvl_w: 0.0,
            line_key: None,
            hidden_key: None,
            line: String::new(),
            xp: Anim::new(3.0),
            xp_shown: 3.0,
            stat: String::new(),
            stat_w: 0.0,
            today_at: None,
            scroll: Scroll::new(),
            scroll_shown: 0.0,
            focus: FocusSection::new(),
            input: InputField::new(),
            queue,
            done,
            settings: SettingsPanel::new(),
            n_focus: 0,
            n_queue: 0,
            count_shown: (usize::MAX, usize::MAX),
            ml: MainLayout::default(),
            implicit_h: (HEADER_Y + HEADER_H + 12.0 + BOTTOM_PAD).min(MAX_H),
            grab_sections: false,
            scratch: DamageSink::new(),
            rng: crate::real::next_seed() | 1,
            inited: false,
            last_rev: 0,
            last_env_rev: u64::MAX,
        }
    }

    /// `opacity > 0.01`.
    pub fn visible(&self, now: Ms) -> bool {
        self.op.value(now) > 0.01
    }

    /// Opacity: 90 ms pause + 280 ms `OutCubic` in, 100 ms out; translate -14 -> 0 over 420 ms.
    pub fn set_shown(&mut self, v: bool, now: Ms) {
        if v == self.shown {
            return;
        }
        self.shown = v;
        if v {
            self.op.animate_to_delayed(1.0, 90.0, 280.0, Easing::OutCubic, now);
        } else {
            self.op.animate_to(0.0, 100.0, Easing::OutCubic, now);
        }
        self.ty.animate_to(if v { 0.0 } else { HIDE_DY }, 420.0, Easing::OutCubic, now);
    }

    /// Crossfades main sections and settings over 200 ms.
    pub fn set_settings_open(&mut self, v: bool, now: Ms) {
        if v == self.settings_open {
            return;
        }
        self.settings_open = v;
        self.main_op.animate_to(if v { 0.0 } else { 1.0 }, 200.0, Easing::Linear, now);
        self.settings.set_shown(v, now);
    }

    /// `min(700, 12 + 70 + 12 + body + 20)`.
    pub fn implicit_height(&self) -> f32 {
        self.implicit_h
    }

    /// Focuses the new-demand field (quick add).
    pub fn focus_input(&mut self, cx: &mut Cx) {
        self.input.focus(cx);
    }

    pub fn input_empty(&self) -> bool {
        self.input.is_empty()
    }

    pub fn pet_react(&mut self, name: &str, now: Ms) {
        self.pet.react(name, pet_event::duration_ms(name), now);
    }

    // ------------------------------------------------------------------ geometry

    fn viewport(&self) -> Rect {
        let h = (self.frame.h - BODY_Y - BOTTOM_PAD).max(0.0);
        Rect::new(self.frame.x, self.frame.y + BODY_Y, self.frame.w, h)
    }

    fn body_h(&self) -> f32 {
        if self.settings_open {
            self.settings.height()
        } else {
            self.ml.total
        }
    }

    fn header_rect(&self) -> Rect {
        Rect::new(self.frame.x + PAD, self.frame.y + HEADER_Y, self.frame.w - 2.0 * PAD, HEADER_H)
    }

    fn body_origin(&self, now: Ms) -> Point {
        Point::new(self.frame.x + PAD, self.viewport().y - self.scroll.offset(now))
    }

    fn column_width(&self) -> f32 {
        self.frame.w - 2.0 * PAD
    }

    fn main_visible(&self, now: Ms) -> bool {
        !self.settings_open || self.main_op.value(now) > 0.01
    }
    fn settings_visible(&self, now: Ms) -> bool {
        self.settings.visible(now)
    }

    fn next_rand(&mut self) -> u64 {
        // xorshift64*
        let mut x = self.rng;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn quip(&mut self, kind: PetKind) -> String {
        let list: &[&str] = if kind == PetKind::Cat { &QUIPS_PETTED_CAT } else { &QUIPS_PETTED_OTHER };
        let i = (self.next_rand() >> 33) as usize % list.len();
        list[i].to_string()
    }

    // ------------------------------------------------------------------ update

    fn refresh_header(&mut self, cx: &mut Cx) {
        let now = cx.now;
        let st = cx.store.state();
        if self.lh.name == 0.0 {
            self.lh = Lh { name: cx.text.line_height(&name_style()), line: cx.text.line_height(&line_style()) };
        }
        let mut col_changed = false;
        if self.name != st.settings.pet_name || !self.inited {
            self.name.clear();
            self.name.push_str(&st.settings.pet_name);
            self.name_w = cx.text.measure(&self.name, &name_style());
            col_changed = true;
        }
        let lvl = format!("nv {}", st.level());
        if lvl != self.lvl {
            self.lvl_w = cx.text.measure(&lvl, &lvl_style(theme::TEXT));
            self.lvl = lvl;
            col_changed = true;
        }
        // XP bar: first value is not animated (a Behavior does not run on the initial binding)
        let xp_w = (XP_W * st.xp()).max(3.0);
        if self.inited {
            self.xp.animate_to(xp_w, 700.0, Easing::OutCubic, now);
        } else {
            self.xp.set(xp_w);
        }
        // pet line
        let mood = model::mood(cx.store, cx.epoch);
        let quip = cx.env.quip.as_deref();
        let key_changed = match &self.line_key {
            Some((m, nf, nq, q)) => {
                *m != mood || *nf != st.focus.len() || *nq != st.queue.len() || q.as_deref() != quip
            }
            None => true,
        };
        self.n_focus = st.focus.len();
        self.n_queue = st.queue.len();
        if key_changed {
            let raw = model::pet_line(mood, st.focus.len(), st.queue.len(), quip);
            self.line = cx.text.elide(&raw, &line_style(), 236.0);
            self.line_key = Some((mood, st.focus.len(), st.queue.len(), quip.map(str::to_string)));
            col_changed = true;
        }
        if col_changed {
            cx.damage.add(self.hl.col);
        }
        // today pill
        if !self.inited || self.today_at.is_some_and(|t| now >= t) || cx.rev != self.last_rev {
            let (count, ms) = model::today(cx.store, cx.epoch);
            let s = format!("{count}  ·  {}", fmt_short(ms));
            if s != self.stat {
                self.stat_w = cx.text.measure(&s, &stat_style());
                self.stat = s;
                cx.damage.add(self.hl.stat.inflate(2.0, 2.0));
                self.layout_header_rects();
                cx.damage.add(self.hl.stat.inflate(2.0, 2.0));
            }
            self.today_at = model::next_today_minute(cx.store, cx.epoch, now);
        }
        // pet
        let ps = (st.settings.kind(), mood, cx.env.accent);
        if self.pet_state != Some(ps) {
            if self.pet_state.map(|p| p.0) != Some(ps.0) {
                self.pet.set_kind(ps.0, now);
            }
            if self.pet_state.map(|p| p.1) != Some(ps.1) {
                self.pet.set_mood(ps.1, now);
            }
            self.pet.set_accent(ps.2);
            self.pet.set_size(PET_SIZE);
            self.pet_state = Some(ps);
            let od = self.pet.overdraw();
            cx.damage.add(self.hl.pet.inflate(od, od));
        }
        let want = if self.settings_open { Icon::X } else { Icon::Sliders };
        self.btn_settings.set_icon(cx, want);
    }

    /// Positions that depend on string widths / line heights (no `Cx` needed).
    fn layout_header_rects(&mut self) {
        let h = self.header_rect();
        let pet = Rect::new(h.x, h.y + (HEADER_H - PET_SIZE) * 0.5, PET_SIZE, PET_SIZE);
        let col_x = pet.right() + 10.0;
        let row_h = self.lh.name.max(17.0);
        let col_h = row_h + 4.0 + self.lh.line + 4.0 + 3.0;
        let top = h.y + (HEADER_H - col_h) * 0.5;
        let name = Point::new(col_x, top);
        let pill = Rect::new(col_x + self.name_w + 7.0, top + (row_h - 17.0) * 0.5, self.lvl_w + 12.0, 17.0);
        let line_y = top + row_h + 4.0;
        let xp = Rect::new(col_x, line_y + self.lh.line + 4.0, XP_W, 3.0);
        // right group: chevron | hide | settings | 2 px gap | stat pill, 4 px apart
        let right = h.right();
        let by = h.y + (HEADER_H - 28.0) * 0.5;
        let chevron = Rect::new(right - 28.0, by, 28.0, 28.0);
        let hide = Rect::new(chevron.x - 4.0 - 28.0, by, 28.0, 28.0);
        let sett = Rect::new(hide.x - 4.0 - 28.0, by, 28.0, 28.0);
        let spacer_x = sett.x - 4.0 - 2.0;
        let stat_w = 12.0 + 5.0 + self.stat_w + 18.0;
        let stat = Rect::new(spacer_x - 4.0 - stat_w, h.y + (HEADER_H - 26.0) * 0.5, stat_w, 26.0);
        let col = Rect::new(col_x - 2.0, h.y, (stat.x - col_x - 2.0).max(0.0), HEADER_H);
        self.hl = HeaderLayout { pet, name, pill, line_y, xp, stat, col, sett, hide, chevron };
    }

    fn update_sections(&mut self, cx: &mut Cx, visible: bool) {
        let now = cx.now;
        let main = self.main_visible(now);
        let sett = self.settings_visible(now);
        self.focus.set_visible(visible && main, now);
        if main {
            self.focus.update(cx);
            self.input.update(cx);
            self.queue.update(cx);
            self.done.update(cx);
        }
        if sett {
            self.settings.update(cx);
        }
        let h_done = if self.done.shown() { self.done.height() } else { 0.0 };
        self.ml = MainLayout::compute(
            self.focus.height(),
            self.input.height(),
            self.n_queue > 0,
            self.queue.height(),
            h_done,
        );
    }
}

impl ExpandedView {
    /// A section animation (crossfade, scroll, xp bar) is still running.
    fn sections_animating(&self, now: Ms) -> bool {
        self.op.is_animating(now)
            || self.ty.is_animating(now)
            || self.main_op.is_animating(now)
            || self.xp.is_animating(now)
            || self.scroll.is_animating(now)
    }

    /// Runs `f` against a scratch damage sink: while the panel is invisible nothing needs a repaint.
    fn quietly(&mut self, cx: &mut Cx, f: impl FnOnce(&mut Self, &mut Cx)) {
        let mut scratch = std::mem::take(&mut self.scratch);
        {
            let mut sub = Cx {
                now: cx.now,
                epoch: cx.epoch,
                rev: cx.rev,
                store: cx.store,
                env: cx.env,
                text: &mut *cx.text,
                damage: &mut scratch,
                msgs: &mut *cx.msgs,
                focus: &mut *cx.focus,
                window_focused: cx.window_focused,
                cursor: cx.cursor,
            };
            f(self, &mut sub);
            cx.cursor = sub.cursor;
        }
        scratch.clear();
        self.scratch = scratch;
    }

    fn update_inner(&mut self, cx: &mut Cx, visible: bool) {
        let now = cx.now;
        if visible != self.was_visible {
            self.was_visible = visible;
            self.pet.set_active(visible, now);
            if !visible {
                self.pet_hover = false;
                self.pet_down = false;
                self.grab_sections = false;
                self.btn_settings.reset(cx);
                self.btn_hide.reset(cx);
                self.btn_collapse.reset(cx);
            }
        }
        // header model state (cheap; runs also while invisible so that heights stay right)
        if !self.inited
            || cx.rev != self.last_rev
            || cx.env.rev != self.last_env_rev
            || self.today_at.is_some_and(|t| now >= t)
        {
            self.refresh_header(cx);
        } else {
            let mood = model::mood(cx.store, cx.epoch);
            if self.line_key.as_ref().map(|k| k.0) != Some(mood) {
                self.refresh_header(cx);
            }
        }
        self.last_rev = cx.rev;
        self.last_env_rev = cx.env.rev;

        self.update_sections(cx, visible);
        self.inited = true;

        // an invisible input cannot keep the keyboard
        if self.settings_open && !self.main_visible(now) && *cx.focus == FocusId::NewTask {
            *cx.focus = FocusId::None;
        }

        let body = self.body_h();
        self.implicit_h = (HEADER_Y + HEADER_H + 12.0 + body + BOTTOM_PAD).min(MAX_H);

        if !visible {
            return;
        }

        // counts in the section labels
        let counts = (self.n_focus, self.n_queue);
        if counts != self.count_shown {
            self.count_shown = counts;
            let vp = self.viewport();
            cx.damage.add(Rect::new(
                self.frame.x + PAD,
                vp.y - self.scroll.offset(now) + self.ml.label_focus,
                120.0,
                SECTION_LABEL_H,
            ));
            if let Some(l) = self.ml.label_queue {
                cx.damage.add(Rect::new(
                    self.frame.x + PAD,
                    vp.y - self.scroll.offset(now) + l,
                    120.0,
                    SECTION_LABEL_H,
                ));
            }
        }

        // opacity / translate of the whole panel and the group crossfade
        let fx = (self.op.value(now), self.ty.value(now));
        if (fx.0 - self.shown_fx.0).abs() >= 1.0 / 255.0 || (fx.1 - self.shown_fx.1).abs() >= 0.05 {
            self.shown_fx = fx;
            cx.damage.add(self.frame.inflate(0.0, 16.0));
        }
        let g = (self.main_op.value(now), self.settings.opacity(now));
        if (g.0 - self.shown_group.0).abs() >= 1.0 / 255.0 || (g.1 - self.shown_group.1).abs() >= 1.0 / 255.0 {
            self.shown_group = g;
            cx.damage.add(self.viewport());
        }

        // xp bar
        let xw = self.xp.value(now);
        if (xw - self.xp_shown).abs() >= 0.05 {
            self.xp_shown = xw;
            cx.damage.add(self.hl.xp.inflate(1.0, 1.0));
        }

        // pet
        let od = self.pet.overdraw();
        if self.pet.update(now) {
            cx.damage.add(self.hl.pet.inflate(od, od));
        }

        // header buttons
        self.btn_settings.tick(cx);
        self.btn_hide.tick(cx);
        self.btn_collapse.tick(cx);

        // scroll offset
        let off = self.scroll.offset(now);
        if (off - self.scroll_shown).abs() >= 0.05 {
            self.scroll_shown = off;
            cx.damage.add(self.viewport());
        }
    }

    fn layout_inner(&mut self, cx: &mut Cx, frame: Rect) {
        let now = cx.now;
        if frame != self.frame {
            cx.damage.add_moved(self.frame, frame);
            self.frame = frame;
        }
        self.layout_header_rects();
        let hl = self.hl;
        self.btn_settings.layout(cx, hl.sett);
        self.btn_hide.layout(cx, hl.hide);
        self.btn_collapse.layout(cx, hl.chevron);

        let vp = self.viewport();
        self.scroll.set_sizes(vp.h, self.body_h(), now);
        let o = self.body_origin(now);
        let w = self.column_width();
        if self.main_visible(now) {
            let ml = self.ml;
            self.focus.layout(cx, Rect::new(o.x, o.y + ml.focus, w, self.focus.height()));
            self.input.layout(cx, Rect::new(o.x, o.y + ml.input, w, self.input.height()));
            if ml.label_queue.is_some() {
                self.queue.layout(cx, Rect::new(o.x, o.y + ml.queue, w, self.queue.height()));
            }
            if let Some(d) = ml.done {
                self.done.layout(cx, Rect::new(o.x, o.y + d, w, self.done.height()));
            }
        }
        if self.settings_visible(now) {
            self.settings.layout(cx, Rect::new(o.x, o.y, w, self.settings.height()));
        }
    }
}

impl View for ExpandedView {
    fn update(&mut self, cx: &mut Cx) {
        let visible = self.visible(cx.now);
        if visible != self.was_visible {
            // appearing or disappearing: the panel's area must be repainted
            cx.damage.add(self.frame.inflate(0.0, 16.0));
        }
        if visible {
            self.hidden_key = None;
            self.update_inner(cx, true);
        } else {
            // Invisible and none of its inputs moved since the last pass: nothing to recompute
            // (a hidden panel only keeps its implicit height current; every input of that is
            // covered by the key, and `visible` re-runs the full pass on the first shown frame).
            let key = (
                cx.rev,
                cx.env.rev,
                self.settings_open,
                self.shown,
                model::mood(cx.store, cx.epoch),
                self.today_at.is_some_and(|t| cx.now >= t),
            );
            if self.inited && !self.was_visible && self.hidden_key == Some(key) && !self.sections_animating(cx.now) {
                return;
            }
            self.quietly(cx, |me, c| me.update_inner(c, false));
            self.hidden_key = Some(key);
        }
    }

    fn layout(&mut self, cx: &mut Cx, frame: Rect) {
        if self.visible(cx.now) {
            self.layout_inner(cx, frame);
        } else {
            self.quietly(cx, |me, c| me.layout_inner(c, frame));
        }
    }

    fn event(&mut self, cx: &mut Cx, ev: &Event) -> Handled {
        let now = cx.now;
        let mut out = Handled::No;
        match ev {
            Event::Key(_) | Event::Paste(_) => {
                if self.main_visible(now) {
                    out = merge(out, self.input.event(cx, ev));
                    out = merge(out, self.queue.event(cx, ev));
                }
                if self.settings_visible(now) {
                    out = merge(out, self.settings.event(cx, ev));
                }
                return out;
            }
            Event::FocusIn | Event::FocusOut => {
                if self.main_visible(now) {
                    self.input.event(cx, ev);
                    self.queue.event(cx, ev);
                }
                if self.settings_visible(now) {
                    self.settings.event(cx, ev);
                }
                return Handled::No;
            }
            Event::Wheel { pos, dy } => {
                let p = Point::new(pos.x, pos.y - self.ty.value(now));
                if self.viewport().contains(p) && self.scroll.wheel(*dy, now) {
                    cx.damage.add(self.viewport());
                    return Handled::Yes;
                }
                return Handled::No;
            }
            _ => {}
        }

        // pointer events: the panel is translated by `ty`
        let ty = self.ty.value(now);
        let ev = &shift_y(ev, -ty);
        let pos = match ev {
            Event::PointerMove { pos } | Event::PointerDown { pos, .. } | Event::PointerUp { pos, .. } => Some(*pos),
            _ => None,
        };

        // header: mascot
        let pr = self.hl.pet;
        match ev {
            Event::PointerMove { pos } => {
                if pr.contains(*pos) {
                    self.pet.pointer((pos.x - pr.x) / pr.w, (pos.y - pr.y) / pr.h, now);
                    self.pet_hover = true;
                    cx.cursor = CursorShape::Hand;
                } else if self.pet_hover {
                    self.pet_hover = false;
                    self.pet.set_hovered(false, now);
                    self.pet.set_look(0.0, 0.0, now);
                }
            }
            Event::PointerLeave => {
                if self.pet_hover {
                    self.pet_hover = false;
                    self.pet.set_hovered(false, now);
                    self.pet.set_look(0.0, 0.0, now);
                }
            }
            Event::PointerDown { pos, button: foco_base::Button::Left, .. } if pr.contains(*pos) => {
                self.pet_down = true;
                out = Handled::Grab;
            }
            Event::PointerUp { pos, button: foco_base::Button::Left } if self.pet_down => {
                self.pet_down = false;
                out = merge(out, Handled::Yes);
                if pr.contains(*pos) {
                    // TapHandler.onTapped: react("love", 1900) + petted quip
                    self.pet.tap(now);
                    let kind = cx.store.state().settings.kind();
                    let q = self.quip(kind);
                    cx.msgs.push(Msg::Say(q));
                }
            }
            _ => {}
        }

        // header: buttons
        let o = self.btn_settings.pointer(cx, ev);
        if o.clicked {
            cx.msgs
                .push(Msg::Cmd(if self.settings_open { AppCmd::CloseSettings } else { AppCmd::OpenSettings }));
        }
        out = merge(out, o.handled);
        let o = self.btn_hide.pointer(cx, ev);
        if o.clicked {
            cx.msgs.push(Msg::Cmd(AppCmd::Hide));
        }
        out = merge(out, o.handled);
        let o = self.btn_collapse.pointer(cx, ev);
        if o.clicked {
            cx.msgs.push(Msg::Cmd(AppCmd::Collapse));
        }
        out = merge(out, o.handled);

        // body (inside the clipped viewport; a grab keeps receiving everything)
        let vp = self.viewport();
        let inside = pos.is_none_or(|p| vp.contains(p));
        let to_sections: Event = if self.grab_sections || inside {
            ev.clone()
        } else if matches!(ev, Event::PointerMove { .. } | Event::PointerDown { .. }) {
            Event::PointerLeave
        } else {
            ev.clone()
        };
        let mut sec = Handled::No;
        if self.main_visible(now) {
            sec = merge(sec, self.focus.event(cx, &to_sections));
            sec = merge(sec, self.input.event(cx, &to_sections));
            sec = merge(sec, self.queue.event(cx, &to_sections));
            sec = merge(sec, self.done.event(cx, &to_sections));
        }
        if self.settings_visible(now) {
            sec = merge(sec, self.settings.event(cx, &to_sections));
        }
        match (&to_sections, sec) {
            (_, Handled::Grab) => self.grab_sections = true,
            (Event::PointerUp { .. }, _) => self.grab_sections = false,
            _ => {}
        }
        merge(out, sec)
    }

    fn paint(&mut self, s: &mut dyn Surface, pc: &PaintCx) {
        let now = pc.now;
        let op = self.op.value(now);
        if op <= 0.01 {
            return;
        }
        let ty = self.ty.value(now);
        if !self.frame.inflate(0.0, 16.0).intersects(&pc.clip) {
            return;
        }
        s.save();
        s.mul_opacity(op);
        s.translate(0.0, ty);
        let clip = Rect::new(pc.clip.x, pc.clip.y - ty, pc.clip.w, pc.clip.h);
        self.paint_header(s, pc);

        // body
        let vp = self.viewport();
        if vp.h > 0.0 && vp.intersects(&clip) {
            s.save();
            s.clip_rect(vp);
            let sub = PaintCx {
                now: pc.now,
                epoch: pc.epoch,
                clip: vp.intersection(&clip).unwrap_or_default(),
                env: pc.env,
                store: pc.store,
                focus: pc.focus,
                window_focused: pc.window_focused,
            };
            let mo = self.main_op.value(now);
            if mo > 0.01 && self.main_visible(now) {
                s.save();
                s.mul_opacity(mo);
                self.paint_main(s, &sub);
                s.restore();
            }
            // SettingsPanel owns its 200 ms fade (no second opacity here)
            if self.settings_visible(now) && self.settings.bounds().intersects(&sub.clip) {
                self.settings.paint(s, &sub);
            }
            s.restore();
        }
        s.restore();
    }

    fn next_wake(&self, now: Ms) -> Wake {
        let fading = self.op.is_animating(now) || self.ty.is_animating(now);
        if !self.visible(now) {
            return if fading { Wake::Frame } else { Wake::Idle };
        }
        if fading || self.main_op.is_animating(now) || self.xp.is_animating(now) || self.scroll.is_animating(now) {
            return Wake::Frame;
        }
        let mut w = self.pet.next_wake(now);
        for b in [&self.btn_settings, &self.btn_hide, &self.btn_collapse] {
            w = w.merge_at(b.next_wake(now), now);
        }
        if let Some(t) = self.today_at {
            w = w.merge_at(Wake::at(now, t), now);
        }
        if self.main_visible(now) {
            w = w.merge_at(self.focus.next_wake(now), now);
            w = w.merge_at(self.input.next_wake(now), now);
            w = w.merge_at(self.queue.next_wake(now), now);
            w = w.merge_at(self.done.next_wake(now), now);
        }
        if self.settings_visible(now) {
            w = w.merge_at(self.settings.next_wake(now), now);
        }
        w
    }

    fn bounds(&self) -> Rect {
        self.frame.inflate(0.0, 16.0)
    }
}

impl ExpandedView {
    fn paint_header(&self, s: &mut dyn Surface, pc: &PaintCx) {
        let now = pc.now;
        let hl = &self.hl;
        let accent = pc.env.accent;
        // the panel is painted under `translate(0, ty)`: bring the damaged rect into that space.
        // A repaint of the breathing pet (10 Hz) then skips every other header element.
        let ty = self.ty.value(now);
        let clip = Rect::new(pc.clip.x, pc.clip.y - ty, pc.clip.w, pc.clip.h);
        let hit = |r: Rect| r.inflate(2.0, 2.0).intersects(&clip);
        let od = self.pet.overdraw();
        if hit(hl.pet.inflate(od, od)) {
            self.pet.paint(s, hl.pet, now);
        }
        if hit(Rect::new(hl.name.x, hl.name.y, self.name_w, self.lh.name.max(17.0))) {
            s.text(&self.name, hl.name.x, hl.name.y, &name_style());
        }
        // level pill
        if hit(hl.pill) {
            s.fill_rrect(hl.pill, 8.5, &Fill::Solid(accent.with_alpha(0.24)));
            let ls = lvl_style(pc.env.accent_text());
            let lw = s.measure(&self.lvl, &ls);
            let c = hl.pill.center();
            let lh = s.line_height(&ls);
            s.text(&self.lvl, c.x - lw * 0.5, c.y - lh * 0.5, &ls);
        }
        if hit(Rect::new(hl.name.x, hl.line_y, hl.col.w, self.lh.line)) {
            s.text(&self.line, hl.name.x, hl.line_y, &line_style());
        }
        // xp bar
        if hit(hl.xp) {
            s.fill_rrect(hl.xp, 1.5, &Fill::Solid(theme::FILL));
            let xw = self.xp.value(now).min(XP_W);
            s.fill_rrect(Rect::new(hl.xp.x, hl.xp.y, xw, 3.0), 1.5, &Fill::Solid(accent));
        }
        // today pill
        let st = hl.stat;
        if hit(st) {
            s.fill_rrect(st, 13.0, &Fill::Solid(theme::CARD));
            s.stroke_rrect(st.inflate(-0.5, -0.5), 12.5, &Fill::Solid(theme::STROKE), 1.0);
            let cy = st.y + st.h * 0.5;
            paint_icon(s, Icon::Check, Point::new(st.x + 9.0 + 6.0, cy), 12.0, theme::GREEN, 2.2);
            let sty = stat_style();
            let ty = cy - s.line_height(&sty) * 0.5;
            s.text(&self.stat, st.x + 9.0 + 12.0 + 5.0, ty, &sty);
        }
        if hit(hl.sett) {
            self.btn_settings.paint(s, now);
        }
        if hit(hl.hide) {
            self.btn_hide.paint(s, now);
        }
        if hit(hl.chevron) {
            self.btn_collapse.paint(s, now);
        }
    }

    fn paint_main(&mut self, s: &mut dyn Surface, pc: &PaintCx) {
        let o = self.body_origin(pc.now);
        let w = self.column_width();
        let ml = self.ml;
        let lab = |y: f32| Rect::new(o.x, o.y + y, w, SECTION_LABEL_H);
        let clip = pc.clip;
        if lab(ml.label_focus).intersects(&clip) {
            paint_section_label(s, lab(ml.label_focus), "NO PALCO", Some(self.n_focus));
        }
        if self.focus.bounds().intersects(&clip) {
            self.focus.paint(s, pc);
        }
        if self.input.bounds().intersects(&clip) {
            self.input.paint(s, pc);
        }
        if let Some(l) = ml.label_queue {
            if lab(l).intersects(&clip) {
                paint_section_label(s, lab(l), "SETLIST", Some(self.n_queue));
            }
            if self.queue.bounds().intersects(&clip) {
                self.queue.paint(s, pc);
            }
        }
        if ml.done.is_some() && self.done.bounds().intersects(&clip) {
            self.done.paint(s, pc);
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

fn shift_y(ev: &Event, dy: f32) -> Event {
    if dy == 0.0 {
        return ev.clone();
    }
    match ev {
        Event::PointerMove { pos } => Event::PointerMove { pos: Point::new(pos.x, pos.y + dy) },
        Event::PointerDown { pos, button, clicks } => {
            Event::PointerDown { pos: Point::new(pos.x, pos.y + dy), button: *button, clicks: *clicks }
        }
        Event::PointerUp { pos, button } => Event::PointerUp { pos: Point::new(pos.x, pos.y + dy), button: *button },
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cx::S;
    use crate::testkit::Harness;
    use foco_base::{Button, Task};

    /// A fixed-height section that paints a flat rect.
    struct Stub {
        h: f32,
        col: Color,
        frame: Rect,
    }
    impl Stub {
        fn boxed(h: f32, col: Color) -> Box<dyn Section> {
            Box::new(Stub { h, col, frame: Rect::default() })
        }
    }
    impl View for Stub {
        fn update(&mut self, _cx: &mut Cx) {}
        fn layout(&mut self, _cx: &mut Cx, frame: Rect) {
            self.frame = frame;
        }
        fn event(&mut self, _cx: &mut Cx, _ev: &Event) -> Handled {
            Handled::No
        }
        fn paint(&mut self, s: &mut dyn Surface, _pc: &PaintCx) {
            s.fill_rrect(self.frame, 14.0, &Fill::Solid(self.col));
        }
        fn next_wake(&self, _now: Ms) -> Wake {
            Wake::Idle
        }
        fn bounds(&self) -> Rect {
            self.frame
        }
    }
    impl Section for Stub {
        fn height(&self) -> f32 {
            self.h
        }
    }

    fn view(queue_h: f32, done_h: f32) -> ExpandedView {
        ExpandedView::with_sections(
            Stub::boxed(queue_h, Color::rgb8(0xd4, 0xe8, 0xd4)),
            Stub::boxed(done_h, Color::rgb8(0xe8, 0xd4, 0xd4)),
        )
    }

    fn task(h: &Harness, tid: &str, running: bool, elapsed_ms: i64) -> Task {
        Task {
            tid: tid.into(),
            title: format!("Tarefa {tid}"),
            acc_ms: if running { 0 } else { elapsed_ms },
            started_at: if running { h.epoch as i64 - elapsed_ms } else { 0 },
            running,
            ..Task::default()
        }
    }

    fn frame(v: &ExpandedView) -> Rect {
        Rect::new(106.0, 0.0, WIDTH, v.implicit_height())
    }

    fn step(h: &mut Harness, v: &mut ExpandedView, now: Ms) {
        h.with_cx(now, |cx| {
            v.update(cx);
            let f = frame(v);
            v.layout(cx, f);
        });
    }

    /// Opens the panel and lets every transition finish.
    fn open(h: &mut Harness, v: &mut ExpandedView) -> Ms {
        v.set_shown(true, 0.0);
        step(h, v, 0.0);
        step(h, v, 10.0);
        step(h, v, 800.0);
        h.damage.clear();
        800.0
    }

    #[test]
    fn column_rules_and_implicit_height() {
        // empty panel: label 20 + 10 + empty state 72 + 10 + input 44
        let ml = MainLayout::compute(72.0, 44.0, false, 0.0, 0.0);
        assert_eq!(ml.total, 20.0 + 10.0 + 72.0 + 10.0 + 44.0);
        assert_eq!((ml.label_focus, ml.focus, ml.input), (0.0, 30.0, 112.0));
        // with a queue: the 2 px spacer is a child too (10 before, 10 after)
        let ml = MainLayout::compute(80.0, 44.0, true, 82.0, 0.0);
        assert_eq!(ml.input, 20.0 + 10.0 + 80.0 + 10.0);
        assert_eq!(ml.label_queue, Some(ml.input + 44.0 + 10.0 + 2.0 + 10.0));
        assert_eq!(ml.queue, ml.label_queue.unwrap() + 20.0 + 10.0);
        assert_eq!(ml.total, ml.queue + 82.0);
        // done section appends 10 + h
        let ml2 = MainLayout::compute(80.0, 44.0, true, 82.0, 60.0);
        assert_eq!(ml2.done, Some(ml.total + 10.0));
        assert_eq!(ml2.total, ml.total + 10.0 + 60.0);

        let mut h = Harness::new();
        let mut v = view(0.0, 0.0);
        step(&mut h, &mut v, 0.0);
        assert_eq!(v.implicit_height(), 12.0 + 70.0 + 12.0 + 156.0 + 20.0);
        h.store.state.focus.push(task(&h, "a", true, 1000));
        h.store.state.queue.push(task(&h, "q", false, 0));
        h.rev += 1;
        let mut v = view(82.0, 60.0);
        step(&mut h, &mut v, 0.0);
        let body = 20.0 + 10.0 + 80.0 + 10.0 + 44.0 + 10.0 + 2.0 + 10.0 + 20.0 + 10.0 + 82.0 + 10.0 + 60.0;
        assert_eq!(v.implicit_height(), 12.0 + 70.0 + 12.0 + body + 20.0);
    }

    #[test]
    fn height_is_capped_and_body_scrolls() {
        let mut h = Harness::new();
        let mut v = view(900.0, 0.0);
        h.store.state.queue.push(task(&h, "q", false, 0));
        step(&mut h, &mut v, 0.0);
        assert_eq!(v.implicit_height(), 700.0);
        let vp = v.viewport();
        assert_eq!(vp.y, 94.0);
        assert_eq!(vp.h, 700.0 - 94.0 - 20.0);
        let t0 = open(&mut h, &mut v);
        // wheel inside the viewport scrolls, 200 ms OutCubic
        let pos = Point::new(300.0, 300.0);
        let r = h.with_cx(t0, |cx| v.event(cx, &Event::Wheel { pos, dy: 120.0 }));
        assert_eq!(r, Handled::Yes);
        step(&mut h, &mut v, t0 + 100.0);
        assert!(!h.damage.is_empty());
        assert!(v.scroll.offset(t0 + 100.0) > 0.0);
        assert_eq!(v.next_wake(t0 + 100.0), Wake::Frame);
        step(&mut h, &mut v, t0 + 300.0);
        assert_eq!(v.scroll.offset(t0 + 300.0), 120.0);
        // section positions follow the offset
        let o = v.body_origin(t0 + 300.0);
        assert_eq!(o.y, 94.0 - 120.0);
        // wheel over the header does nothing
        let r = h.with_cx(t0 + 400.0, |cx| v.event(cx, &Event::Wheel { pos: Point::new(300.0, 40.0), dy: 50.0 }));
        assert_eq!(r, Handled::No);
        // short content: not scrollable
        let mut v2 = view(0.0, 0.0);
        step(&mut h, &mut v2, 0.0);
        assert!(!v2.scroll.scrollable());
    }

    #[test]
    fn idle_when_nothing_runs_and_exact_deadlines_when_it_does() {
        let mut h = Harness::new();
        let mut v = view(0.0, 0.0);
        h.store.state.focus.push(task(&h, "a", false, 1000));
        let t0 = open(&mut h, &mut v);
        step(&mut h, &mut v, t0 + 50.0);
        // the pet placeholder never wakes and nothing animates
        assert_eq!(v.next_wake(t0 + 50.0), Wake::Idle);
        // run a task whose clock is at 1.25 s: wake on the second boundary
        h.store.state.focus[0] = Task {
            running: true,
            acc_ms: 0,
            started_at: (h.epoch + t0 + 100.0) as i64 - 1250,
            ..h.store.state.focus[0].clone()
        };
        h.rev += 1;
        step(&mut h, &mut v, t0 + 100.0);
        step(&mut h, &mut v, t0 + 900.0); // let the card fade-in settle
        let w = v.next_wake(t0 + 900.0);
        assert!(matches!(w, Wake::At(_) | Wake::Frame), "{w:?}");
        step(&mut h, &mut v, t0 + 1300.0);
        let w = v.next_wake(t0 + 1300.0);
        let Wake::At(t) = w else { panic!("{w:?}") };
        // elapsed 1250 at t0+100 -> 2.45 s at t0+1300 -> next boundary 3 s at t0+1850 (+2 ms slack)
        assert!((t - (t0 + 1852.0)).abs() <= 1.0, "{t}");
        // the tick damages the clock text + ring only, never the whole panel
        h.damage.clear();
        step(&mut h, &mut v, t);
        let area: i64 = h.damage.rects().iter().map(foco_base::IRect::area).sum();
        assert!(area > 0 && area < (548.0f32 * S * 200.0 * S) as i64, "{area}");
    }

    #[test]
    fn hidden_panel_is_idle_but_keeps_its_height_current() {
        let mut h = Harness::new();
        let mut v = view(0.0, 0.0);
        h.store.state.focus.push(task(&h, "a", true, 1000));
        step(&mut h, &mut v, 0.0);
        assert!(!v.visible(0.0));
        assert_eq!(v.next_wake(0.0), Wake::Idle);
        let before = v.implicit_height();
        h.store.state.focus.push(task(&h, "b", true, 1000));
        h.rev += 1;
        step(&mut h, &mut v, 5.0);
        assert_eq!(v.implicit_height(), before + 88.0);
        assert_eq!(v.next_wake(5.0), Wake::Idle, "no clock wakes while collapsed");
        assert!(h.damage.is_empty() || v.visible(5.0), "no repaint for a hidden panel");
    }

    #[test]
    fn show_hide_animation() {
        let mut h = Harness::new();
        let mut v = view(0.0, 0.0);
        v.set_shown(true, 1000.0);
        step(&mut h, &mut v, 1000.0);
        // 90 ms pause, then 280 ms
        assert!(!v.visible(1050.0));
        assert!(v.visible(1200.0));
        assert_eq!(v.op.value(1370.0), 1.0);
        assert!((v.ty.value(1000.0 + 210.0) - (HIDE_DY * (1.0 - Easing::OutCubic.apply(0.5)))).abs() < 1e-4);
        assert_eq!(v.ty.value(1420.0), 0.0);
        step(&mut h, &mut v, 2000.0);
        assert_eq!(v.next_wake(2000.0), Wake::Idle);
        v.set_shown(false, 3000.0);
        assert_eq!(v.next_wake(3000.0), Wake::Frame);
        assert!(!v.visible(3100.0));
        step(&mut h, &mut v, 3500.0);
        assert_eq!(v.next_wake(3500.0), Wake::Idle);
    }

    #[test]
    fn header_buttons_and_pet() {
        let mut h = Harness::new();
        let mut v = view(0.0, 0.0);
        let t0 = open(&mut h, &mut v);
        let click = |h: &mut Harness, v: &mut ExpandedView, now: Ms, r: Rect| {
            let p = r.center();
            h.with_cx(now, |cx| {
                v.event(cx, &Event::PointerMove { pos: p });
                v.event(cx, &Event::PointerDown { pos: p, button: Button::Left, clicks: 1 });
                v.event(cx, &Event::PointerUp { pos: p, button: Button::Left });
            });
        };
        let r = v.hl.chevron;
        click(&mut h, &mut v, t0 + 1.0, r);
        assert_eq!(h.take_msgs(), vec![Msg::Cmd(AppCmd::Collapse)]);
        let r = v.hl.hide;
        click(&mut h, &mut v, t0 + 2.0, r);
        assert_eq!(h.take_msgs(), vec![Msg::Cmd(AppCmd::Hide)]);
        let r = v.hl.sett;
        click(&mut h, &mut v, t0 + 3.0, r);
        assert_eq!(h.take_msgs(), vec![Msg::Cmd(AppCmd::OpenSettings)]);
        v.set_settings_open(true, t0 + 4.0);
        step(&mut h, &mut v, t0 + 5.0);
        let r = v.hl.sett;
        click(&mut h, &mut v, t0 + 6.0, r);
        assert_eq!(h.take_msgs(), vec![Msg::Cmd(AppCmd::CloseSettings)]);
        v.set_settings_open(false, t0 + 7.0);
        // petting: a quip from the store's petted list
        let r = v.hl.pet;
        click(&mut h, &mut v, t0 + 8.0, r);
        let msgs = h.take_msgs();
        assert_eq!(msgs.len(), 1);
        let Msg::Say(q) = &msgs[0] else { panic!("{msgs:?}") };
        assert!(QUIPS_PETTED_CAT.contains(&q.as_str()), "{q}");
        // a press that is released outside is not a tap
        let c = v.hl.pet.center();
        h.with_cx(t0 + 9.0, |cx| {
            v.event(cx, &Event::PointerDown { pos: c, button: Button::Left, clicks: 1 });
            v.event(cx, &Event::PointerUp { pos: Point::new(c.x + 200.0, c.y), button: Button::Left });
        });
        assert!(h.take_msgs().is_empty());
    }

    #[test]
    fn xp_bar_animates_700ms() {
        let mut h = Harness::new();
        let mut v = view(0.0, 0.0);
        let t0 = open(&mut h, &mut v);
        assert_eq!(v.xp.value(t0), 3.0);
        h.store.state.total_done = 3; // 60 %
        h.rev += 1;
        step(&mut h, &mut v, t0 + 10.0);
        assert_eq!(v.lvl, "nv 1");
        assert!(v.xp.is_animating(t0 + 100.0));
        assert_eq!(v.xp.value(t0 + 710.0), 66.0);
        assert_eq!(v.next_wake(t0 + 100.0), Wake::Frame);
        // damages only the 110x3 bar while it grows
        h.damage.clear();
        step(&mut h, &mut v, t0 + 200.0);
        let area: i64 = h.damage.rects().iter().map(foco_base::IRect::area).sum();
        assert!(area > 0 && area < 130 * 20, "{area}");
    }

    #[test]
    fn settings_crossfade_and_focus_drop() {
        let mut h = Harness::new();
        let mut v = view(0.0, 0.0);
        let t0 = open(&mut h, &mut v);
        h.with_cx(t0, |cx| v.focus_input(cx));
        assert_eq!(h.focus, FocusId::NewTask);
        v.set_settings_open(true, t0 + 1.0);
        assert!(v.main_visible(t0 + 100.0));
        assert_eq!(v.main_op.value(t0 + 101.0), 0.5);
        step(&mut h, &mut v, t0 + 120.0);
        assert_eq!(v.next_wake(t0 + 120.0), Wake::Frame);
        step(&mut h, &mut v, t0 + 300.0);
        assert!(!v.main_visible(t0 + 300.0));
        assert_eq!(h.focus, FocusId::None, "an invisible input loses the keyboard");
    }

    fn save(name: &str, s: &foco_base::PixSurface) {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/ui-test/wave3b");
        std::fs::create_dir_all(&dir).unwrap();
        s.save_png(dir.join(name)).unwrap();
    }

    fn snapshot(h: &Harness, v: &mut ExpandedView, now: Ms, name: &str) {
        let mut s = Harness::surface();
        // the notch body behind the panel
        let f = frame(v);
        s.fill_rrect(Rect::new(f.x - 12.0, -40.0, f.w + 24.0, f.h + 40.0), 32.0, &Fill::Solid(theme::NOTCH));
        let clip = Rect::new(0.0, 0.0, 760.0, 760.0);
        h.with_pc(now, clip, |pc| v.paint(&mut s, pc));
        save(name, &s);
    }

    #[test]
    fn snapshots() {
        for (n_focus, with_queue, name) in [
            (0, false, "expanded-0-focus.png"),
            (1, true, "expanded-1-focus-queue.png"),
            (2, true, "expanded-2-focus-queue-done.png"),
        ] {
            let mut h = Harness::new();
            h.store.state.settings.pet_name = "Foquinho".into();
            h.store.state.total_done = 7;
            for i in 0..n_focus {
                h.store
                    .state
                    .focus
                    .push(task(&h, &format!("t{i}"), i == 0, 61_000 * (i as i64 + 1)));
            }
            if with_queue {
                h.store.state.queue.push(task(&h, "q1", false, 0));
                h.store.state.queue.push(task(&h, "q2", false, 0));
            }
            let (qh, dh) = if with_queue { (82.0, if n_focus == 2 { 60.0 } else { 0.0 }) } else { (0.0, 0.0) };
            let mut v = view(qh, dh);
            let t0 = open(&mut h, &mut v);
            step(&mut h, &mut v, t0 + 10.0);
            snapshot(&h, &mut v, t0 + 10.0, name);
        }
        // settings open (a real SettingsPanel, whatever its state)
        let mut h = Harness::new();
        let mut v = view(0.0, 0.0);
        let t0 = open(&mut h, &mut v);
        v.set_settings_open(true, t0);
        step(&mut h, &mut v, t0 + 400.0);
        snapshot(&h, &mut v, t0 + 400.0, "expanded-settings.png");
    }
}
