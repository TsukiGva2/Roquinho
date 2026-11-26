//! `TipBubble.qml` (wave 3a): the shortcut tip that hangs under the notch, above the pet.
//!
//! Timeline after `set_shown(true)` at `t0` (every segment is computed from `t0`, nothing is
//! stepped): `appear` 0 -> 1 `OutBack` 1.8 over 520 ms after a 120 ms pause; the title types out
//! linearly over 900 ms starting when `appear` ends (`typing()` is true meanwhile, so the pet
//! talks); then `rowsT` 0 -> 1 over 700 ms staggers the four shortcut rows and the button row.
//! Hiding: `appear` -> 0 over 200 ms `InCubic`; `chars`/`rowsT` freeze where they were.
//! The bubble floats 0 <-> 2 px (1600 ms `InOutSine` each way) while shown.
//!
//! Damage: appear / float / content changes damage the transformed bubble extent (old u new);
//! a new typed character damages only the title line, the row stagger only the rows + buttons
//! block, and button hover / press only that button. The float is quantised to 0.5 px.
//!
//! Wakes: `At(t0 + 120)` before the pop, `Frame` while `appear` or the row stagger run,
//! `At(next character)` while typing, `At(next float level)` otherwise, `Idle` when hidden.

use super::collapsed::next_level_change;
use crate::cx::{CursorShape, Cx, Handled, Msg, PaintCx};
use crate::view::View;
use crate::widgets::{ColorAnim, MouseArea};
use foco_base::{
    path_from_svg, theme, Anim, Color, Easing, Event, Fill, Ms, Point, Rect, Size, Surface, TextStyle, Wake, Weight,
};
use std::f32::consts::PI;
use tiny_skia::{LineJoin, Path, Stroke};

const WIDTH: f32 = 318.0;
const TAIL_H: f32 = 9.0;
const PAD_X: f32 = 16.0;
const CONTENT_W: f32 = WIDTH - 32.0;
const TITLE_Y: f32 = TAIL_H + 14.0;
const ROW_H: f32 = 22.0;
const ROW_GAP: f32 = 5.0;
const ROWS_H: f32 = 4.0 * ROW_H + 3.0 * ROW_GAP;
const BTN_H: f32 = 30.0;
const RADIUS: f32 = 18.0;
const TITLE: &str = "Ô punk, lembra dos meus atalhos?";

const POP_DELAY: Ms = 120.0;
const POP_MS: Ms = 520.0;
const TYPE_MS: Ms = 900.0;
const ROWS_MS: Ms = 700.0;
const OUT_MS: Ms = 200.0;
/// One float half period; the float is `1 - cos(pi t / 1600)` (0 -> 2 -> 0).
const FLOAT_HALF_MS: f32 = 1600.0;
/// Float quantum, logical px. Every step repaints the whole bubble (~0.75 ms), so 0.5 px halves the idle cost of
/// 0.25 px while staying below one device pixel (S = 25/24) per step.
const FLOAT_STEP: f32 = 0.5;

/// `(key letter, description, command)` of the four rows.
const ROWS: [(&str, &str, &str); 4] = [
    ("N", "Nova demanda", "add"),
    ("F", "Abrir / fechar o painel", "toggle"),
    ("H", "Me esconder numa bolinha", "hide"),
    ("D", "Concluir o que está em foco", "done"),
];

fn title_style() -> TextStyle {
    TextStyle::new(13.0, Weight::DemiBold, theme::TEXT)
}
fn chip_style(c: Color) -> TextStyle {
    TextStyle::new(10.0, Weight::DemiBold, c)
}
fn label_style() -> TextStyle {
    TextStyle::new(12.0, Weight::Regular, theme::TEXT2)
}
fn never_style(c: Color) -> TextStyle {
    TextStyle::new(11.0, Weight::Regular, c)
}
fn ok_style() -> TextStyle {
    TextStyle::new(12.0, Weight::DemiBold, Color::WHITE)
}

#[derive(Clone, Debug, Default)]
struct KeyRow {
    keys: Vec<(String, f32)>,
    keys_w: f32,
}

/// Everything that decides what is drawn, to damage only on a visible change.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Sig {
    appear: f32,
    float_q: f32,
    chars: usize,
    cursor: bool,
    rows: f32,
    ok_scale: f32,
    ok_color: Color,
    never_hover: bool,
}

