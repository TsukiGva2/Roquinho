//! foco-x11: DOCK window (ARGB visual, EWMH hints), XShape input region, MIT-SHM memfd
//! presentation, RandR monitors, xkb + Compose keyboard, CLIPBOARD paste and XGrabKey.
//!
//! Single-threaded and non-blocking in the hot paths: `dispatch()` only drains what is already
//! readable, `present()` is fire-and-forget (completion arrives as `ShmCompletion`), and
//! nothing here arms a timer, so an idle process never wakes up. Blocking round-trips happen
//! only on user actions (focus request, paste, monitor query) and at startup.

mod edid;
mod keyboard;
mod seq;
mod shmbuf;

use std::cell::RefCell;
use std::collections::VecDeque;
use std::os::fd::{AsRawFd, RawFd};

use foco_base::event::{Button, Event as InEvent};
use foco_base::geom::{IRect, Point};
use x11rb::connection::{Connection, RequestConnection};
use x11rb::properties::{WmHints, WmSizeHints, WmSizeHintsSpecification};
use x11rb::protocol::randr::{ConnectionExt as _, NotifyMask};
use x11rb::protocol::shape::{self, ConnectionExt as _};
use x11rb::protocol::shm::{self, ConnectionExt as _};
use x11rb::protocol::xkb::{self, ConnectionExt as _};
use x11rb::protocol::xproto::{
    self, AtomEnum, ClientMessageEvent, ColormapAlloc, ConfigureWindowAux, ConnectionExt as _, CreateGCAux,
    CreateWindowAux, EventMask, GrabMode, InputFocus, NotifyDetail, NotifyMode, PropMode, Window, WindowClass,
};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;
use x11rb::{CURRENT_TIME, NONE};

use keyboard::{Keyboard, RuleNames};

/// Pointer shapes `X11::set_cursor` can show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cursor {
    Arrow = 0,
    Hand,
    /// Open hand (draggable).
    Grab,
    /// Closed hand while dragging.
    Grabbing,
    IBeam,
}

/// Rectangles up to this many bytes are presented through the socket (`PutImage`), larger ones
/// through the shared-memory segment.
const SOCKET_PUT_MAX: usize = 96 * 1024;

pub use seq::{parse_sequence, Seq};

/// Pixels per mouse-wheel notch (button 4/5). Qt's Flickable scrolls 3 lines per 120 angle
/// units; at roughly 20 px a line that is 60 px per notch.
pub const WHEEL_STEP_PX: f32 = 60.0;
/// Double-click window and slop, as Qt's defaults on Linux.
const DOUBLE_CLICK_MS: u32 = 400;
const DOUBLE_CLICK_SLOP: i32 = 5;

/// Modifier bits that matter for shortcut matching (Shift, Control, Mod1/Alt, Mod4/Super).
const SHORTCUT_MODS: u16 = seq::MOD_SHIFT | seq::MOD_CONTROL | seq::MOD_1 | seq::MOD_4;
/// CapsLock and NumLock must not stop a shortcut from firing.
const LOCK_COMBOS: [u16; 4] = [0, 1 << 1, 1 << 4, (1 << 1) | (1 << 4)];

/// An output as the app sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Monitor {
    /// Connector name, e.g. `HDMI-2`.
    pub name: String,
    /// Human label, as `Backend::screens()` builds it (EDID model, "Notebook" for panels).
    pub label: String,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub primary: bool,
}

/// What `dispatch()` reports.
#[derive(Clone, Debug, PartialEq)]
pub enum XEvent {
    /// Input in device pixels, window-local.
    Input(InEvent),
    /// A grabbed global shortcut fired (the `foco-maxrc` shortcut id).
    Shortcut(String),
    FocusIn,
    FocusOut,
    Expose,
    ScreensChanged,
    /// All `present()` rectangles were consumed by the server; the buffer may be rewritten.
    PresentDone,
    PasteText(String),
}

struct Atoms {
    wm_type: u32,
    wm_type_dock: u32,
    wm_state: u32,
    state_above: u32,
    state_sticky: u32,
    state_skip_taskbar: u32,
    state_skip_pager: u32,
    wm_desktop: u32,
    wm_name: u32,
    wm_pid: u32,
    user_time: u32,
    utf8: u32,
    active_window: u32,
    clipboard: u32,
    xkb_rules: u32,
    paste_prop: u32,
    incr: u32,
    edid: u32,
    restack: u32,
}

const ATOM_NAMES: [&str; 19] = [
    "_NET_WM_WINDOW_TYPE",
    "_NET_WM_WINDOW_TYPE_DOCK",
    "_NET_WM_STATE",
    "_NET_WM_STATE_ABOVE",
    "_NET_WM_STATE_STICKY",
    "_NET_WM_STATE_SKIP_TASKBAR",
    "_NET_WM_STATE_SKIP_PAGER",
    "_NET_WM_DESKTOP",
    "_NET_WM_NAME",
    "_NET_WM_PID",
    "_NET_WM_USER_TIME",
    "UTF8_STRING",
    "_NET_ACTIVE_WINDOW",
    "CLIPBOARD",
    "_XKB_RULES_NAMES",
    "FOCO_PASTE",
    "INCR",
    "EDID",
    "_NET_RESTACK_WINDOW",
];

struct Grab {
    code: u8,
    mods: u16,
    id: String,
}

