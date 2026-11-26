//! Static step tables for the reaction/fidget animations of `Pet.qml`
//! (`SequentialAnimation` / `ParallelAnimation` / `NumberAnimation` / `PauseAnimation` /
//! `ScriptAction`). They are played by a tiny sampling interpreter in `lib.rs` that, like
//! Qt, captures a segment's start value when the segment begins.

use foco_base::Easing;

/// An animated property of the pet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ch {
    Squash,
    Jump,
    HandRY,
    HandRRot,
    HandLY,
    FaceDy,
    EarL,
    EarR,
    TailA,
    TalkH,
    HeartsT,
    SparkT,
    PopS,
    NotesT,
    Blink,
    /// `Behavior on tilt` (320 ms `OutBack`).
    Tilt,
    /// `Behavior on lookX` (240 ms `OutCubic`).
    LookX,
    /// `Behavior on lookY` (240 ms `OutCubic`).
    LookY,
}

/// One `NumberAnimation`. `from` is NaN when the animation starts from the current value.
#[derive(Clone, Copy, Debug)]
pub struct Mv {
    pub ch: Ch,
    pub from: f32,
    pub to: f32,
    pub dur: f32,
    pub ease: Easing,
}

#[derive(Clone, Copy, Debug)]
pub enum Step {
    /// Moves that run together; the step lasts as long as the longest.
    Par(&'static [Mv]),
    Pause(f32),
    /// `ScriptAction` setting a property.
    Set(Ch, f32),
}

impl Step {
    pub fn dur(&self) -> f32 {
        match self {
            Step::Par(m) => m.iter().fold(0.0, |a, x| a.max(x.dur)),
            Step::Pause(d) => *d,
            Step::Set(..) => 0.0,
        }
    }
}

/// A whole animation: `loops` passes (0 = infinite), then the `fin` assignments (`onFinished`).
pub struct Def {
    pub steps: &'static [Step],
    pub loops: u8,
    pub fin: &'static [(Ch, f32)],
}

impl Def {
    /// Duration of one pass in ms.
    pub fn cycle(&self) -> f64 {
        self.steps.iter().map(|s| s.dur() as f64).sum()
    }
}

const fn mv(ch: Ch, to: f32, dur: f32, ease: Easing) -> Mv {
    Mv { ch, from: f32::NAN, to, dur, ease }
}
const fn mvf(ch: Ch, from: f32, to: f32, dur: f32, ease: Easing) -> Mv {
    Mv { ch, from, to, dur, ease }
}
const fn one(m: &'static [Mv]) -> Step {
    Step::Par(m)
}

use Easing::{
    InOutQuad as IOQ, InOutSine as IOS, InQuad as IQ, Linear as LIN, OutCubic as OC, OutQuad as OQ, OutSine as OS,
};
const OB: Easing = Easing::OUT_BACK;

// run ids (index into DEFS)
pub const JUMP: usize = 0;
pub const HOP: usize = 1;
pub const SQUISH: usize = 2;
pub const WAVE: usize = 3;
pub const FIST: usize = 4;
pub const STRETCH: usize = 5;
pub const SIGH: usize = 6;
pub const LOOK: usize = 7;
pub const NOD: usize = 8;
pub const EAR: usize = 9;
pub const TAIL_FLICK: usize = 10;
pub const TURN: usize = 11;
pub const SWAY: usize = 12;
pub const HEARTS: usize = 13;
pub const SPARK: usize = 14;
pub const POP: usize = 15;
pub const NOTES: usize = 16;
pub const BLINK: usize = 17;
pub const BLINK2: usize = 18;
pub const TALK: usize = 19;
pub const NRUN: usize = 20;

use Ch::{
    Blink, EarL, EarR, FaceDy, HandLY, HandRRot, HandRY, HeartsT, Jump, LookX, LookY, NotesT, PopS, SparkT, Squash,
    TailA, TalkH, Tilt,
};

pub static DEFS: [Def; NRUN] = [
    // jumpAnim
    Def {
        steps: &[
            one(&[mv(Squash, 0.16, 90.0, OQ)]),
            one(&[mv(Jump, -8.0, 180.0, OQ), mv(Squash, -0.1, 120.0, LIN)]),
            one(&[mv(Jump, 0.0, 170.0, IQ), mv(Squash, 0.0, 170.0, LIN)]),
            one(&[mv(Squash, 0.18, 70.0, LIN)]),
            one(&[mv(Squash, 0.0, 160.0, OB)]),
            one(&[mv(Jump, -4.5, 150.0, OQ)]),
            one(&[mv(Jump, 0.0, 150.0, IQ)]),
            one(&[mv(Squash, 0.1, 60.0, LIN)]),
            one(&[mv(Squash, 0.0, 180.0, OB)]),
        ],
        loops: 1,
        fin: &[],
    },
    // hopAnim
    Def {
        steps: &[
            one(&[mv(Squash, 0.1, 80.0, LIN)]),
            one(&[mv(Jump, -4.0, 140.0, OQ), mv(Squash, -0.06, 100.0, LIN)]),
            one(&[mv(Jump, 0.0, 140.0, IQ), mv(Squash, 0.0, 140.0, LIN)]),
            one(&[mv(Squash, 0.1, 60.0, LIN)]),
            one(&[mv(Squash, 0.0, 160.0, OB)]),
        ],
        loops: 1,
        fin: &[],
    },
    // squishAnim
    Def {
        steps: &[
            one(&[mv(Squash, 0.2, 150.0, OQ)]),
            one(&[mv(Squash, 0.0, 700.0, Easing::OutElastic { amplitude: 1.2, period: 0.35 })]),
        ],
        loops: 1,
        fin: &[],
    },
    // waveAnim (inner `loops: 3` unrolled)
    Def {
        steps: &[
            one(&[mv(HandRY, -9.0, 200.0, OB)]),
            one(&[mv(HandRRot, -30.0, 130.0, IOS)]),
            one(&[mv(HandRRot, 18.0, 130.0, IOS)]),
            one(&[mv(HandRRot, -30.0, 130.0, IOS)]),
            one(&[mv(HandRRot, 18.0, 130.0, IOS)]),
            one(&[mv(HandRRot, -30.0, 130.0, IOS)]),
            one(&[mv(HandRRot, 18.0, 130.0, IOS)]),
            one(&[mv(HandRRot, 0.0, 120.0, LIN)]),
            one(&[mv(HandRY, 0.0, 220.0, IOQ)]),
        ],
        loops: 1,
        fin: &[],
    },
    // fistAnim (loops: 2)
    Def {
        steps: &[
            one(&[mv(HandRY, -10.0, 130.0, OQ), mv(HandLY, -3.0, 130.0, LIN), mv(Jump, -2.5, 130.0, OQ)]),
            one(&[mv(HandRY, -4.0, 150.0, IQ), mv(HandLY, 0.0, 150.0, LIN), mv(Jump, 0.0, 150.0, IQ)]),
        ],
        loops: 2,
        fin: &[(HandRY, 0.0)],
    },
    // stretchAnim
    Def {
        steps: &[one(&[mv(Squash, -0.1, 600.0, OS)]), Step::Pause(350.0), one(&[mv(Squash, 0.0, 450.0, OB)])],
        loops: 1,
        fin: &[],
    },
    // sighAnim
    Def {
        steps: &[
            one(&[mv(Squash, -0.05, 500.0, IOS)]),
            one(&[mv(Squash, 0.1, 700.0, IOS)]),
            one(&[mv(Squash, 0.0, 500.0, IOS)]),
        ],
        loops: 1,
        fin: &[],
    },
    // lookAnim
    Def {
        steps: &[
            Step::Set(LookX, -1.6),
            Step::Set(LookY, -0.3),
            Step::Pause(700.0),
            Step::Set(LookX, 1.6),
            Step::Set(LookY, 0.2),
            Step::Pause(700.0),
            Step::Set(LookX, 0.0),
            Step::Set(LookY, 0.0),
        ],
        loops: 1,
        fin: &[],
    },
    // nodAnim (loops: 2)
    Def { steps: &[one(&[mv(FaceDy, 1.3, 140.0, OQ)]), one(&[mv(FaceDy, 0.0, 180.0, IOQ)])], loops: 2, fin: &[] },
    // earAnim
    Def {
        steps: &[
            one(&[mv(EarR, -16.0, 70.0, LIN)]),
            one(&[mv(EarR, 0.0, 110.0, LIN)]),
            one(&[mv(EarR, -10.0, 70.0, LIN)]),
            one(&[mv(EarR, 0.0, 140.0, LIN)]),
            one(&[mv(EarL, 12.0, 80.0, LIN)]),
            one(&[mv(EarL, 0.0, 160.0, LIN)]),
        ],
        loops: 1,
        fin: &[],
    },
    // tailFlick
    Def {
        steps: &[
            one(&[mv(TailA, 24.0, 120.0, OQ)]),
            one(&[mv(TailA, -14.0, 160.0, LIN)]),
            one(&[mv(TailA, 0.0, 200.0, LIN)]),
        ],
        loops: 1,
        fin: &[],
    },
    // turnAnim
    Def { steps: &[Step::Set(Tilt, 6.0), Step::Pause(1400.0), Step::Set(Tilt, 0.0)], loops: 1, fin: &[] },
    // swayAnim (loops: 2, onFinished tilt = 0)
    Def {
        steps: &[Step::Set(Tilt, -5.0), Step::Pause(600.0), Step::Set(Tilt, 5.0), Step::Pause(600.0)],
        loops: 2,
        fin: &[(Tilt, 0.0)],
    },
    // heartsAnim
    Def { steps: &[one(&[mvf(HeartsT, 0.0, 1.0, 1500.0, OC)])], loops: 1, fin: &[] },
    // sparkAnim
    Def { steps: &[one(&[mvf(SparkT, 0.0, 1.0, 1100.0, LIN)])], loops: 1, fin: &[] },
    // popAnim (easing.overshoot: 3)
    Def { steps: &[one(&[mvf(PopS, 0.0, 1.0, 420.0, Easing::OutBack { overshoot: 3.0 })])], loops: 1, fin: &[] },
    // notesAnim
    Def { steps: &[one(&[mvf(NotesT, 0.0, 1.0, 2400.0, LIN)])], loops: 1, fin: &[] },
    // blinkAnim
    Def { steps: &[one(&[mv(Blink, 0.1, 70.0, LIN)]), one(&[mv(Blink, 1.0, 110.0, LIN)])], loops: 1, fin: &[] },
    // doubleBlink
    Def {
        steps: &[
            one(&[mv(Blink, 0.1, 60.0, LIN)]),
            one(&[mv(Blink, 1.0, 80.0, LIN)]),
            Step::Pause(90.0),
            one(&[mv(Blink, 0.1, 60.0, LIN)]),
            one(&[mv(Blink, 1.0, 100.0, LIN)]),
        ],
        loops: 1,
        fin: &[],
    },
    // talk loop (talkH)
    Def {
        steps: &[
            one(&[mv(TalkH, 3.6, 90.0, LIN)]),
            one(&[mv(TalkH, 1.4, 80.0, LIN)]),
            one(&[mv(TalkH, 3.0, 110.0, LIN)]),
            one(&[mv(TalkH, 0.9, 120.0, LIN)]),
            one(&[mv(TalkH, 2.4, 90.0, LIN)]),
            one(&[mv(TalkH, 1.0, 160.0, LIN)]),
        ],
        loops: 0,
        fin: &[],
    },
];