pub struct TipView {
    shown: bool,
    frame: Rect,
    t0: Ms,
    appear: Anim,
    /// Frozen `(chars, rowsT)` while hidden (QML stops the animation and keeps the values).
    frozen: Option<(f32, f32)>,
    /// Float phase: `theta = PI * (now - float_t0) / 1600`; `None` while not floating.
    float_t0: Option<Ms>,
    float_theta0: f32,
    typing: bool,
    // layout (local coordinates, origin = bubble top-left)
    lh_title: f32,
    lh_never: f32,
    height: f32,
    rows: [KeyRow; 4],
    ok_w: f32,
    never_w: f32,
    laid_out: bool,
    env_rev: u64,
    path: Option<Path>,
    // interaction
    ok_ma: MouseArea,
    never_ma: MouseArea,
    ok_scale: Anim,
    ok_color: ColorAnim,
    // damage
    sig: Option<Sig>,
    last_ext: Rect,
}

impl Default for TipView {
    fn default() -> Self {
        Self::new()
    }
}

impl TipView {
    /// X of the tail tip inside the bubble.
    pub const TAIL_X: f32 = 34.0;

    pub fn new() -> Self {
        TipView {
            shown: false,
            frame: Rect::default(),
            t0: 0.0,
            appear: Anim::new(0.0),
            frozen: None,
            float_t0: None,
            float_theta0: 0.0,
            typing: false,
            lh_title: 15.7,
            lh_never: 13.3,
            height: 15.7 + 190.0,
            rows: Default::default(),
            ok_w: 0.0,
            never_w: 0.0,
            laid_out: false,
            env_rev: u64::MAX,
            path: None,
            ok_ma: MouseArea::new(),
            never_ma: MouseArea::new(),
            ok_scale: Anim::new(1.0),
            ok_color: ColorAnim::new(Color::TRANSPARENT),
            sig: None,
            last_ext: Rect::default(),
        }
    }

    /// `shown`: restarts the pop-in (chars and rows reset) or plays the 200 ms fade-out.
    pub fn set_shown(&mut self, v: bool, now: Ms) {
        if v == self.shown {
            return;
        }
        if v {
            self.shown = true;
            self.t0 = now;
            self.frozen = None;
            self.appear
                .animate_to_delayed(1.0, POP_DELAY, POP_MS, Easing::OutBack { overshoot: 1.8 }, now);
            self.float_t0 = Some(now);
        } else {
            let (c, r) = self.chars_rows(now);
            self.frozen = Some((c, r));
            self.shown = false;
            self.typing = false;
            self.appear.animate_to(0.0, OUT_MS, Easing::InCubic, now);
            if let Some(t) = self.float_t0.take() {
                self.float_theta0 = self.theta(t, now);
            }
            self.ok_ma.reset();
            self.never_ma.reset();
        }
    }

    /// Bubble size (App uses it for the input region and `tail` placement).
    pub fn size(&self) -> Size {
        Size::new(WIDTH, self.height)
    }

    /// `typing`: the pet's mouth talks while the text types out.
    pub fn typing(&self) -> bool {
        self.typing
    }

    /// `opacity > 0.01` (`appear > 0.01`).
    pub fn visible(&self, now: Ms) -> bool {
        self.appear.value(now) > 0.01
    }

    // ---- timeline -------------------------------------------------------------------------

    fn type_start(&self) -> Ms {
        self.t0 + POP_DELAY + POP_MS
    }
    fn type_end(&self) -> Ms {
        self.type_start() + TYPE_MS
    }
    fn rows_end(&self) -> Ms {
        self.type_end() + ROWS_MS
    }

    /// `(chars, rowsT)` at `now`.
    fn chars_rows(&self, now: Ms) -> (f32, f32) {
        if let Some(f) = self.frozen {
            return f;
        }
        let n = TITLE.chars().count() as f32;
        let c = (((now - self.type_start()) / TYPE_MS) as f32).clamp(0.0, 1.0) * n;
        let r = (((now - self.type_end()) / ROWS_MS) as f32).clamp(0.0, 1.0);
        (c, r)
    }

    fn chars_shown(&self, now: Ms) -> usize {
        let n = TITLE.chars().count();
        (self.chars_rows(now).0.round() as usize).min(n)
    }

    fn theta(&self, t0: Ms, now: Ms) -> f32 {
        self.float_theta0 + PI * ((now - t0) as f32) / FLOAT_HALF_MS
    }

    /// Float offset in px, quantised.
    fn float_q(&self, now: Ms) -> f32 {
        if let Some(t0) = self.float_t0 {
            let y = 1.0 - self.theta(t0, now).cos();
            (y / FLOAT_STEP).round() * FLOAT_STEP
        } else {
            let y = 1.0 - self.float_theta0.cos();
            (y / FLOAT_STEP).round() * FLOAT_STEP
        }
    }

