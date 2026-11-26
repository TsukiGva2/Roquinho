//! foco-max: the Foquinho notch. Startup, the single-threaded frame loop (ui-design.md §8) and
//! the headless `--snapshot` mode.

#[macro_use]
mod trace;
mod app;

use app::{App, AppInit, PlatformReq};
use foco_base::event::Event;
use foco_base::{epoch_ms, mono_ms, parse_command, split_line, AppCmd, IRect, Ms, Point};
use foco_core::reactor::Ready;
use foco_core::{autostart, cli, ipc, paths, rc, Rc, Reactor, Store};
use foco_ui::{CursorShape, S, WIN_DEV};
use foco_x11::{Cursor as X11Cursor, XEvent, X11};
use std::path::PathBuf;
use std::process::ExitCode;

const TOKEN_X: u64 = 0;
const TOKEN_IPC: u64 = 1;

fn main() -> ExitCode {
    trace::init();
    let main_us = trace::exec_to_now_us();
    let t_main = trace::stamp();
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--snapshot") {
        return match snapshot(&args[1..]) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("foco-max: {e}");
                ExitCode::FAILURE
            }
        };
    }
    let line = cli::command_line(&args);
    // single instance: hand the command to the running one
    if cli::forward(&line) {
        return ExitCode::SUCCESS;
    }
    match run(&line, main_us, t_main) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("foco-max: {e}");
            ExitCode::FAILURE
        }
    }
}

fn scale_to_logical(ev: Event) -> Event {
    let p = |q: Point| Point::new(q.x / S, q.y / S);
    match ev {
        Event::PointerMove { pos } => Event::PointerMove { pos: p(pos) },
        Event::PointerDown { pos, button, clicks } => Event::PointerDown { pos: p(pos), button, clicks },
        Event::PointerUp { pos, button } => Event::PointerUp { pos: p(pos), button },
        Event::Wheel { pos, dy } => Event::Wheel { pos: p(pos), dy },
        other => other,
    }
}

