//! `IconButton.qml`: a round button with an icon, hover background and press scale.

use super::color_anim::ColorAnim;
use super::icon::{paint_icon, Icon, DEFAULT_STROKE};
use super::mouse::{MouseArea, MouseOut};
use crate::cx::{CursorShape, Cx};
use foco_base::{theme, Anim, Color, Easing, Event, Fill, Ms, Rect, Surface, Wake};

/// Largest scale the button paints at: hover 1.1 plus the `OutBack` overshoot (up to 1.13 when released from a press).
const MAX_SCALE: f32 = 1.15;

/// `hoverBg` default `#14000000` (a dark overlay on the light surfaces).
pub const HOVER_BG: Color = Color::rgba8(0x00, 0x00, 0x00, 0x14);

#[derive(Clone, Debug)]
pub struct IconButton {
    pub icon: Icon,
    pub size: f32,
    pub icon_size: f32,
    pub stroke: f32,
    pub color: Color,
    pub hover_color: Color,
    pub bg: Color,
    pub hover_bg: Color,
    ma: MouseArea,
    scale: Anim,
    bg_anim: ColorAnim,
    // what was last painted, to damage only on a visible change
    shown_scale: f32,
    shown_bg: Color,
    shown_icon_color: Color,
    icon_color: ColorAnim,
}

