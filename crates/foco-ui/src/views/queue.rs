//! `QueueList.qml`: the "A SEGUIR" (now "SETLIST") list (the section label is drawn by `expanded.rs`).
//!
//! 40 px rows, spacing 2. Grip drag on the Y axis with live reorder (`MoveQueue` whenever the
//! dragged row's centre enters another slot, like the QML `DropArea`), double click on the title
//! renames in place, hovering a row reveals the "focus" and "trash" buttons. Add/remove/displaced
//! transitions come from [`KeyedList`] with [`ListSpec::queue`].
//!
//! Damage: list transitions damage the union of the list's rows before and after; hover fades damage
//! only their row; the dragged row damages its own old and new rect.

use crate::cx::{CursorShape, Cx, FocusId, Handled, Msg, PaintCx};
use crate::view::{Section, View};
use crate::widgets::{
    paint_icon, paint_text_centered, ColorAnim, Icon, IconButton, InputOutcome, KeyedList, ListSpec, TextInput,
    DEFAULT_STROKE,
};
use foco_base::{
    model::{fmt_short, MAX_TITLE_LEN},
    theme, Action, Anim, Button, Color, Easing, Event, Fill, Ms, Point, Rect, Surface, TextStyle, Wake, Weight,
};

const ROW_H: f32 = 40.0;
const SPACING: f32 = 2.0;
const GRIP_W: f32 = 28.0;
const NUM_X: f32 = 28.0;
/// `num.right + 6`.
const TITLE_X: f32 = 52.0;
/// Two 28 px buttons with a 2 px gap.
const ACTS_W: f32 = 58.0;
const ACTS_RIGHT: f32 = 6.0;
const DRAG_THRESHOLD: f32 = 10.0;
const DEFAULT_W: f32 = 512.0;

fn title_style() -> TextStyle {
    TextStyle::new(13.0, Weight::Regular, theme::TEXT)
}
fn num_style(first: bool, accent_text: Color) -> TextStyle {
    TextStyle::new(12.0, Weight::DemiBold, if first { accent_text } else { theme::TEXT3 }).tabular()
}
fn spent_style() -> TextStyle {
    TextStyle::new(11.0, Weight::Regular, theme::TEXT3)
}

/// Width of the title text (`acts.left - 8 - 52`).
fn title_width(w: f32) -> f32 {
    (w - ACTS_RIGHT - ACTS_W - 8.0 - TITLE_X).max(0.0)
}

/// Per-row data cached at sync time (strings are not rebuilt per frame).
#[derive(Clone, Debug)]
struct RowData {
    tid: String,
    elided: String,
    num: String,
    spent: Option<String>,
    first: bool,
}

/// Hover state of one row (fades of the background, grip and buttons, and the two buttons).
struct RowFade {
    key: String,
    bg: ColorAnim,
    grip: Anim,
    acts: Anim,
    target: IconButton,
    trash: IconButton,
    shown: (Color, f32, f32),
}

impl RowFade {
    fn new(key: &str, accent: Color, accent_text: Color) -> RowFade {
        let mut f = RowFade {
            key: key.to_string(),
            bg: ColorAnim::new(Color::TRANSPARENT),
            grip: Anim::new(0.25),
            acts: Anim::new(0.0),
            target: IconButton::new(Icon::Target, 28.0, 15.0),
            trash: IconButton::new(Icon::Trash, 28.0, 14.0),
            shown: (Color::TRANSPARENT, 0.25, 0.0),
        };
        f.trash.hover_color = theme::RED;
        f.trash.hover_bg = theme::RED.with_alpha(0.15);
        f.restyle(accent, accent_text);
        f
    }
    fn restyle(&mut self, accent: Color, accent_text: Color) {
        self.target.color = accent_text;
        self.target.hover_color = accent_text;
        self.target.hover_bg = accent.with_alpha(0.18);
    }
    fn animating(&self, now: Ms) -> bool {
        self.bg.is_animating(now)
            || self.grip.is_animating(now)
            || self.acts.is_animating(now)
            || self.target.next_wake(now) != Wake::Idle
            || self.trash.next_wake(now) != Wake::Idle
    }
}

/// Grip press / drag in progress.
struct Grip {
    key: String,
    /// Optimistic model index (follows the `MoveQueue`s already emitted).
    index: usize,
    /// Slot the dragged centre was last inside (`DropArea.entered` fires on change).
    last_in: Option<usize>,
    /// Pointer y (list coordinates) at press.
    press_y: f32,
    /// Row y (list coordinates) at press.
    row0_y: f32,
    /// Current row y (list coordinates).
    y: f32,
    active: bool,
}

struct Edit {
    tid: String,
    orig: String,
    seen_active: bool,
}

#[derive(Clone, Debug, PartialEq)]
enum Active {
    None,
    /// A row button holds the pointer (`key`, `which`: 0 = focus, 1 = trash).
    Btn(String),
    Editor,
}

pub struct QueueSection {
    frame: Rect,
    w: f32,
    list: KeyedList<RowData>,
    rev: u64,
    synced: bool,
    accent: Color,
    n: usize,
    last_bounds: Rect,
    was_animating: bool,
    hover: Option<String>,
    fades: Vec<RowFade>,
    grip: Option<Grip>,
    active: Active,
    edit: Option<Edit>,
    editor: TextInput,
    editor_active: bool,
}

impl Default for QueueSection {
    fn default() -> Self {
        Self::new()
    }
}

impl QueueSection {
    pub fn new() -> Self {
        QueueSection {
            frame: Rect::default(),
            w: DEFAULT_W,
            list: KeyedList::new(ListSpec::queue()),
            rev: 0,
            synced: false,
            accent: crate::cx::AUTO_ACCENT,
            n: 0,
            last_bounds: Rect::default(),
            was_animating: false,
            hover: None,
            fades: Vec::new(),
            grip: None,
            active: Active::None,
            edit: None,
            editor: TextInput::new(title_style(), MAX_TITLE_LEN),
            editor_active: false,
        }
    }

