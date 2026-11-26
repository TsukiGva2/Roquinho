//! `SettingsPanel.qml`: mascot tiles, name, behaviour toggles, monitor tiles, accent swatches,
//! global shortcuts, quit button and footer. The music row and the three music shortcut rows are gone
//! (the behaviour card is 150 px, the shortcuts card 5 rows).
//!
//! The panel owns its 200 ms linear fade: the owner calls [`SettingsPanel::set_shown`] and paints it
//! as is (no extra opacity). Hidden (`opacity <= 0.01`) it has no events, no wakes and no paint, and
//! the three tile pets are inactive.
//!
//! `height()` is the QML `implicitHeight` (independent of the fade), valid after `update`.
//!
//! Damage: only what changes (a tile on hover/selection/border fade, a toggle, a swatch, the
//! quit button, a pet's box); a moved frame (scrolling) damages the frame once.
//! Wake: `Frame` while any fade/ring/knob runs, pets' own `Ambient`/`At` while visible, the name
//! caret blink while focused, otherwise `Idle`.

use crate::cx::{CursorShape, Cx, FocusId, Handled, Msg, PaintCx};
use crate::pet::{new_pet, PetLike};
use crate::view::{Section, View};
use crate::widgets::{
    paint_chip, paint_icon, paint_section_label, paint_text_centered, wrap, ColorAnim, Icon, InputOutcome, MouseArea,
    TextInput, Toggle, DEFAULT_STROKE, SECTION_LABEL_H,
};
use foco_base::{
    model::PetKind, theme, Action, Anim, AppCmd, Button, Color, Easing, Event, Fill, Mood, Ms, Point, Rect, ScreenArg,
    Surface, TextStyle, Wake, Weight,
};

const DEFAULT_W: f32 = 512.0;
const SP: f32 = 10.0;
const PET_TILE_H: f32 = 92.0;
const NAME_H: f32 = 44.0;
const BEH_ROW_H: f32 = 50.0;
const BEH_H: f32 = 3.0 * BEH_ROW_H;
const MON_H: f32 = 74.0;
const ACC_H: f32 = 50.0;
const SC_ROW_H: f32 = 28.0;
const SC_H: f32 = 5.0 * SC_ROW_H + 4.0 * 2.0 + 16.0;
const QUIT_ROW_H: f32 = 36.0;
const FADE_MS: Ms = 200.0;
const SHORTCUTS_HINT: &str =
    "Dá para trocar em ~/.config/foco-maxrc, na seção [shortcuts] (deixe vazio para desligar e usar o rc.xml do Openbox).";
const NO_SCREEN_HINT: &str = "Só um monitor conectado agora. Quando ligar outro, ele aparece aqui.";

const PETS: [(PetKind, &str, &str); 3] =
    [(PetKind::Cat, "cat", "Gato"), (PetKind::Ghost, "ghost", "Fantasma"), (PetKind::Slime, "slime", "Slime")];
/// (hex, is "auto").
const SWATCHES: [&str; 9] =
    ["", "#315bef", "#926ee4", "#f74f9e", "#ff5257", "#f7821b", "#ffc600", "#62ba46", "#8c8c8c"];
/// (letter, label, command)
const SHORTCUTS: [(&str, &str, &str); 5] = [
    ("N", "Nova demanda", "add"),
    ("F", "Abrir / fechar painel", "toggle"),
    ("H", "Esconder em bolinha", "hide"),
    ("D", "Concluir demanda em foco", "done"),
    ("M", "Mover pro próximo monitor", "screen"),
];

fn txt(px: f32, w: Weight, c: Color) -> TextStyle {
    TextStyle::new(px, w, c)
}

fn same(a: Color, b: Color) -> bool {
    (a.r - b.r).abs() < 0.004 && (a.g - b.g).abs() < 0.004 && (a.b - b.b).abs() < 0.004
}

fn swatch_color(i: usize) -> Option<Color> {
    Color::hex(SWATCHES[i])
}

/// A selectable card (mascot or monitor tile): hover background, border colour fading 160 ms.
struct Tile {
    ma: MouseArea,
    border: ColorAnim,
    shown_border: Color,
    selected: bool,
    hov: bool,
    init: bool,
}

impl Tile {
    fn new() -> Tile {
        Tile {
            ma: MouseArea::new(),
            border: ColorAnim::new(theme::STROKE),
            shown_border: theme::STROKE,
            selected: false,
            hov: false,
            init: false,
        }
    }
    fn place(&mut self, cx: &mut Cx, r: Rect) {
        if r != self.ma.rect {
            cx.damage.add_moved(self.ma.rect, r);
            self.ma.rect = r;
        }
    }
    /// Retargets from the selection/accent and damages on visible changes.
    fn sync(&mut self, cx: &mut Cx, selected: bool, accent: Color) {
        let want = if selected { accent } else { theme::STROKE };
        if !self.init {
            // QML bindings start at their value: no fade on creation
            self.init = true;
            self.selected = selected;
            self.border.set(want);
            self.shown_border = want;
        }
        if selected != self.selected {
            self.selected = selected;
            cx.damage.add(self.ma.rect);
        }
        self.border.animate_to(want, 160.0, cx.now);
        let b = self.border.value(cx.now);
        if b != self.shown_border {
            self.shown_border = b;
            cx.damage.add(self.ma.rect);
        }
        if self.ma.hovered() != self.hov {
            self.hov = self.ma.hovered();
            cx.damage.add(self.ma.rect);
        }
    }
    fn paint(&self, s: &mut dyn Surface, now: Ms) {
        let r = self.ma.rect;
        let bg = if self.ma.hovered() { theme::CARD_HOVER } else { theme::CARD };
        s.fill_rrect(r, 16.0, &Fill::Solid(bg));
        let (bw, col) = (if self.selected { 1.5 } else { 1.0 }, self.border.value(now));
        let i = bw * 0.5;
        s.stroke_rrect(r.inflate(-i, -i), 16.0 - i, &Fill::Solid(col), bw);
    }
    fn animating(&self, now: Ms) -> bool {
        self.border.is_animating(now)
    }
}

struct Swatch {
    ma: MouseArea,
    ring: Anim,
    scale: Anim,
    selected: bool,
    shown: (f32, f32),
}

struct ScreenTile {
    /// `""` = primary.
    key: String,
    title: String,
    sub: String,
    tile: Tile,
}