    fn scale_k(appear: f32) -> f32 {
        0.4 + 0.6 * appear
    }

    // ---- geometry -------------------------------------------------------------------------

    fn col_y(&self) -> f32 {
        TITLE_Y + self.lh_title + 9.0
    }
    fn btn_y(&self) -> f32 {
        self.col_y() + ROWS_H + 9.0
    }
    fn ok_rect(&self) -> Rect {
        Rect::new(PAD_X + CONTENT_W - self.ok_w, self.btn_y() + 1.0, self.ok_w, 28.0)
    }
    fn never_rect(&self) -> Rect {
        let h = self.lh_never;
        Rect::new(PAD_X, self.btn_y() + BTN_H * 0.5 - h * 0.5, self.never_w, h)
    }
    /// Area of the row stagger and the buttons (local).
    fn body_rect(&self) -> Rect {
        Rect::new(PAD_X - 12.0, self.col_y() - 1.0, CONTENT_W + 16.0, ROWS_H + 9.0 + BTN_H + 4.0)
    }
    fn title_rect(&self) -> Rect {
        Rect::new(PAD_X - 1.0, TITLE_Y - 1.0, CONTENT_W + 2.0, self.lh_title + 2.0)
    }
    /// Everything the bubble can paint (shadow included), local.
    fn extent_local(&self) -> Rect {
        Rect::new(-30.0, -2.0, WIDTH + 60.0, self.height + 2.0 + 8.0 + 30.0)
    }

    /// Local -> window bounding box at the given appear scale and float.
    fn to_win(&self, r: Rect, k: f32, fq: f32) -> Rect {
        let tx = Self::TAIL_X;
        Rect::new(self.frame.x + tx + k * (r.x - tx), self.frame.y + fq + k * r.y, r.w * k, r.h * k).inflate(1.0, 1.0)
    }

    /// Window -> local (inverse of the paint transform).
    fn to_local(&self, p: Point, k: f32, fq: f32) -> Point {
        let tx = Self::TAIL_X;
        let k = k.max(0.05);
        Point::new((p.x - self.frame.x - tx) / k + tx, (p.y - self.frame.y - fq) / k)
    }

    fn rebuild_layout(&mut self, cx: &mut Cx) {
        self.lh_title = cx.text.line_height(&title_style());
        self.lh_never = cx.text.line_height(&never_style(theme::TEXT3));
        let new_h = TAIL_H + (self.lh_title + 9.0 + ROWS_H + 9.0 + BTN_H) + 30.0;
        self.height = new_h;
        for (i, (letter, _, cmd)) in ROWS.iter().enumerate() {
            let keys = cx.env.keys_for(cmd, letter);
            let st = chip_style(theme::TEXT2);
            let mut row = KeyRow::default();
            for k in keys {
                let w = (cx.text.measure(&k, &st) + 10.0).max(19.0);
                row.keys_w += w;
                row.keys.push((k, w));
            }
            if row.keys.len() > 1 {
                row.keys_w += 3.0 * (row.keys.len() - 1) as f32;
            }
            self.rows[i] = row;
        }
        self.ok_w = cx.text.measure("Valeu, punk!", &ok_style()) + 28.0;
        self.never_w = cx.text.measure("Me deixa em paz", &never_style(theme::TEXT3));
        self.build_path();
        self.ok_ma.rect = self.ok_rect();
        self.never_ma.rect = self.never_rect().inflate(6.0, 6.0);
        self.laid_out = true;
    }

    /// The bubble outline: the QML `PathSvg` verbatim (w/h/t/r/x0 as in the QML).
    fn build_path(&mut self) {
        let (w, h, t, r, x0) = (WIDTH - 0.5, self.height - 0.5, TAIL_H + 0.5, RADIUS, Self::TAIL_X);
        let d = format!(
            "M {} {t} L {} {t} Q {} {t} {} 1.6 Q {x0} 0.2 {} 1.6 Q {} {t} {} {t} L {} {t} Q {w} {t} {w} {} L {w} {} \
             Q {w} {h} {} {h} L {} {h} Q 0.5 {h} 0.5 {} L 0.5 {} Q 0.5 {t} {} {t} Z",
            r + 0.5,
            x0 - 8.0,
            x0 - 2.0,
            x0 - 0.8,
            x0 + 0.8,
            x0 + 2.0,
            x0 + 8.0,
            w - r,
            t + r,
            h - r,
            w - r,
            r + 0.5,
            h - r,
            t + r,
            r + 0.5,
        );
        self.path = path_from_svg(&d);
    }

