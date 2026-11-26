# foco-ui + foco-max: UI design (waves 2 and 3)

Scope: the `foco-ui` crate (widgets + views) and the `foco-max` binary (`src/app.rs` = `qml/Main.qml`,
`src/main.rs` = startup and frame loop). The QML is the spec. **Music is gone**: no `MusicCard`, `Disc`,
`MiniBars`, `musicW`/`musicOn`, music alerts, the music settings row or the music shortcut rows. Read
`foco-base` (frozen, API may only grow) before writing code.

Conventions used below:
- **Logical px** = QML units. The window is 760×760 logical, 792×792 device. `S = 25/24`.
  Views lay out and paint in logical px. Only damage, the input region and incoming pointer events
  are converted (`device = logical * S`, `logical = device / S`).
- `now: Ms` is monotonic (`mono_ms`). `epoch: f64` is wall clock (`epoch_ms`); the store and task
  timers use it. To turn an epoch deadline into a monotonic one: `now + (t_epoch - epoch)`.
- "QML `X.qml:N`" means the line that defines the behaviour.

---

## 1. Architecture: retained views, explicit layout

Use **retained structs with hand-written layout** (not immediate mode). Reasons: each element has
to remember its previous rect for damage, its running `Anim`s and its hover/press state, and
the QML implicit sizes feed back into the notch springs. Immediate mode would rebuild all of that
every frame and could not damage less than the whole notch.

QML to Rust mapping:
- `anchors.*`, `Row`/`Column`: inline arithmetic in `layout()`. A `Column` **skips invisible children for
  both size and spacing** (this matters in MainSections, ExpandedView and SettingsPanel).
- `implicitWidth/Height`: computed in `update()` and read by the parent through an inherent getter
  (`CollapsedView::implicit_width()`, `ExpandedView::implicit_height()`, `Section::height()`).
- `Behavior on p { NumberAnimation }`: `foco_base::Anim` with `animate_to` (a no-op when the target is unchanged,
  like QML). `SequentialAnimation`: `Seq` or a few chained `Anim`s. `ColorAnimation`:
  `widgets::ColorAnim` (lerp over an `Anim` 0→1).
- `visible: opacity > 0.01`: `fn visible(&self, now) -> bool`. Invisible views get no events, no wakes and
  no paint, and their pets are `set_active(false)`.
- `Item.opacity`: `s.save(); s.mul_opacity(a); …; s.restore()`. Per-primitive opacity is what Qt does
  without a layer, so this matches.