struct Chips {
    label: &'static str,
    keys: Vec<String>,
    widths: Vec<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Click {
    Pet(usize),
    Screen(usize),
    Swatch(usize),
    Quit,
    Toggle(usize, bool),
}

/// Relative y offsets of the column items (QML `Column`, spacing 10).
#[derive(Clone, Copy, Debug, Default)]
struct Offs {
    l_pets: f32,
    t_pets: f32,
    name: f32,
    l_beh: f32,
    beh: f32,
    l_mon: f32,
    t_mon: f32,
    hint_mon: f32,
    l_acc: f32,
    acc: f32,
    l_sc: f32,
    sc: f32,
    hint_sc: f32,
    quit: f32,
}

pub struct SettingsPanel {
    frame: Rect,
    w: f32,
    h: f32,
    off: Offs,
    shown: bool,
    opacity: Anim,
    shown_opacity: f32,
    rev: u64,
    env_rev: u64,
    dirty: bool,
    values_init: bool,
    // content
    pets: Vec<Box<dyn PetLike>>,
    pet_active: bool,
    pet_tiles: Vec<Tile>,
    pet_moods: [Mood; 3],
    pet_kind: PetKind,
    name_input: TextInput,
    last_name: String,
    name_editing: bool,
    name_focus_seen: bool,
    name_rect: Rect,
    name_label_w: f32,
    shown_name_active: bool,
    toggles: [Toggle; 3],
    tips_title: String,
    screens: Vec<ScreenTile>,
    screens_n: usize,
    mon_hint: Vec<String>,
    swatches: Vec<Swatch>,
    sc_rows: Vec<Chips>,
    sc_hint: Vec<String>,
    quit: MouseArea,
    quit_bg: ColorAnim,
    shown_quit_bg: Color,
    quit_w: f32,
    footer: String,
    total_done: u32,
    accent: Color,
    /// A press on the name field holds the pointer (selection drag).
    name_drag: bool,
}

impl Default for SettingsPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl SettingsPanel {
    pub fn new() -> Self {
        Self::with_pets(new_pet)
    }

    /// Constructor with an explicit pet factory (tests inject a recording pet).
    pub fn with_pets(make: fn() -> Box<dyn PetLike>) -> Self {
        let name_style = txt(13.0, Weight::Regular, theme::TEXT);
        let mut name_input = TextInput::new(name_style, 20);
        name_input.align_right = true;
        let mut pets: Vec<Box<dyn PetLike>> = Vec::new();
        for (kind, _, _) in PETS {
            let mut p = make();
            p.set_kind(kind, 0.0);
            p.set_size(46.0);
            pets.push(p);
        }
        SettingsPanel {
            frame: Rect::default(),
            w: DEFAULT_W,
            h: 0.0,
            off: Offs::default(),
            shown: false,
            opacity: Anim::new(0.0),
            shown_opacity: 0.0,
            rev: u64::MAX,
            env_rev: u64::MAX,
            dirty: true,
            values_init: false,
            pets,
            pet_active: false,
            pet_tiles: (0..3).map(|_| Tile::new()).collect(),
            pet_moods: [Mood::Sleep; 3],
            pet_kind: PetKind::Cat,
            name_input,
            last_name: String::new(),
            name_editing: false,
            name_focus_seen: false,
            name_rect: Rect::default(),
            name_label_w: 0.0,
            shown_name_active: false,
            toggles: [Toggle::new(false), Toggle::new(false), Toggle::new(false)],
            tips_title: String::new(),
            screens: Vec::new(),
            screens_n: usize::MAX,
            mon_hint: Vec::new(),
            swatches: (0..SWATCHES.len())
                .map(|_| Swatch {
                    ma: MouseArea::new(),
                    ring: Anim::new(22.0),
                    scale: Anim::new(1.0),
                    selected: false,
                    shown: (22.0, 1.0),
                })
                .collect(),
            sc_rows: Vec::new(),
            sc_hint: Vec::new(),
            quit: MouseArea::new(),
            quit_bg: ColorAnim::new(theme::RED.with_alpha(0.12)),
            shown_quit_bg: theme::RED.with_alpha(0.12),
            quit_w: 0.0,
            footer: String::new(),
            total_done: u32::MAX,
            accent: crate::cx::AUTO_ACCENT,
            name_drag: false,
        }
    }

    /// Shows or hides the panel (`shown`): fades over 200 ms linear; the tile pets follow the
    /// visibility. Call it whenever "settings open" changes.
    pub fn set_shown(&mut self, v: bool, now: Ms) {
        if self.shown != v {
            self.shown = v;
            self.opacity.animate_to(if v { 1.0 } else { 0.0 }, FADE_MS, Easing::Linear, now);
        }
    }

    /// `visible: opacity > 0.01`.
    pub fn visible(&self, now: Ms) -> bool {
        self.opacity.value(now) > 0.01
    }

    pub fn opacity(&self, now: Ms) -> f32 {
        self.opacity.value(now)
    }

    /// Whether the three tile pets are animating (test hook).
    pub fn pets_active(&self) -> bool {
        self.pet_active
    }

    fn line_h(cx: &mut Cx, px: f32, w: Weight) -> f32 {
        cx.text.line_height(&txt(px, w, theme::TEXT))
    }

