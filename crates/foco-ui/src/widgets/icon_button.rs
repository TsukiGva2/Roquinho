//! `IconButton.qml`: a round button with an icon, hover background and press scale.

use super::color_anim::ColorAnim;
use super::icon::{paint_icon, Icon, DEFAULT_STROKE};
use super::mouse::{MouseArea, MouseOut};
use crate::cx::{CursorShape, Cx};
use foco_base::{theme, Anim, Color, Easing, Event, Fill, Ms, Rect, Surface, Wake};

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
            cx.damage.add(self.ma.rect);
        }
    }
    /// Places the button; damages old and new rects when it moved.
    pub fn layout(&mut self, cx: &mut Cx, rect: Rect) {
        if rect != self.ma.rect {
            cx.damage.add_moved(self.ma.rect, rect);
            self.ma.rect = rect;
        }
    }
    /// Clears hover/press (the owner became invisible).
    pub fn reset(&mut self, cx: &mut Cx) {
        if self.ma.reset() {
            self.retarget(cx.now);
            cx.damage.add(self.ma.rect);
        }
    }
    fn retarget(&mut self, now: Ms) {
        self.scale
            .animate_to(if self.ma.pressed_inside() { 0.86 } else { 1.0 }, 140.0, Easing::OutQuad, now);
        let want = if self.ma.hovered() { self.hover_bg } else { self.bg };
        self.bg_anim.animate_to(want, 140.0, now);
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
        if (sc - self.shown_scale).abs() >= 0.0005 || bg != self.shown_bg {
            self.shown_scale = sc;
            self.shown_bg = bg;
            cx.damage.add(self.ma.rect);
        }
    }
    pub fn next_wake(&self, now: Ms) -> Wake {
        if self.scale.is_animating(now) || self.bg_anim.is_animating(now) {
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
        let col = if self.ma.hovered() { self.hover_color } else { self.color };
        paint_icon(s, self.icon, c, self.icon_size, col, self.stroke);
        s.restore();
    }
}