#[derive(Clone, Copy)]
struct Click {
    time: u32,
    x: i32,
    y: i32,
    button: Button,
    clicks: u8,
}

/// The X11 side of the app. See the crate docs.
pub struct X11 {
    conn: RustConnection,
    root: Window,
    atoms: Atoms,
    has_shape: bool,
    has_randr: bool,
    has_shm: bool,
    /// Compiled lazily (5-20 ms): after the first frame, on the first grab or key press.
    kb: Option<Keyboard>,

    win: Window,
    gc: u32,
    width: u16,
    height: u16,
    buf: Option<shmbuf::ShmBuf>,
    shmseg: u32,
    pending: u32,
    /// Staging rows for socket presents (reused).
    stage: Vec<u8>,
    /// Core cursors made so far (`Cursor` index), created on first use.
    cursors: [u32; 5],
    /// Index of the cursor currently set on the window.
    cursor_now: Option<usize>,
    mapped: bool,

    defs: Vec<(String, String)>,
    grabs: Vec<Grab>,
    down: Vec<u8>,
    keyboard_dirty: bool,
    /// A keyboard-change event left the rule names unchanged; setxkbmap updates the property
    /// after the keymap, so look again at the next key press.
    recheck_names: bool,

    previous_focus: Window,
    has_focus: bool,
    /// Monotonic ms of the last self-raise (rate limit against panels that re-raise too).
    last_raise: Option<std::time::Instant>,
    last_time: u32,
    last_click: Option<Click>,
    paste_tried_string: bool,

    /// Events pulled from the connection by `has_buffered`.
    stash: RefCell<VecDeque<Event>>,
    debug: bool,
}

fn s<E: std::fmt::Display>(what: &'static str) -> impl Fn(E) -> String {
    move |e| format!("{what}: {e}")
}

impl X11 {
    /// Connects to `$DISPLAY`, negotiates the extensions and builds the keymap.
    pub fn connect() -> Result<Self, String> {
        let (conn, screen_num) = RustConnection::connect(None).map_err(s("connect"))?;
        let root = conn.setup().roots[screen_num].root;

        // Everything below is pipelined: requests first, replies afterwards.
        for ext in ["SHAPE", "MIT-SHM", "RANDR", "XKEYBOARD"] {
            conn.prefetch_extension_information(ext).map_err(s("prefetch"))?;
        }
        let cookies: Vec<_> = ATOM_NAMES
            .iter()
            .map(|n| conn.intern_atom(false, n.as_bytes()))
            .collect::<Result<_, _>>()
            .map_err(s("intern_atom"))?;
        let mut ids = [0u32; ATOM_NAMES.len()];
        for (i, c) in cookies.into_iter().enumerate() {
            ids[i] = c.reply().map_err(s("intern_atom"))?.atom;
        }
        let atoms = Atoms {
            wm_type: ids[0],
            wm_type_dock: ids[1],
            wm_state: ids[2],
            state_above: ids[3],
            state_sticky: ids[4],
            state_skip_taskbar: ids[5],
            state_skip_pager: ids[6],
            wm_desktop: ids[7],
            wm_name: ids[8],
            wm_pid: ids[9],
            user_time: ids[10],
            utf8: ids[11],
            active_window: ids[12],
            clipboard: ids[13],
            xkb_rules: ids[14],
            paste_prop: ids[15],
            incr: ids[16],
            edid: ids[17],
            restack: ids[18],
        };

        let ext = |n: &'static str| matches!(conn.extension_information(n), Ok(Some(_)));
        let has_shape = ext("SHAPE");
        let has_randr = ext("RANDR");
        let has_shm = ext("MIT-SHM");
        let has_xkb = ext("XKEYBOARD");

        if has_randr {
            // Version negotiation, then ask for output/screen change events.
            drop(conn.randr_query_version(1, 5));
            let _ = conn.randr_select_input(
                root,
                NotifyMask::SCREEN_CHANGE | NotifyMask::OUTPUT_CHANGE | NotifyMask::CRTC_CHANGE,
            );
        }
        if has_xkb {
            // Detectable auto-repeat (held grabbed keys do not produce release/press pairs) and
            // keyboard-change notifications (XKB-aware clients no longer get MappingNotify).
            drop(conn.xkb_use_extension(1, 0));
            drop(conn.xkb_per_client_flags(
                xkb::ID::USE_CORE_KBD.into(),
                xkb::PerClientFlag::DETECTABLE_AUTO_REPEAT,
                xkb::PerClientFlag::DETECTABLE_AUTO_REPEAT,
                xkb::BoolCtrl::from(0u16),
                xkb::BoolCtrl::from(0u16),
                xkb::BoolCtrl::from(0u16),
            ));
            let _ = conn.xkb_select_events(
                xkb::ID::USE_CORE_KBD.into(),
                xkb::EventType::from(0u16),
                xkb::EventType::NEW_KEYBOARD_NOTIFY | xkb::EventType::MAP_NOTIFY,
                xkb::MapPart::from(0xffu16),
                xkb::MapPart::from(0xffu16),
                &xkb::SelectEventsAux::default(),
            );
        }
        // Stacking changes of top-level windows: another dock (e.g. tint2) raised by a click can
        // end up above the notch in Openbox's dock layer; see `keep_on_top`.
        let _ = conn.change_window_attributes(
            root,
            &xproto::ChangeWindowAttributesAux::new().event_mask(EventMask::SUBSTRUCTURE_NOTIFY),
        );
        // Core MappingNotify needs no selection.
        conn.flush().map_err(s("flush"))?;

        Ok(X11 {
            conn,
            root,
            atoms,
            has_shape,
            has_randr,
            has_shm,
            kb: None,
            win: 0,
            gc: 0,
            width: 0,
            height: 0,
            buf: None,
            shmseg: 0,
            pending: 0,
            stage: Vec::new(),
            cursors: [0; 5],
            cursor_now: None,
            mapped: false,
            defs: Vec::new(),
            grabs: Vec::new(),
            down: Vec::new(),
            keyboard_dirty: false,
            recheck_names: false,
            previous_focus: 0,
            has_focus: false,
            last_raise: None,
            last_time: CURRENT_TIME,
            last_click: None,
            paste_tried_string: false,
            stash: RefCell::new(VecDeque::new()),
            debug: std::env::var_os("FOCO_X11_DEBUG").is_some(),
        })
    }

