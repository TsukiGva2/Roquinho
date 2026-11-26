//! Manual check of foco-x11: DOCK window centred at the top of HDMI-2, a 300x40 notch drawn
//! with tiny-skia straight into the SHM buffer, click-through everywhere else.
//!
//! usage: demo [seconds=5]      env: FOCUS=1 requests keyboard focus after the first frame

use std::os::fd::BorrowedFd;
use std::time::{Duration, Instant};

use foco_base::geom::IRect;
use foco_x11::{XEvent, X11};
use rustix::event::{poll, PollFd, PollFlags};
use tiny_skia::{Color, FillRule, Paint, PathBuilder, PixmapMut, Transform};

const W: u16 = 792;
const H: u16 = 792;

fn rss_kb() -> String {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            let pick = |k: &str| {
                s.lines()
                    .find(|l| l.starts_with(k))
                    .map(|l| l.split_whitespace().nth(1).unwrap_or("?").to_string())
            };
            Some(format!("VmRSS={} kB VmHWM={} kB", pick("VmRSS:")?, pick("VmHWM:")?))
        })
        .unwrap_or_default()
}

fn draw(buf: &mut [u8], rect: IRect) {
    buf.fill(0);
    let mut pm = PixmapMut::from_bytes(buf, W as u32, H as u32).expect("pixmap");
    let (x, y, w, h, r) = (rect.x as f32, rect.y as f32, rect.w as f32, rect.h as f32, 16.0f32);
    let mut pb = PathBuilder::new();
    pb.move_to(x, y);
    pb.line_to(x + w, y);
    pb.line_to(x + w, y + h - r);
    pb.quad_to(x + w, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.quad_to(x, y + h, x, y + h - r);
    pb.close();
    let path = pb.finish().expect("path");
    let mut paint = Paint::default();
    paint.set_color(Color::from_rgba8(0, 0, 0, 255));
    paint.anti_alias = true;
    pm.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
    // an orange dot; tiny-skia writes R,G,B,A bytes but the buffer is B,G,R,A, so R and B are
    // swapped here on purpose (foco-gfx must do the same swap)
    let mut pb = PathBuilder::new();
    pb.push_circle(x + 24.0, y + h / 2.0, 7.0);
    let dot = pb.finish().expect("dot");
    paint.set_color(Color::from_rgba8(0, 128, 255, 255));
    pm.fill_path(&dot, &paint, FillRule::Winding, Transform::identity(), None);
}

fn main() {
    let t0 = Instant::now();
    let secs: f64 = std::env::args().nth(1).and_then(|a| a.parse().ok()).unwrap_or(5.0);
    let mut x = X11::connect().expect("connect");
    println!("connect: {} us", t0.elapsed().as_micros());
    let mons = x.monitors();
    for m in &mons {
        println!("monitor {m:?}");
    }
    let target = mons
        .iter()
        .find(|m| m.name == "HDMI-2")
        .or(mons.last())
        .expect("monitor")
        .clone();
    x.create_window(W, H).expect("create_window");
    println!("window 0x{:x} created: {} us", x.window_id(), t0.elapsed().as_micros());
    let nx = target.x + (target.w - W as i32) / 2;
    x.place(nx, target.y);
    let notch = IRect::new((W as i32 - 300) / 2, 0, 300, 40);
    draw(x.buffer(), notch);
    x.set_input_region(&[notch]);
    x.map();
    x.present(&[IRect::new(0, 0, W as i32, H as i32)]);
    x.flush();

    let end = t0 + Duration::from_secs_f64(secs);
    let mut first_done = false;
    let mut wakeups = 0u32;
    loop {
        let now = Instant::now();
        if now >= end {
            break;
        }
        if !x.has_buffered() {
            let left = end - now;
            let ts = rustix::fs::Timespec { tv_sec: left.as_secs() as _, tv_nsec: left.subsec_nanos() as _ };
            let fd = unsafe { BorrowedFd::borrow_raw(x.fd()) };
            let mut fds = [PollFd::new(&fd, PollFlags::IN)];
            let _ = poll(&mut fds, Some(&ts));
        }
        wakeups += 1;
        for ev in x.dispatch() {
            println!("[{:>7} us] {ev:?}", t0.elapsed().as_micros());
            match ev {
                XEvent::PresentDone if !first_done => {
                    first_done = true;
                    println!("first ShmCompletion {} us after start; {}", t0.elapsed().as_micros(), rss_kb());
                    // keymap compilation is deferred until now, off the path to the first frame
                    let g = x.grab_shortcuts(&[
                        ("demo".to_string(), "Meta+Alt+F12".to_string()),
                        ("bad".to_string(), "Meta+Alt+Banana".to_string()),
                    ]);
                    println!("grabs: {g:?} ({} us)", t0.elapsed().as_micros());
                    x.flush();
                    if std::env::var_os("FOCUS").is_some() {
                        x.request_keyboard_focus(true);
                    }
                }
                XEvent::Expose if !x.present_pending() => x.present(&[IRect::new(0, 0, W as i32, H as i32)]),
                XEvent::Input(foco_base::event::Event::Key(k)) if k.key == foco_base::event::Key::Escape => {
                    x.release_keyboard_focus();
                }
                _ => {}
            }
        }
    }
    println!("loop iterations: {wakeups}; final {}", rss_kb());
    x.ungrab_shortcuts();
    x.release_keyboard_focus();
    x.flush();
}
