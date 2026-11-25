//! foco-ui: widgets (IconButton, Toggle, pill, TextInput, animated list, scroll), the notch
//! silhouette and the views. Views talk to the store through [`StoreRead`], measure text through
//! [`TextMeasure`] and reach the mascot through [`pet::PetLike`], so this crate depends only on
//! `foco-base`; the `foco-max` binary wires the real implementations.

pub mod cx;
pub mod model;
pub mod notch;
pub mod pet;
pub mod real;
pub mod testkit;
pub mod view;
pub mod views;
pub mod widgets;

pub use cx::{
    Cx, CursorShape, DamageSink, Env, FocusId, Handled, Msg, PaintCx, ScreenInfo, StoreRead, TextMeasure, AUTO_ACCENT, S,
    WIN, WIN_DEV,
};
pub use notch::NotchGeom;
pub use pet::{new_pet, PetLike};
pub use view::{Section, View};