    /// Recomputes strings, wrapped hints, chips and the implicit height.
    fn rebuild(&mut self, cx: &mut Cx) {
        self.dirty = false;
        let st = cx.store.state();
        let env = cx.env;
        let w = self.w;
        self.pet_kind = st.settings.kind();
        if !self.values_init {
            // controls start at their bound value (QML bindings do not animate on creation)
            self.values_init = true;
            self.toggles = [
                Toggle::new(st.settings.auto_advance),
                Toggle::new(env.autostart),
                Toggle::new(st.settings.tips_enabled),
            ];
            for (i, sw) in self.swatches.iter_mut().enumerate() {
                sw.selected = match (env.accent_choice, swatch_color(i)) {
                    (None, None) => true,
                    (Some(c), Some(k)) => same(c, k),
                    _ => false,
                };
                let d = if sw.selected { 26.0 } else { 22.0 };
                sw.ring.set(d);
                sw.shown.0 = d;
            }
        }
        // name
        if st.settings.pet_name != self.last_name {
            self.last_name = st.settings.pet_name.clone();
            self.name_input.set_text(&self.last_name);
        }
        self.tips_title = format!("Dicas de atalhos do {}", st.settings.pet_name);
        self.name_label_w = cx.text.measure("Nome", &txt(13.0, Weight::Regular, theme::TEXT2));
        // monitor options
        let primary_label = env.screens.iter().find(|s| s.primary).map(|s| s.label.as_str());
        let mut old = std::mem::take(&mut self.screens);
        let principal_sub = match primary_label {
            Some(l) => format!("Segue o {l}"),
            None => "Segue o principal".to_string(),
        };
        let mut push = |key: &str, title: &str, sub: String, this: &mut Vec<ScreenTile>| {
            // keep hover / fade state of tiles that survive
            let tile = match old.iter().position(|t| t.key == key) {
                Some(i) => old.swap_remove(i).tile,
                None => Tile::new(),
            };
            this.push(ScreenTile { key: key.to_string(), title: title.to_string(), sub, tile });
        };
        push("", "Principal", principal_sub, &mut self.screens);
        for s in &env.screens {
            let sub = format!("{}{}", s.name, if s.primary { " · principal" } else { "" });
            push(&s.name, &s.label, sub, &mut self.screens);
        }
        self.screens_n = self.screens.len();
        let hint_st = txt(11.0, Weight::Regular, theme::TEXT3);
        let hint_w = w - 8.0;
        let mon_text = if env.screens.len() > 1 {
            format!("Ou puxe pro próximo monitor de qualquer lugar com {}.", env.keys_for("screen", "M").join(" + "))
        } else {
            NO_SCREEN_HINT.to_string()
        };
        self.mon_hint = wrap(cx.text, &mon_text, &hint_st, hint_w);
        self.sc_hint = wrap(cx.text, SHORTCUTS_HINT, &hint_st, hint_w);
        // shortcut chips
        let chip_st = txt(11.0, Weight::Regular, theme::TEXT);
        self.sc_rows.clear();
        for (letter, label, cmd) in SHORTCUTS {
            let keys = env.keys_for(cmd, letter);
            let widths = keys.iter().map(|k| (cx.text.measure(k, &chip_st) + 12.0).max(20.0)).collect();
            self.sc_rows.push(Chips { label, keys, widths });
        }
        // footer / quit
        self.total_done = st.total_done;
        self.footer = format!("Foquinho 1.1 · {} demandas destruídas desde sempre", st.total_done);
        let quit_st = txt(12.0, Weight::DemiBold, theme::RED);
        self.quit_w = 14.0 + 7.0 + cx.text.measure("Fim do show", &quit_st) + 26.0;
        // heights
        let hint_lh = Self::line_h(cx, 11.0, Weight::Regular);
        let mut y = 0.0;
        let mut place = |h: f32| {
            let t = y;
            y += h + SP;
            t
        };
        let mon_hint_h = self.mon_hint.len() as f32 * hint_lh;
        let sc_hint_h = self.sc_hint.len() as f32 * hint_lh;
        self.off = Offs {
            l_pets: place(SECTION_LABEL_H),
            t_pets: place(PET_TILE_H),
            name: place(NAME_H),
            // 2 px spacer
            l_beh: {
                place(2.0);
                place(SECTION_LABEL_H)
            },
            beh: place(BEH_H),
            l_mon: {
                place(2.0);
                place(SECTION_LABEL_H)
            },
            t_mon: place(MON_H),
            hint_mon: place(mon_hint_h),
            l_acc: {
                place(2.0);
                place(SECTION_LABEL_H)
            },
            acc: place(ACC_H),
            l_sc: {
                place(2.0);
                place(SECTION_LABEL_H)
            },
            sc: place(SC_H),
            hint_sc: place(sc_hint_h),
            quit: {
                place(4.0);
                place(QUIT_ROW_H)
            },
        };
        let h = y - SP;
        if (h - self.h).abs() > 0.01 {
            self.h = h;
        }
        cx.damage.add(self.frame.inflate(0.0, 2.0));
    }

    fn msg_for(&self, c: Click) -> Option<Msg> {
        Some(match c {
            Click::Pet(i) => Msg::Act(Action::SetPetKind(PETS[i].1.to_string())),
            Click::Screen(i) => {
                let key = &self.screens.get(i)?.key;
                Msg::Cmd(AppCmd::Screen(if key.is_empty() {
                    ScreenArg::Primary
                } else {
                    ScreenArg::Named(key.clone())
                }))
            }
            Click::Swatch(i) => Msg::Cmd(AppCmd::SetAccent(swatch_color(i))),
            Click::Quit => Msg::Cmd(AppCmd::Quit),
            Click::Toggle(i, v) => match i {
                0 => Msg::Act(Action::SetAutoAdvance(v)),
                1 => Msg::Cmd(AppCmd::SetAutostart(v)),
                _ => Msg::Act(Action::SetTipsEnabled(v)),
            },
        })
    }

    /// Commits the pet name field (`editingFinished`: trimmed, ignored when empty).
    fn commit_name(&mut self, cx: &mut Cx) {
        let t = self.name_input.text.trim().to_string();
        if !t.is_empty() && t != cx.store.state().settings.pet_name {
            cx.msgs.push(Msg::Act(Action::SetPetName(t)));
        }
    }

    // --- placement -------------------------------------------------------------------------

    fn place_all(&mut self, cx: &mut Cx) {
        let (x, w) = (self.frame.x, self.w);
        let top = self.frame.y;
        let o = self.off;
        // mascot tiles
        let tw = (w - 16.0) / 3.0;
        for i in 0..3 {
            let r = Rect::new(x + i as f32 * (tw + 8.0), top + o.t_pets, tw, PET_TILE_H);
            self.pet_tiles[i].place(cx, r);
        }
        // name
        let name = Rect::new(x, top + o.name, w, NAME_H);
        let lh = cx.text.line_height(self.name_input.style());
        let ix = name.x + 14.0 + self.name_label_w + 14.0;
        let inp = Rect::new(ix, name.y + (NAME_H - lh) * 0.5, (name.right() - 14.0 - ix).max(0.0), lh);
        if name != self.name_rect {
            cx.damage.add_moved(self.name_rect, name);
            self.name_rect = name;
        }
        self.name_input.layout(cx, inp);
        // toggles
        for i in 0..3 {
            let ry = top + o.beh + i as f32 * BEH_ROW_H;
            self.toggles[i].layout(cx, x + w - 14.0 - 38.0, ry + (BEH_ROW_H - 22.0) * 0.5);
        }
        // monitor tiles
        let n = self.screens.len().max(1);
        let mw = (w - 8.0 * (n as f32 - 1.0)) / n as f32;
        for (i, t) in self.screens.iter_mut().enumerate() {
            t.tile.place(cx, Rect::new(x + i as f32 * (mw + 8.0), top + o.t_mon, mw, MON_H));
        }
        // swatches: 9 * 26 + 8 * 12, centred in the 50 px card
        let total = SWATCHES.len() as f32 * 26.0 + (SWATCHES.len() as f32 - 1.0) * 12.0;
        let sx = x + (w - total) * 0.5;
        for (i, sw) in self.swatches.iter_mut().enumerate() {
            let cx0 = sx + i as f32 * 38.0;
            let r = Rect::new(cx0 - 3.0, top + o.acc + (ACC_H - 26.0) * 0.5 - 3.0, 32.0, 32.0);
            if r != sw.ma.rect {
                cx.damage.add_moved(sw.ma.rect, r);
                sw.ma.rect = r;
            }
        }
        // quit
        let q = Rect::new(x, top + o.quit, self.quit_w, 34.0);
        if q != self.quit.rect {
            cx.damage.add_moved(self.quit.rect, q);
            self.quit.rect = q;
        }
    }