- `scale`/`Translate`: `s.translate(cx,cy); s.scale(k,k); s.translate(-cx,-cy)` about the item centre
  (QML's default `transformOrigin`). The tip bubble uses `(tailX, 0)` as its origin.

### Per-iteration pipeline (all views)
```
update(cx)   pull store/env changes, retarget anims, recompute strings + implicit sizes, damage changes
[app]        targets from implicit sizes → springs step → notch geometry
layout(cx,frame)  place in window logical coords; if a rect moved: damage old ∪ new
paint(s,pc)  only if bounds ∩ pc.clip
next_wake(now)
```
`event()` runs between iterations. It pushes `Msg`s; the app applies them after dispatch, so no view
ever borrows the store mutably.

### Core types (`foco-ui/src/cx.rs`, `view.rs`, `model.rs`) — frozen by wave 2
```rust
pub const S: f32 = 25.0 / 24.0;
pub const WIN: f32 = 760.0;            // logical window side

pub trait StoreRead {                  // wave 2 implements it for foco_core::Store
    fn state(&self) -> &StateFile;     // focus/queue/done/settings/total_done
    fn celebrate_until(&self) -> f64;  // epoch ms
    fn first_run(&self) -> bool;
}

pub trait TextMeasure {                // implemented over foco-gfx fonts (outside a render pass)
    fn measure(&mut self, s: &str, st: &TextStyle) -> f32;
    fn line_height(&mut self, st: &TextStyle) -> f32;
    fn elide(&mut self, s: &str, st: &TextStyle, max_w: f32) -> String;
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScreenInfo { pub name: String, pub label: String, pub primary: bool }

pub struct Env {                       // owned by App; `rev` bumps on any change
    pub rev: u64,
    pub accent: Color,                 // choice or auto (#315bef, see §10)
    pub accent_choice: Option<Color>,
    pub autostart: bool,
    pub screens: Vec<ScreenInfo>,      // sorted like Backend::sortedScreens
    pub screen_choice: String,         // "" = primary
    pub current_screen: String,
    pub shortcut_keys: Vec<(&'static str, Vec<String>)>, // cmd -> ["Meta","Alt","N"] or ["—"]
    pub quip: Option<String>,          // Store.quip (App-owned, 2600 ms)
    pub tip_talking: bool,             // Store.tipTalking
}
impl Env { pub fn keys_for(&self, cmd: &str, letter: &str) -> Vec<String>; pub fn accent_text(&self) -> Color; }

#[derive(Clone, Debug, PartialEq)]
pub enum FocusId { None, NewTask, Rename(String /*tid*/), PetName }

pub enum Msg {
    Act(Action),                       // store mutations
    Cmd(AppCmd),                       // Open, Collapse, Hide, Show, Quit, Open/CloseSettings,
                                       // SetAccent, SetAutostart, Screen(..)
    TipAnswer { never: bool },
    Say(String),                       // petted quips; goes through the same path as Effect::Say
    RequestPaste,
}

pub struct DamageSink { /* Damage in device px + scale */ }
impl DamageSink {
    pub fn add(&mut self, r: Rect);    // logical; → (r*S).round_out() inflated 1px, clipped to window
    pub fn add_moved(&mut self, old: Rect, new: Rect) { if old != new { self.add(old); self.add(new) } }
}

pub struct Cx<'a> {
    pub now: Ms, pub epoch: f64,
    pub rev: u64,                      // model revision (bumped by App on ModelChanged/SettingsChanged)
    pub store: &'a dyn StoreRead,
    pub env: &'a Env,
    pub text: &'a mut dyn TextMeasure,
    pub damage: &'a mut DamageSink,
    pub msgs: &'a mut Vec<Msg>,
    pub focus: &'a mut FocusId,
    pub window_focused: bool,          // active focus = window_focused && *focus == mine
    pub cursor: CursorShape,           // views set it on hover; App forwards changes to X11
}

pub struct PaintCx<'a> { pub now: Ms, pub epoch: f64, pub clip: Rect, pub env: &'a Env,
                         pub store: &'a dyn StoreRead, pub focus: &'a FocusId, pub window_focused: bool }

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Handled { No, Yes, Grab }     // Grab = capture pointer until PointerUp

pub trait View {
    fn update(&mut self, cx: &mut Cx);
    fn layout(&mut self, cx: &mut Cx, frame: Rect);
    fn event(&mut self, cx: &mut Cx, ev: &Event) -> Handled;   // Event positions in logical px
    fn paint(&mut self, s: &mut dyn Surface, pc: &PaintCx);
    fn next_wake(&self, now: Ms) -> Wake;
    fn bounds(&self) -> Rect;          // painted extent incl. pet overdraw / shadows
}
/// Expanded-body sections stacked in a Column.
pub trait Section: View { fn height(&self) -> f32; fn shown(&self) -> bool { self.height() > 0.0 } }
```
`model.rs` helpers: `mood(store, epoch)`, `today(store, epoch) -> (u32, i64)`,
`done_today(store, epoch) -> usize`, `pet_line(mood, n_focus, n_queue, quip)` (Store.qml:78-92 without the
music branch), `next_tick(task, epoch, now, period) -> Option<Ms>` (the monotonic time when
`floor(elapsed/period)` next changes; `None` if the task is paused), and
`next_today_minute(store, epoch, now)`.

---

## 2. Damage model

Every element keeps `rect` (last laid out) and repaints only when its **quantized visual state** changes.
Rules:
1. Damage the element's own rect, never its parent's. Moves: `add_moved(old, new)`.
2. Compare before damaging: text strings (`!=`), colours (after `ColorAnim`), opacity (Δ ≥ 1/255),
   positions/scales (Δ ≥ 0.05 px). `Anim::is_animating` alone is not a reason.
3. PointerMove damages only when a hover flag flips (or the header pet's look target changes).
4. Clip damage to the enclosing clip (notch clipper, flick viewport) before adding.
5. Pets: damage `pet_rect.inflate(pet.overdraw())` only when `pet.update(now)` returns true.

Typical frames:
- **Notch spring/radius/shadow animating** (`!spr_w.settled() || !spr_h.settled() || anim_r/shadow running`):
  the App adds `notch_bbox(prev) ∪ notch_bbox(cur)`, where `notch_bbox` covers the shape incl. ears,
  plus shadow overhang (inflate x 36, bottom +50) when shadow opacity > 0. View damage still flows,
  but `Damage` merges it into the bbox. Nothing else is needed for crossfades that happen inside it.
- **Ambient only** (collapsed, idle): the pet's box (26 px + overdraw), the liveDot 6×6 when running
  (900+900 ms InOutSine, CollapsedView.qml:133-139), the dot pulse ring (≤33×33), the tip float (whole
  tip rect, ±2 px).
- **Clock tick**: collapsed shows the timer text rect (old ∪ new width). If its width changes (h:mm:ss),
  `implicit_width` changes, the spring retargets, and the frame becomes a notch frame. Expanded shows
  each running card's timer text + the 48×48 ring. Header shows the stat pill only when its string changes.
- **Hover**: that row, button or card only. Notch hover changes `targetH` 38→42 and becomes a spring frame.
- **Scroll**: the whole flick viewport.
- **Lists in transition** (`KeyedList::is_animating`): union of the list's bounds before and after.

---

## 3. Wake model

```rust
fn next_wake(&self, now) -> Wake  // per view: Frame if any Anim/Seq is running and the view is visible,
                                   // Ambient/At from pets (pet.next_wake), At(next_tick) for clocks, else Idle
```
The App merges: `views.fold(Wake::Idle, |w, v| w.merge_at(v, now))`,
`.merge_at(Wake::at(now, sched.next_deadline()?), now)`,
`.merge_at(Wake::at(now, epoch_to_mono(store.next_change(epoch)?)), now)`, plus `Frame` while a spring is
unsettled. `main.rs` arms the single timerfd at `w.deadline(now)`, or disarms it on `Idle`.
Frame cadence: when the previous wake was `Frame`, use `last_frame + FRAME_MS` (not `now + FRAME_MS`) to avoid drift.

What can wake us: springs, `Anim`s, pets (breathing at 30 Hz only while active, blink/fidget as `At`),
clock ticks (only for **visible** running timers: collapsed shown task at 1 s; expanded cards at 1 s for
text and 500 ms for the ring, as Store.ticker was 2 Hz), today-pill minute, liveDot/dot pulse/tip
float (Ambient), text cursor blink (500 ms, only while an input has active focus), and Scheduler keys
(§7). Hidden views return `Idle`. **Nothing visible animating and no deadline ⇒ the loop blocks in epoll forever.**

Suspend: `mono_ms` (Instant) does not advance during suspend. On every wake, if
`|(epoch - now) - last_offset| > 1000`, recompute all epoch-derived deadlines (midnight, tired,
ticks).

---

## 4. Input

- **Conversion**: `main.rs` divides `PointerMove/Down/Up/Wheel` positions by `S` before `app.on_event`.
- **On any PointerDown while `!window_focused`**: `x11.request_keyboard_focus()`. Openbox does not
  focus docks on click (platform_x11.cpp:268-274). This is what makes FocusOut-collapse and typing work.
- **Hit-test order** (topmost first): grab owner (if any) → TipView (when visible) → DotView (when shown)
  → notch content (ExpandedView / AlertView / CollapsedView, inside the clipper rect) → notch
  background (`MouseArea` over the whole notch incl. ears: click ⇒ `Cmd(Open)` when `!expanded`).
  A view returning `Grab` on Down receives all Moves/Up until Up. A **click** fires on Up when the Up
  lands inside the pressed element (MouseArea semantics). The IconButton scale (0.86 pressed in
  90 ms, 1.1 hovered, 1 otherwise in 260 ms, `OutBack` overshoot 2) follows `pressed && inside` / hover.
- **Hover** (HoverHandler/containsMouse): on PointerMove with no grab, every visible view updates its
  flags by containment, so nested elements both report hover (card + its button). `PointerLeave`
  clears everything. Notch hover = pointer inside the *current* notch rect
  (`x ∈ [notch.x, notch.x+notch.w)`, `y < curH`); `hovered = notch_hover && !expanded && alert.is_none()`.
- **Drag** (queue grip): Down on the grip ⇒ `Grab`. Moving > 10 px (Qt startDragDistance) starts the drag.
  See §6/§9c.
- **Double-click**: `PointerDown{clicks:2}` (foco-x11 counts the clicks). Queue title ⇒ rename.
- **Wheel**: ExpandedView's Scroll, only when `content_h > view_h + 1`. Target `clamp(target + dy, 0, max)`,
  animated 200 ms OutCubic. Clamp again whenever the content shrinks (StopAtBounds).
- **Keyboard**: `Key` goes to the focused TextInput (`*cx.focus`), if any. Unconsumed `Escape`
  ⇒ `Cmd(Collapse)` when expanded (Main.qml:256). Ctrl+V in an input ⇒ `Msg::RequestPaste` ⇒
  `x11.request_paste()` ⇒ `Event::Paste(text)` goes to the focus owner.
- **FocusIn/FocusOut**: set `window_focused`. On FocusOut: if `expanded && now - expand_stamp > 450`,
  collapse (Main.qml:143-146). Also, as with Qt's window deactivation, active focus drops visually
  (inputs redraw unfocused), the rename editor **commits**, and the pet-name field fires editingFinished.
- **Quick-add** (`add` with no text / quick-add shortcut): `expand(true)` (§7) sets `*focus = NewTask`.

---

## 5. TextInput widget (`widgets/text_input.rs`)

```rust
pub struct TextInput { pub text: String, cursor: usize /*byte, char boundary*/, anchor: usize,
                       pub max_chars: usize, pub align_right: bool, scroll_x: f32, style: TextStyle, rect: Rect, … }
pub enum InputOutcome { None, Changed, Submit { ctrl: bool }, Escape, Paste }
impl TextInput {
    pub fn new(style: TextStyle, max_chars: usize) -> Self;
    pub fn set_text(&mut self, s: &str);      // cursor to end, no selection
    pub fn select_all(&mut self);
    pub fn key(&mut self, cx: &mut Cx, k: &KeyEvent) -> InputOutcome;
    pub fn paste(&mut self, cx: &mut Cx, s: &str);
    pub fn pointer(&mut self, cx: &mut Cx, ev: &Event) -> Handled;   // click = caret, drag = select, dbl = word
    pub fn layout(&mut self, cx: &mut Cx, rect: Rect);
    pub fn paint(&self, s: &mut dyn Surface, pc: &PaintCx, active: bool, sel: Color, sel_text: Option<Color>);
    pub fn next_wake(&self, now: Ms, active: bool) -> Wake;           // caret blink 500 ms
}
```
Behaviour: insert `KeyEvent.text` (composed; dead keys already resolved; a replaced selection counts),
truncating so `chars().count() <= max_chars`. Backspace/Delete remove one char or the selection.
**Ctrl+Backspace** deletes back to the previous word start. Left/Right (Shift extends; Ctrl jumps words),
Home/End, **Ctrl+A** selects all, Ctrl+V returns `Paste` (owner pushes `RequestPaste`; pasted
newlines become spaces). Enter (any mods) returns `Submit{ctrl}`, Esc returns `Escape`. Caret: 1 px wide,
text colour, blinking 500/500 ms; reset to "on" on every edit. Horizontal scroll keeps the caret
visible (clip to rect). Selection fill: `accent.with_alpha(0.6)`. Damage = the input rect on any change.
No IME, no undo, no Ctrl+C/X (no clipboard ownership in foco-x11).

The **shake** belongs to InputField, not TextInput: `Seq::new(0).to(-6,50,Linear).to(6,70).to(-3,60).to(0,60)`,
used as an x offset of the whole field (InputField.qml:387-394).

---

## 6. Keyed list animator (`widgets/keyed_list.rs`)

```rust
pub struct Track { pub from: Option<f32>, pub to: f32, pub dur: Ms, pub easing: Easing }
pub struct Transition { pub opacity: Option<Track>, pub scale: Option<Track>, pub x: Option<Track> }
pub struct ListSpec { pub add: Transition, pub remove: Transition,
                      pub displaced: (Ms, Easing), pub move_displaced: (Ms, Easing) }
pub struct Fx { pub dx: f32, pub y: f32, pub opacity: f32, pub scale: f32 }
pub struct Entry<T> { pub key: String, pub data: T, pub index: Option<usize> /* None = removing */, … }

impl<T: Clone> KeyedList<T> {
    pub fn new(spec: ListSpec) -> Self;
    /// `items` in model order: (key, data, slot_y relative to list top). First call is instant.
    /// New keys → add transition. Vanished keys → kept with their last data, play remove,
    /// dropped when done. Live keys whose slot_y changed → y animates (displaced,
    /// or move_displaced when `dragging.is_some()`). `dragging`'s own y is not animated.
    pub fn sync<'k>(&mut self, items: impl Iterator<Item = (&'k str, T, f32)>, now: Ms, dragging: Option<&str>) -> bool;
    pub fn iter(&self, now: Ms) -> impl Iterator<Item = (&Entry<T>, Fx)>;  // removing entries last (drawn on top)
    pub fn is_animating(&self, now: Ms) -> bool;
    pub fn bounds(&self, now: Ms, x: f32, w: f32, row_h: impl Fn(&T) -> f32) -> Rect;
}
```
As in QML, the list height is the **final** content height (removals shrink it at once and the spring
absorbs it). Specs: FocusList (MainSections.qml:224-236), Queue (QueueList.qml:15-28). DoneSection is a
Repeater: no transitions.

Other widgets (wave 2, `widgets/`): `IconButton{icon,size,icon_size,color,hover_color,bg,hover_bg}`
(IconButton.qml; hover_bg default `#1fffffff`), `Toggle` (Toggle.qml: knob x OutBack 1.6 240 ms, colour
180 ms), `Pill` (h 17, r 8.5, `accent.with_alpha(a)`, 10 px DemiBold `accent_text`, padding 12) and `KeyChip`
(h 20/19, r 6/5, min-width), `SectionLabel` (SectionLabel.qml), `Label` helpers incl. `wrap(text,w)` for
the WordWrap texts, `Scroll`, `ColorAnim`, `Hover` (flag + damage helper), `notch.rs` (§9-wave 2).

---

## 7. App scene (`foco-max/src/app.rs`, port of Main.qml)

State: `booted, quitting, expanded, settings_open, quick_mode, expand_stamp, alert: Option<Alert>,
tip_pending, last_wave (=-∞), notch_hover, window_focused, focus: FocusId, env: Env, rev`.
Derived exactly as Main.qml:21-34: `hidden_mode = hidden && !quitting`;
`target_w = (!booted||quitting) ? 0 : hidden_mode ? 36 : expanded ? 548 : alert ? 400 : collapsed.implicit_width()`;
`target_h = … hidden_mode ? 0 : expanded ? expanded.implicit_height() : alert ? 62 : hovered ? 42 : 38`;
`target_r = expanded ? 32 : alert ? 28 : hovered ? 21 : 19`.
Springs: `Spring::new(0, 3.8, 0.32, 0.2)` (w), `Spring::new(0, 3.4, 0.38, 0.2)` (h); `r: Anim` 320 ms OutCubic;
shadow opacity `Anim` 280 ms Linear to 0.65/0.45/0.

View visibility (`set_shown`): collapsed `booted && !quitting && !expanded && !alert && !hidden_mode`;
alert `alert && !expanded && !hidden_mode && !quitting`; expanded `expanded && !quitting`;
dot `hidden_mode && booted`; tip `tip_pending && booted && !quitting && !expanded && !hidden_mode && !alert`.

Actions: `expand(with_input)`, `collapse()`, `hide_to_dot()`, `quit_app()`, `show_alert(a, ms)`, as in
Main.qml:44-82. `collapse()` also sets `focus = None` (firing rename/name commits) and
`x11.release_keyboard_focus()`. `expand(true)` sets `settings_open=false`, `quick_mode`,
`request_keyboard_focus()`, `focus = NewTask`, and schedules `quick-idle` (45 s: if the input is empty, collapse,
else reschedule).

Scheduler keys (`TimerKey(name,0)`): `boot` 250 ms → `booted=true`; if `first_run`, enable autostart and
schedule `welcome` 900 ms → alert {Info, "Oi! Eu sou o "+petName, "Clique aqui e registre sua primeira demanda"} 5200 ms.
`alert` (duration) → `alert=None`. `quit` 650 ms → exit. `wave` 380 ms → pet event "wave" (scheduled when
`expanded` turns true and `now - last_wave > 90 000`). `tip-startup` 3000 ms (7500 if first run) →
`maybe_tip(true)`. `tip-day` at next local midnight → `maybe_tip(false)` then re-arm (replaces the 60 s poll;
also schedule it at `now+60 s` when tips are re-enabled). `quip` 2600 ms → `env.quip=None`. `save` 400 ms
(debounced) → write state. `heartbeat` 60 s, only while `running_count>0` → save (lastSeen). `cycle` 5 s →
CollapsedView (owned by the view via its own `At` wake, not the App).

Alerts: there is **no queue**. A new alert replaces the current one and restarts its timer; it is ignored while
`expanded || hidden`. Kinds/durations: welcome 5200; Completed → {Done, "Concluída!",
title+"  ·  "+fmt_short(spent)} 2600; Added → first `if quick_mode { collapse() }`, then {Added, to_focus ?
"Em foco agora" : "Adicionada à fila", title} 2000; screen change after boot → {Info, "Cheguei!",
"Agora no monitor "+label} 2200.

Effects → scene: `PetEvent(n)` → `collapsed.pet_react`/`expanded.pet_react` (each ignores it if its pet is
not active; Pet.qml:84-85). `Say(t)` → `env.quip`, schedule `quip`. `ModelChanged`/`SettingsChanged`
→ `rev += 1`, and on a `hidden` flip run the mask/visibility update. `Save` → schedule `save`.
`Celebrate` → nothing (mood comes from `store.next_change`). Tip answer (Main.qml:116-122):
`SetLastTipDay(today)`; never ⇒ `SetTipsEnabled(false)` + `Say("Combinado, não falo mais!")`, else
`PetEvent("love")`; `env.tip_talking=false`. Petted (header pet tap) ⇒ `Say(one_of(..))` (ExpandedView.qml:51-53)
plus the pet's own `react("love",1900)`.

AppCmd (IPC lines via `split_line`+`parse_command`, shortcuts via `shortcut_command`, and view `Msg::Cmd`):
Toggle, Open, QuickAdd(None) → `expand(true)`, Hide (hidden ? show : hide_to_dot), Show
(`SetHidden(false)`), Quit, Screen(Next: cycle sorted monitors if ≥2 / Primary: choice "" / Named(n)),
SetAccent, SetAutostart, Open/CloseSettings, PetReact; `to_action()` covers the rest. Screen/accent changes
are persisted in foco-maxrc via foco-core.

Placement: target monitor = `screen_choice` if connected, else primary. `x = mon.x + (mon.w-792)/2, y = mon.y`.
On `ScreensChanged`, refresh `env.screens` and re-place. If `current_screen` changes and `booted`, show the "Cheguei!" alert.

Input region (Main.qml:204-216), recomputed when target_w/target_h/hidden_mode/tip visibility or tip rect changes
(never per spring frame): hidden ⇒ `(380-23, 2, 46, 46)`; else `w = max(target_w,8)+32`,
`h = max(target_h,6)+4`, rect `((760-w)/2, 0, w, h)`; plus `(tip.x-4, tip.y-2, tip.w+8, tip.h+8)` while
the tip is shown. Converted ×S (round out) for `x11.set_input_region`.

Notch geometry (Main.qml:258-300): `ear = min(12, h/2.6)`, `notch.w = max(0,cur_w)+2*ear`,
`notch.x = round((760-notch.w)/2)`, visible iff `h>0.5 && cur_w>1`. The clipper is the **rectangle**
`(notch.x+ear, 0, cur_w, cur_h)`. Children are horizontally centred in it, at y 0: collapsed `(implicit_w ×
(hovered?42:38))`, alert `400×62`, expanded `548×implicit_h`. Tip position:
`x = round((760 - collapsed.implicit_width())/2 + collapsed.pet_center() - 34)`, `y = round(max(cur_h,30)+8)`.

---

## 8. Frame loop (`foco-max/src/main.rs`)

```
main: t_exec from /proc/self/stat starttime; parse CLI → if a command and a server is running:
      ipc::send_to_running → exit 0. Else: Rc::load, Store::load, X11::connect, Renderer::new,
      ipc::Server::bind, Reactor::new(x11.fd, server.fd, signalfd{TERM,INT}, timerfd),
      x11.create_window(792,792) + place + set_input_region(1×1) + grab_shortcuts; App::new; map.
loop {
    reactor.arm(app.next_deadline(now));             // None ⇒ disarm
    let ready = reactor.wait();                      // blocks
    now = mono_ms(); epoch = epoch_ms();
    X fd     → for e in x11.dispatch(): Input(ev) → app.on_event(scale_to_logical(ev)),
               Shortcut(id) → app.on_cmd(shortcut_command(id)), FocusIn/Out, Expose → app.damage_all(),
               ScreensChanged → app.on_screens(x11.monitors()), PresentDone → (nothing), PasteText → Event::Paste
    IPC      → for line in server.drain(): app.on_cmd(parse_command(split_line(line)))
    signal   → app.save_now(); exit
    timer    → (nothing: app.tick pops due keys)
    app.tick(now, epoch);        // sched.pop_due, Action::Tick if store.next_change passed, apply msgs,
                                 // effects, update → springs → layout; collects PlatformReq
    for req in app.take_requests() { x11.place / set_input_region / request_keyboard_focus /
                                     release_keyboard_focus / request_paste / set_cursor / exit }
    if !app.damage.is_empty() && !x11.present_pending() {
        let rects = app.damage.take();
        for r in &rects { renderer.render(x11.buffer(), 792, 792, &[*r], &mut |c| app.paint(c, logical(r))); }
        x11.present(&rects);
    }   // if present is pending, damage stays queued; PresentDone wakes epoll and we render then
}
```
`App` never touches X11 directly (it returns `PlatformReq`s). That keeps `foco-max --snapshot <state.json>
<out.png> [collapsed|expanded|settings|alert|tip|dot]` working with a `PixSurface` and no display.

`FOCO_TRACE=1` (stderr, one line each): `exec→main µs`, `main→first present µs`,
`boot→first notch present µs`, and per frame `frame n wake=<timer|x|ipc> upd µs paint µs present µs rects k px N`.
Measure with `Instant`, print only when the env var is set (check once at startup).

---

## 9. Work split and file ownership

### Wave 2 (one agent, before wave 3)
Owns: `foco-ui/src/{lib.rs, cx.rs, view.rs, model.rs, notch.rs}`, `foco-ui/src/widgets/*`,
`foco-ui/src/views/mod.rs`, **stubs** of every view file listed below (compiling, correct public
signatures, empty paint), a working minimal `views/collapsed.rs` (pet + "Sem foco agora"/title + timer),
and all of `foco-max/src/{main.rs, app.rs, trace.rs}`, **app.rs complete** (§7) since wave 3 cannot
edit it. `notch.rs`: NotchShape path (NotchShape.qml:14-31: quad ears `(0,0)→(ear,earY)` with control
`(ear,0)`, CCW arcs of `r = max(0,min(radius,h/2,bw/2))`, `earY = max(0,min(ear,h-r))`), shadow
(`s.shadow(rect(ear,0,cur_w,h), min(cur_r,h/2), 34, -4, (0,14), black*opacity)`), and the clip rect.
Done when: on the real display the collapsed notch appears with its pet, a click expands (empty panel is fine),
Esc/click-outside/chevron collapse, the quick-add shortcut works, picom shows the transparency, and
`FOCO_TRACE` shows idle = no wakes besides pet ambient.

Frozen view interfaces (in `views/mod.rs` re-exports; wave 3 fills in the bodies only):
```rust
// (a) collapsed.rs
impl CollapsedView { pub fn new() -> Self; pub fn set_shown(&mut self, v: bool, now: Ms);
    pub fn set_hovered(&mut self, v: bool, now: Ms); pub fn implicit_width(&self) -> f32;
    pub fn pet_center(&self) -> f32; pub fn pet_react(&mut self, name: &str, now: Ms); }
// (a) alert.rs
pub enum AlertKind { Info, Done, Added }
pub struct Alert { pub kind: AlertKind, pub title: String, pub sub: String }
impl AlertView { pub fn new() -> Self; pub fn set_alert(&mut self, a: Option<&Alert>, now: Ms); // Some ⇒ last=a, restart pop
    pub fn set_shown(&mut self, v: bool, now: Ms); }
// (a) dot.rs    impl DotView { pub fn new() -> Self; pub fn set_shown(&mut self, v: bool, now: Ms); }  // click ⇒ Act(SetHidden(false))
// (a) tip.rs    impl TipView { pub const TAIL_X: f32 = 34.0; pub fn new() -> Self; pub fn set_shown(&mut self, v: bool, now: Ms);
//                   pub fn size(&self) -> Size; pub fn typing(&self) -> bool; pub fn visible(&self, now: Ms) -> bool; }
// (b) expanded.rs
impl ExpandedView { pub fn new() -> Self; pub fn set_shown(&mut self, v: bool, now: Ms);
    pub fn set_settings_open(&mut self, v: bool, now: Ms); pub fn implicit_height(&self) -> f32;
    pub fn focus_input(&mut self, cx: &mut Cx); pub fn input_empty(&self) -> bool; pub fn pet_react(&mut self, name: &str, now: Ms); }
// (b) focus_card.rs  pub struct FocusSection;  impl Section  (list + empty state)
// (b) input_field.rs pub struct InputField;    impl Section  + pub fn is_empty(&self) -> bool
// (c) queue.rs       pub struct QueueSection;  impl Section  (list only; label drawn by expanded.rs)
// (c) done.rs        pub struct DoneSection;   impl Section
// (c) settings.rs    pub struct SettingsPanel; impl Section
// all: pub fn new() -> Self
```
All views implement `View`. Every section is laid out in window coordinates with the scroll offset already
applied, so expanded.rs only forwards events (pointer inside the flick viewport) and paint (clip =
viewport ∩ pc.clip).

### Wave 3 (three agents in parallel; each edits only its files)
- **(a)** `views/collapsed.rs, alert.rs, dot.rs, tip.rs`: CollapsedView.qml (minus music), cycle 5 s + swap
  animation, controls; AlertView.qml with pop; Dot.qml (normal ball only, pulse); TipBubble.qml (shape path,
  shadow, typing, rows, buttons, float).
- **(b)** `views/expanded.rs, focus_card.rs, input_field.rs`: header (pet 66 interactive: look from pointer
  Pet.qml:341-351, tap = love + petted), name/level pill/pet line/XP bar, stat pill, three IconButtons,
  MainSections column + SettingsPanel crossfade (200 ms), the Flickable, the show/hide opacity +
  translate; FocusCard + list transitions + empty state; InputField.
- **(c)** `views/queue.rs, done.rs, settings.rs`: QueueList (grip drag with live `MoveQueue`, double-click
  rename with its own `TextInput` + `FocusId::Rename(tid)`), DoneSection, SettingsPanel (3 behaviour rows,
  monitor tiles, accent swatches, 5 shortcut rows, quit).

Rules: a missing widget gets written privately in the agent's own file. Public signatures in `views/mod.rs`
and `widgets/` are frozen and may only grow, via a note to the integrator. Each agent adds a
`--snapshot` state that shows its views.

---

## 10. Top parity traps (read the QML line before coding)

1. **Column skips invisible children** for spacing: MainSections.qml:196,311-327 (the 2 px spacer exists only when
   the queue is non-empty). ExpandedView height: `min(700, 12+70+12+body+20)`, ExpandedView.qml:19.
2. **Collapsed width** is `max(250, 14+26+9+min(titleImplicitW,230)+(badge? badgeW+8:0)+22+controlsW+timerW+16)`,
   but the title is drawn at x 50 (`+10`, not `+9`). `controlsW` jumps 0↔62 with no animation (the spring
   smooths it). Height 38→42 on hover re-centres the content 2 px lower: CollapsedView.qml:10,26-29,83; Main.qml:306.
3. **Notch clip is a rectangle**, not the notch shape, and the ears shrink when `h < 31.2` (`ear = min(12,h/2.6)`):
   Main.qml:260,295-300.
4. **Opacity Behaviors are asymmetric**: collapsed in = 120 ms pause + 240 OutCubic, out = 90 ms;
   alert 110+240 Linear / 100; expanded 90+280 OutCubic / 100 plus translate −14→0 420 ms. Collapsed scale
   0.92↔1 320 ms: CollapsedView.qml:33-40, AlertView.qml:14-21, ExpandedView.qml:21-32.
5. **Pet reactions require the pet to be visible** (`react` returns if `!animated`): Pet.qml:84-85. That is why the
   380 ms wave only plays on the header pet (Main.qml:127-133). Listening pets: collapsed and header only.
6. **Focus**: Openbox click-to-activate on press (platform_x11.cpp:268-274); FocusOut collapses only after
   450 ms (Main.qml:145); window deactivation drops active focus, so the rename commits (QueueList.qml:157) and the
   name field saves (SettingsPanel.qml:87). Clicking empty panel space does **not** unfocus the input.
7. **Alerts**: no queue; ignored while expanded/hidden; `onAdded` collapses quick mode *before* the alert;
   AlertView keeps `last` for the fade-out: Main.qml:77-82,197-200, AlertView.qml:5-6.
8. **FocusCard check** = green flash to 0.35 over 130 ms, *then* `Complete(index)`. Resolve the index by
   `tid` at that moment (it may have shifted). The removing card fades with the flash still on
   (FocusCard.qml:155-159). The ring sweep is `max(2°, 360·frac)` from −90° with a round cap.

Also: auto accent on X11 is **#315bef** (backend.cpp:360), not `theme::DEFAULT_ACCENT`; `Theme.a(c,x)` is
`with_alpha(x)` (absolute). Without the music row the behaviour card is **150** px tall
(SettingsPanel.qml:96,119), and the shortcuts card has 5 rows. A queue drag snaps back instantly on release, and
`MoveQueue` fires when the dragged row's centre enters another slot, clamped to `[-4, h-40+4]` (QueueList.qml:39,93-94).
Tip `y` follows the height spring every frame (Main.qml:335).

---

## 11. Recommended foco-base / sibling additions (additive only)

- `foco_base::event::CursorShape { Arrow, Hand, OpenHand, ClosedHand, IBeam }` + `foco_x11::X11::set_cursor`
  (the QML sets `cursorShape` everywhere; `Cx.cursor` uses it).
- `Rect::scale(self, k) -> Rect` and `Rect::to_device(self, k) -> IRect` (round_out + 1 px), used by DamageSink and the mask.
- `Seq::end_time(now) -> Option<Ms>`, for exact `At` wakes after a sequence instead of `Frame` polling.
- foco-gfx: a text measurer usable outside `render()` (e.g. `Renderer::fonts_mut()` implementing
  `foco_ui::TextMeasure`), and `render` honouring a single-rect clip with the canvas pre-scaled by `S`.
- foco-core `Store`: `celebrate_until()`, `first_run()`, `state()` getters (for `StoreRead`), and
  `next_change(epoch)` covering the celebrate end, the 50-min tired crossing and midnight (done-today/today pill).