    fn signature(&self, now: Ms) -> Sig {
        let appear = self.appear.value(now);
        let n = TITLE.chars().count();
        let chars = self.chars_shown(now);
        Sig {
            appear,
            float_q: self.float_q(now),
            chars,
            cursor: chars < n && appear > 0.9,
            rows: self.chars_rows(now).1,
            ok_scale: self.ok_scale.value(now),
            ok_color: self.ok_color.value(now),
            never_hover: self.never_ma.hovered(),
        }
    }

    fn accent_hover(accent: Color) -> Color {
        accent.darker(1.12)
    }

    fn retarget_ok(&mut self, accent: Color, now: Ms) {
        let want = if self.ok_ma.hovered() { Self::accent_hover(accent) } else { accent };
        self.ok_color.animate_to(want, 120.0, now);
        self.ok_scale
            .animate_to(if self.ok_ma.pressed() { 0.92 } else { 1.0 }, 120.0, Easing::Linear, now);
    }
}

impl View for TipView {
    fn update(&mut self, cx: &mut Cx) {
        let now = cx.now;
        if !self.laid_out || cx.env.rev != self.env_rev {
            self.rebuild_layout(cx);
            self.env_rev = cx.env.rev;
            self.sig = None;
        }
        // an accent change re-targets the Ok colour (Behavior on color)
        if self.ok_color.target().a == 0.0 {
            self.ok_color.set(cx.env.accent);
        } else if !self.ok_ma.hovered() {
            self.ok_color.animate_to(cx.env.accent, 120.0, now);
        }
        // typing flag: true between the end of the pop and the end of the text
        self.typing = self.shown && now >= self.type_start() && now < self.type_end();

        let visible = self.visible(now);
        let new_sig = self.signature(now);
        let old = self.sig;
        if old == Some(new_sig) {
            return;
        }
        let k = Self::scale_k(new_sig.appear);
        let fq = new_sig.float_q;
        let wide = match old {
            None => true,
            Some(o) => o.appear != new_sig.appear || o.float_q != new_sig.float_q,
        };
        if wide {
            let ext = if visible { self.to_win(self.extent_local(), k, fq) } else { Rect::default() };
            let both = ext.union(&self.last_ext);
            if !both.is_empty() {
                cx.damage.add(both);
            }
            self.last_ext = ext;
        } else if visible {
            let o = old.unwrap_or(new_sig);
            if o.chars != new_sig.chars || o.cursor != new_sig.cursor {
                cx.damage.add(self.to_win(self.title_rect(), k, fq));
            }
            if o.rows != new_sig.rows {
                cx.damage.add(self.to_win(self.body_rect(), k, fq));
            }
            if o.ok_scale != new_sig.ok_scale || o.ok_color != new_sig.ok_color {
                cx.damage.add(self.to_win(self.ok_rect().inflate(3.0, 3.0), k, fq));
            }
            if o.never_hover != new_sig.never_hover {
                cx.damage.add(self.to_win(self.never_ma.rect.inflate(2.0, 2.0), k, fq));
            }
        }
        self.sig = Some(new_sig);
    }

    fn layout(&mut self, cx: &mut Cx, frame: Rect) {
        let frame = Rect::new(frame.x, frame.y, WIDTH, self.height);
        if frame != self.frame {
            if self.visible(cx.now) {
                let a = self.appear.value(cx.now);
                let k = Self::scale_k(a);
                let fq = self.float_q(cx.now);
                let old = self.to_win(self.extent_local(), k, fq);
                self.frame = frame;
                let new = self.to_win(self.extent_local(), k, fq);
                cx.damage.add(old.union(&new));
                self.last_ext = new;
            } else {
                self.frame = frame;
            }
        }
    }