    /// Key of the row being renamed, if any.
    pub fn editing(&self) -> Option<&str> {
        self.edit.as_ref().map(|e| e.tid.as_str())
    }

    fn dragging_key(&self) -> Option<&str> {
        self.grip.as_ref().filter(|g| g.active).map(|g| g.key.as_str())
    }

    /// Rect (window coordinates) of the row `key` at `now`; the dragged row follows the pointer.
    fn row_rect(&self, key: &str, now: Ms) -> Option<Rect> {
        if let Some(g) = &self.grip {
            if g.active && g.key == key {
                return Some(Rect::new(self.frame.x, self.frame.y + g.y, self.w, ROW_H));
            }
        }
        let (_, fx) = self.list.get(key, now)?;
        Some(Rect::new(self.frame.x + fx.dx, self.frame.y + fx.y, self.w, ROW_H))
    }

    fn list_rect(&self, now: Ms) -> Rect {
        self.list
            .bounds(now, self.frame.x, self.w, |_| ROW_H)
            .translate(0.0, self.frame.y)
            .inflate(6.0, 2.0)
    }

    fn drag_damage_rect(&self) -> Rect {
        match &self.grip {
            Some(g) if g.active => Rect::new(self.frame.x, self.frame.y + g.y, self.w, ROW_H).inflate(8.0, 3.0),
            _ => Rect::default(),
        }
    }

    fn model_index(cx: &Cx, tid: &str) -> Option<usize> {
        cx.store.state().queue.iter().position(|t| t.tid == tid)
    }

    fn fade_index(&mut self, key: &str, cx: &Cx) -> usize {
        if let Some(i) = self.fades.iter().position(|f| f.key == key) {
            return i;
        }
        self.fades.push(RowFade::new(key, cx.env.accent, cx.env.accent_text()));
        self.fades.len() - 1
    }

    /// Retargets the hover fades of `key` (`rowBg`, grip and buttons opacity).
    fn retarget(&mut self, cx: &mut Cx, key: &str, hovered: bool) {
        let dragging = self.dragging_key() == Some(key);
        let i = self.fade_index(key, cx);
        let f = &mut self.fades[i];
        let bg = if dragging {
            theme::FILL
        } else if hovered {
            theme::HOVER
        } else {
            Color::TRANSPARENT
        };
        f.bg.animate_to(bg, 120.0, cx.now);
        f.grip
            .animate_to(if hovered || dragging { 0.9 } else { 0.25 }, 140.0, Easing::Linear, cx.now);
        f.acts
            .animate_to(if hovered { 1.0 } else { 0.0 }, 140.0, Easing::Linear, cx.now);
        if !hovered {
            f.target.reset(cx);
            f.trash.reset(cx);
        }
    }

    fn set_hover(&mut self, cx: &mut Cx, new: Option<String>) {
        if self.hover == new {
            return;
        }
        if let Some(old) = self.hover.take() {
            self.retarget(cx, &old, false);
        }
        if let Some(k) = new {
            self.retarget(cx, &k, true);
            self.hover = Some(k);
        }
    }

    /// Row under `pos` (live rows only), as a borrowed key.
    fn row_at(&self, pos: Point, now: Ms) -> Option<&str> {
        self.list
            .iter(now)
            .filter(|(e, _)| e.index.is_some())
            .find(|(_, fx)| Rect::new(self.frame.x + fx.dx, self.frame.y + fx.y, self.w, ROW_H).contains(pos))
            .map(|(e, _)| e.key.as_str())
    }

    fn update_hover_at(&mut self, cx: &mut Cx, pos: Point) {
        let changed = {
            let hit = self.row_at(pos, cx.now);
            if hit == self.hover.as_deref() {
                None
            } else {
                Some(hit.map(str::to_string))
            }
        };
        if let Some(h) = changed {
            self.set_hover(cx, h);
        }
    }

    fn resync(&mut self, cx: &mut Cx) {
        self.rev = cx.rev;
        self.synced = true;
        let st = cx.store.state();
        self.n = st.queue.len();
        let w = self.w;
        let tw = title_width(w);
        let ts = title_style();
        let text = &mut *cx.text;
        let Self { list, grip, .. } = self;
        let dragging = grip.as_ref().filter(|g| g.active).map(|g| g.key.as_str());
        let items = st.queue.iter().enumerate().map(|(i, t)| {
            let data = RowData {
                tid: t.tid.clone(),
                elided: text.elide(&t.title, &ts, tw),
                num: (i + 1).to_string(),
                spent: (t.acc_ms > 60_000).then(|| fmt_short(t.acc_ms)),
                first: i == 0,
            };
            (t.tid.as_str(), data, i as f32 * (ROW_H + SPACING))
        });
        let changed = list.sync(items, cx.now, dragging);
        if changed {
            let b = self.list_rect(cx.now);
            cx.damage.add(self.last_bounds.union(&b));
            self.last_bounds = b;
            self.was_animating = true;
        } else {
            // same slots, but a row's text may have changed (rename committed by a focus change
            // while the pointer is elsewhere): repaint the list
            let b = self.list_rect(cx.now);
            cx.damage.add(b);
        }
        // rows that vanished under an interaction cancel it
        if let Some(g) = &mut self.grip {
            match st.queue.iter().position(|t| t.tid == g.key) {
                Some(i) => g.index = i,
                None => {
                    cx.damage.add(self.drag_damage_rect());
                    self.grip = None;
                    if matches!(self.active, Active::Btn(_)) {
                        self.active = Active::None;
                    }
                }
            }
        }
        if let Some(e) = &self.edit {
            if !st.queue.iter().any(|t| t.tid == e.tid) {
                if *cx.focus == FocusId::Rename(e.tid.clone()) {
                    *cx.focus = FocusId::None;
                }
                self.edit = None;
                self.active = Active::None;
            }
        }
        if let Some(h) = &self.hover {
            if !st.queue.iter().any(|t| t.tid == *h) {
                self.hover = None;
            }
        }
    }

