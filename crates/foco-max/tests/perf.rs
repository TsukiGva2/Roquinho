//! CPU cost of the real `App` per simulated second, tick + render only (no X11). Perf numbers mean nothing in a debug
//! build: run `cargo test --profile profiling -p foco-max --test perf -- --ignored --nocapture`.

mod common;

use common::{Driver, Stats, TASKS, TASKS_TIPS};
use foco_base::{AppCmd, Ms};

/// Time for boot and the first tip to pass before measuring, ms.
const WARMUP_MS: Ms = 8_000.0;
/// Simulated time measured per steady state, ms.
const MEASURE_MS: Ms = 30_000.0;

/// Measured: 0.31 ms/s (profiling build, tick + render only, no X11 present); budgets are 2.5-3x the measured values.
const IDLE_MS_PER_S: f64 = 1.0;
/// Measured: 0.48 ms/s.
const TIMER_MS_PER_S: f64 = 1.5;
/// Measured: 4.4 ms/s (the header pet repaints at 30 Hz).
const OPEN_MS_PER_S: f64 = 12.0;
/// Measured: 2.6 ms/s.
const TIP_MS_PER_S: f64 = 7.0;
/// Measured: 76 ms for a warm open and 14 ms for a close, tick + render; one open is ~50 big frames.
const TRANSITION_MS: f64 = 200.0;

fn ms(s: &Stats) -> f64 {
    s.busy.as_secs_f64() * 1e3
}

/// Runs `MEASURE_MS` of a steady state, prints the cost and returns ms of CPU per simulated second.
fn steady(name: &str, mut d: Driver) -> f64 {
    d.run_for(WARMUP_MS);
    d.take_stats();
    d.run_for(MEASURE_MS);
    let s = d.take_stats();
    let per_s = ms(&s) / (MEASURE_MS / 1e3);
    let mean_us = s.busy.as_secs_f64() * 1e6 / f64::from(s.wakes.max(1));
    println!(
        "{name:16} {per_s:6.2} ms/s  {} wakes {} frames  mean {mean_us:7.1} us/wake  worst {} us",
        s.wakes,
        s.frames,
        s.worst.as_micros()
    );
    per_s
}

/// Cost of one `cmd` plus the 2 s that follow it (the springs settle in about 1 s).
fn transition(d: &mut Driver, cmd: AppCmd) -> Stats {
    d.take_stats();
    d.cmd(cmd);
    d.run_for(2_000.0);
    d.take_stats()
}

#[test]
#[ignore = "perf: run with --release -- --ignored --nocapture"]
fn perf_cpu_budget() {
    let idle = steady("collapsed idle", Driver::new(TASKS));
    let mut d = Driver::new(TASKS);
    d.cmd(AppCmd::FocusNew("Tarefa".into()));
    let timer = steady("collapsed timer", d);
    let mut d = Driver::new(TASKS);
    d.cmd(AppCmd::Open);
    let open = steady("panel open", d);
    let tip = steady("tip showing", Driver::new(TASKS_TIPS));

    let mut d = Driver::new(TASKS);
    d.run_for(WARMUP_MS);
    let cold = transition(&mut d, AppCmd::Open);
    d.run_for(WARMUP_MS);
    let close_cold = transition(&mut d, AppCmd::Collapse);
    d.run_for(WARMUP_MS);
    let open_warm = transition(&mut d, AppCmd::Open);
    d.run_for(WARMUP_MS);
    let close_warm = transition(&mut d, AppCmd::Collapse);
    println!(
        "open {:.1} ms (first, cold caches) / {:.1} ms (warm), close {:.1} / {:.1} ms",
        ms(&cold),
        ms(&open_warm),
        ms(&close_cold),
        ms(&close_warm)
    );

    assert!(idle <= IDLE_MS_PER_S, "collapsed idle {idle:.2} ms/s");
    assert!(timer <= TIMER_MS_PER_S, "collapsed timer {timer:.2} ms/s");
    assert!(open <= OPEN_MS_PER_S, "panel open {open:.2} ms/s");
    assert!(tip <= TIP_MS_PER_S, "tip showing {tip:.2} ms/s");
    assert!(ms(&open_warm) <= TRANSITION_MS, "open {:.1} ms", ms(&open_warm));
    assert!(ms(&close_warm) <= TRANSITION_MS, "close {:.1} ms", ms(&close_warm));
}