    /// The connection's file descriptor, for epoll.
    pub fn fd(&self) -> RawFd {
        self.conn.stream().as_raw_fd()
    }

    pub fn window_id(&self) -> u32 {
        self.win
    }

    pub fn flush(&mut self) {
        let _ = self.conn.flush();
    }

    /// True when events are already queued (so the reactor must call `dispatch()` even though
    /// the fd is not readable).
    pub fn has_buffered(&self) -> bool {
        let mut st = self.stash.borrow_mut();
        if !st.is_empty() {
            return true;
        }
        if let Ok(Some(ev)) = self.conn.poll_for_event() {
            st.push_back(ev);
            return true;
        }
        false
    }

    // ------------------------------------------------------------------ cursor

    /// Sets the pointer shape shown over the window (core cursor-font glyphs, no Xcursor
    /// needed). Cheap when unchanged; each shape is created once.
    pub fn set_cursor(&mut self, cursor: Cursor) {
        if self.win == 0 {
            return;
        }
        let i = cursor as usize;
        if self.cursor_now == Some(i) {
            return;
        }
        if self.cursors[i] == 0 {
            // (glyph, mask glyph) pairs of the standard "cursor" font
            let (src, mask) = match cursor {
                Cursor::Arrow => (68, 69),    // left_ptr
                Cursor::Hand => (60, 61),     // hand2
                Cursor::Grab => (58, 59),     // hand1
                Cursor::Grabbing => (52, 53), // fleur
                Cursor::IBeam => (152, 153),  // xterm
            };
            let made = (|| {
                let font = self.conn.generate_id().ok()?;
                self.conn.open_font(font, b"cursor").ok()?;
                let cid = self.conn.generate_id().ok()?;
                let r = self
                    .conn
                    .create_glyph_cursor(cid, font, font, src, mask, 0, 0, 0, 0xffff, 0xffff, 0xffff)
                    .ok()?;
                drop(r);
                let _ = self.conn.close_font(font);
                Some(cid)
            })();
            match made {
                Some(cid) => self.cursors[i] = cid,
                None => return,
            }
        }
        let _ = self
            .conn
            .change_window_attributes(self.win, &xproto::ChangeWindowAttributesAux::new().cursor(self.cursors[i]));
        self.cursor_now = Some(i);
    }

    // ------------------------------------------------------------------ monitors

    /// Active monitors, left to right (then top to bottom). Blocking (a few round-trips, all
    /// pipelined); call at startup and on `ScreensChanged` only.
    pub fn monitors(&mut self) -> Vec<Monitor> {
        let mut out = self.query_monitors().unwrap_or_default();
        if out.is_empty() {
            // No RandR 1.5: one monitor covering the whole root screen.
            let sc = &self.conn.setup().roots[0];
            out.push(Monitor {
                name: "default".into(),
                label: "default".into(),
                x: 0,
                y: 0,
                w: sc.width_in_pixels as i32,
                h: sc.height_in_pixels as i32,
                primary: true,
            });
        }
        // No primary output set (as with this machine's xrandr): Qt calls the first one primary.
        if !out.iter().any(|m| m.primary) {
            if let Some(m) = out.first_mut() {
                m.primary = true;
            }
        }
        out.sort_by_key(|m| (m.x, m.y));
        out
    }

    fn query_monitors(&mut self) -> Option<Vec<Monitor>> {
        if !self.has_randr {
            return None;
        }
        let reply = self.conn.randr_get_monitors(self.root, true).ok()?.reply().ok()?;
        let mut pending = Vec::new();
        for m in &reply.monitors {
            let name = self.conn.get_atom_name(m.name).ok();
            let edid = m.outputs.first().and_then(|&o| {
                self.conn
                    .randr_get_output_property(o, self.atoms.edid, AtomEnum::NONE, 0, 64, false, false)
                    .ok()
            });
            pending.push((m, name, edid));
        }
        let mut out = Vec::new();
        for (m, name, edid) in pending {
            let name = name
                .and_then(|c| c.reply().ok())
                .map(|r| String::from_utf8_lossy(&r.name).into_owned())
                .unwrap_or_default();
            let model = edid.and_then(|c| c.reply().ok()).and_then(|r| edid::monitor_name(&r.data));
            out.push(Monitor {
                label: edid::label_for(&name, model.as_deref()),
                name,
                x: m.x as i32,
                y: m.y as i32,
                w: m.width as i32,
                h: m.height as i32,
                primary: m.primary,
            });
        }
        Some(out)
    }