    fn commit_edit(&mut self, cx: &mut Cx) {
        let Some(e) = self.edit.take() else { return };
        if matches!(&*cx.focus, FocusId::Rename(t) if *t == e.tid) {
            *cx.focus = FocusId::None;
        }
        if self.active == Active::Editor {
            self.active = Active::None;
        }
        if let Some(r) = self.row_rect(&e.tid, cx.now) {
            cx.damage.add(r);
        }
        if self.editor.text != e.orig {
            if let Some(index) = Self::model_index(cx, &e.tid) {
                cx.msgs
                    .push(Msg::Act(Action::RenameQueue { index, title: self.editor.text.clone() }));
            }
        }
    }

    fn cancel_edit(&mut self, cx: &mut Cx) {
        let Some(e) = self.edit.take() else { return };
        if matches!(&*cx.focus, FocusId::Rename(t) if *t == e.tid) {
            *cx.focus = FocusId::None;
        }
        if self.active == Active::Editor {
            self.active = Active::None;
        }
        if let Some(r) = self.row_rect(&e.tid, cx.now) {
            cx.damage.add(r);
        }
    }

    fn start_edit(&mut self, cx: &mut Cx, tid: &str) {
        if self.edit.as_ref().is_some_and(|e| e.tid == tid) {
            return;
        }
        self.commit_edit(cx);
        let Some(t) = cx.store.state().queue.iter().find(|t| t.tid == tid) else { return };
        self.editor.set_text(&t.title);
        self.editor.select_all();
        self.edit = Some(Edit { tid: tid.to_string(), orig: t.title.clone(), seen_active: false });
        *cx.focus = FocusId::Rename(tid.to_string());
        if let Some(r) = self.row_rect(tid, cx.now) {
            cx.damage.add(r);
        }
    }

    /// Onset of the drag (> 10 px): the row leaves its slot and follows the pointer.
    fn begin_drag(&mut self, cx: &mut Cx) {
        let Some(g) = &mut self.grip else { return };
        g.active = true;
        let key = g.key.clone();
        self.retarget(cx, &key, true);
        cx.damage.add(self.drag_damage_rect());
    }

    fn drag_to(&mut self, cx: &mut Cx, pointer_y: f32) {
        let max_y = (self.height() - ROW_H + 4.0).max(-4.0);
        let n = self.n;
        let old = self.drag_damage_rect();
        let Some(g) = &mut self.grip else { return };
        g.y = (g.row0_y + (pointer_y - self.frame.y - g.press_y)).clamp(-4.0, max_y);
        // `DropArea` slots are the 40 px rows (the 2 px gaps belong to none)
        let centre = g.y + ROW_H * 0.5;
        let slot = (centre >= 0.0)
            .then(|| (centre / (ROW_H + SPACING)).floor() as usize)
            .filter(|&s| s < n && centre - s as f32 * (ROW_H + SPACING) < ROW_H);
        if slot != g.last_in {
            g.last_in = slot;
            if let Some(s) = slot {
                if s != g.index {
                    cx.msgs.push(Msg::Act(Action::MoveQueue { from: g.index, to: s }));
                    g.index = s;
                }
            }
        }
        cx.cursor = CursorShape::ClosedHand;
        let new = self.drag_damage_rect();
        cx.damage.add(old);
        cx.damage.add(new);
    }

    fn end_drag(&mut self, cx: &mut Cx) {
        let Some(g) = self.grip.take() else { return };
        if g.active {
            // snaps back to the slot at once: repaint both places
            let drag = Rect::new(self.frame.x, self.frame.y + g.y, self.w, ROW_H).inflate(8.0, 3.0);
            cx.damage.add(drag);
            if let Some(r) = self.row_rect(&g.key, cx.now) {
                cx.damage.add(r.inflate(8.0, 3.0));
            }
            self.retarget(cx, &g.key, self.hover.as_deref() == Some(g.key.as_str()));
        }
    }

    fn grip_rect(r: Rect) -> Rect {
        Rect::new(r.x, r.y, GRIP_W, ROW_H)
    }
    fn title_rect(r: Rect, lh: f32) -> Rect {
        Rect::new(r.x + TITLE_X, r.y + (ROW_H - lh) * 0.5, title_width(r.w), lh)
    }

    /// Feeds a pointer event to the buttons of fade `fi`. Returns (handled, clicked button).
    fn buttons_event(&mut self, cx: &mut Cx, fi: usize, ev: &Event) -> (Handled, Option<u8>) {
        let f = &mut self.fades[fi];
        let a = f.target.pointer(cx, ev);
        let b = f.trash.pointer(cx, ev);
        let handled = if a.handled == Handled::Grab || b.handled == Handled::Grab {
            Handled::Grab
        } else if a.handled == Handled::Yes || b.handled == Handled::Yes {
            Handled::Yes
        } else {
            Handled::No
        };
        let clicked = if a.clicked {
            Some(0)
        } else if b.clicked {
            Some(1)
        } else {
            None
        };
        (handled, clicked)
    }

    fn button_action(&mut self, cx: &mut Cx, key: &str, which: u8) {
        let Some(index) = Self::model_index(cx, key) else { return };
        let a = if which == 0 { Action::FocusFromQueue { index, silent: false } } else { Action::RemoveQueue(index) };
        cx.msgs.push(Msg::Act(a));
    }

    fn editor_is_mine(&self, cx: &Cx) -> bool {
        match (&self.edit, &*cx.focus) {
            (Some(e), FocusId::Rename(t)) => *t == e.tid,
            _ => false,
        }
    }

