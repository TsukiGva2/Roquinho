//! foco-core: epoll reactor, abstract-socket IPC + CLI, pure store state machine
//! (`apply(Action, now) -> Vec<Effect>`), state.json persistence, foco-maxrc, autostart.

pub mod autostart;
pub mod cli;
pub mod fsutil;
pub mod ipc;
pub mod paths;
pub mod rc;
pub mod reactor;
pub mod store;

pub use rc::Rc;
pub use reactor::{Reactor, Ready};
pub use store::Store;
