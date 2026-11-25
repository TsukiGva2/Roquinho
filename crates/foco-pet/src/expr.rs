//! Expression tables of `Pet.qml` (`eyes`, `mouth`, `brows` lookups keyed by `expr`).

use foco_base::{Mood, PetKind};

/// The current "expression" (`pet.expr`): talk > reaction > hover (content) > mood.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Expr {
    Sleep,
    Idle,
    Focus,
    Tired,
    Happy,
    Celebrate,
    Wave,
    Surprised,
    Love,
    Wink,
    Ready,
    Talk,
    Content,
    Yawn,
    Curious,
    Hum,
    Sigh,
    /// A reaction name Pet.qml has no table entry for (open eyes, smile, no brows).
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Eyes {
    Closed,
    Open,
    Half,
    Happy,
    Star,
    Wide,
    Heart,
    Wink,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mouth {
    Small,
    Smile,
    Cat,
    Flat,
    Wavy,
    Tongue,
    O,
    Grin,
    Talk,
    Yawn,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Brows {
    None,
    Determined,
    Worried,
    Raised,
    Tilt,
}

impl Expr {
    pub fn from_mood(m: Mood) -> Expr {
        match m {
            Mood::Sleep => Expr::Sleep,
            Mood::Idle => Expr::Idle,
            Mood::Focus => Expr::Focus,
            Mood::Tired => Expr::Tired,
            Mood::Happy => Expr::Happy,
        }
    }

    pub fn from_name(n: &str) -> Expr {
        match n {
            "sleep" => Expr::Sleep,
            "idle" => Expr::Idle,
            "focus" => Expr::Focus,
            "tired" => Expr::Tired,
            "happy" => Expr::Happy,
            "celebrate" => Expr::Celebrate,
            "wave" => Expr::Wave,
            "surprised" => Expr::Surprised,
            "love" => Expr::Love,
            "wink" => Expr::Wink,
            "ready" => Expr::Ready,
            "talk" => Expr::Talk,
            "content" => Expr::Content,
            "yawn" => Expr::Yawn,
            "curious" => Expr::Curious,
            "hum" => Expr::Hum,
            "sigh" => Expr::Sigh,
            _ => Expr::Other,
        }
    }

    pub fn eyes(self) -> Eyes {
        match self {
            Expr::Sleep | Expr::Yawn | Expr::Hum => Eyes::Closed,
            Expr::Tired | Expr::Sigh => Eyes::Half,
            Expr::Happy | Expr::Content => Eyes::Happy,
            Expr::Celebrate => Eyes::Star,
            Expr::Surprised => Eyes::Wide,
            Expr::Love => Eyes::Heart,
            Expr::Wink => Eyes::Wink,
            Expr::Idle
            | Expr::Focus
            | Expr::Wave
            | Expr::Ready
            | Expr::Talk
            | Expr::Curious
            | Expr::Other => Eyes::Open,
        }
    }

    pub fn mouth(self, kind: PetKind) -> Mouth {
        let cat = kind == PetKind::Cat;
        match self {
            Expr::Sleep | Expr::Curious => Mouth::Small,
            Expr::Idle | Expr::Other => Mouth::Smile,
            Expr::Focus | Expr::Content => {
                if cat {
                    Mouth::Cat
                } else {
                    Mouth::Smile
                }
            }
            Expr::Tired => Mouth::Wavy,
            Expr::Happy | Expr::Celebrate | Expr::Wave | Expr::Ready => Mouth::Grin,
            Expr::Surprised | Expr::Hum => Mouth::O,
            Expr::Love | Expr::Wink => Mouth::Tongue,
            Expr::Talk => Mouth::Talk,
            Expr::Yawn => Mouth::Yawn,
            Expr::Sigh => Mouth::Flat,
        }
    }

    pub fn brows(self) -> Brows {
        match self {
            Expr::Focus | Expr::Ready => Brows::Determined,
            Expr::Tired | Expr::Sigh => Brows::Worried,
            Expr::Surprised | Expr::Talk | Expr::Wave => Brows::Raised,
            Expr::Curious => Brows::Tilt,
            _ => Brows::None,
        }
    }
}