    // ------------------------------------------------------------------ window

    /// Creates the (unmapped) window plus its shared-memory buffer of `w*h*4` bytes.
    pub fn create_window(&mut self, w: u16, h: u16) -> Result<(), String> {
        if self.win != 0 {
            return Err("window already created".into());
        }
        if !self.has_shm {
            return Err("MIT-SHM extension missing".into());
        }
        let screen = &self.conn.setup().roots[0];
        let visual = screen
            .allowed_depths
            .iter()
            .filter(|d| d.depth == 32)
            .flat_map(|d| d.visuals.iter())
            .find(|v| {
                v.class == xproto::VisualClass::TRUE_COLOR
                    && v.red_mask == 0xff0000
                    && v.green_mask == 0xff00
                    && v.blue_mask == 0xff
            })
            .map(|v| v.visual_id)
            .ok_or("no 32-bit ARGB visual")?;
        let c = &self.conn;
        let a = &self.atoms;

        let cmap = c.generate_id().map_err(s("id"))?;
        c.create_colormap(ColormapAlloc::NONE, cmap, self.root, visual)
            .map_err(s("colormap"))?;
        let win = c.generate_id().map_err(s("id"))?;
        c.create_window(
            32,
            win,
            self.root,
            0,
            0,
            w,
            h,
            0,
            WindowClass::INPUT_OUTPUT,
            visual,
            &CreateWindowAux::new().border_pixel(0).colormap(cmap).event_mask(
                EventMask::EXPOSURE
                    | EventMask::KEY_PRESS
                    | EventMask::BUTTON_PRESS
                    | EventMask::BUTTON_RELEASE
                    | EventMask::POINTER_MOTION
                    | EventMask::LEAVE_WINDOW
                    | EventMask::FOCUS_CHANGE,
            ),
        )
        .map_err(s("create_window"))?;

        // Dock: no border, no placement by the WM, out of Alt+Tab/taskbar, all desktops, above.
        c.change_property32(PropMode::REPLACE, win, a.wm_type, AtomEnum::ATOM, &[a.wm_type_dock])
            .map_err(s("prop"))?;
        c.change_property32(
            PropMode::REPLACE,
            win,
            a.wm_state,
            AtomEnum::ATOM,
            &[a.state_above, a.state_sticky, a.state_skip_taskbar, a.state_skip_pager],
        )
        .map_err(s("prop"))?;
        c.change_property32(PropMode::REPLACE, win, a.wm_desktop, AtomEnum::CARDINAL, &[0xFFFF_FFFF])
            .map_err(s("prop"))?;
        c.change_property8(PropMode::REPLACE, win, AtomEnum::WM_CLASS, AtomEnum::STRING, b"foco-max\0foco-max\0")
            .map_err(s("prop"))?;
        c.change_property8(PropMode::REPLACE, win, AtomEnum::WM_NAME, AtomEnum::STRING, b"Foquinho")
            .map_err(s("prop"))?;
        c.change_property8(PropMode::REPLACE, win, a.wm_name, a.utf8, b"Foquinho")
            .map_err(s("prop"))?;
        c.change_property32(PropMode::REPLACE, win, a.wm_pid, AtomEnum::CARDINAL, &[std::process::id()])
            .map_err(s("prop"))?;
        // User time 0: do not take focus when mapped.
        c.change_property32(PropMode::REPLACE, win, a.user_time, AtomEnum::CARDINAL, &[0])
            .map_err(s("prop"))?;
        WmHints { input: Some(false), ..Default::default() }
            .set(c, win)
            .map_err(s("hints"))?;
        let hints = WmSizeHints {
            position: Some((WmSizeHintsSpecification::UserSpecified, 0, 0)),
            min_size: Some((w as i32, h as i32)),
            max_size: Some((w as i32, h as i32)),
            ..Default::default()
        };
        hints.set_normal_hints(c, win).map_err(s("hints"))?;

        if self.has_shape {
            // Start fully click-through.
            c.shape_rectangles(shape::SO::SET, shape::SK::INPUT, xproto::ClipOrdering::UNSORTED, win, 0, 0, &[])
                .map_err(s("shape"))?;
        }

        // Shared-memory buffer.
        let shm_ver = c.shm_query_version().map_err(s("shm"))?.reply().map_err(s("shm"))?;
        if (shm_ver.major_version, shm_ver.minor_version) < (1, 2) {
            return Err("MIT-SHM >= 1.2 (fd passing) required".into());
        }
        let len = w as usize * h as usize * 4;
        let (buf, fd) = shmbuf::ShmBuf::new(len)?;
        let seg = c.generate_id().map_err(s("id"))?;
        shm::attach_fd(c, seg, fd, true).map_err(s("attach_fd"))?;
        let gc = c.generate_id().map_err(s("id"))?;
        c.create_gc(gc, win, &CreateGCAux::new().graphics_exposures(0))
            .map_err(s("gc"))?;
        c.flush().map_err(s("flush"))?;

        self.win = win;
        self.gc = gc;
        self.width = w;
        self.height = h;
        self.buf = Some(buf);
        self.shmseg = seg;
        Ok(())
    }