    fn set_visible_state(&mut self, cx: &mut Cx, vis: bool) {
        if vis != self.pet_active {
            self.pet_active = vis;
            for p in &mut self.pets {
                p.set_active(vis, cx.now);
            }
            if !vis {
                self.reset_pointer();
                self.name_editing = false;
            }
        }
    }

    fn reset_pointer(&mut self) {
        for t in &mut self.pet_tiles {
            t.ma.reset();
        }
        for t in &mut self.screens {
            t.tile.ma.reset();
        }
        for s in &mut self.swatches {
            s.ma.reset();
        }
        self.quit.reset();
        self.name_drag = false;
    }

    /// Feeds one pointer event to every control; returns (handled, click).
    fn feed(&mut self, cx: &mut Cx, ev: &Event) -> (Handled, Option<Click>) {
        let mut handled = Handled::No;
        let mut click = None;
        let mut hand = false;
        let merge = |h: Handled, handled: &mut Handled| {
            if h == Handled::Grab || (h == Handled::Yes && *handled == Handled::No) {
                *handled = h;
            }
        };
        for (i, t) in self.pet_tiles.iter_mut().enumerate() {
            let o = t.ma.event(ev);
            hand |= t.ma.hovered();
            merge(o.handled, &mut handled);
            if o.clicked {
                click = Some(Click::Pet(i));
            }
        }
        for (i, t) in self.screens.iter_mut().enumerate() {
            let o = t.tile.ma.event(ev);
            hand |= t.tile.ma.hovered();
            merge(o.handled, &mut handled);
            if o.clicked {
                click = Some(Click::Screen(i));
            }
        }
        for (i, s) in self.swatches.iter_mut().enumerate() {
            let o = s.ma.event(ev);
            hand |= s.ma.hovered();
            merge(o.handled, &mut handled);
            if o.clicked {
                click = Some(Click::Swatch(i));
            }
        }
        let o = self.quit.event(ev);
        hand |= self.quit.hovered();
        merge(o.handled, &mut handled);
        if o.clicked {
            click = Some(Click::Quit);
        }
        for (i, t) in self.toggles.iter_mut().enumerate() {
            let (h, c) = t.pointer(cx, ev);
            merge(h, &mut handled);
            if let Some(v) = c {
                click = Some(Click::Toggle(i, v));
            }
        }
        if hand {
            cx.cursor = CursorShape::Hand;
        }
        (handled, click)
    }

    fn name_active(&self, cx: &Cx) -> bool {
        self.shown && cx.has_focus(&FocusId::PetName)
    }

    // --- painting --------------------------------------------------------------------------

    fn card(s: &mut dyn Surface, r: Rect, radius: f32) {
        s.fill_rrect(r, radius, &Fill::Solid(theme::CARD));
        s.stroke_rrect(r.inflate(-0.5, -0.5), radius - 0.5, &Fill::Solid(theme::STROKE), 1.0);
    }
}

impl View for SettingsPanel {
    fn update(&mut self, cx: &mut Cx) {
        let now = cx.now;
        let op = self.opacity.value(now);
        let vis = op > 0.01;
        if !self.shown && *cx.focus == FocusId::PetName {
            *cx.focus = FocusId::None;
        }
        // pet name commit on blur (`editingFinished`): focus left, or the window was deactivated
        {
            let mine = *cx.focus == FocusId::PetName;
            if mine && cx.window_focused {
                self.name_focus_seen = true;
                self.name_editing = true;
            }
            if self.name_editing && (!mine || (!cx.window_focused && self.name_focus_seen)) {
                self.name_editing = false;
                self.name_focus_seen = false;
                self.commit_name(cx);
            }
        }
        self.set_visible_state(cx, vis);
        if (op - self.shown_opacity).abs() >= 1.0 / 255.0 {
            self.shown_opacity = op;
            cx.damage.add(self.frame.inflate(0.0, 2.0));
        }
        if self.dirty || cx.rev != self.rev || cx.env.rev != self.env_rev {
            self.rev = cx.rev;
            self.env_rev = cx.env.rev;
            self.rebuild(cx);
            self.place_all(cx);
        }
        if !vis {
            return;
        }
        let st = cx.store.state();
        let accent = cx.env.accent;
        let accent_changed = accent != self.accent;
        self.accent = accent;
        // pets: kind selection and hover drive the mood
        #[allow(clippy::needless_range_loop)]
        for i in 0..3 {
            let selected = self.pet_kind == PETS[i].0;
            let mood = if selected {
                Mood::Happy
            } else if self.pet_tiles[i].ma.hovered() {
                Mood::Focus
            } else {
                Mood::Idle
            };
            if mood != self.pet_moods[i] || accent_changed {
                self.pet_moods[i] = mood;
                self.pets[i].set_mood(mood, now);
                self.pets[i].set_accent(accent);
                cx.damage
                    .add(self.pet_rect(i).inflate(self.pets[i].overdraw(), self.pets[i].overdraw()));
            }
            let pr = self.pet_rect(i);
            if self.pets[i].update(now) {
                let od = self.pets[i].overdraw();
                cx.damage.add(pr.inflate(od, od));
            }
            self.pet_tiles[i].sync(cx, selected, accent);
        }
        // name field
        let active = self.name_active(cx);
        self.name_input.tick(cx, active);
        if active != self.shown_name_active {
            self.shown_name_active = active;
            cx.damage.add(self.name_rect);
        }
        // toggles
        let vals = [st.settings.auto_advance, cx.env.autostart, st.settings.tips_enabled];
        for (t, v) in self.toggles.iter_mut().zip(vals) {
            t.sync(cx, v);
        }
        // monitor tiles
        for t in &mut self.screens {
            let selected = t.key == cx.env.screen_choice;
            t.tile.sync(cx, selected, accent);
        }
        // swatches
        let chosen = cx.env.accent_choice;
        for (i, sw) in self.swatches.iter_mut().enumerate() {
            let selected = match (chosen, swatch_color(i)) {
                (None, None) => true,
                (Some(c), Some(k)) => same(c, k),
                _ => false,
            };
            if selected != sw.selected {
                sw.selected = selected;
                sw.ring
                    .animate_to(if selected { 26.0 } else { 22.0 }, 140.0, Easing::Linear, now);
                cx.damage.add(sw.ma.rect);
            }
            sw.scale
                .animate_to(if sw.ma.hovered() { 1.12 } else { 1.0 }, 120.0, Easing::Linear, now);
            let cur = (sw.ring.value(now), sw.scale.value(now));
            if (cur.0 - sw.shown.0).abs() >= 0.05 || (cur.1 - sw.shown.1).abs() >= 0.002 {
                sw.shown = cur;
                cx.damage.add(sw.ma.rect);
            }
        }
        // quit button
        let bg = theme::RED.with_alpha(if self.quit.hovered() { 0.22 } else { 0.12 });
        self.quit_bg.animate_to(bg, 140.0, now);
        let v = self.quit_bg.value(now);
        if v != self.shown_quit_bg {
            self.shown_quit_bg = v;
            cx.damage.add(self.quit.rect);
        }
    }