    /// `fx` = (opacity, scale, dragged).
    fn paint_row(&self, s: &mut dyn Surface, pc: &PaintCx, data: &RowData, r: Rect, fx: (f32, f32, bool)) {
        let (opacity, scale, dragged) = fx;
        let now = pc.now;
        s.save();
        if opacity < 1.0 {
            s.mul_opacity(opacity);
        }
        let c = r.center();
        // the slot item scales about its own centre; the dragged row (1.02) about the row's
        let scale = if dragged { 1.02 } else { scale };
        if scale != 1.0 {
            s.translate(c.x, c.y);
            s.scale(scale, scale);
            s.translate(-c.x, -c.y);
        }
        let fade = self.fades.iter().find(|f| f.key == data.tid);
        let bg = fade.map_or(Color::TRANSPARENT, |f| f.bg.value(now));
        if bg.a > 0.0 {
            s.fill_rrect(r, 12.0, &Fill::Solid(bg));
        }
        if dragged {
            s.stroke_rrect(r.inflate(-0.5, -0.5), 11.5, &Fill::Solid(pc.env.accent.with_alpha(0.5)), 1.0);
        }
        // grip
        let g = fade.map_or(0.25, |f| f.grip.value(now));
        s.save();
        s.mul_opacity(g);
        paint_icon(s, Icon::Grip, Point::new(r.x + GRIP_W * 0.5, c.y), 14.0, theme::TEXT2, DEFAULT_STROKE);
        s.restore();
        // number
        paint_text_centered(s, &data.num, r.x + NUM_X, c.y, &num_style(data.first, pc.env.accent_text()));
        let editing = self.edit.as_ref().is_some_and(|e| e.tid == data.tid);
        let hovered = self.hover.as_deref() == Some(data.tid.as_str());
        if editing {
            let active = pc.window_focused && matches!(pc.focus, FocusId::Rename(t) if *t == data.tid);
            self.editor.paint(s, pc, active, pc.env.accent.with_alpha(0.6), None);
        } else {
            paint_text_centered(s, &data.elided, r.x + TITLE_X, c.y, &title_style());
            if let (Some(sp), false) = (&data.spent, hovered) {
                let st = spent_style();
                let w = s.measure(sp, &st);
                paint_text_centered(s, sp, r.right() - 12.0 - w, c.y, &st);
            }
            if let Some(f) = fade {
                let a = f.acts.value(now);
                if a > 0.004 {
                    s.save();
                    s.mul_opacity(a);
                    f.target.paint(s, now);
                    f.trash.paint(s, now);
                    s.restore();
                }
            }
        }
        s.restore();
    }
}

impl View for QueueSection {
    fn update(&mut self, cx: &mut Cx) {
        let now = cx.now;
        if cx.env.accent != self.accent {
            self.accent = cx.env.accent;
            for f in &mut self.fades {
                f.restyle(cx.env.accent, cx.env.accent_text());
            }
            cx.damage.add(self.frame.inflate(8.0, 4.0));
        }
        if !self.synced || cx.rev != self.rev {
            self.resync(cx);
        }
        // rename: focus stolen or window deactivated commits (QueueList.qml:157)
        if let Some(e) = &mut self.edit {
            let mine = matches!(&*cx.focus, FocusId::Rename(t) if *t == e.tid);
            if mine && cx.window_focused {
                e.seen_active = true;
            }
            if !mine || (e.seen_active && !cx.window_focused) {
                self.commit_edit(cx);
            }
        }
        self.editor_active = self.edit.is_some() && self.editor_is_mine(cx) && cx.window_focused;
        if self.edit.is_some() {
            self.editor.tick(cx, self.editor_active);
        }
        // transitions: union of the list before and after, plus one trailing repaint
        let animating = self.list.is_animating(now);
        if animating || self.was_animating {
            let b = self.list_rect(now);
            cx.damage.add(self.last_bounds.union(&b));
            self.last_bounds = b;
            if !animating {
                self.list.prune(now);
            }
        }
        self.was_animating = animating;
        // hover fades damage their own row
        let frame = self.frame;
        let w = self.w;
        for f in &mut self.fades {
            let bg = f.bg.value(now);
            let g = f.grip.value(now);
            let a = f.acts.value(now);
            if bg != f.shown.0 || (g - f.shown.1).abs() >= 1.0 / 255.0 || (a - f.shown.2).abs() >= 1.0 / 255.0 {
                f.shown = (bg, g, a);
                if let Some((_, fx)) = self.list.get(&f.key, now) {
                    cx.damage.add(Rect::new(frame.x + fx.dx, frame.y + fx.y, w, ROW_H));
                }
            }
            f.target.tick(cx);
            f.trash.tick(cx);
        }
        let hover = &self.hover;
        let dragging = self.grip.as_ref().filter(|g| g.active).map(|g| g.key.as_str());
        self.fades.retain(|f| {
            hover.as_deref() == Some(f.key.as_str()) || dragging == Some(f.key.as_str()) || f.animating(now)
        });
    }

    fn layout(&mut self, cx: &mut Cx, frame: Rect) {
        if frame != self.frame {
            let old = self.frame;
            if (frame.w - self.w).abs() > f32::EPSILON {
                // titles are elided to the row width: re-sync at the next update
                self.w = frame.w;
                self.rev = u64::MAX;
            }
            self.frame = frame;
            cx.damage.add_moved(old.inflate(8.0, 4.0), frame.inflate(8.0, 4.0));
        }
        let now = cx.now;
        // buttons follow their rows
        for i in 0..self.fades.len() {
            let Some(r) = self.row_rect(&self.fades[i].key, now) else { continue };
            let bx = r.right() - ACTS_RIGHT - ACTS_W;
            let by = r.y + (ROW_H - 28.0) * 0.5;
            let f = &mut self.fades[i];
            f.target.layout(cx, Rect::new(bx, by, 28.0, 28.0));
            f.trash.layout(cx, Rect::new(bx + 30.0, by, 28.0, 28.0));
        }
        if let Some(e) = &self.edit {
            if let Some(r) = self.row_rect(&e.tid, now) {
                let lh = cx.text.line_height(self.editor.style());
                let x = r.x + TITLE_X;
                let rect = Rect::new(x, r.y + (ROW_H - lh) * 0.5, (r.right() - 12.0 - x).max(0.0), lh);
                self.editor.layout(cx, rect);
            }
        }
    }

