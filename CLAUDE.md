# Foquinho

X11 desktop focus/pomodoro widget (a "notch" at the top of the screen) with a mascot, the cat/ghost/slime
"foquinho". Binary: `foco-max`. Rust 2021 workspace, stable toolchain, CPU rendering (tiny-skia), no GUI toolkit,
single-threaded epoll loop. It is a rewrite of a QML/KDE app: no KDE, no music. Comments that say `Foo.qml:N` point
at the original spec, which is not in this repo; keep that behaviour when changing ported code. UI strings are
Portuguese (user-facing); code, comments and docs are English.

## Commands
- `make check` — fmt --check + clippy `-D warnings` + tests. This is what CI runs. **Run it before declaring work done.**
- `make fmt` (`cargo fmt --all`), `make lint`, `make test` (`cargo test --workspace`).
- `make build` (release), `make install` (to `~/.local/bin`), `make deploy` (build + install + restart the running
  instance), `make restart`, `make stop`.
- Single crate / test: `cargo test -p foco-ui views::queue`. Needs no X server.
- Ignored perf tests (release only): `cargo test --release -p foco-pet -- --ignored --nocapture`, same with `-p foco-gfx`.
- Headless render: `foco-max --snapshot <state.json> <out.png> [collapsed|expanded|settings|alert|tip|dot]`.
- Runtime env: `FOCO_TRACE=1` (stderr event trace), `FOCO_FONT=inter`. `.cargo/config.toml` builds with
  `target-cpu=native`, so release binaries are not portable.
- CI (`.github/workflows/ci.yml`): Arch Linux container, rustup installs the pinned toolchain; fmt, clippy, tests on
  push to main and PRs. Tests need the Arch fonts and libx11 Compose tables (the keyboard test uses pt_BR dead keys);
  no X server needed.
- This machine is 4-core and low-RAM: use `CARGO_BUILD_JOBS=2` for heavy runs. `make build` is fat-LTO and slow.

## Architecture
```
foco-base            (leaf: geometry, color, anim, events, actions, model, Surface trait, time, damage, wake)
 ^    ^    ^    ^
 |    |    |    foco-pet   mascot (defs, expression, paint)
 |    |    foco-core        store state machine, IPC/CLI, epoll reactor, rc, autostart, paths
 |    foco-gfx              CPU renderer: Canvas (Surface over tiny-skia), fonts, glyph cache, text, icons, shadows
 foco-x11                   DOCK window, MIT-SHM present, RandR, xkb, clipboard, key grabs
foco-ui  -> base, core, pet     widgets + views (NOT gfx, NOT x11)
foco-max -> everything          app.rs (scene), main.rs (startup + frame loop), trace.rs
```
- Dependencies only point toward `foco-base`. Never add an edge between siblings (e.g. ui -> gfx, x11 -> core);
  wire things together in `foco-max`.
- `foco-base` is the frozen shared contract: its public API only grows, never changes or shrinks.
- `foco-ui` reaches the store, text measurement and the mascot through traits (`StoreRead`, `TextMeasure`,
  `pet::PetLike`). Real impls live in `foco-ui/src/real.rs`, fakes in `foco-ui/src/testkit.rs`.
- The store is pure: `Store::apply(Action, now_epoch) -> Vec<Effect>`. Side effects are returned as `Effect`s and
  carried out by the app; nothing in the store reads the clock except the `load`/`save` wrappers.
- Frame pipeline: `update -> layout -> paint -> next_wake`. Views are retained structs with hand-written layout.
  They push `Msg`s from `event()`; the app applies them after dispatch, so views never borrow the store mutably.
  Wakes are exact deadlines (`Wake::At` / `Frame` / `Idle`); an idle process never wakes up.
- Units: views work in logical px; device px = logical * `S` (`S = 25/24`); only damage, the input region and
  incoming pointer events are converted. Repaint only damaged rects, and only when the quantized visual state changed.
- Time: `now: Ms` is monotonic (`mono_ms`) and drives animation/wakes; `epoch: f64` is wall clock (`epoch_ms`) and
  drives the store and task timers. Do not mix them; convert with `now + (t_epoch - epoch)`.
- Details (damage model, wake model, input, widgets, parity traps): `docs/ui-design.md`. Read it before touching
  `foco-ui` or `foco-max/src/app.rs`. Do not duplicate it here.

## Code style
- `cargo fmt --all` always (`rustfmt.toml`, stable options, ~120 columns). Do not hand-wrap against it.
- Workspace lints (`[workspace.lints]` in root Cargo.toml, each crate has `[lints] workspace = true`): clippy `all` +
  `pedantic`, minus a few allowed on purpose (rationale in the Cargo.toml comments): numeric casts (pixel math),
  `float_cmp` (exact equality is deliberate), `must_use_candidate`/`return_self_not_must_use`, `many_single_char_names`/
  `similar_names`, `module_name_repetitions`, `struct_excessive_bools` (UI flag state), `too_many_lines` (linear
  paint/layout fns). On top: `undocumented_unsafe_blocks`, `dbg_macro`, `todo`, `unimplemented`; rustc
  `unsafe_op_in_unsafe_fn`, `unused_qualifications`. Warnings are errors in CI.