    /// Moves the window (root coordinates).
    pub fn place(&mut self, x: i32, y: i32) {
        if self.win == 0 {
            return;
        }
        let hints = WmSizeHints {
            position: Some((WmSizeHintsSpecification::UserSpecified, x, y)),
            min_size: Some((self.width as i32, self.height as i32)),
            max_size: Some((self.width as i32, self.height as i32)),
            ..Default::default()
        };
        let _ = hints.set_normal_hints(&self.conn, self.win);
        let _ = self.conn.configure_window(self.win, &ConfigureWindowAux::new().x(x).y(y));
    }

    pub fn map(&mut self) {
        if self.win != 0 && !self.mapped {
            self.mapped = true;
            let _ = self.conn.map_window(self.win);
        }
    }

    pub fn unmap(&mut self) {
        if self.win != 0 && self.mapped {
            self.mapped = false;
            let _ = self.conn.unmap_window(self.win);
        }
    }

    /// Sets the XShape input region (window-local device pixels). Empty = click-through
    /// everywhere.
    pub fn set_input_region(&mut self, rects: &[IRect]) {
        if self.win == 0 || !self.has_shape {
            return;
        }
        let xr: Vec<xproto::Rectangle> = rects
            .iter()
            .filter(|r| !r.is_empty())
            .map(|r| xproto::Rectangle { x: r.x as i16, y: r.y as i16, width: r.w as u16, height: r.h as u16 })
            .collect();
        let _ = self.conn.shape_rectangles(
            shape::SO::SET,
            shape::SK::INPUT,
            xproto::ClipOrdering::UNSORTED,
            self.win,
            0,
            0,
            &xr,
        );
    }

    // ------------------------------------------------------------------ presentation

    /// The pixel buffer: BGRA premultiplied, stride `w*4`, `w*h*4` bytes. Do not write while
    /// `present_pending()`.
    pub fn buffer(&mut self) -> &mut [u8] {
        match self.buf.as_mut() {
            Some(b) => b.as_mut_slice(),
            None => &mut [],
        }
    }

    /// Sends the given sub-rectangles of the buffer to the server (clamped to the window).
    pub fn present(&mut self, rects: &[IRect]) {
        if self.win == 0 {
            return;
        }
        let bounds = IRect::new(0, 0, self.width as i32, self.height as i32);
        for r in rects {
            let x0 = r.x.max(bounds.x);
            let y0 = r.y.max(bounds.y);
            let x1 = r.right().min(bounds.right());
            let y1 = r.bottom().min(bounds.bottom());
            if x1 <= x0 || y1 <= y0 {
                continue;
            }
            let (w, h) = ((x1 - x0) as usize, (y1 - y0) as usize);
            if w * h * 4 <= SOCKET_PUT_MAX {
                // Small update (the ambient animations): the pixels travel in the request itself.
                // No completion event (so no extra wake of the event loop), and the buffer is
                // free to be rewritten at once, because nothing refers to it any more.
                let stride = self.width as usize * 4;
                self.stage.clear();
                if let Some(buf) = self.buf.as_mut() {
                    let src = buf.as_mut_slice();
                    for row in y0 as usize..y1 as usize {
                        let o = row * stride + x0 as usize * 4;
                        self.stage.extend_from_slice(&src[o..o + w * 4]);
                    }
                }
                let _ = self.conn.put_image(
                    xproto::ImageFormat::Z_PIXMAP,
                    self.win,
                    self.gc,
                    w as u16,
                    h as u16,
                    x0 as i16,
                    y0 as i16,
                    0,
                    32,
                    &self.stage,
                );
                continue;
            }
            // Large update: zero-copy through the shared segment; the server confirms when it
            // has read it, and the buffer must not be written before that.
            let sent = shm::put_image(
                &self.conn,
                self.win,
                self.gc,
                self.width,
                self.height,
                x0 as u16,
                y0 as u16,
                w as u16,
                h as u16,
                x0 as i16,
                y0 as i16,
                32,
                xproto::ImageFormat::Z_PIXMAP.into(),
                true,
                self.shmseg,
                0,
            );
            if sent.is_ok() {
                self.pending += 1;
            }
        }
        let _ = self.conn.flush();
    }

    /// True until every `ShmCompletion` for the last `present()` has arrived.
    pub fn present_pending(&self) -> bool {
        self.pending > 0
    }

    // ------------------------------------------------------------------ global shortcuts

    /// Registers the shortcuts (`(id, sequence text)`); returns, per id, the display keys
    /// (`["Meta","Alt","N"]`), empty when the sequence is invalid, the key is missing from the
    /// layout or another client already holds the grab. Replaces earlier grabs.
    pub fn grab_shortcuts(&mut self, list: &[(String, String)]) -> Vec<(String, Vec<String>)> {
        self.defs = list.to_vec();
        self.do_grab()
    }