    fn event(&mut self, cx: &mut Cx, ev: &Event) -> Handled {
        let now = cx.now;
        if !self.shown || !self.visible(now) {
            return Handled::No;
        }
        let k = Self::scale_k(self.appear.value(now));
        let fq = self.float_q(now);
        let local = match ev {
            Event::PointerMove { pos } => Event::PointerMove { pos: self.to_local(*pos, k, fq) },
            Event::PointerDown { pos, button, clicks } => {
                Event::PointerDown { pos: self.to_local(*pos, k, fq), button: *button, clicks: *clicks }
            }
            Event::PointerUp { pos, button } => Event::PointerUp { pos: self.to_local(*pos, k, fq), button: *button },
            Event::PointerLeave => Event::PointerLeave,
            _ => return Handled::No,
        };
        let accent = cx.env.accent;
        let ok = self.ok_ma.event(&local);
        let never = self.never_ma.event(&local);
        if ok.changed {
            self.retarget_ok(accent, now);
        }
        if self.ok_ma.hovered() || self.never_ma.hovered() {
            cx.cursor = CursorShape::Hand;
        }
        if ok.clicked {
            cx.msgs.push(Msg::TipAnswer { never: false });
        }
        if never.clicked {
            cx.msgs.push(Msg::TipAnswer { never: true });
        }
        [ok.handled, never.handled]
            .into_iter()
            .max_by_key(|h| *h as u8)
            .unwrap_or(Handled::No)
    }

    fn paint(&mut self, s: &mut dyn Surface, pc: &PaintCx) {
        let now = pc.now;
        let appear = self.appear.value(now);
        if appear <= 0.01 || self.frame.is_empty() {
            return;
        }
        let k = Self::scale_k(appear);
        let fq = self.float_q(now);
        let ext = self.to_win(self.extent_local(), k, fq);
        if ext.intersection(&pc.clip).is_none() {
            return;
        }
        let h = self.height;
        let accent = pc.env.accent;
        s.save();
        s.mul_opacity((appear * 1.4).min(1.0));
        s.translate(self.frame.x, self.frame.y + fq);
        s.translate(Self::TAIL_X, 0.0);
        s.scale(k, k);
        s.translate(-Self::TAIL_X, 0.0);

        // shadow + bubble
        s.shadow(
            Rect::new(0.0, TAIL_H, WIDTH, h - TAIL_H),
            RADIUS,
            26.0,
            -2.0,
            Point::new(0.0, 8.0),
            Color::BLACK.with_alpha(0x4a as f32 / 255.0),
        );
        if let Some(p) = &self.path {
            s.fill_path(p, &Fill::Solid(theme::CARD));
            let st = Stroke { width: 1.0, line_join: LineJoin::Round, ..Stroke::default() };
            s.stroke_path(p, &Fill::Solid(theme::STROKE), &st);
        }

        // typed title + caret
        let (chars, rows_t) = self.chars_rows(now);
        let n = TITLE.chars().count();
        let ci = (chars.round() as usize).min(n);
        let end = TITLE.char_indices().nth(ci).map_or(TITLE.len(), |(i, _)| i);
        let typed = &TITLE[..end];
        let ts = title_style();
        let tw = s.text(typed, PAD_X, TITLE_Y, &ts);
        if ci < n && appear > 0.9 {
            let cxp = PAD_X + tw + 6.0;
            s.fill_rrect(Rect::new(cxp, TITLE_Y + self.lh_title * 0.5 - 6.5, 2.0, 13.0), 1.0, &Fill::Solid(accent));
        }

        // rows
        let col_y = self.col_y();
        for (i, (_, label, _)) in ROWS.iter().enumerate() {
            let t = (rows_t * 4.0 - i as f32 * 0.9).clamp(0.0, 1.0);
            if t <= 0.0 {
                continue;
            }
            let ry = col_y + i as f32 * (ROW_H + ROW_GAP);
            s.save();
            s.mul_opacity(t);
            s.translate((1.0 - t) * -10.0, 0.0);
            let row = &self.rows[i];
            let mut x = PAD_X;
            for (j, (key, w)) in row.keys.iter().enumerate() {
                let hot = j == 2;
                let r = Rect::new(x, ry + (ROW_H - 19.0) * 0.5, *w, 19.0);
                let (fill, stroke, text) = if hot {
                    (accent.with_alpha(0.3), accent.with_alpha(0.6), pc.env.accent_text())
                } else {
                    (theme::FILL, theme::STROKE, theme::TEXT2)
                };
                s.fill_rrect(r, 5.0, &Fill::Solid(fill));
                s.stroke_rrect(r.inflate(-0.5, -0.5), 4.5, &Fill::Solid(stroke), 1.0);
                let st = chip_style(text);
                let kw = s.measure(key, &st);
                let lh = s.line_height(&st);
                s.text(key, r.x + (r.w - kw) * 0.5, r.y + (r.h - lh) * 0.5, &st);
                x += w + 3.0;
            }
            let ls = label_style();
            let lh = s.line_height(&ls);
            s.text(label, PAD_X + row.keys_w + 10.0, ry + (ROW_H - lh) * 0.5, &ls);
            s.restore();
        }

        // buttons
        let bt = (rows_t * 2.0 - 1.0).clamp(0.0, 1.0);
        if bt > 0.0 {
            s.save();
            s.mul_opacity(bt);
            let hov = self.never_ma.hovered();
            let nr = self.never_rect();
            let col = if hov { theme::TEXT2 } else { theme::TEXT3 };
            s.text("Me deixa em paz", nr.x, nr.y, &never_style(col));
            if hov {
                // `font.underline`: ~1 px line just under the baseline
                let uy = (nr.y + 11.0 * 1.06).round();
                s.fill_rrect(Rect::new(nr.x, uy, self.never_w, 1.0), 0.0, &Fill::Solid(col));
            }
            let or = self.ok_rect();
            let c = or.center();
            s.save();
            let sc = self.ok_scale.value(now);
            s.translate(c.x, c.y);
            s.scale(sc, sc);
            s.translate(-c.x, -c.y);
            let mut bg = self.ok_color.value(now);
            if bg.a == 0.0 {
                bg = accent;
            }
            s.fill_rrect(or, 14.0, &Fill::Solid(bg));
            let os = ok_style();
            let w = s.measure("Valeu, punk!", &os);
            let lh = s.line_height(&os);
            s.text("Valeu, punk!", c.x - w * 0.5, c.y - lh * 0.5, &os);
            s.restore();
            s.restore();
        }
        s.restore();
    }

