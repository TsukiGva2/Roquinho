//! What a view wants next from the event loop.

use crate::time::Ms;

/// Frame period at 60 Hz in ms.
pub const FRAME_MS: Ms = 1000.0 / 60.0;
/// Ambient animation period (30 Hz) in ms.
pub const AMBIENT_MS: Ms = 1000.0 / 30.0;

/// A view's wake request.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Wake {
    /// Nothing to do: sleep indefinitely.
    Idle,
    /// Animating: next frame at 60 Hz.
    Frame,
    /// Ambient animation (breathing, pulsing dot): 30 Hz.
    Ambient,
    /// One-shot wake at this monotonic time.
    At(Ms),
}

impl Wake {
    /// Builds an `At(t)`, normalised: a deadline within one frame of `now` is just `Frame`.
    pub fn at(now: Ms, t: Ms) -> Wake {
        if t <= now + FRAME_MS {
            Wake::Frame
        } else {
            Wake::At(t)
        }
    }
    fn rank(self) -> u8 {
        match self {
            Wake::Idle => 0,
            Wake::At(_) => 1,
            Wake::Ambient => 2,
            Wake::Frame => 3,
        }
    }
    /// Combines two requests by priority: Frame > Ambient > At (earliest) > Idle.
    /// Build `At` values with [`Wake::at`] so near deadlines are already `Frame`.
    /// When an early `At` may compete with `Ambient`, use [`merge_at`](Self::merge_at).
    pub fn merge(self, other: Wake) -> Wake {
        match (self, other) {
            (Wake::At(a), Wake::At(b)) => Wake::At(a.min(b)),
            _ => {
                if self.rank() >= other.rank() {
                    self
                } else {
                    other
                }
            }
        }
    }
    /// Exact merge: whichever request fires first relative to `now` wins
    /// (ties go to the higher-priority kind).
    pub fn merge_at(self, other: Wake, now: Ms) -> Wake {
        match (self.deadline(now), other.deadline(now)) {
            (None, _) => other,
            (_, None) => self,
            (Some(a), Some(b)) => {
                if a < b || (a == b && self.rank() >= other.rank()) {
                    self
                } else {
                    other
                }
            }
        }
    }
    /// Absolute monotonic deadline (`None` for Idle).
    pub fn deadline(self, now: Ms) -> Option<Ms> {
        match self {
            Wake::Idle => None,
            Wake::Frame => Some(now + FRAME_MS),
            Wake::Ambient => Some(now + AMBIENT_MS),
            Wake::At(t) => Some(t),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priority_merge() {
        assert_eq!(Wake::Idle.merge(Wake::Idle), Wake::Idle);
        assert_eq!(Wake::Idle.merge(Wake::At(5.0)), Wake::At(5.0));
        assert_eq!(Wake::At(5.0).merge(Wake::At(3.0)), Wake::At(3.0));
        assert_eq!(Wake::At(5.0).merge(Wake::Ambient), Wake::Ambient);
        assert_eq!(Wake::Ambient.merge(Wake::Frame), Wake::Frame);
        assert_eq!(Wake::Frame.merge(Wake::At(1.0)), Wake::Frame);
    }

    #[test]
    fn at_normalises() {
        assert_eq!(Wake::at(100.0, 110.0), Wake::Frame);
        assert_eq!(Wake::at(100.0, 500.0), Wake::At(500.0));
    }

    #[test]
    fn exact_merge_and_deadline() {
        let now = 1000.0;
        assert_eq!(Wake::Frame.deadline(now), Some(1000.0 + FRAME_MS));
        assert_eq!(Wake::Ambient.deadline(now), Some(1000.0 + AMBIENT_MS));
        assert_eq!(Wake::Idle.deadline(now), None);
        assert_eq!(Wake::Ambient.merge_at(Wake::At(1020.0), now), Wake::At(1020.0));
        assert_eq!(Wake::Ambient.merge_at(Wake::At(1040.0), now), Wake::Ambient);
        assert_eq!(Wake::Idle.merge_at(Wake::At(1040.0), now), Wake::At(1040.0));
        assert_eq!(Wake::Frame.merge_at(Wake::Ambient, now), Wake::Frame);
    }
}