    fn event(&mut self, cx: &mut Cx, ev: &Event) -> Handled {
        let now = cx.now;
        match ev {
            Event::Key(k) => {
                if !self.editor_is_mine(cx) {
                    return Handled::No;
                }
                match self.editor.key(cx, k) {
                    InputOutcome::Submit { .. } => self.commit_edit(cx),
                    InputOutcome::Escape => self.cancel_edit(cx),
                    InputOutcome::Paste => cx.msgs.push(Msg::RequestPaste),
                    InputOutcome::Changed | InputOutcome::None => {}
                }
                Handled::Yes
            }
            Event::Paste(text) => {
                if !self.editor_is_mine(cx) {
                    return Handled::No;
                }
                self.editor.paste(cx, text);
                Handled::Yes
            }
            Event::PointerLeave => {
                if self.active == Active::None && self.grip.is_none() {
                    self.set_hover(cx, None);
                }
                Handled::No
            }
            Event::PointerMove { pos } => {
                if let Some(g) = &mut self.grip {
                    if !g.active && (pos.y - self.frame.y - g.press_y).abs() > DRAG_THRESHOLD {
                        self.begin_drag(cx);
                    }
                    if self.grip.as_ref().is_some_and(|g| g.active) {
                        self.drag_to(cx, pos.y);
                    }
                    return Handled::Yes;
                }
                if self.edit.is_some() {
                    let h = self.editor.pointer(cx, ev);
                    if self.active == Active::Editor {
                        return h;
                    }
                }
                if let Active::Btn(key) = self.active.clone() {
                    if let Some(fi) = self.fades.iter().position(|f| f.key == key) {
                        self.buttons_event(cx, fi, ev);
                    }
                    return Handled::Yes;
                }
                self.update_hover_at(cx, *pos);
                if let Some(fi) = self.hover.as_deref().and_then(|k| self.fades.iter().position(|f| f.key == k)) {
                    if self.edit.is_none() {
                        self.buttons_event(cx, fi, ev);
                    }
                    if cx.cursor == CursorShape::Arrow {
                        if let Some(r) = self.row_rect(&self.fades[fi].key, now) {
                            if Self::grip_rect(r).contains(*pos) {
                                cx.cursor = CursorShape::OpenHand;
                            }
                        }
                    }
                }
                Handled::No
            }
            Event::PointerDown { pos, button: Button::Left, clicks } => {
                // the editor first (it sits on top of its row)
                if self.edit.is_some() && self.editor.rect().contains(*pos) {
                    let h = self.editor.pointer(cx, ev);
                    if h == Handled::Grab {
                        self.active = Active::Editor;
                    }
                    return h;
                }
                self.update_hover_at(cx, *pos);
                let Some(key) = self.hover.clone().filter(|k| self.row_at(*pos, now) == Some(k.as_str())) else {
                    return Handled::No;
                };
                let Some(r) = self.row_rect(&key, now) else { return Handled::No };
                if self.edit.is_none() {
                    if let Some(fi) = self.fades.iter().position(|f| f.key == key) {
                        let (h, _) = self.buttons_event(cx, fi, ev);
                        if h == Handled::Grab {
                            self.active = Active::Btn(key);
                            return Handled::Grab;
                        }
                    }
                }
                if Self::grip_rect(r).contains(*pos) {
                    let Some(index) = Self::model_index(cx, &key) else { return Handled::No };
                    self.grip = Some(Grip {
                        key,
                        index,
                        last_in: Some(index),
                        press_y: pos.y - self.frame.y,
                        row0_y: r.y - self.frame.y,
                        y: r.y - self.frame.y,
                        active: false,
                    });
                    cx.cursor = CursorShape::OpenHand;
                    return Handled::Grab;
                }
                let lh = cx.text.line_height(&title_style());
                if *clicks >= 2 && self.edit.is_none() && Self::title_rect(r, lh).contains(*pos) {
                    self.start_edit(cx, &key);
                    return Handled::Yes;
                }
                if Self::title_rect(r, lh).contains(*pos) {
                    return Handled::Yes;
                }
                Handled::No
            }
            Event::PointerUp { pos, button: Button::Left } => {
                if self.grip.is_some() {
                    self.end_drag(cx);
                    self.update_hover_at(cx, *pos);
                    return Handled::Yes;
                }
                if let Active::Btn(key) = std::mem::replace(&mut self.active, Active::None) {
                    if let Some(fi) = self.fades.iter().position(|f| f.key == key) {
                        let (_, clicked) = self.buttons_event(cx, fi, ev);
                        if let Some(which) = clicked {
                            self.button_action(cx, &key, which);
                        }
                    }
                    self.update_hover_at(cx, *pos);
                    return Handled::Yes;
                }
                if self.active == Active::Editor {
                    self.active = Active::None;
                    return self.editor.pointer(cx, ev);
                }
                Handled::No
            }
            _ => Handled::No,
        }
    }

    fn paint(&mut self, s: &mut dyn Surface, pc: &PaintCx) {
        if self.frame.h <= 0.0 && self.last_bounds.is_empty() {
            return;
        }
        let now = pc.now;
        let dragging = self.dragging_key();
        let mut dragged: Option<(&RowData, Rect)> = None;
        for (e, fx) in self.list.iter(now) {
            if dragging == Some(e.key.as_str()) {
                if let Some(r) = self.row_rect(&e.key, now) {
                    dragged = Some((&e.data, r));
                }
                continue;
            }
            let r = Rect::new(self.frame.x + fx.dx, self.frame.y + fx.y, self.w, ROW_H);
            if !r.inflate(8.0, 4.0).intersects(&pc.clip) || fx.opacity <= 0.0 {
                continue;
            }
            self.paint_row(s, pc, &e.data, r, (fx.opacity, fx.scale, false));
        }
        if let Some((d, r)) = dragged {
            self.paint_row(s, pc, d, r, (1.0, 1.0, true));
        }
    }

    fn next_wake(&self, now: Ms) -> Wake {
        let mut w = self.list.next_wake(now);
        if self.fades.iter().any(|f| f.animating(now)) {
            w = Wake::Frame;
        }
        if self.edit.is_some() {
            w = w.merge_at(self.editor.next_wake(now, self.editor_active), now);
        }
        w
    }