    fn do_grab(&mut self) -> Vec<(String, Vec<String>)> {
        self.ungrab_shortcuts();
        if !self.ensure_kb() {
            return self.defs.iter().map(|(id, _)| (id.clone(), Vec::new())).collect();
        }
        let Some(kb) = self.kb.as_ref() else {
            return Vec::new();
        };
        let conn = &self.conn;
        let root = self.root;
        let mut result: Vec<(String, Vec<String>)> = Vec::with_capacity(self.defs.len());
        let mut inflight: Vec<(usize, Vec<x11rb::cookie::VoidCookie<'_, RustConnection>>)> = Vec::new();
        let mut new_grabs: Vec<Grab> = Vec::new();

        // Send every grab first...
        for (id, text) in &self.defs {
            let Some(sq) = parse_sequence(text) else {
                result.push((id.clone(), Vec::new()));
                continue;
            };
            let codes = kb.keycodes_for(sq.sym);
            if codes.is_empty() {
                result.push((id.clone(), Vec::new()));
                continue;
            }
            let mut cookies = Vec::new();
            for &code in &codes {
                for lock in LOCK_COMBOS {
                    if let Ok(ck) =
                        conn.grab_key(false, root, (sq.mods | lock).into(), code, GrabMode::ASYNC, GrabMode::ASYNC)
                    {
                        cookies.push(ck);
                    }
                }
                new_grabs.push(Grab { code, mods: sq.mods, id: id.clone() });
            }
            inflight.push((result.len(), cookies));
            result.push((id.clone(), sq.display));
        }
        let _ = conn.flush();
        // ...then check them all (one round-trip in total, the rest are already answered).
        let mut failed_idx: Vec<usize> = Vec::new();
        for (idx, cookies) in inflight {
            let mut taken = false;
            for ck in cookies {
                if ck.check().is_err() {
                    taken = true;
                }
            }
            if taken {
                failed_idx.push(idx);
            }
        }
        for idx in failed_idx {
            let id = result[idx].0.clone();
            result[idx].1.clear();
            // Drop any partial grabs of that shortcut.
            new_grabs.retain(|g| {
                if g.id != id {
                    return true;
                }
                for lock in LOCK_COMBOS {
                    let _ = conn.ungrab_key(g.code, root, (g.mods | lock).into());
                }
                false
            });
        }
        let _ = conn.flush();
        self.grabs = new_grabs;
        result
    }

    /// Releases every global grab.
    pub fn ungrab_shortcuts(&mut self) {
        for g in &self.grabs {
            for lock in LOCK_COMBOS {
                let _ = self.conn.ungrab_key(g.code, self.root, (g.mods | lock).into());
            }
        }
        self.grabs.clear();
        self.down.clear();
        let _ = self.conn.flush();
    }

    // ------------------------------------------------------------------ keyboard focus

    fn active_window(&self) -> Window {
        self.conn
            .get_property(false, self.root, self.atoms.active_window, AtomEnum::WINDOW, 0, 1)
            .ok()
            .and_then(|c| c.reply().ok())
            .and_then(|r| r.value32().and_then(|mut it| it.next()))
            .unwrap_or(0)
    }

    /// Sends `_NET_ACTIVE_WINDOW` as if a pager asked (source 2): Openbox's focus-stealing
    /// prevention lets that through.
    fn send_active(&self, target: Window) {
        let ev = ClientMessageEvent::new(32, target, self.atoms.active_window, [2, self.last_time, 0, 0, 0]);
        let _ = self.conn.send_event(
            false,
            self.root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            ev,
        );
    }

    /// The top-level window holding ours: the WM frame, or the window itself if unparented.
    fn frame(&self) -> Window {
        let mut w = self.win;
        while let Some(tree) = self.conn.query_tree(w).ok().and_then(|c| c.reply().ok()) {
            if tree.parent == self.root || tree.parent == 0 {
                return w;
            }
            w = tree.parent;
        }
        self.win
    }

    /// A managed (non override-redirect) top-level was just stacked directly above our frame:
    /// ask the WM to raise us back (`_NET_RESTACK_WINDOW`, source = pager, so Openbox honours
    /// it within the dock layer). Menus, tooltips and notifications are override-redirect and
    /// never trigger this, so the notch does not cover them. Rate-limited to avoid ping-pong
    /// with a panel that re-raises itself.
    fn keep_on_top(&mut self, moved: Window, above: Window) {
        if self.win == 0 || !self.mapped {
            return;
        }
        let frame = self.frame();
        if moved == frame || above != frame {
            return;
        }
        let now = std::time::Instant::now();
        if self.last_raise.is_some_and(|t| now.duration_since(t).as_millis() < 250) {
            return;
        }
        self.last_raise = Some(now);
        let ev = ClientMessageEvent::new(32, self.win, self.atoms.restack, [2, 0, 0, 0, 0]);
        let _ = self.conn.send_event(
            false,
            self.root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            ev,
        );
        if self.debug {
            eprintln!("foco-x11: 0x{moved:x} stacked above us, raising back");
        }
    }

    fn set_wm_input(&self, input: bool) {
        let _ = WmHints { input: Some(input), ..Default::default() }.set(&self.conn, self.win);
    }

    /// Port of `grabKeyboard()`: with `exclusive` asks for keyboard focus (remembering the
    /// previously active window), otherwise gives it back.
    pub fn request_keyboard_focus(&mut self, exclusive: bool) {
        if self.win == 0 {
            return;
        }
        if !exclusive {
            self.release_keyboard_focus();
            return;
        }
        if !self.has_focus {
            let prev = self.active_window();
            if prev != 0 && prev != self.win {
                self.previous_focus = prev;
            }
        }
        self.set_wm_input(true);
        self.send_active(self.win);
        // Fallback for a WM that refuses focus on docks (the Qt build did this after 150 ms;
        // doing it right away costs nothing and avoids a timer).
        let _ = self.conn.set_input_focus(InputFocus::PARENT, self.win, CURRENT_TIME);
        let _ = self.conn.flush();
    }

    /// Gives the keyboard focus back to the window that had it before `request_keyboard_focus`.
    pub fn release_keyboard_focus(&mut self) {
        if self.win == 0 {
            return;
        }
        if self.has_focus && self.previous_focus != 0 {
            self.send_active(self.previous_focus);
        }
        self.previous_focus = 0;
        self.set_wm_input(false);
        let _ = self.conn.flush();
    }

    // ------------------------------------------------------------------ clipboard

    /// Asks for the CLIPBOARD as UTF-8 (falling back to STRING); the answer arrives as
    /// `XEvent::PasteText`.
    pub fn request_paste(&mut self) {
        if self.win == 0 {
            return;
        }
        self.paste_tried_string = false;
        let _ = self.conn.convert_selection(
            self.win,
            self.atoms.clipboard,
            self.atoms.utf8,
            self.atoms.paste_prop,
            CURRENT_TIME,
        );
        let _ = self.conn.flush();
    }

    fn on_selection_notify(&mut self, ev: &xproto::SelectionNotifyEvent, out: &mut Vec<XEvent>) {
        if ev.property == NONE {
            // Owner could not convert to UTF8_STRING: try plain STRING once.
            if ev.target == self.atoms.utf8 && !self.paste_tried_string {
                self.paste_tried_string = true;
                let _ = self.conn.convert_selection(
                    self.win,
                    self.atoms.clipboard,
                    AtomEnum::STRING.into(),
                    self.atoms.paste_prop,
                    CURRENT_TIME,
                );
            }
            return;
        }
        let Some(rep) = self
            .conn
            .get_property(true, self.win, ev.property, AtomEnum::NONE, 0, 16384)
            .ok()
            .and_then(|c| c.reply().ok())
        else {
            return;
        };
        if rep.type_ == self.atoms.incr {
            return; // huge selections (INCR) are not supported; this is a one-line field
        }
        let text = if rep.type_ == u32::from(AtomEnum::STRING) {
            rep.value.iter().map(|&b| b as char).collect()
        } else {
            String::from_utf8_lossy(&rep.value).into_owned()
        };
        out.push(XEvent::PasteText(text));
    }

    // ------------------------------------------------------------------ events

    fn next_event(&self) -> Option<Event> {
        if let Some(ev) = self.stash.borrow_mut().pop_front() {
            return Some(ev);
        }
        self.conn.poll_for_event().ok().flatten()
    }

    /// Builds the keymap on first use; `false` when xkbcommon cannot compile one.
    fn ensure_kb(&mut self) -> bool {
        if self.kb.is_none() {
            match Keyboard::new(self.read_rule_names()) {
                Ok(k) => self.kb = Some(k),
                Err(e) => {
                    if self.debug {
                        eprintln!("foco-x11: {e}");
                    }
                    return false;
                }
            }
        }
        true
    }

    fn read_rule_names(&self) -> RuleNames {
        self.conn
            .get_property(false, self.root, self.atoms.xkb_rules, AtomEnum::STRING, 0, 64)
            .ok()
            .and_then(|c| c.reply().ok())
            .filter(|r| !r.value.is_empty())
            .map(|r| RuleNames::parse(&r.value))
            .unwrap_or_default()
    }

    /// Re-reads `_XKB_RULES_NAMES`; when it changed rebuilds the keymap and re-grabs the
    /// shortcuts. Returns whether anything changed.
    fn refresh_keyboard(&mut self) -> bool {
        let names = self.read_rule_names();
        match self.kb.as_mut() {
            Some(kb) if names != *kb.names() => kb.rebuild(names),
            _ => return false,
        }
        if !self.defs.is_empty() {
            self.do_grab();
        }
        true
    }

    /// Drains everything buffered or readable without blocking. Pointer motion is coalesced
    /// to the latest position.
    pub fn dispatch(&mut self) -> Vec<XEvent> {
        let mut out: Vec<XEvent> = Vec::new();
        let mut motion: Option<Point> = None;
        let mut expose = false;
        let mut screens = false;

        fn flush_motion(out: &mut Vec<XEvent>, motion: &mut Option<Point>) {
            if let Some(pos) = motion.take() {
                out.push(XEvent::Input(InEvent::PointerMove { pos }));
            }
        }

        while let Some(ev) = self.next_event() {
            match ev {
                Event::MotionNotify(m) => {
                    self.last_time = m.time;
                    motion = Some(Point::new(m.event_x as f32, m.event_y as f32));
                }
                Event::ButtonPress(b) => {
                    flush_motion(&mut out, &mut motion);
                    self.last_time = b.time;
                    let pos = Point::new(b.event_x as f32, b.event_y as f32);
                    match b.detail {
                        1..=3 => {
                            let button = match b.detail {
                                1 => Button::Left,
                                2 => Button::Middle,
                                _ => Button::Right,
                            };
                            let (x, y) = (b.event_x as i32, b.event_y as i32);
                            let clicks = match self.last_click {
                                Some(c)
                                    if c.button == button
                                        && c.clicks == 1
                                        && b.time.wrapping_sub(c.time) <= DOUBLE_CLICK_MS
                                        && (c.x - x).abs() <= DOUBLE_CLICK_SLOP
                                        && (c.y - y).abs() <= DOUBLE_CLICK_SLOP =>
                                {
                                    2
                                }
                                _ => 1,
                            };
                            self.last_click = Some(Click { time: b.time, x, y, button, clicks });
                            out.push(XEvent::Input(InEvent::PointerDown { pos, button, clicks }));
                        }
                        4 => out.push(XEvent::Input(InEvent::Wheel { pos, dy: -WHEEL_STEP_PX })),
                        5 => out.push(XEvent::Input(InEvent::Wheel { pos, dy: WHEEL_STEP_PX })),
                        _ => {}
                    }
                }
                Event::ButtonRelease(b) => {
                    flush_motion(&mut out, &mut motion);
                    self.last_time = b.time;
                    let button = match b.detail {
                        1 => Button::Left,
                        2 => Button::Middle,
                        3 => Button::Right,
                        _ => continue,
                    };
                    let pos = Point::new(b.event_x as f32, b.event_y as f32);
                    out.push(XEvent::Input(InEvent::PointerUp { pos, button }));
                }
                Event::LeaveNotify(l) => {
                    if l.detail != NotifyDetail::INFERIOR {
                        flush_motion(&mut out, &mut motion);
                        out.push(XEvent::Input(InEvent::PointerLeave));
                    }
                }
                Event::KeyPress(k) => {
                    self.last_time = k.time;
                    if k.event == self.root {
                        // A grabbed global shortcut.
                        let mods = u16::from(k.state) & SHORTCUT_MODS;
                        let hit = self.grabs.iter().find(|g| g.code == k.detail && g.mods == mods);
                        if let Some(g) = hit {
                            if !self.down.contains(&k.detail) {
                                // Holding the key does not repeat the command.
                                self.down.push(k.detail);
                                flush_motion(&mut out, &mut motion);
                                out.push(XEvent::Shortcut(g.id.clone()));
                            }
                        }
                    } else {
                        if self.recheck_names {
                            self.recheck_names = false;
                            self.refresh_keyboard();
                        }
                        if !self.ensure_kb() {
                            continue;
                        }
                        let (conn, Some(kb)) = (&self.conn, self.kb.as_mut()) else {
                            continue;
                        };
                        let mut core_sym = |code: u8, shift: bool| -> Option<u32> {
                            let r = conn.get_keyboard_mapping(code, 1).ok()?.reply().ok()?;
                            let per = r.keysyms_per_keycode as usize;
                            let idx = usize::from(shift && per > 1);
                            r.keysyms.get(idx).copied()
                        };
                        if let Some(ke) = kb.key_press(k.detail, u16::from(k.state), &mut core_sym) {
                            flush_motion(&mut out, &mut motion);
                            out.push(XEvent::Input(InEvent::Key(ke)));
                        }
                    }
                }
                Event::KeyRelease(k) => {
                    if k.event == self.root {
                        self.down.retain(|&c| c != k.detail);
                    }
                }
                Event::FocusIn(f) => {
                    if f.detail != NotifyDetail::POINTER && f.mode != NotifyMode::GRAB && f.mode != NotifyMode::UNGRAB {
                        self.has_focus = true;
                        flush_motion(&mut out, &mut motion);
                        out.push(XEvent::FocusIn);
                    }
                }
                Event::FocusOut(f) => {
                    if f.detail != NotifyDetail::POINTER && f.mode != NotifyMode::GRAB && f.mode != NotifyMode::UNGRAB {
                        self.has_focus = false;
                        if let Some(kb) = self.kb.as_mut() {
                            kb.reset_compose();
                        }
                        flush_motion(&mut out, &mut motion);
                        out.push(XEvent::FocusOut);
                    }
                }
                Event::Expose(_) => expose = true,
                Event::ShmCompletion(_) => {
                    self.pending = self.pending.saturating_sub(1);
                    if self.pending == 0 {
                        flush_motion(&mut out, &mut motion);
                        out.push(XEvent::PresentDone);
                    }
                }
                Event::SelectionNotify(sn) => {
                    flush_motion(&mut out, &mut motion);
                    self.on_selection_notify(&sn, &mut out);
                }
                Event::MappingNotify(m) => {
                    if m.request != xproto::Mapping::POINTER {
                        self.keyboard_dirty = true;
                    }
                }
                Event::XkbNewKeyboardNotify(_) | Event::XkbMapNotify(_) => self.keyboard_dirty = true,
                Event::ConfigureNotify(c) if c.event == self.root && !c.override_redirect && c.above_sibling != 0 => {
                    self.keep_on_top(c.window, c.above_sibling)
                }
                Event::RandrScreenChangeNotify(_) | Event::RandrNotify(_) => screens = true,
                Event::Error(e) if self.debug => eprintln!("foco-x11: X error: {e:?}"),
                _ => {}
            }
        }
        flush_motion(&mut out, &mut motion);
        if expose {
            out.push(XEvent::Expose);
        }
        if screens {
            out.push(XEvent::ScreensChanged);
        }
        if self.keyboard_dirty {
            // Layout changed: new keycodes for the same keys, so rebuild and re-grab.
            self.keyboard_dirty = false;
            if !self.refresh_keyboard() {
                self.recheck_names = true;
            }
        }
        let _ = self.conn.flush();
        out
    }
}
