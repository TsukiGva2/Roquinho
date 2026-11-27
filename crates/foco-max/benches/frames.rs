//! Instruction counts (valgrind) of the frame pipeline in the scenarios of the budget tests, so CI can fail a PR
//! that makes a frame more expensive. Setup (boot, font loading, warm-up) runs outside the measured part.

#![expect(unused_qualifications, reason = "the gungraun macros expand to qualified paths")]

#[path = "../tests/common/mod.rs"]
mod common;

use common::{Driver, TASKS, TASKS_TIPS};
use foco_base::{AppCmd, Ms};
use gungraun::{library_benchmark, library_benchmark_group, main};
use std::hint::black_box;

/// Time for the boot frames and the first tip to pass before measuring, ms (as in `tests/wakes.rs`).
const WARMUP_MS: Ms = 8_000.0;

/// A driver past boot and font loading, in the state the scenario starts from.
fn warm(state: &str, setup: impl FnOnce(&mut Driver)) -> Driver {
    let mut d = Driver::new(state);
    d.run_for(WARMUP_MS);
    setup(&mut d);
    d.run_for(WARMUP_MS);
    d
}

fn idle() -> Driver {
    warm(TASKS, |_| {})
}

fn with_timer() -> Driver {
    warm(TASKS, |d| d.cmd(AppCmd::FocusNew("Tarefa".into())))
}

fn with_tips() -> Driver {
    warm(TASKS_TIPS, |_| {})
}

// The benchmarks return the driver so that dropping it is not counted.
#[library_benchmark]
#[bench::scenario(setup = idle)]
fn collapsed_idle(mut d: Driver) -> Driver {
    d.run_for(10_000.0);
    black_box(d)
}

#[library_benchmark]
#[bench::scenario(setup = idle)]
fn open_close(mut d: Driver) -> Driver {
    d.cmd(AppCmd::Open);
    d.run_for(2_000.0);
    d.cmd(AppCmd::Collapse);
    d.run_for(2_000.0);
    black_box(d)
}

// One 3.2 s float cycle of the tip bubble, which popped in during the warm-up.
#[library_benchmark]
#[bench::scenario(setup = with_tips)]
fn tip_float(mut d: Driver) -> Driver {
    d.run_for(3_200.0);
    black_box(d)
}

#[library_benchmark]
#[bench::scenario(setup = with_timer)]
fn timer_tick(mut d: Driver) -> Driver {
    d.run_for(10_000.0);
    black_box(d)
}

library_benchmark_group!(name = frames; benchmarks = collapsed_idle, open_close, tip_float, timer_tick);
main!(library_benchmark_groups = frames);