    fn next_wake(&self, now: Ms) -> Wake {
        let a_anim = self.appear.is_animating(now);
        if !self.shown {
            return if a_anim { Wake::Frame } else { Wake::Idle };
        }
        let pop_start = self.t0 + POP_DELAY;
        if now < pop_start {
            return Wake::at(now, pop_start);
        }
        if a_anim {
            return Wake::Frame;
        }
        let mut w = Wake::Idle;
        // typing: one wake per new character
        let n = TITLE.chars().count() as f32;
        if now < self.type_start() {
            w = w.merge_at(Wake::at(now, self.type_start()), now);
        } else if now < self.type_end() {
            let next_c = (self.chars_rows(now).0.round() + 0.5).min(n);
            let t = if next_c >= n { self.type_end() } else { self.type_start() + (next_c / n) as Ms * TYPE_MS + 0.5 };
            w = w.merge_at(Wake::at(now, t.min(self.type_end())), now);
        } else if now < self.rows_end() {
            return Wake::Frame;
        }
        if self.ok_scale.is_animating(now) || self.ok_color.is_animating(now) {
            return Wake::Frame;
        }
        if let Some(t0) = self.float_t0 {
            let th = self.theta(t0, now);
            if let Some(d) = next_level_change(1.0, -1.0, FLOAT_STEP, th) {
                let t = now + (d / PI * FLOAT_HALF_MS) as Ms + 0.5;
                w = w.merge_at(Wake::at(now, t), now);
            }
        }
        w
    }

