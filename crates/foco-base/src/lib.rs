//! foco-base: the frozen shared contract of the Foquinho Rust rewrite.
//!
//! Geometry, colours/theme, time, animation (easings, tween, spring), damage tracking,
//! timer scheduling, wake requests, input events, the persisted model, store actions/effects,
//! shell commands and the `Surface` drawing contract. The public API only grows, never changes.

pub mod action;
pub mod anim;
pub mod color;
pub mod damage;
pub mod event;
pub mod geom;
pub mod model;
pub mod sched;
pub mod surface;
pub mod theme;
pub mod time;
pub mod wake;

pub use action::{parse_command, pet_event, shortcut_command, split_line, Action, AppCmd, Effect, ScreenArg};
pub use anim::{Anim, Easing, Seq, Spring, Tween};
pub use color::{swap_rb_in_place, Color};
pub use damage::Damage;
pub use event::{Button, Event, Key, KeyEvent, Mods};
pub use geom::{IRect, Point, Rect, Size};
pub use model::{Mood, PetKind, Settings, StateFile, Task};
pub use sched::{Scheduler, TimerKey};
pub use surface::{
    path_arc, path_circle, path_ellipse, path_from_svg, path_rect, path_rrect, Fill, PixSurface, Surface, TextStyle,
    Weight,
};
pub use time::{day_key, epoch_ms, mono_ms, next_midnight_epoch, start_of_day_epoch, Ms};
pub use wake::Wake;
