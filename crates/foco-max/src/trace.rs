//! `FOCO_TRACE=1`: one stderr line per event, checked once at startup. Zero cost otherwise
//! (a single relaxed bool load).

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

static ON: AtomicBool = AtomicBool::new(false);

pub fn init() {
    ON.store(std::env::var_os("FOCO_TRACE").is_some_and(|v| !v.is_empty() && v != "0"), Ordering::Relaxed);
}

#[inline]
pub fn on() -> bool {
    ON.load(Ordering::Relaxed)
}

/// Prints `args` when tracing. Callers that format expensive values should test `on()` first.
macro_rules! trace {
    ($($arg:tt)*) => {
        if $crate::trace::on() {
            eprintln!($($arg)*);
        }
    };
}

/// A `now()` that is only a clock read when tracing is on.
#[inline]
pub fn stamp() -> Option<Instant> {
    if on() {
        Some(Instant::now())
    } else {
        None
    }
}

/// Microseconds since `t` (0 when tracing is off).
#[inline]
pub fn since_us(t: Option<Instant>) -> u64 {
    t.map_or(0, |t| t.elapsed().as_micros() as u64)
}

extern "C" {
    fn sysconf(name: i32) -> i64;
}

/// Microseconds from process start (`/proc/self/stat` starttime) until now.
pub fn exec_to_now_us() -> Option<u64> {
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    // the command name may contain spaces/parens: fields after the last ')'
    let rest = &stat[stat.rfind(')')? + 2..];
    let start_ticks: u64 = rest.split_whitespace().nth(19)?.parse().ok()?;
    // SAFETY: sysconf(_SC_CLK_TCK = 2) has no preconditions.
    let hz = unsafe { sysconf(2) }.max(1) as u64;
    let boot = rustix::time::clock_gettime(rustix::time::ClockId::Boottime);
    let boot_us = boot.tv_sec as u64 * 1_000_000 + boot.tv_nsec as u64 / 1000;
    Some(boot_us.saturating_sub(start_ticks * 1_000_000 / hz))
}

/// Per-second loop statistics (only fed and printed while tracing).
#[derive(Default)]
pub struct Stats {
    t0: Option<Instant>,
    wakes: u32,
    frames: u32,
    idle_x: u32,
    timer: u32,
    ctx0: u64,
    cpu0: u64,
    busy_since: Option<Instant>,
    busy_us: u64,
}

fn status_ctx() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .filter(|l| l.starts_with("voluntary_ctxt_switches") || l.starts_with("nonvoluntary_ctxt_switches"))
                .map(|l| l.split_whitespace().nth(1).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0))
                .reduce(|a, b| a + b)
        })
        .unwrap_or(0)
}

fn cpu_ticks_us() -> u64 {
    let t = rustix::time::clock_gettime(rustix::time::ClockId::ProcessCPUTime);
    t.tv_sec as u64 * 1_000_000 + t.tv_nsec as u64 / 1000
}

impl Stats {
    /// Marks the start of the awake part of an iteration.
    pub fn busy_from(&mut self, t: Option<Instant>) {
        self.busy_since = t;
    }
    /// Marks the end of the awake part (just before sleeping).
    pub fn busy_to(&mut self) {
        if let Some(t) = self.busy_since.take() {
            self.busy_us += t.elapsed().as_micros() as u64;
        }
    }
    /// One loop iteration: `x_only` = only `PresentDone`-class X traffic, `timer` = timer fired.
    pub fn wake(&mut self, x_only: bool, timer: bool, frame: bool) {
        if !on() {
            return;
        }
        self.wakes += 1;
        self.idle_x += x_only as u32;
        self.timer += timer as u32;
        self.frames += frame as u32;
        let now = Instant::now();
        let t0 = *self.t0.get_or_insert(now);
        if now.duration_since(t0).as_millis() >= 1000 {
            let ctx = status_ctx();
            let cpu = cpu_ticks_us();
            eprintln!(
                "stat: wakes {} (timer {}, x-only {}) frames {} ctxsw {} cpu {} us busy {} us",
                self.wakes,
                self.timer,
                self.idle_x,
                self.frames,
                ctx - self.ctx0,
                cpu - self.cpu0,
                self.busy_us
            );
            self.busy_us = 0;
            self.t0 = Some(now);
            self.wakes = 0;
            self.frames = 0;
            self.idle_x = 0;
            self.timer = 0;
            self.ctx0 = ctx;
            self.cpu0 = cpu;
        }
    }
}