fn run(line: &str, main_us: Option<u64>, t_main: Option<std::time::Instant>) -> Result<(), String> {
    let epoch = epoch_ms();
    let mut rc = Rc::load(paths::rc_path());
    let shortcuts: Vec<(String, String)> = rc::shortcut_list(&mut rc)
        .into_iter()
        .map(|(id, raw)| (id, rc::normalize_sequence(&raw)))
        .collect();
    let store = Store::load(&paths::state_path());
    // like the QML: load() ends with save(), so the file exists from the first run on
    if let Err(e) = store.save(&paths::state_path()) {
        eprintln!("foco-max: could not write {}: {e}", paths::state_path().display());
    }

    let mut x11 = X11::connect()?;
    let monitors = x11.monitors();
    let mut server = ipc::Server::bind().map_err(|e| {
        if e.kind() == std::io::ErrorKind::AddrInUse {
            // lost a race with another instance
            let _ = cli::forward(line);
        }
        format!("ipc: {e}")
    })?;
    let mut reactor = Reactor::new().map_err(|e| format!("reactor: {e}"))?;
    reactor.add_fd(x11.fd(), TOKEN_X).map_err(|e| e.to_string())?;
    reactor.add_fd(server.fd(), TOKEN_IPC).map_err(|e| e.to_string())?;

    x11.create_window(WIN_DEV as u16, WIN_DEV as u16)?;
    x11.set_input_region(&[IRect::new(0, 0, 1, 1)]);

    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("foco-max"));
    let initial = (!line.is_empty()).then(|| line.to_string());
    let mut now = mono_ms();
    let mut app = App::new(
        AppInit {
            store,
            rc,
            persist: true,
            state_path: paths::state_path(),
            exe,
            monitors,
            autostart: autostart::enabled(),
            initial_cmd: initial,
        },
        now,
        epoch,
    );
    app.set_shortcuts(&shortcuts);
    apply_requests(&mut app, &mut x11, now);
    x11.map();
    x11.flush();
    if trace::on() {
        eprintln!("trace: exec->main {} us, main->window mapped {} us", main_us.unwrap_or(0), trace::since_us(t_main));
    }

    let mut first_present = false;
    let mut first_notch = false;
    let mut shortcuts_grabbed = false;
    let mut frame_n: u64 = 0;
    let mut exposed = false;

    // The wake plan is kept while only `PresentDone` events arrive: they must not pull the next
    // animation frame forward (otherwise animations would run at the server's present rate).
    let mut plan: Option<Ms> = None;
    let mut need_plan = true;
    let mut stats = trace::Stats::default();
    loop {
        now = mono_ms();
        let mut epoch = epoch_ms();
        if need_plan {
            plan = app.next_deadline(now, epoch);
            need_plan = false;
        }
        let deadline = plan;
        x11.flush();
        stats.busy_to();
        let ready = if x11.has_buffered() {
            vec![Ready::Fd(TOKEN_X)]
        } else {
            reactor.wait(deadline).map_err(|e| format!("epoll: {e}"))?
        };
        now = mono_ms();
        epoch = epoch_ms();
        let t_frame = trace::stamp();
        stats.busy_from(t_frame);
        let mut why = "timer";
        let mut quit = false;
        let mut work = false;
        for r in ready {
            match r {
                Ready::Fd(TOKEN_X) => {
                    why = "x";
                    for e in x11.dispatch() {
                        if e != XEvent::PresentDone {
                            work = true;
                        }
                        match e {
                            XEvent::Input(ev) => app.on_event(&scale_to_logical(ev), now, epoch),
                            XEvent::Shortcut(id) => app.on_shortcut(&id, now, epoch),
                            XEvent::FocusIn => {
                                trace!("trace: FocusIn");
                                app.on_event(&Event::FocusIn, now, epoch);
                            }
                            XEvent::FocusOut => {
                                trace!("trace: FocusOut");
                                app.on_event(&Event::FocusOut, now, epoch);
                            }
                            XEvent::Expose => {
                                exposed = true;
                                app.damage_all();
                            }
                            XEvent::ScreensChanged => app.on_screens_changed(now),
                            XEvent::PresentDone => {}
                            XEvent::PasteText(t) => app.on_event(&Event::Paste(t), now, epoch),
                        }
                    }
                }
                Ready::Fd(_) => {
                    why = "ipc";
                    work = true;
                    for l in server.drain() {
                        if let Some((n, a)) = split_line(&l) {
                            app.on_cmd(parse_command(n, a), now, epoch);
                        }
                    }
                }
                Ready::Timer => work = true,
                Ready::Signal(_) => {
                    work = true;
                    app.save_on_exit();
                    quit = true;
                }
            }
        }
        if quit {
            return Ok(());
        }

        if !work && !app.has_damage() && deadline.is_none_or(|d| now + 0.25 < d) {
            stats.wake(true, false, false);
            continue;
        }
        need_plan = true;
        app.tick(now, epoch);
        let upd_us = trace::since_us(t_frame);
        loop {
            let reqs = app.take_requests();
            if reqs.is_empty() {
                break;
            }
            let refresh = reqs.contains(&PlatformReq::RefreshMonitors);
            let exit = reqs.contains(&PlatformReq::Exit);
            for req in reqs {
                handle_request(&mut x11, req);
            }
            if exit {
                return Ok(());
            }
            if refresh {
                app.set_monitors(x11.monitors(), now);
                app.tick(now, epoch);
            }
        }

        let mut rendered = false;
        if exposed && app.has_damage() && !x11.present_pending() {
            rendered = true;
            let rects = app.take_damage();
            let t_paint = trace::stamp();
            app.render(x11.buffer(), &rects, now, epoch);
            let paint_us = trace::since_us(t_paint);
            let t_present = trace::stamp();
            x11.present(&rects);
            frame_n += 1;
            if trace::on() {
                let px: i64 = rects.iter().map(IRect::area).sum();
                eprintln!(
                    "frame {frame_n} t={now:.0} wake={why} upd {upd_us} us paint {paint_us} us present {} us rects {} px {px} first {:?}",
                    trace::since_us(t_present),
                    rects.len(),
                    rects[0]
                );
                if !first_present {
                    eprintln!("trace: main->first present {} us", trace::since_us(t_main));
                }
                if !first_notch && app.first_notch_visible() {
                    first_notch = true;
                    eprintln!("trace: main->first notch present {} us", trace::since_us(t_main));
                }
            }
            first_present = true;
            if !shortcuts_grabbed {
                // the keymap compiles lazily (5-19 ms): only after something is on screen
                shortcuts_grabbed = true;
                for (id, keys) in x11.grab_shortcuts(&shortcuts) {
                    if keys.is_empty() {
                        eprintln!("foco-max: shortcut '{id}' could not be grabbed");
                    }
                }
            }
        }
        stats.wake(false, why == "timer", rendered);
    }
}