impl IconButton {
    /// Defaults of the QML: `color` text2, `hoverColor` text, `bg` transparent, `hoverBg` `#1fffffff`.
    pub fn new(icon: Icon, size: f32, icon_size: f32) -> IconButton {
        IconButton {
            icon,
            size,
            icon_size,
            stroke: DEFAULT_STROKE,
            color: theme::TEXT2,
            hover_color: theme::TEXT,
            bg: Color::TRANSPARENT,
            hover_bg: HOVER_BG,
            ma: MouseArea::new(),
            scale: Anim::new(1.0),
            bg_anim: ColorAnim::new(Color::TRANSPARENT),
            shown_scale: 1.0,
            shown_bg: Color::TRANSPARENT,
            shown_icon_color: theme::TEXT2,
            icon_color: ColorAnim::new(theme::TEXT2),
        }
    }
    /// Colour set (icon colour, icon hover colour, background, hover background).
    pub fn colors(mut self, color: Color, hover_color: Color, bg: Color, hover_bg: Color) -> IconButton {
        self.color = color;
        self.hover_color = hover_color;
        self.bg = bg;
        self.hover_bg = hover_bg;
        self.bg_anim.set(bg);
        self.shown_bg = bg;
        self.icon_color.set(color);
        self.shown_icon_color = color;
        self
    }
    pub fn rect(&self) -> Rect {
        self.ma.rect
    }
    pub fn hovered(&self) -> bool {
        self.ma.hovered()
    }
    pub fn pressed(&self) -> bool {
        self.ma.pressed()
    }
    /// Changes the icon (damages when it differs).
    pub fn set_icon(&mut self, cx: &mut Cx, icon: Icon) {
        if self.icon != icon {
            self.icon = icon;
            cx.damage.add(self.painted());
        }
    }
    /// Places the button; damages old and new rects when it moved.
    pub fn layout(&mut self, cx: &mut Cx, rect: Rect) {
        if rect != self.ma.rect {
            let old = self.painted();
            self.ma.rect = rect;
            cx.damage.add_moved(old, self.painted());
        }
    }
    /// The rect the button can paint into: its own, grown for the largest scale.
    fn painted(&self) -> Rect {
        let r = self.ma.rect;
        r.inflate(r.w * (MAX_SCALE - 1.0) * 0.5, r.h * (MAX_SCALE - 1.0) * 0.5)
    }
    /// Clears hover/press (the owner became invisible).
    pub fn reset(&mut self, cx: &mut Cx) {
        if self.ma.reset() {
            self.retarget(cx.now);
            cx.damage.add(self.painted());
        }
    }
    fn retarget(&mut self, now: Ms) {
        let (target, dur) = if self.ma.pressed_inside() {
            (0.86, 90.0)
        } else if self.ma.hovered() {
            (1.1, 260.0)
        } else {
            (1.0, 260.0)
        };
        self.scale.animate_to(target, dur, Easing::OutBack { overshoot: 2.0 }, now);
        let want = if self.ma.hovered() { self.hover_bg } else { self.bg };
        self.bg_anim.animate_to(want, 140.0, now);
        self.sync_icon_color(now);
    }
    /// Retargets the icon colour; also run from `tick` because owners assign `color`/`hover_color`
    /// after construction (accent changes).
    fn sync_icon_color(&mut self, now: Ms) {
        let want = if self.ma.hovered() { self.hover_color } else { self.color };
        self.icon_color.animate_to(want, 140.0, now);
    }
    /// Feeds a pointer event. `clicked` is set on release inside; `handled` is `Grab` on press.
    pub fn pointer(&mut self, cx: &mut Cx, ev: &Event) -> MouseOut {
        let out = self.ma.event(ev);
        if out.changed {
            self.retarget(cx.now);
        }
        if self.ma.hovered() {
            cx.cursor = CursorShape::Hand;
        }
        out
    }
    /// Damages the button while its animations produce a visibly different picture. Call from
    /// the owner's `update`.
    pub fn tick(&mut self, cx: &mut Cx) {
        let sc = self.scale.value(cx.now);
        let bg = self.bg_anim.value(cx.now);
        self.sync_icon_color(cx.now);
        let icon = self.icon_color.value(cx.now);
        if (sc - self.shown_scale).abs() >= 0.0005 || bg != self.shown_bg || icon != self.shown_icon_color {
            self.shown_scale = sc;
            self.shown_bg = bg;
            self.shown_icon_color = icon;
            cx.damage.add(self.painted());
        }
    }
    pub fn next_wake(&self, now: Ms) -> Wake {
        if self.scale.is_animating(now) || self.bg_anim.is_animating(now) || self.icon_color.is_animating(now) {
            Wake::Frame
        } else {
            Wake::Idle
        }
    }
    pub fn paint(&self, s: &mut dyn Surface, now: Ms) {
        let r = self.ma.rect;
        if r.is_empty() {
            return;
        }
        let c = r.center();
        let sc = self.scale.value(now);
        s.save();
        if sc != 1.0 {
            s.translate(c.x, c.y);
            s.scale(sc, sc);
            s.translate(-c.x, -c.y);
        }
        let bg = self.bg_anim.value(now);
        if bg.a > 0.0 {
            s.fill_rrect(r, r.w * 0.5, &Fill::Solid(bg));
        }
        paint_icon(s, self.icon, c, self.icon_size, self.icon_color.value(now), self.stroke);
        s.restore();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cx::S;
    use crate::testkit::Harness;
    use foco_base::{Button, Point};

    const R: Rect = Rect::new(100.0, 100.0, 28.0, 28.0);

    fn button(h: &mut Harness) -> IconButton {
        let mut b = IconButton::new(Icon::Check, 28.0, 14.0);
        h.with_cx(0.0, |cx| b.layout(cx, R));
        h.damage.clear();
        b
    }

    fn mv(h: &mut Harness, b: &mut IconButton, now: Ms, pos: Point) {
        h.with_cx(now, |cx| b.pointer(cx, &Event::PointerMove { pos }));
    }

    #[test]
    fn hover_grows_press_sinks_release_returns() {
        let mut h = Harness::new();
        let mut b = button(&mut h);
        mv(&mut h, &mut b, 0.0, R.center());
        assert_eq!(b.scale.target(), 1.1);
        assert_eq!(b.scale.value(260.0), 1.1);
        let down = Event::PointerDown { pos: R.center(), button: Button::Left, clicks: 1 };
        h.with_cx(300.0, |cx| b.pointer(cx, &down));
        assert_eq!(b.scale.target(), 0.86);
        assert_eq!(b.scale.value(390.0), 0.86);
        // releasing inside goes back to the hover size, leaving the button to 1
        h.with_cx(400.0, |cx| b.pointer(cx, &Event::PointerUp { pos: R.center(), button: Button::Left }));
        assert_eq!(b.scale.target(), 1.1);
        mv(&mut h, &mut b, 500.0, Point::new(0.0, 0.0));
        assert_eq!(b.scale.target(), 1.0);
    }

    #[test]
    fn icon_color_follows_hover() {
        let mut h = Harness::new();
        let mut b = button(&mut h);
        mv(&mut h, &mut b, 0.0, R.center());
        h.with_cx(70.0, |cx| b.tick(cx));
        let mid = b.icon_color.value(70.0);
        assert!(mid != theme::TEXT2 && mid != theme::TEXT);
        h.with_cx(140.0, |cx| b.tick(cx));
        assert_eq!(b.icon_color.value(140.0), theme::TEXT);
    }

    #[test]
    fn damage_covers_the_scaled_button() {
        let mut h = Harness::new();
        let mut b = button(&mut h);
        mv(&mut h, &mut b, 0.0, R.center());
        // the scale is above 1 well before it settles
        h.with_cx(100.0, |cx| b.tick(cx));
        let sc = b.scale.value(100.0);
        assert!(sc > 1.05);
        let half = 14.0 * sc;
        let c = R.center();
        let drawn = Rect::new((c.x - half) * S, (c.y - half) * S, 2.0 * half * S, 2.0 * half * S).round_out();
        assert!(h.damage.rects().iter().any(|d| d.union(&drawn) == *d), "{:?} vs {drawn:?}", h.damage.rects());
    }

    #[test]
    fn goes_idle_once_settled() {
        let mut h = Harness::new();
        let mut b = button(&mut h);
        mv(&mut h, &mut b, 0.0, R.center());
        assert_eq!(b.next_wake(100.0), Wake::Frame);
        h.with_cx(300.0, |cx| b.tick(cx));
        assert_eq!(b.next_wake(300.0), Wake::Idle);
    }
}