    fn bounds(&self) -> Rect {
        let k = 1.12;
        let r = self.extent_local();
        let tx = Self::TAIL_X;
        Rect::new(self.frame.x + tx + k * (r.x - tx), self.frame.y - 4.0, r.w * k, r.h * k + 8.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::Harness;
    use foco_base::{Button, PixSurface};
    use std::path::PathBuf;

    fn out_dir() -> PathBuf {
        let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/ui-test/wave3a");
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    const B: Ms = 10_000.0;

    fn place(h: &mut Harness, v: &mut TipView, now: Ms) {
        h.with_cx(now, |cx| {
            v.update(cx);
            v.layout(cx, Rect::new(300.0, 46.0, WIDTH, v.size().h));
        });
    }

    #[test]
    fn size_matches_the_qml_formula() {
        let mut h = Harness::new();
        let mut v = TipView::new();
        place(&mut h, &mut v, B);
        // FakeText: 13 px => line height 15.6; height = 9 + (lh + 9 + 103 + 9 + 30) + 30
        let expect = 9.0 + (15.6 + 9.0 + 103.0 + 9.0 + 30.0) + 30.0;
        assert!((v.size().h - expect).abs() < 0.01, "{} vs {expect}", v.size().h);
        assert_eq!(v.size().w, 318.0);
        assert!(v.path.is_some(), "outline path parsed");
    }

    #[test]
    fn timeline_visibility_typing_and_rows() {
        let mut h = Harness::new();
        let mut v = TipView::new();
        place(&mut h, &mut v, B);
        assert_eq!(v.next_wake(B), Wake::Idle);
        assert!(!v.visible(B));
        v.set_shown(true, B);
        assert_eq!(v.next_wake(B), Wake::At(B + 120.0), "waits out the 120 ms pause");
        place(&mut h, &mut v, B + 100.0);
        assert!(!v.visible(B + 100.0));
        assert!(!v.typing());
        place(&mut h, &mut v, B + 300.0);
        assert!(v.visible(B + 300.0));
        assert_eq!(v.next_wake(B + 300.0), Wake::Frame);
        // appear ends at 640: typing starts
        place(&mut h, &mut v, B + 641.0);
        assert!(v.typing());
        assert_eq!(v.chars_shown(B + 641.0), 0);
        let mid = B + 640.0 + 450.0;
        place(&mut h, &mut v, mid);
        assert!(v.typing());
        assert_eq!(v.chars_shown(mid), TITLE.chars().count() / 2);
        match v.next_wake(mid) {
            Wake::At(t) => assert!(t > mid && t < mid + 40.0, "next char soon: {t}"),
            Wake::Frame => {} // the next character is within one frame
            w => panic!("{w:?}"),
        }
        // typing done at 1540: rows stagger
        place(&mut h, &mut v, B + 1545.0);
        assert!(!v.typing());
        assert_eq!(v.chars_shown(B + 1545.0), TITLE.chars().count());
        assert_eq!(v.next_wake(B + 1545.0), Wake::Frame);
        let (_, r) = v.chars_rows(B + 1540.0 + 350.0);
        assert!((r - 0.5).abs() < 1e-4);
        // settled: only the float wakes us, at exact deadlines (never Frame/Ambient)
        place(&mut h, &mut v, B + 2300.0);
        match v.next_wake(B + 2300.0) {
            Wake::At(t) => assert!(t > B + 2300.0),
            Wake::Frame => {} // within a frame of a level change is fine
            w => panic!("{w:?}"),
        }
        // hide: fades out in 200 ms, then Idle
        v.set_shown(false, B + 3000.0);
        assert!(!v.typing());
        assert!(v.visible(B + 3100.0) && !v.visible(B + 3201.0));
        assert_eq!(v.next_wake(B + 3100.0), Wake::Frame);
        place(&mut h, &mut v, B + 3300.0);
        assert_eq!(v.next_wake(B + 3300.0), Wake::Idle);
    }

    #[test]
    fn float_is_quantised_and_wakes_are_sparse() {
        let mut h = Harness::new();
        let mut v = TipView::new();
        v.set_shown(true, B);
        let t = B + 2500.0;
        place(&mut h, &mut v, t);
        // count distinct float levels over one 3200 ms cycle and the wakes needed for them
        let mut now = t;
        let mut wakes = 0;
        while now < t + 3200.0 {
            match v.next_wake(now) {
                Wake::At(a) => now = a,
                Wake::Frame => now += 16.7,
                Wake::Idle => break,
                Wake::Ambient => now += 33.3,
            }
            wakes += 1;
        }
        // 0.25 px steps gave 17 wakes per cycle (16 level changes + the cycle edge), 0.5 px gives 9; each wake repaints the bubble
        assert!(wakes <= 9, "{wakes} wakes per 3.2 s cycle");
        let lv: Vec<f32> = (0..64).map(|i| v.float_q(t + i as f64 * 50.0)).collect();
        assert!(lv.iter().all(|y| (0.0..=2.0).contains(y)));
        assert!(lv.iter().all(|y| ((y / FLOAT_STEP).round() * FLOAT_STEP - y).abs() < 1e-5));
    }

    #[test]
    fn damage_is_scoped() {
        let mut h = Harness::new();
        let mut v = TipView::new();
        v.set_shown(true, B);
        place(&mut h, &mut v, B + 700.0);
        // mid typing: a new character only damages the title line
        place(&mut h, &mut v, B + 640.0 + 400.0);
        h.damage.clear();
        let mut t = B + 640.0 + 400.0;
        // advance to the next character without crossing a float step
        for _ in 0..40 {
            t += 1.0;
            place(&mut h, &mut v, t);
            if !h.damage.is_empty() {
                break;
            }
        }
        let r = h.damage.take();
        assert!(!r.is_empty());
        let tl = v.to_win(v.title_rect(), 1.0, v.float_q(t));
        let wide_ok = r
            .iter()
            .all(|d| (d.h as f32) <= (tl.h + 4.0) * crate::S + 4.0 || v.float_q(t) != v.float_q(t - 1.0));
        assert!(wide_ok, "{r:?} vs {tl:?}");
        // fully settled + float between level changes: no damage at all
        place(&mut h, &mut v, B + 2400.0);
        h.damage.clear();
        let q = v.float_q(B + 2400.0);
        let mut quiet = None;
        for i in 1..30 {
            let tt = B + 2400.0 + i as f64;
            if v.float_q(tt) == q {
                place(&mut h, &mut v, tt);
                quiet = Some(h.damage.is_empty());
            }
        }
        assert_eq!(quiet, Some(true));
        // hidden: nothing is painted or damaged
        v.set_shown(false, B + 4000.0);
        place(&mut h, &mut v, B + 4300.0);
        h.damage.clear();
        place(&mut h, &mut v, B + 4400.0);
        assert!(h.damage.is_empty());
    }

    fn window_pos(v: &TipView, now: Ms, local: Point) -> Point {
        let k = TipView::scale_k(v.appear.value(now));
        let fq = v.float_q(now);
        let tx = TipView::TAIL_X;
        Point::new(v.frame.x + tx + k * (local.x - tx), v.frame.y + fq + k * local.y)
    }

    #[test]
    fn buttons_answer_the_tip() {
        let mut h = Harness::new();
        let mut v = TipView::new();
        v.set_shown(true, B);
        let t = B + 3000.0;
        place(&mut h, &mut v, t);
        let ok = window_pos(&v, t, v.ok_rect().center());
        let down = Event::PointerDown { pos: ok, button: Button::Left, clicks: 1 };
        let up = Event::PointerUp { pos: ok, button: Button::Left };
        h.with_cx(t, |cx| v.event(cx, &Event::PointerMove { pos: ok }));
        assert_eq!(h.cursor, CursorShape::Hand);
        assert_eq!(h.with_cx(t, |cx| v.event(cx, &down)), Handled::Grab);
        assert!(v.ok_scale.target() < 1.0, "press scale 0.92");
        assert_eq!(h.with_cx(t, |cx| v.event(cx, &up)), Handled::Yes);
        assert_eq!(h.take_msgs(), vec![Msg::TipAnswer { never: false }]);
        let nv = window_pos(&v, t, v.never_rect().center());
        let down = Event::PointerDown { pos: nv, button: Button::Left, clicks: 1 };
        let up = Event::PointerUp { pos: nv, button: Button::Left };
        h.with_cx(t, |cx| v.event(cx, &Event::PointerMove { pos: nv }));
        assert!(v.never_ma.hovered());
        h.with_cx(t, |cx| v.event(cx, &down));
        h.with_cx(t, |cx| v.event(cx, &up));
        assert_eq!(h.take_msgs(), vec![Msg::TipAnswer { never: true }]);
        // elsewhere on the bubble: no message, not handled
        let body = window_pos(&v, t, Point::new(150.0, 30.0));
        assert_eq!(
            h.with_cx(t, |cx| v.event(cx, &Event::PointerDown { pos: body, button: Button::Left, clicks: 1 })),
            Handled::No
        );
        // hidden: ignores events
        v.set_shown(false, t + 100.0);
        assert_eq!(h.with_cx(t + 500.0, |cx| v.event(cx, &down)), Handled::No);
    }

    fn snap(name: &str, since: Ms, hover_never: bool) {
        let mut h = Harness::new();
        h.env
            .shortcut_keys
            .push(("done", vec!["Meta".into(), "Alt".into(), "D".into()]));
        let mut v = TipView::new();
        v.set_shown(true, B);
        let now = B + since;
        place(&mut h, &mut v, now);
        if hover_never {
            let p = window_pos(&v, now, v.never_rect().center());
            h.with_cx(now, |cx| v.event(cx, &Event::PointerMove { pos: p }));
        }
        place(&mut h, &mut v, now);
        let mut px = PixSurface::new(792, 330).unwrap();
        px.clear(Color::rgb8(0x8a, 0x8d, 0x94));
        px.scale(crate::S, crate::S);
        // a stub of the notch above the tip
        px.fill_rrect(Rect::new(274.0, -20.0, 212.0, 58.0), 19.0, &Fill::Solid(theme::NOTCH));
        h.with_pc(now, Rect::new(0.0, 0.0, 760.0, 320.0), |pc| v.paint(&mut px, pc));
        px.save_png(out_dir().join(name)).unwrap();
    }

    #[test]
    fn snapshots() {
        snap("tip-popping.png", 400.0, false);
        snap("tip-typing.png", 640.0 + 450.0, false);
        snap("tip-rows.png", 1540.0 + 300.0, false);
        snap("tip-settled.png", 2600.0, false);
        snap("tip-settled-never-hover.png", 2600.0, true);
    }
}
