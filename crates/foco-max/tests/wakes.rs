//! Wake and damage budgets of the real `App` under a fake clock (ui-design.md §2-3): how often an idle notch wakes
//! and how many pixels it repaints. Pet blink/fidget seeds come from the injected epoch, so a run is reproducible; the
//! measured ranges below are from wall-clock seeds, and the budgets leave 30-50% over them so a new seed still fits.

mod common;

use common::{Driver, Stats, TASKS, TASKS_TIPS};
use foco_base::{AppCmd, Ms};

/// Frames bigger than this are a panel or tip in motion (a settled panel repaints < 12k px, a collapsed notch 2k).
const BIG_PX: i64 = 20_000;
/// Time for the first frames (boot, first tip) to pass before measuring, ms.
const WARMUP_MS: Ms = 8_000.0;

/// Measured: 7.5-9.2 wakes/s (about 6.6 frames/s) over 60 s; the real app at 60 Hz showed 8.4.
const COLLAPSED_WAKES_PER_S: f64 = 13.0;
/// Measured: 2156 px, the notch plus the pet.
const COLLAPSED_MAX_PX: i64 = 3_000;
/// Measured: 13.2-14.1 wakes/s with a focus timer running; the real app showed 15.
const TIMER_WAKES_PER_S: f64 = 20.0;
/// Measured: 4247 px, the timer text growing.
const TIMER_MAX_PX: i64 = 6_000;
/// Measured: 14.5-16.8 wakes/s with the panel open and nothing happening; the real app showed 13.
const OPEN_WAKES_PER_S: f64 = 24.0;
/// Measured: 11.6k px at most (mean 8.7k; the header pet is most of it).
const OPEN_MAX_PX: i64 = 16_000;
/// Measured: 48 big frames within 2 s of `open` (the real app: ~49).
const OPEN_BIG_FRAMES: usize = 70;
/// Measured: 17-20 big frames within 2 s of `collapse` (the real app: ~19).
const CLOSE_BIG_FRAMES: usize = 30;
/// Measured: 7.9-8.3 frames/s with the tip bubble showing (pet frames included).
const TIP_FRAMES_PER_S: f64 = 12.0;
/// Measured: 2.5 big repaints/s (the bubble floating, 105k px each); it was ~5/s before the 0.5 px float step.
const TIP_BIG_FRAMES_PER_S: f64 = 3.75;

fn per_s(n: usize, secs: f64) -> f64 {
    n as f64 / secs
}

fn big_frames(s: &Stats) -> usize {
    s.px.iter().filter(|&&p| p > BIG_PX).count()
}

/// A driver that has run `WARMUP_MS` and then been through `setup`, with the counters reset.
fn warm(state: &str, setup: impl FnOnce(&mut Driver)) -> Driver {
    let mut d = Driver::new(state);
    d.run_for(WARMUP_MS);
    setup(&mut d);
    d.run_for(WARMUP_MS);
    d.take_stats();
    d
}

#[test]
fn hidden_dot_never_wakes() {
    let mut d = warm(TASKS, |d| d.cmd(AppCmd::Hide));
    d.run_for(600_000.0);
    let s = d.take_stats();
    assert_eq!((s.wakes, s.frames), (0, 0), "the dot must sleep until an event arrives");
}

#[test]
fn collapsed_idle() {
    let mut d = warm(TASKS, |_| {});
    d.run_for(60_000.0);
    let s = d.take_stats();
    println!("collapsed: {:.1} wakes/s, max {} px", per_s(s.wakes as usize, 60.0), s.max_px());
    assert!(per_s(s.wakes as usize, 60.0) <= COLLAPSED_WAKES_PER_S);
    assert!(s.max_px() <= COLLAPSED_MAX_PX);
}

#[test]
fn collapsed_with_timer() {
    let mut d = warm(TASKS, |d| d.cmd(AppCmd::FocusNew("Tarefa".into())));
    d.run_for(60_000.0);
    let s = d.take_stats();
    println!("timer: {:.1} wakes/s, max {} px", per_s(s.wakes as usize, 60.0), s.max_px());
    assert!(per_s(s.wakes as usize, 60.0) <= TIMER_WAKES_PER_S);
    assert!(s.max_px() <= TIMER_MAX_PX);
}

#[test]
fn panel_open_idle() {
    let mut d = warm(TASKS, |d| d.cmd(AppCmd::Open));
    d.run_for(30_000.0);
    let s = d.take_stats();
    println!("open: {:.1} wakes/s, max {} px", per_s(s.wakes as usize, 30.0), s.max_px());
    assert!(per_s(s.wakes as usize, 30.0) <= OPEN_WAKES_PER_S);
    assert!(s.max_px() <= OPEN_MAX_PX);
}

#[test]
fn open_and_close_animations_settle() {
    let mut d = warm(TASKS, |_| {});
    d.cmd(AppCmd::Open);
    d.run_for(2_000.0);
    let open = big_frames(&d.take_stats());
    d.run_for(WARMUP_MS);
    d.take_stats();
    d.cmd(AppCmd::Collapse);
    d.run_for(2_000.0);
    let close = big_frames(&d.take_stats());
    println!("open: {open} big frames, close: {close}");
    assert!(open <= OPEN_BIG_FRAMES, "the open springs keep running");
    assert!(close <= CLOSE_BIG_FRAMES, "the close springs keep running");
}

#[test]
fn tip_bubble_repaints() {
    let mut d = warm(TASKS_TIPS, |_| {});
    d.run_for(30_000.0);
    let s = d.take_stats();
    let (frames, big) = (per_s(s.frames as usize, 30.0), per_s(big_frames(&s), 30.0));
    println!("tip: {frames:.1} frames/s, {big:.1} big/s");
    assert!(frames <= TIP_FRAMES_PER_S);
    assert!(big <= TIP_BIG_FRAMES_PER_S);
}