fn apply_requests(app: &mut App, x11: &mut X11, _now: Ms) {
    for req in app.take_requests() {
        handle_request(x11, req);
    }
}

fn handle_request(x11: &mut X11, req: PlatformReq) {
    match req {
        PlatformReq::Place { x, y } => x11.place(x, y),
        PlatformReq::SetInputRegion(r) => x11.set_input_region(&r),
        PlatformReq::RequestKeyboardFocus => x11.request_keyboard_focus(true),
        PlatformReq::ReleaseKeyboardFocus => x11.release_keyboard_focus(),
        PlatformReq::RequestPaste => x11.request_paste(),
        PlatformReq::SetCursor(c) => x11.set_cursor(match c {
            CursorShape::Arrow => X11Cursor::Arrow,
            CursorShape::Hand => X11Cursor::Hand,
            CursorShape::OpenHand => X11Cursor::Grab,
            CursorShape::ClosedHand => X11Cursor::Grabbing,
            CursorShape::IBeam => X11Cursor::IBeam,
        }),
        PlatformReq::RefreshMonitors | PlatformReq::Exit => {}
    }
}

/// `foco-max --snapshot <state.json> <out.png> [collapsed|expanded|settings|alert|tip|dot]`:
/// renders the scene to a PNG through foco-gfx without X.
fn snapshot(args: &[String]) -> Result<(), String> {
    let state_path = args.first().ok_or("usage: --snapshot <state.json> <out.png> [state]")?;
    let out = args.get(1).ok_or("usage: --snapshot <state.json> <out.png> [state]")?;
    let name = args.get(2).map_or("collapsed", String::as_str);
    let epoch0 = epoch_ms();
    let raw = std::fs::read_to_string(state_path).map_err(|e| format!("{state_path}: {e}"))?;
    let store = Store::from_json_at(&raw, epoch0);
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
        epoch0,
    );
    let mut t: Ms = 0.0;
    let run_to = |app: &mut App, until: Ms, t: &mut Ms| {
        while *t < until {
            app.tick(*t, epoch0 + *t);
            let _ = app.take_requests();
            let _ = app.take_damage();
            *t += 16.0;
        }
    };
    run_to(&mut app, 500.0, &mut t);
    match name {
        "collapsed" => {}
        "expanded" => app.on_cmd(AppCmd::Open, t, epoch0 + t),
        "settings" => {
            app.on_cmd(AppCmd::Open, t, epoch0 + t);
            app.on_cmd(AppCmd::OpenSettings, t, epoch0 + t);
        }
        "alert" => app.debug_alert(t),
        "tip" => app.debug_tip(),
        "dot" => app.on_cmd(AppCmd::Hide, t, epoch0 + t),
        other => return Err(format!("unknown state '{other}'")),
    }
    run_to(&mut app, t + 4000.0, &mut t);
    app.tick(t, epoch0 + t);
    let _ = app.take_requests();
    app.damage_all();
    let rects = app.take_damage();
    let mut buf = vec![0u8; (WIN_DEV * WIN_DEV * 4) as usize];
    app.render(&mut buf, &rects, t, epoch0 + t);
    foco_base::swap_rb_in_place(&mut buf);
    let size = tiny_skia::IntSize::from_wh(WIN_DEV as u32, WIN_DEV as u32).ok_or("size")?;
    let pm = tiny_skia::Pixmap::from_vec(buf, size).ok_or("pixmap")?;
    pm.save_png(out).map_err(|e| e.to_string())
}