- Fix lints, don't silence them. To allow a lint workspace-wide, add it to the root Cargo.toml with a one-line
  `# why` comment. For a single site use `#[expect(lint, reason = "...")]`, never a bare `#[allow]`.
- Toolchain: `rust-toolchain.toml` pins Rust 1.97.0 (clippy + rustfmt); CI installs the same pin via rustup. Bumping
  it is a deliberate change: bump, fix the new lints, and make it its own commit.
- Naming: standard Rust. Modules and files are `snake_case` nouns for one concept (`keyed_list.rs`, `focus_card.rs`).
  Constants carry their unit or meaning in a doc comment (`/// 180 ms`, `/// logical px`); times are `Ms`.
- Errors: no `anyhow`/`thiserror`. Library code returns `io::Result` or `Option` and degrades gracefully (a missing
  or corrupt state file is not fatal). The binary maps errors to `Result<(), String>`, prints `foco-max: {e}` to
  stderr and returns `ExitCode::FAILURE`. No `unwrap`/`expect`/`panic!` in non-test code except for invariants that hold by
  construction (hardcoded pet paths use `.expect("valid pet path")`); they are fine in tests.
- `unsafe`: only for libc/mmap/fd FFI. Keep each block tiny, wrap it in a small safe fn, and put a `// SAFETY:`
  comment on it stating the invariant. No `unsafe` in views, store or pet.
- Dependencies are deliberately few (tiny-skia, x11rb, xkbcommon, rustix, serde/serde_json, ttf-parser,
  ab_glyph_rasterizer). Do not add a crate without a clear reason; prefer ~30 lines of std. New deps go in
  `[workspace.dependencies]`.
- Comments say *why* (a constraint, a parity quirk, a measured number), not *what*. Doc comments (`///`, `//!`) on
  every pub item and module; module docs open with one line saying what the file is.

## Clean code and KISS (rules for this repo)
- Make the smallest change that solves the problem. No drive-by refactors, renames or reformatting of untouched code.
- No speculative abstractions. Add a trait only when there is a second implementation or a test seam (as with
  `StoreRead`, `TextMeasure`, `PetLike`). No generics, builders, macros or frameworks where a struct and a function do.
  (The only macro today is `trace!`.)
- Mirror the existing pattern before inventing one: read a sibling widget/view first (e.g. `widgets/toggle.rs`) and
  match its shape (`new` / `sync` or `update` / `layout` / `event` / `paint` / `next_wake`).
- Functions short and single-purpose. Avoid deep nesting: early returns, `let ... else`, `?`.
- Keep pure logic (layout math, state transitions, timing, parsing) separate from X11/IO so it is unit-testable.
  Anything that touches a socket, file or X lives at the edge (`foco-x11`, `foco-core::{ipc,reactor,fsutil}`, `foco-max`).
- No dead code, no commented-out code, no unused params "for later", no `todo!()`/`unimplemented!()` (lint-denied),
  no TODO comments (do it or delete it). Delete rather than disable. Remove an import/dep when its last use goes away.
- No config knobs, flags or env vars nobody asked for. Constants over settings.
- Keep modules cohesive. Several files are already 1000+ lines (views, `app.rs`, `foco-x11/lib.rs`); when adding a
  distinct concern, put it in a new module instead of growing those further.
- Don't allocate or repaint per frame when nothing changed; compare before damaging (see ui-design.md section 2).

## Testing
- Unit tests live next to the code: `#[cfg(test)] mod tests { ... }` at file bottom, or a sibling `tests.rs` when
  large (`foco-core/src/store/tests.rs`, `foco-pet/src/tests.rs`, `foco-gfx/src/tests.rs`). Integration/visual tests:
  `crates/foco-ui/tests/visual.rs`.
- Test pure logic: store transitions and migrations (fixture JSON under `crates/foco-core/tests/data/`), parsers, anims,
  layout, wake deadlines, rc/shortcut handling.
- UI tests use `foco_ui::testkit` (`Harness`, `FakeStore`, `FakeText`, `PixSurface`): no renderer, no X server, text is a
  fixed-width placeholder. Visual tests write PNGs to `target/ui-test/` and `target/gfx-test/` for eyeballing and
  assert a few pixels/numbers; never compare whole images.
- Deterministic: inject `now`/`epoch` as parameters (`apply(action, now)`, `next_tick(task, epoch, now, ..)`); never
  call `mono_ms`/`epoch_ms` or sleep inside logic under test. Use `TempDir` (`fsutil::testutil`) for files.
- Every bug fix ships with a regression test that fails without the fix.
- Perf/bench tests are `#[ignore]` with a reason; don't make them part of `make test`.

## Git
- Small, focused commits; one logical change each. `target/` is ignored; never commit build output, PNGs from tests, or
  `.claude/settings.local.json`.
- Messages: short, imperative, match the existing language (the history so far is Portuguese). Don't commit unless asked.