    fn layout(&mut self, cx: &mut Cx, frame: Rect) {
        let moved = frame != self.frame;
        if moved {
            if (frame.w - self.w).abs() > f32::EPSILON {
                self.w = frame.w;
                self.dirty = true;
            }
            let old = self.frame;
            self.frame = frame;
            if self.visible(cx.now) {
                cx.damage.add_moved(old, frame);
            }
        }
        if moved && !self.dirty {
            self.place_all(cx);
        }
    }

    fn event(&mut self, cx: &mut Cx, ev: &Event) -> Handled {
        if !self.shown || !self.visible(cx.now) {
            return Handled::No;
        }
        match ev {
            Event::Key(k) => {
                if !self.name_active(cx) {
                    return Handled::No;
                }
                match self.name_input.key(cx, k) {
                    InputOutcome::Submit { .. } => {
                        self.commit_name(cx);
                        Handled::Yes
                    }
                    // Escape is not consumed by a Qt TextInput: it reaches the scene (collapse)
                    InputOutcome::Escape => Handled::No,
                    InputOutcome::Paste => {
                        cx.msgs.push(Msg::RequestPaste);
                        Handled::Yes
                    }
                    InputOutcome::Changed | InputOutcome::None => Handled::Yes,
                }
            }
            Event::Paste(t) => {
                if !self.name_active(cx) {
                    return Handled::No;
                }
                self.name_input.paste(cx, t);
                Handled::Yes
            }
            Event::PointerLeave => {
                self.feed(cx, ev);
                self.name_input.pointer(cx, ev);
                Handled::No
            }
            Event::PointerDown { pos, button: Button::Left, .. } if self.name_input.rect().contains(*pos) => {
                if *cx.focus != FocusId::PetName {
                    *cx.focus = FocusId::PetName;
                    self.name_focus_seen = cx.window_focused;
                    self.name_editing = true;
                }
                let h = self.name_input.pointer(cx, ev);
                self.name_drag = h == Handled::Grab;
                h
            }
            Event::PointerUp { .. } | Event::PointerMove { .. } if self.name_drag => {
                if matches!(ev, Event::PointerUp { .. }) {
                    self.name_drag = false;
                }
                self.name_input.pointer(cx, ev);
                Handled::Yes
            }
            _ => {
                if let Event::PointerMove { pos } = ev {
                    if self.name_input.rect().contains(*pos) {
                        self.name_input.pointer(cx, ev);
                    }
                }
                let (h, click) = self.feed(cx, ev);
                if let Some(c) = click {
                    if let Some(m) = self.msg_for(c) {
                        cx.msgs.push(m);
                    }
                    return Handled::Yes;
                }
                h
            }
        }
    }

