//! Widgets shared by the views (ui-design.md §5, §6 and the "other widgets" list).
//!
//! Widgets are not `View`s: they own their visual state and animations, take a `Cx` for
//! damage/text, and are driven by the view that contains them.

pub mod color_anim;
pub mod icon;
pub mod icon_button;
pub mod keyed_list;
pub mod label;
pub mod mouse;
pub mod pill;
pub mod scroll;
pub mod section_label;
pub mod text_input;
pub mod toggle;

pub use color_anim::ColorAnim;
pub use icon::{paint_icon, Icon, DEFAULT_STROKE};
pub use icon_button::IconButton;
pub use keyed_list::{Entry, Fx, KeyedList, ListSpec, Track, Transition};
pub use label::{paint_text_centered, paint_text_centered_in, text_top_for_center, wrap};
pub use mouse::{Hover, MouseArea, MouseOut};
pub use pill::{chip_width, paint_chip, paint_pill, pill_style, pill_width, PILL_H};
pub use scroll::Scroll;
pub use section_label::{paint_section_label, SECTION_LABEL_H};
pub use text_input::{InputOutcome, TextInput};
pub use toggle::Toggle;