    fn bounds(&self) -> Rect {
        self.frame.inflate(26.0, 6.0).union(&self.last_bounds)
    }
}

impl Section for QueueSection {
    fn height(&self) -> f32 {
        if self.n == 0 {
            0.0
        } else {
            self.n as f32 * ROW_H + (self.n - 1) as f32 * SPACING
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::Harness;
    use foco_base::{Key, KeyEvent, Mods, Task};
    use std::path::PathBuf;

    fn queue_h(titles: &[&str]) -> Harness {
        let mut h = Harness::new();
        for (i, t) in titles.iter().enumerate() {
            let mut task = Task::new(format!("t{i}"), t, 0.0);
            if i == 1 {
                task.acc_ms = 125_000;
            }
            h.store.state.queue.push(task);
        }
        h
    }

    const FRAME: Rect = Rect::new(18.0, 100.0, 512.0, 166.0);

    fn step(q: &mut QueueSection, h: &mut Harness, now: Ms) {
        h.with_cx(now, |cx| {
            q.update(cx);
            q.layout(cx, Rect::new(FRAME.x, FRAME.y, FRAME.w, q.height()));
        });
    }

    fn mv(x: f32, y: f32) -> Event {
        Event::PointerMove { pos: Point::new(x, y) }
    }
    fn down(x: f32, y: f32, clicks: u8) -> Event {
        Event::PointerDown { pos: Point::new(x, y), button: Button::Left, clicks }
    }
    fn up(x: f32, y: f32) -> Event {
        Event::PointerUp { pos: Point::new(x, y), button: Button::Left }
    }
    fn ev(q: &mut QueueSection, h: &mut Harness, now: Ms, e: &Event) -> Handled {
        h.with_cx(now, |cx| q.event(cx, e))
    }
    fn acts(h: &mut Harness) -> Vec<Action> {
        h.take_msgs()
            .into_iter()
            .filter_map(|m| if let Msg::Act(a) = m { Some(a) } else { None })
            .collect()
    }
    fn row_y(i: usize) -> f32 {
        FRAME.y + i as f32 * 42.0
    }

    #[test]
    fn section_heights() {
        let mut h = queue_h(&[]);
        let mut q = QueueSection::new();
        step(&mut q, &mut h, 0.0);
        assert_eq!(q.height(), 0.0);
        assert!(!q.shown());
        for n in 1..=4usize {
            let titles: Vec<&str> = vec!["x"; n];
            let mut h = queue_h(&titles);
            let mut q = QueueSection::new();
            step(&mut q, &mut h, 0.0);
            assert_eq!(q.height(), n as f32 * 40.0 + (n - 1) as f32 * 2.0);
        }
    }

    #[test]
    fn drag_emits_live_move_queue() {
        let mut h = queue_h(&["a", "b", "c", "d"]);
        let mut q = QueueSection::new();
        step(&mut q, &mut h, 0.0);
        // press the grip of row 0
        let gx = FRAME.x + 14.0;
        let y0 = row_y(0) + 20.0;
        ev(&mut q, &mut h, 0.0, &mv(gx, y0));
        assert_eq!(ev(&mut q, &mut h, 0.0, &down(gx, y0, 1)), Handled::Grab);
        // below the 10 px threshold nothing happens
        ev(&mut q, &mut h, 5.0, &mv(gx, y0 + 6.0));
        assert!(acts(&mut h).is_empty());
        // centre enters slot 1 (centre y = 20 + 42)
        ev(&mut q, &mut h, 10.0, &mv(gx, y0 + 42.0));
        assert_eq!(acts(&mut h), vec![Action::MoveQueue { from: 0, to: 1 }]);
        // staying inside slot 1 is silent; entering slot 2 moves again
        ev(&mut q, &mut h, 20.0, &mv(gx, y0 + 46.0));
        assert!(acts(&mut h).is_empty());
        ev(&mut q, &mut h, 30.0, &mv(gx, y0 + 84.0));
        assert_eq!(acts(&mut h), vec![Action::MoveQueue { from: 1, to: 2 }]);
        // the gap between slots is not a slot
        ev(&mut q, &mut h, 40.0, &mv(gx, y0 + 84.0 - 21.0)); // centre at 83: slot 1 starts at 42..82, gap 82..84
        assert!(acts(&mut h).is_empty(), "gap");
        // back into slot 1
        ev(&mut q, &mut h, 50.0, &mv(gx, y0 + 42.0));
        assert_eq!(acts(&mut h), vec![Action::MoveQueue { from: 2, to: 1 }]);
        // far below: clamped to the list end (+4)
        ev(&mut q, &mut h, 60.0, &mv(gx, y0 + 1000.0));
        assert_eq!(acts(&mut h), vec![Action::MoveQueue { from: 1, to: 3 }]);
        let g = q.grip.as_ref().unwrap();
        assert_eq!(g.y, q.height() - 40.0 + 4.0);
        assert_eq!(ev(&mut q, &mut h, 70.0, &up(gx, y0 + 1000.0)), Handled::Yes);
        assert!(q.grip.is_none());
    }

    #[test]
    fn drag_reorder_follows_the_model() {
        let mut h = queue_h(&["a", "b", "c"]);
        let mut q = QueueSection::new();
        step(&mut q, &mut h, 0.0);
        let gx = FRAME.x + 14.0;
        let y0 = row_y(0) + 20.0;
        ev(&mut q, &mut h, 0.0, &down(gx, y0, 1));
        ev(&mut q, &mut h, 1.0, &mv(gx, y0 + 42.0));
        let a = acts(&mut h);
        assert_eq!(a, vec![Action::MoveQueue { from: 0, to: 1 }]);
        // apply like the store would
        let t = h.store.state.queue.remove(0);
        h.store.state.queue.insert(1, t);
        h.rev += 1;
        step(&mut q, &mut h, 2.0);
        assert_eq!(q.grip.as_ref().unwrap().index, 1);
        // "b" is displaced up over 220 ms OutCubic; the dragged "a" does not animate
        let (_, fa) = q.list.get("t0", 2.0).unwrap();
        assert_eq!(fa.y, 42.0);
        let (_, fb) = q.list.get("t1", 2.0).unwrap();
        assert_eq!(fb.y, 42.0, "starts where it was");
        let (_, fb) = q.list.get("t1", 2.0 + 220.0).unwrap();
        assert_eq!(fb.y, 0.0);
        assert_eq!(q.next_wake(100.0), Wake::Frame);
        assert_eq!(q.next_wake(300.0), Wake::Idle);
    }

    #[test]
    fn hover_buttons_emit_actions() {
        let mut h = queue_h(&["a", "b", "c"]);
        let mut q = QueueSection::new();
        step(&mut q, &mut h, 0.0);
        let r1 = Rect::new(FRAME.x, row_y(1), 512.0, 40.0);
        let bx = r1.right() - 6.0 - 58.0;
        // hover row 1 then the "target" button
        ev(&mut q, &mut h, 10.0, &mv(r1.x + 100.0, r1.y + 20.0));
        step(&mut q, &mut h, 10.0);
        assert_eq!(q.hover.as_deref(), Some("t1"));
        ev(&mut q, &mut h, 20.0, &mv(bx + 14.0, r1.y + 20.0));
        assert_eq!(h.cursor, CursorShape::Hand);
        assert_eq!(ev(&mut q, &mut h, 30.0, &down(bx + 14.0, r1.y + 20.0, 1)), Handled::Grab);
        ev(&mut q, &mut h, 40.0, &up(bx + 14.0, r1.y + 20.0));
        assert_eq!(acts(&mut h), vec![Action::FocusFromQueue { index: 1, silent: false }]);
        // trash
        ev(&mut q, &mut h, 50.0, &mv(bx + 44.0, r1.y + 20.0));
        ev(&mut q, &mut h, 60.0, &down(bx + 44.0, r1.y + 20.0, 1));
        ev(&mut q, &mut h, 70.0, &up(bx + 44.0, r1.y + 20.0));
        assert_eq!(acts(&mut h), vec![Action::RemoveQueue(1)]);
        // release outside does not click
        ev(&mut q, &mut h, 80.0, &down(bx + 44.0, r1.y + 20.0, 1));
        ev(&mut q, &mut h, 90.0, &up(bx - 100.0, r1.y + 20.0));
        assert!(acts(&mut h).is_empty());
    }

    fn type_text(q: &mut QueueSection, h: &mut Harness, s: &str) {
        for c in s.chars() {
            let t = c.to_string();
            let k = Event::Key(KeyEvent { key: Key::Char(c), mods: Mods::NONE, text: Some(t) });
            ev(q, h, 0.0, &k);
        }
    }
    fn key(q: &mut QueueSection, h: &mut Harness, k: Key) -> Handled {
        ev(q, h, 0.0, &Event::Key(KeyEvent { key: k, mods: Mods::NONE, text: None }))
    }

    fn start_rename(q: &mut QueueSection, h: &mut Harness, row: usize) {
        // title area of the row
        let p = Point::new(FRAME.x + 100.0, row_y(row) + 20.0);
        ev(q, h, 0.0, &mv(p.x, p.y));
        assert_ne!(ev(q, h, 0.0, &down(p.x, p.y, 1)), Handled::Grab);
        assert!(q.edit.is_none(), "single click does not rename");
        ev(q, h, 0.0, &down(p.x, p.y, 2));
        step(q, h, 0.0);
    }

    #[test]
    fn rename_commit_and_cancel() {
        let mut h = queue_h(&["alpha", "beta"]);
        let mut q = QueueSection::new();
        step(&mut q, &mut h, 0.0);
        start_rename(&mut q, &mut h, 1);
        assert_eq!(h.focus, FocusId::Rename("t1".into()));
        assert_eq!(q.editing(), Some("t1"));
        assert_eq!(q.editor.selected_text(), "beta");
        // typing replaces the selection; Enter commits
        type_text(&mut q, &mut h, "gama");
        assert_eq!(key(&mut q, &mut h, Key::Enter), Handled::Yes);
        assert_eq!(acts(&mut h), vec![Action::RenameQueue { index: 1, title: "gama".into() }]);
        assert_eq!(h.focus, FocusId::None);
        assert!(q.edit.is_none());
        // Esc cancels without an action
        start_rename(&mut q, &mut h, 0);
        type_text(&mut q, &mut h, "zzz");
        assert_eq!(key(&mut q, &mut h, Key::Escape), Handled::Yes);
        assert!(acts(&mut h).is_empty());
        assert!(q.edit.is_none());
        assert_eq!(h.focus, FocusId::None);
        // keys with no editor are not consumed
        assert_eq!(key(&mut q, &mut h, Key::Escape), Handled::No);
    }

    #[test]
    fn rename_commits_on_blur() {
        let mut h = queue_h(&["alpha", "beta"]);
        let mut q = QueueSection::new();
        step(&mut q, &mut h, 0.0);
        start_rename(&mut q, &mut h, 0);
        type_text(&mut q, &mut h, "novo");
        step(&mut q, &mut h, 10.0);
        assert!(q.edit.is_some(), "still focused");
        // focus moves to the new-task field
        h.focus = FocusId::NewTask;
        step(&mut q, &mut h, 20.0);
        assert_eq!(acts(&mut h), vec![Action::RenameQueue { index: 0, title: "novo".into() }]);
        assert_eq!(h.focus, FocusId::NewTask, "someone else's focus is left alone");
        // window deactivation also commits
        start_rename(&mut q, &mut h, 1);
        step(&mut q, &mut h, 30.0);
        type_text(&mut q, &mut h, "x");
        h.window_focused = false;
        step(&mut q, &mut h, 40.0);
        assert_eq!(acts(&mut h), vec![Action::RenameQueue { index: 1, title: "x".into() }]);
    }

    #[test]
    fn rename_applied_repaints_the_row() {
        // QA: the title changed after a blur-commit while the pointer was elsewhere and the row
        // stayed stale until the next hover
        let mut h = queue_h(&["alpha", "beta"]);
        let mut q = QueueSection::new();
        step(&mut q, &mut h, 0.0);
        let _ = h.damage.take();
        h.store.state.queue[0].title = "novo".into();
        h.rev += 1;
        step(&mut q, &mut h, 10.0);
        assert!(!h.damage.is_empty(), "a title change must damage the list");
    }

    #[test]
    fn wakes_idle_when_static_and_frame_while_fading() {
        let mut h = queue_h(&["a", "b"]);
        let mut q = QueueSection::new();
        step(&mut q, &mut h, 0.0);
        assert_eq!(q.next_wake(0.0), Wake::Idle);
        h.damage.clear();
        step(&mut q, &mut h, 16.0);
        assert!(h.damage.is_empty(), "nothing moves, nothing is damaged");
        ev(&mut q, &mut h, 1000.0, &mv(FRAME.x + 100.0, row_y(0) + 20.0));
        step(&mut q, &mut h, 1000.0);
        assert_eq!(q.next_wake(1000.0), Wake::Frame);
        h.damage.clear();
        step(&mut q, &mut h, 1050.0);
        assert!(!h.damage.is_empty());
        // damage stays near the hovered row
        for r in h.damage.rects() {
            assert!(r.y < ((row_y(0) + 60.0) * crate::cx::S) as i32, "{r:?}");
        }
        step(&mut q, &mut h, 1200.0);
        step(&mut q, &mut h, 1210.0);
        assert_eq!(q.next_wake(1210.0), Wake::Idle);
        // leaving fades back to idle
        ev(&mut q, &mut h, 2000.0, &Event::PointerLeave);
        step(&mut q, &mut h, 2000.0);
        assert_eq!(q.next_wake(2000.0), Wake::Frame);
        step(&mut q, &mut h, 2300.0);
        step(&mut q, &mut h, 2310.0);
        assert_eq!(q.next_wake(2310.0), Wake::Idle);
        assert!(q.fades.is_empty());
    }

    #[test]
    fn add_and_remove_transitions_run_then_settle() {
        let mut h = queue_h(&["a", "b"]);
        let mut q = QueueSection::new();
        step(&mut q, &mut h, 0.0);
        let t = Task::new("n".into(), "new", 0.0);
        h.store.state.queue.insert(0, t);
        h.rev += 1;
        step(&mut q, &mut h, 100.0);
        assert_eq!(q.next_wake(100.0), Wake::Frame);
        step(&mut q, &mut h, 100.0 + 345.0);
        step(&mut q, &mut h, 100.0 + 346.0);
        assert_eq!(q.next_wake(100.0 + 346.0), Wake::Idle);
        h.store.state.queue.remove(1);
        h.rev += 1;
        step(&mut q, &mut h, 1000.0);
        assert_eq!(q.next_wake(1000.0), Wake::Frame);
        step(&mut q, &mut h, 1265.0);
        step(&mut q, &mut h, 1266.0);
        assert_eq!(q.next_wake(1266.0), Wake::Idle);
    }

    fn out_dir() -> PathBuf {
        let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/ui-test/wave3c");
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn snapshot(q: &mut QueueSection, h: &Harness, now: Ms, name: &str) {
        let mut s = Harness::surface();
        let panel = Rect::new(0.0, 60.0, 548.0, 230.0);
        s.fill_rrect(Rect::new(0.0, 0.0, 548.0, 300.0), 32.0, &Fill::Solid(theme::NOTCH));
        s.fill_rrect(panel, 0.0, &Fill::Solid(Color::rgb8(0xe2, 0xe2, 0xe7)));
        let clip = Rect::new(0.0, 0.0, 760.0, 760.0);
        h.with_pc(now, clip, |pc| q.paint(&mut s, pc));
        s.save_png(out_dir().join(name)).unwrap();
    }

    #[test]
    fn png_snapshots() {
        let mut h =
            queue_h(&["Escrever o relatório", "Revisar o PR do Foquinho", "Pagar a conta de luz", "Marcar dentista"]);
        let mut q = QueueSection::new();
        let frame = |q: &mut QueueSection, h: &mut Harness, now| {
            h.with_cx(now, |cx| {
                q.update(cx);
                q.layout(cx, Rect::new(18.0, 100.0, 512.0, q.height()));
            })
        };
        frame(&mut q, &mut h, 0.0);
        // hover row 1
        ev(&mut q, &mut h, 10.0, &mv(150.0, row_y(1) + 20.0));
        frame(&mut q, &mut h, 10.0);
        frame(&mut q, &mut h, 400.0);
        snapshot(&mut q, &h, 400.0, "queue-4rows.png");
        // mid drag: row 1 grabbed and moved half way to row 2
        let gx = FRAME.x + 14.0;
        ev(&mut q, &mut h, 500.0, &down(gx, row_y(1) + 20.0, 1));
        ev(&mut q, &mut h, 510.0, &mv(gx, row_y(1) + 20.0 + 16.0));
        let a = acts(&mut h);
        assert!(a.is_empty());
        frame(&mut q, &mut h, 520.0);
        ev(&mut q, &mut h, 530.0, &mv(gx, row_y(1) + 20.0 + 44.0));
        assert_eq!(acts(&mut h), vec![Action::MoveQueue { from: 1, to: 2 }]);
        let t = h.store.state.queue.remove(1);
        h.store.state.queue.insert(2, t);
        h.rev += 1;
        frame(&mut q, &mut h, 540.0);
        snapshot(&mut q, &h, 640.0, "queue-mid-drag.png");
        ev(&mut q, &mut h, 700.0, &up(gx, row_y(1) + 64.0));
        frame(&mut q, &mut h, 710.0);
        frame(&mut q, &mut h, 1200.0);
        // rename editing
        start_rename(&mut q, &mut h, 3);
        type_text(&mut q, &mut h, "Marcar o dentista");
        frame(&mut q, &mut h, 1300.0);
        snapshot(&mut q, &h, 1300.0, "queue-rename.png");
    }
}