    fn paint(&mut self, s: &mut dyn Surface, pc: &PaintCx) {
        let now = pc.now;
        let op = self.opacity.value(now);
        if op <= 0.01 || !self.frame.inflate(0.0, 2.0).intersects(&pc.clip) {
            return;
        }
        let clip = pc.clip;
        let (x, w, top) = (self.frame.x, self.w, self.frame.y);
        let o = self.off;
        s.save();
        if op < 1.0 {
            s.mul_opacity(op);
        }
        let vis = |y: f32, h: f32| Rect::new(x - 4.0, y - 4.0, w + 8.0, h + 8.0).intersects(&clip);
        // MASCOTE
        if vis(top + o.l_pets, SECTION_LABEL_H) {
            paint_section_label(s, Rect::new(x, top + o.l_pets, w, SECTION_LABEL_H), "MASCOTE", None);
        }
        if vis(top + o.t_pets, PET_TILE_H + 24.0) {
            #[allow(clippy::needless_range_loop)]
            for i in 0..3 {
                let t = &self.pet_tiles[i];
                let r = t.ma.rect;
                t.paint(s, now);
                self.pets[i].paint(s, self.pet_rect(i), now);
                let (label, selected) = (PETS[i].2, t.selected);
                let st = if selected {
                    txt(12.0, Weight::DemiBold, theme::TEXT)
                } else {
                    txt(12.0, Weight::Regular, theme::TEXT2)
                };
                let lw = s.measure(label, &st);
                let lh = s.line_height(&st);
                s.text(label, r.x + (r.w - lw) * 0.5, r.bottom() - 9.0 - lh, &st);
            }
        }
        // name field
        if vis(top + o.name, NAME_H) {
            let r = self.name_rect;
            let active = self.name_active_paint(pc);
            s.fill_rrect(r, 14.0, &Fill::Solid(theme::CARD));
            let bc = if active { pc.env.accent.with_alpha(0.75) } else { theme::STROKE };
            s.stroke_rrect(r.inflate(-0.5, -0.5), 13.5, &Fill::Solid(bc), 1.0);
            paint_text_centered(s, "Nome", r.x + 14.0, r.y + NAME_H * 0.5, &txt(13.0, Weight::Regular, theme::TEXT2));
            self.name_input.paint(s, pc, active, pc.env.accent.with_alpha(0.6), None);
        }
        // COMPORTAMENTO
        if vis(top + o.l_beh, SECTION_LABEL_H) {
            paint_section_label(s, Rect::new(x, top + o.l_beh, w, SECTION_LABEL_H), "COMPORTAMENTO", None);
        }
        if vis(top + o.beh, BEH_H) {
            let card = Rect::new(x, top + o.beh, w, BEH_H);
            Self::card(s, card, 14.0);
            let titles = ["Puxar a próxima ao concluir", "Iniciar junto com o sistema", self.tips_title.as_str()];
            let subs = [
                "A primeira da fila entra em foco na hora",
                "Abre o notch quando você faz login",
                "Um balãozinho chato ao abrir e uma vez por dia",
            ];
            let (ts, ss) = (txt(13.0, Weight::Regular, theme::TEXT), txt(11.0, Weight::Regular, theme::TEXT3));
            let (tlh, slh) = (s.line_height(&ts), s.line_height(&ss));
            for i in 0..3 {
                let ry = card.y + i as f32 * BEH_ROW_H;
                if i > 0 {
                    s.fill_rrect(Rect::new(x + 14.0, ry, w - 28.0, 1.0), 0.0, &Fill::Solid(theme::STROKE));
                }
                let colh = tlh + 1.0 + slh;
                let ty = ry + (BEH_ROW_H - colh) * 0.5;
                s.text(titles[i], x + 14.0, ty, &ts);
                s.text(subs[i], x + 14.0, ty + tlh + 1.0, &ss);
                self.toggles[i].paint(s, pc);
            }
        }
        // MONITOR
        if vis(top + o.l_mon, SECTION_LABEL_H) {
            paint_section_label(s, Rect::new(x, top + o.l_mon, w, SECTION_LABEL_H), "MONITOR", None);
        }
        if vis(top + o.t_mon, MON_H) {
            for t in &self.screens {
                t.tile.paint(s, now);
                let r = t.tile.ma.rect;
                let c = Point::new(r.x + r.w * 0.5, r.y + 10.0 + 10.0);
                let (icon, col) = (
                    if t.key.is_empty() { Icon::Star } else { Icon::Display },
                    if t.tile.selected { pc.env.accent } else { theme::TEXT2 },
                );
                paint_icon(s, icon, c, 20.0, col, DEFAULT_STROKE);
                let tw = r.w - 12.0;
                let ts = if t.tile.selected {
                    txt(12.0, Weight::DemiBold, theme::TEXT)
                } else {
                    txt(12.0, Weight::Regular, theme::TEXT2)
                };
                let title = s.elide(&t.title, &ts, tw);
                let w1 = s.measure(&title, &ts);
                s.text(&title, r.x + (r.w - w1) * 0.5, r.y + 35.0, &ts);
                let ss = txt(10.0, Weight::Regular, theme::TEXT3);
                let sub = s.elide(&t.sub, &ss, tw);
                let w2 = s.measure(&sub, &ss);
                s.text(&sub, r.x + (r.w - w2) * 0.5, r.y + 52.0, &ss);
            }
        }
        let hint_st = txt(11.0, Weight::Regular, theme::TEXT3);
        let hlh = s.line_height(&hint_st);
        if vis(top + o.hint_mon, self.mon_hint.len() as f32 * hlh) {
            for (i, l) in self.mon_hint.iter().enumerate() {
                s.text(l, x + 4.0, top + o.hint_mon + i as f32 * hlh, &hint_st);
            }
        }
        // COR DE DESTAQUE
        if vis(top + o.l_acc, SECTION_LABEL_H) {
            paint_section_label(s, Rect::new(x, top + o.l_acc, w, SECTION_LABEL_H), "COR DE DESTAQUE", None);
        }
        if vis(top + o.acc, ACC_H) {
            Self::card(s, Rect::new(x, top + o.acc, w, ACC_H), 14.0);
            for (i, sw) in self.swatches.iter().enumerate() {
                let c = sw.ma.rect.center();
                if sw.selected {
                    let d = sw.ring.value(now);
                    let ring = Rect::new(c.x - d * 0.5, c.y - d * 0.5, d, d);
                    s.stroke_rrect(ring.inflate(-1.0, -1.0), d * 0.5 - 1.0, &Fill::Solid(theme::TEXT), 2.0);
                }
                let k = sw.scale.value(now);
                let r = Rect::new(c.x - 9.0 * k, c.y - 9.0 * k, 18.0 * k, 18.0 * k);
                let fill = match swatch_color(i) {
                    Some(col) => Fill::Solid(col),
                    None => Fill::Linear {
                        p0: Point::new(r.x, c.y),
                        p1: Point::new(r.right(), c.y),
                        stops: vec![
                            (0.0, Color::rgb8(0x31, 0x5b, 0xef)),
                            (0.5, Color::rgb8(0x92, 0x6e, 0xe4)),
                            (1.0, Color::rgb8(0xf7, 0x4f, 0x9e)),
                        ],
                    },
                };
                s.fill_rrect(r, r.w * 0.5, &fill);
            }
        }
        // ATALHOS GLOBAIS
        if vis(top + o.l_sc, SECTION_LABEL_H) {
            paint_section_label(s, Rect::new(x, top + o.l_sc, w, SECTION_LABEL_H), "ATALHOS GLOBAIS", None);
        }
        if vis(top + o.sc, SC_H) {
            let card = Rect::new(x, top + o.sc, w, SC_H);
            Self::card(s, card, 14.0);
            let ls = txt(12.0, Weight::Regular, theme::TEXT2);
            let cs = txt(11.0, Weight::Regular, theme::TEXT);
            for (i, row) in self.sc_rows.iter().enumerate() {
                let ry = card.y + 8.0 + i as f32 * (SC_ROW_H + 2.0);
                let cy = ry + SC_ROW_H * 0.5;
                paint_text_centered(s, row.label, x + 14.0, cy, &ls);
                let mut right = x + w - 14.0;
                for (k, cw) in row.keys.iter().zip(&row.widths).rev() {
                    let r = Rect::new(right - cw, cy - 10.0, *cw, 20.0);
                    paint_chip(s, r, 6.0, k, &cs, true);
                    right -= cw + 4.0;
                }
            }
        }
        if vis(top + o.hint_sc, self.sc_hint.len() as f32 * hlh) {
            for (i, l) in self.sc_hint.iter().enumerate() {
                s.text(l, x + 4.0, top + o.hint_sc + i as f32 * hlh, &hint_st);
            }
        }
        // quit + footer
        if vis(top + o.quit, QUIT_ROW_H) {
            let q = self.quit.rect;
            s.fill_rrect(q, 17.0, &Fill::Solid(self.quit_bg.value(now)));
            let qs = txt(12.0, Weight::DemiBold, theme::RED);
            paint_icon(s, Icon::Power, Point::new(q.x + 13.0 + 7.0, q.y + 17.0), 14.0, theme::RED, DEFAULT_STROKE);
            paint_text_centered(s, "Fim do show", q.x + 13.0 + 14.0 + 7.0, q.y + 17.0, &qs);
            let fs = txt(11.0, Weight::Regular, theme::TEXT3);
            let fw = s.measure(&self.footer, &fs);
            paint_text_centered(s, &self.footer, x + w - 4.0 - fw, top + o.quit + QUIT_ROW_H * 0.5, &fs);
        }
        s.restore();
    }

    fn next_wake(&self, now: Ms) -> Wake {
        let mut w = Wake::Idle;
        let vis = self.visible(now);
        if self.opacity.is_animating(now) {
            return Wake::Frame;
        }
        if !vis {
            return Wake::Idle;
        }
        if self.pet_tiles.iter().any(|t| t.animating(now))
            || self.screens.iter().any(|t| t.tile.animating(now))
            || self
                .swatches
                .iter()
                .any(|s| s.ring.is_animating(now) || s.scale.is_animating(now))
            || self.quit_bg.is_animating(now)
            || self.toggles.iter().any(|t| t.next_wake(now) != Wake::Idle)
        {
            return Wake::Frame;
        }
        for p in &self.pets {
            w = w.merge_at(p.next_wake(now), now);
        }
        w.merge_at(self.name_input.next_wake(now, self.shown_name_active), now)
    }

    fn bounds(&self) -> Rect {
        Rect::new(self.frame.x - 4.0, self.frame.y - 12.0, self.w + 8.0, self.h + 26.0)
    }
}

impl SettingsPanel {
    fn pet_rect(&self, i: usize) -> Rect {
        let r = self.pet_tiles[i].ma.rect;
        Rect::new(r.x + (r.w - 46.0) * 0.5, r.y + 12.0, 46.0, 46.0)
    }
    fn name_active_paint(&self, pc: &PaintCx) -> bool {
        self.shown && pc.has_focus(&FocusId::PetName)
    }
}

impl Section for SettingsPanel {
    fn height(&self) -> f32 {
        self.h
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cx::ScreenInfo;
    use crate::testkit::{FakePet, Harness};
    use std::path::PathBuf;

    const FRAME: Rect = Rect::new(18.0, 20.0, 512.0, 0.0);

    fn fake_pet() -> Box<dyn PetLike> {
        Box::new(FakePet::default())
    }

    fn real_pet() -> Box<dyn PetLike> {
        Box::new(crate::real::RealPet::new())
    }

    fn step(p: &mut SettingsPanel, h: &mut Harness, now: Ms) {
        h.with_cx(now, |cx| {
            p.update(cx);
            p.layout(cx, Rect::new(FRAME.x, FRAME.y, FRAME.w, p.height()));
            p.update(cx);
        });
    }
    fn open(p: &mut SettingsPanel, h: &mut Harness, now: Ms) -> Ms {
        p.set_shown(true, now);
        step(p, h, now);
        step(p, h, now + 250.0);
        step(p, h, now + 500.0);
        now + 500.0
    }
    fn mv(pt: Point) -> Event {
        Event::PointerMove { pos: pt }
    }
    fn click(p: &mut SettingsPanel, h: &mut Harness, now: Ms, pt: Point) -> Handled {
        h.with_cx(now, |cx| p.event(cx, &mv(pt)));
        let d = h.with_cx(now, |cx| p.event(cx, &Event::PointerDown { pos: pt, button: Button::Left, clicks: 1 }));
        h.with_cx(now, |cx| p.event(cx, &Event::PointerUp { pos: pt, button: Button::Left }));
        d
    }
    fn msgs(h: &mut Harness) -> Vec<Msg> {
        h.take_msgs()
    }
    fn two_screens(h: &mut Harness) {
        h.env.screens = vec![
            ScreenInfo { name: "HDMI-1".into(), label: "HDMI-1".into(), primary: true },
            ScreenInfo { name: "HDMI-2".into(), label: "HDMI-2".into(), primary: false },
        ];
        h.env.rev += 1;
    }

    #[test]
    fn section_heights_follow_the_qml_column() {
        let mut h = Harness::new();
        let mut p = SettingsPanel::with_pets(fake_pet);
        step(&mut p, &mut h, 0.0);
        let o = p.off;
        // label 20 + 10 spacing
        assert_eq!(o.t_pets - o.l_pets, 30.0);
        assert_eq!(o.name - o.t_pets, 92.0 + 10.0);
        // 3 behaviour rows: 150 px card (music row removed), then spacer(2) and label
        assert_eq!(o.l_mon - o.beh, 150.0 + 10.0 + 2.0 + 10.0);
        assert_eq!(o.t_mon - o.l_mon, 30.0);
        assert_eq!(o.acc - o.l_acc, 30.0);
        assert_eq!(o.hint_sc - o.sc, 164.0 + 10.0, "5 rows * 28 + 4 * 2 + 16");
        // the height is the end of the quit row
        assert_eq!(p.height(), o.quit + 36.0);
        assert!(p.shown());
        // a second monitor adds a line only when the hint wraps; the single-monitor hint differs
        let single = p.mon_hint.clone();
        two_screens(&mut h);
        step(&mut p, &mut h, 1.0);
        assert_ne!(p.mon_hint, single);
        assert_eq!(p.screens.len(), 3, "Principal + 2");
    }

    #[test]
    fn pets_follow_visibility_and_wakes_go_idle() {
        let mut h = Harness::new();
        let mut p = SettingsPanel::with_pets(fake_pet);
        step(&mut p, &mut h, 0.0);
        assert!(!p.pets_active(), "hidden at start");
        assert_eq!(p.next_wake(0.0), Wake::Idle);
        let t = open(&mut p, &mut h, 100.0);
        assert!(p.pets_active());
        assert!(p.visible(t));
        assert_eq!(p.next_wake(t), Wake::Ambient, "fake pets breathe while visible");
        // fade out: still visible for 200 ms, pets stop after it
        p.set_shown(false, t);
        step(&mut p, &mut h, t + 100.0);
        assert!(p.pets_active());
        assert_eq!(p.next_wake(t + 100.0), Wake::Frame);
        step(&mut p, &mut h, t + 250.0);
        assert!(!p.pets_active());
        assert!(!p.visible(t + 250.0));
        assert_eq!(p.next_wake(t + 250.0), Wake::Idle);
        // hidden panels ignore events
        let d = click(&mut p, &mut h, t + 300.0, Point::new(100.0, 100.0));
        assert_eq!(d, Handled::No);
        assert!(msgs(&mut h).is_empty());
    }

    #[test]
    fn static_panel_sleeps_and_does_not_damage() {
        let mut h = Harness::new();
        let mut p = SettingsPanel::new(); // NullPet in tests: never wakes
        let t = open(&mut p, &mut h, 0.0);
        step(&mut p, &mut h, t + 400.0);
        h.damage.clear();
        step(&mut p, &mut h, t + 416.0);
        assert!(h.damage.is_empty(), "{:?}", h.damage.rects());
        assert_eq!(p.next_wake(t + 416.0), Wake::Idle);
    }

    #[test]
    fn toggles_emit_actions() {
        let mut h = Harness::new();
        let mut p = SettingsPanel::with_pets(fake_pet);
        let t = open(&mut p, &mut h, 0.0);
        let centre = |p: &SettingsPanel, i: usize| p.toggles[i].rect().center();
        let c0 = centre(&p, 0);
        click(&mut p, &mut h, t, c0);
        assert_eq!(msgs(&mut h), vec![Msg::Act(Action::SetAutoAdvance(false))], "autoAdvance starts on");
        let c1 = centre(&p, 1);
        click(&mut p, &mut h, t, c1);
        assert_eq!(msgs(&mut h), vec![Msg::Cmd(AppCmd::SetAutostart(true))]);
        let c2 = centre(&p, 2);
        click(&mut p, &mut h, t, c2);
        assert_eq!(msgs(&mut h), vec![Msg::Act(Action::SetTipsEnabled(false))]);
        // the knob animates after the store changes
        h.store.state.settings.auto_advance = false;
        h.rev += 1;
        step(&mut p, &mut h, t + 10.0);
        assert_eq!(p.next_wake(t + 10.0), Wake::Frame);
        step(&mut p, &mut h, t + 500.0);
        step(&mut p, &mut h, t + 501.0);
        assert!(!p.toggles[0].checked());
    }

    #[test]
    fn tiles_swatches_and_quit_emit_commands() {
        let mut h = Harness::new();
        two_screens(&mut h);
        let mut p = SettingsPanel::with_pets(fake_pet);
        let t = open(&mut p, &mut h, 0.0);
        // mascot tile "Fantasma"
        let c = p.pet_tiles[1].ma.rect.center();
        click(&mut p, &mut h, t, c);
        assert_eq!(msgs(&mut h), vec![Msg::Act(Action::SetPetKind("ghost".into()))]);
        // monitor tiles: Principal, then HDMI-1, HDMI-2
        let c = p.screens[0].tile.ma.rect.center();
        click(&mut p, &mut h, t, c);
        assert_eq!(msgs(&mut h), vec![Msg::Cmd(AppCmd::Screen(ScreenArg::Primary))]);
        let c = p.screens[2].tile.ma.rect.center();
        click(&mut p, &mut h, t, c);
        assert_eq!(msgs(&mut h), vec![Msg::Cmd(AppCmd::Screen(ScreenArg::Named("HDMI-2".into())))]);
        // accent swatches: auto = None, #926ee4 = index 2
        let c = p.swatches[0].ma.rect.center();
        click(&mut p, &mut h, t, c);
        assert_eq!(msgs(&mut h), vec![Msg::Cmd(AppCmd::SetAccent(None))]);
        let c = p.swatches[2].ma.rect.center();
        click(&mut p, &mut h, t, c);
        assert_eq!(msgs(&mut h), vec![Msg::Cmd(AppCmd::SetAccent(Color::hex("#926ee4")))]);
        // quit
        let c = p.quit.rect.center();
        click(&mut p, &mut h, t, c);
        assert_eq!(msgs(&mut h), vec![Msg::Cmd(AppCmd::Quit)]);
        // releasing outside a pressed tile does not click
        let c = p.pet_tiles[0].ma.rect.center();
        h.with_cx(t, |cx| p.event(cx, &mv(c)));
        h.with_cx(t, |cx| p.event(cx, &Event::PointerDown { pos: c, button: Button::Left, clicks: 1 }));
        h.with_cx(t, |cx| p.event(cx, &Event::PointerUp { pos: Point::new(c.x + 500.0, c.y), button: Button::Left }));
        assert!(msgs(&mut h).is_empty());
    }

    fn type_text(p: &mut SettingsPanel, h: &mut Harness, s: &str) {
        for ch in s.chars() {
            let k = Event::Key(foco_base::KeyEvent {
                key: foco_base::Key::Char(ch),
                mods: foco_base::Mods::NONE,
                text: Some(ch.to_string()),
            });
            h.with_cx(1000.0, |cx| p.event(cx, &k));
        }
    }
    fn key(p: &mut SettingsPanel, h: &mut Harness, k: foco_base::Key) -> Handled {
        let e = Event::Key(foco_base::KeyEvent { key: k, mods: foco_base::Mods::NONE, text: None });
        h.with_cx(1000.0, |cx| p.event(cx, &e))
    }

    #[test]
    fn name_field_commits_on_enter_and_blur_and_trims() {
        let mut h = Harness::new();
        h.store.state.settings.pet_name = "Foquinho".into();
        let mut p = SettingsPanel::with_pets(fake_pet);
        let t = open(&mut p, &mut h, 0.0);
        assert_eq!(p.name_input.text, "Foquinho");
        // keys are ignored until the field has focus
        assert_eq!(key(&mut p, &mut h, foco_base::Key::Enter), Handled::No);
        let c = p.name_input.rect().center();
        click(&mut p, &mut h, t, c);
        assert_eq!(h.focus, FocusId::PetName);
        step(&mut p, &mut h, t + 1.0);
        p.name_input.select_all();
        type_text(&mut p, &mut h, "  Bolinha ");
        assert_eq!(key(&mut p, &mut h, foco_base::Key::Enter), Handled::Yes);
        assert_eq!(msgs(&mut h), vec![Msg::Act(Action::SetPetName("Bolinha".into()))]);
        // max 20 characters
        type_text(&mut p, &mut h, "xxxxxxxxxxxxxxxxxxxxxxxxxx");
        assert_eq!(p.name_input.text.chars().count(), 20);
        // Escape is not consumed (it collapses the notch in the scene)
        assert_eq!(key(&mut p, &mut h, foco_base::Key::Escape), Handled::No);
        // blur commits
        h.store.state.settings.pet_name = "Bolinha".into();
        h.rev += 1;
        h.focus = FocusId::None;
        step(&mut p, &mut h, t + 10.0);
        let m = msgs(&mut h);
        assert_eq!(m.len(), 1, "{m:?}");
        assert!(matches!(&m[0], Msg::Act(Action::SetPetName(n)) if n.len() == 20 || n.starts_with("Bolinha")));
        // a blank name is never sent
        p.name_input.set_text("   ");
        h.focus = FocusId::PetName;
        step(&mut p, &mut h, t + 20.0);
        h.focus = FocusId::None;
        step(&mut p, &mut h, t + 30.0);
        assert!(msgs(&mut h).is_empty());
        // closing the panel drops the field's focus
        h.focus = FocusId::PetName;
        p.set_shown(false, t + 40.0);
        step(&mut p, &mut h, t + 41.0);
        assert_eq!(h.focus, FocusId::None);
    }

    fn out_dir() -> PathBuf {
        let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/ui-test/wave3c");
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn png_snapshot() {
        let mut h = Harness::new();
        two_screens(&mut h);
        h.env.screen_choice = "HDMI-2".into();
        h.env.accent_choice = Color::hex("#926ee4");
        h.env.accent = Color::hex("#926ee4").unwrap();
        h.env.shortcut_keys = vec![("hide", vec!["Ctrl".into(), "Alt".into(), "H".into()])];
        h.store.state.total_done = 42;
        h.store.state.settings.pet_kind = "ghost".into();
        h.store.state.settings.pet_name = "Foquinho".into();
        h.env.autostart = true;
        let mut p = SettingsPanel::with_pets(real_pet);
        let t = open(&mut p, &mut h, 0.0);
        // hover the slime tile
        let c = p.pet_tiles[2].ma.rect.center();
        h.with_cx(t, |cx| p.event(cx, &mv(c)));
        step(&mut p, &mut h, t + 300.0);
        step(&mut p, &mut h, t + 301.0);
        let mut s = foco_base::PixSurface::new(600, (p.height() * crate::cx::S) as u32 + 80).unwrap();
        s.clear(theme::NOTCH);
        s.scale(crate::cx::S, crate::cx::S);
        h.with_pc(t + 301.0, Rect::new(0.0, 0.0, 760.0, 2000.0), |pc| p.paint(&mut s, pc));
        s.save_png(out_dir().join("settings-full.png")).unwrap();
    }
}
