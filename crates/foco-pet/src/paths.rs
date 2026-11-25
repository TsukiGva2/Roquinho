//! Cached `tiny_skia::Path`s for every shape of `Pet.qml` that does not depend on animated
//! values. Coordinates are those of the QML items (40x40 design grid or the item's own box).

use foco_base::{path_from_svg, path_rrect, Rect};
use std::sync::OnceLock;
use tiny_skia::Path;

pub struct Paths {
    pub tail: Path,
    pub tail_tip: Path,
    pub ear_l: Path,
    pub ear_l_in: Path,
    pub ear_r: Path,
    pub ear_r_in: Path,
    pub ghost: Path,
    pub slime: Path,
    pub brow_determined: Path,
    pub brow_worried: Path,
    pub brow_tilt: Path,
    pub brow_raised: Path,
    pub eye_closed: Path,
    pub eye_happy: Path,
    pub star: Path,
    pub star_edge: Path,
    pub heart: Path,
    pub m_small: Path,
    pub m_cat: Path,
    pub m_flat: Path,
    pub m_wavy: Path,
    pub m_smile: Path,
    pub grin: Path,
    pub grin_tongue: Path,
    pub sweat: Path,
    pub spark: Path,
    pub bolt: Path,
    pub cat_body: Path,
    pub cat_foot_l: Path,
    pub cat_foot_r: Path,
    pub slime_hl1: Path,
    pub slime_hl2: Path,
    pub cheek_l: Path,
    pub cheek_r: Path,
    pub hand: Path,
    pub hand_border: Path,
    pub tongue: Path,
    pub mouth_o: Path,
    pub yawn: Path,
    pub yawn_tongue: Path,
    pub eye_hl1: Path,
    pub eye_hl2: Path,
    pub wide_hl: Path,
    // ---- goth-punk cat accessories
    pub mohawk: Path,
    pub mohawk_root: Path,
    /// Two silver studs on the left ear, a hoop on the right one.
    pub studs_l: Path,
    pub ring_r: Path,
    /// Fangs under a line mouth (mouth item coordinates).
    pub fangs: Path,
    /// Fangs hanging from the top edge of the open grin.
    pub grin_fangs: Path,
}

fn studs(at: &[(f32, f32)], r: f32) -> Path {
    let mut pb = tiny_skia::PathBuilder::new();
    for &(x, y) in at {
        pb.push_circle(x, y, r);
    }
    pb.finish().expect("studs")
}

fn ring(cx: f32, cy: f32, r: f32) -> Path {
    stroked(&foco_base::path_circle(cx, cy, r).expect("ring"), 0.6, true)
}

fn svg(d: &str) -> Path {
    path_from_svg(d).expect("valid pet path")
}

fn rr(x: f32, y: f32, w: f32, h: f32, r: f32) -> Path {
    path_rrect(Rect::new(x, y, w, h), r).expect("valid pet rrect")
}

/// A stroked outline pre-converted to a fill path (cheaper than stroking at every paint).
/// `res_scale` 4 keeps curves smooth up to 4x magnification.
fn outline(d: &str, width: f32, round_join: bool) -> Path {
    stroked(&svg(d), width, round_join)
}

fn stroked(p: &Path, width: f32, round_join: bool) -> Path {
    let st = tiny_skia::Stroke {
        width,
        line_cap: tiny_skia::LineCap::Round,
        line_join: if round_join { tiny_skia::LineJoin::Round } else { tiny_skia::LineJoin::Miter },
        ..tiny_skia::Stroke::default()
    };
    p.stroke(&st, 4.0).expect("stroke pet path")
}

const STAR: &str = "M4 0.3L5.1 2.8L7.8 3L5.7 4.8L6.4 7.5L4 6L1.6 7.5L2.3 4.8L0.2 3L2.9 2.8Z";
const HEART: &str = "M4 7.2C1 5.2 0.3 3.5 0.9 2.2C1.6 0.9 3.3 1 4 2.5C4.7 1 6.4 0.9 7.1 2.2C7.7 3.5 7 5.2 4 7.2Z";

impl Paths {
    fn build() -> Paths {
        Paths {
            tail: outline("M29 32.5Q37.5 33 37 24.5Q36.8 20.8 34.6 19.6", 3.4, false),
            tail_tip: outline("M36.2 21.6Q35.8 20.2 34.6 19.6", 3.4, false),
            ear_l: svg("M7 19.5L9.4 5.8Q9.9 4.6 10.9 5.4L19 12.5Z"),
            ear_l_in: svg("M10.4 14.5L11.3 8.8L15.6 12.6Z"),
            ear_r: svg("M33 19.5L30.6 5.8Q30.1 4.6 29.1 5.4L21 12.5Z"),
            ear_r_in: svg("M29.6 14.5L28.7 8.8L24.4 12.6Z"),
            ghost: svg(
                "M7 35V19.5A13 13 0 0 1 33 19.5V35Q30.8 32.2 28.6 35Q26.4 37.8 24.2 35Q22 32.2 19.8 35Q17.6 37.8 15.4 35Q13.2 32.2 11 35Q9 37.6 7 35Z",
            ),
            slime: svg("M4 35.5C4 22 11.5 12.5 20 12.5C28.5 12.5 36 22 36 35.5Q36 36.6 34.9 36.6H5.1Q4 36.6 4 35.5Z"),
            brow_determined: outline("M11.6 14.4L16.2 15.9M28.4 14.4L23.8 15.9", 1.25, false),
            brow_worried: outline("M11.8 15.9L16.2 14.5M28.2 15.9L23.8 14.5", 1.25, false),
            brow_tilt: outline("M11.8 14.2Q14 12.8 16.4 14M23.8 15.5L28.2 15.5", 1.25, false),
            brow_raised: outline("M11.8 14.6Q14 13.2 16.4 14.4M23.6 14.4Q26 13.2 28.2 14.6", 1.25, false),
            eye_closed: outline("M1.6 3.8Q4 6.4 6.4 3.8", 1.5, false),
            eye_happy: outline("M1.6 5.2Q4 1.6 6.4 5.2", 1.5, false),
            star: svg(STAR),
            star_edge: outline(STAR, 0.4, true),
            heart: svg(HEART),
            m_small: outline("M3.9 2.4Q5 3.4 6.1 2.4", 1.2, true),
            m_cat: outline("M2.6 1.9Q3.8 3.5 5 1.9Q6.2 3.5 7.4 1.9", 1.2, true),
            m_flat: outline("M3.4 2.8L6.6 2.8", 1.2, true),
            m_wavy: outline("M2.8 3Q3.9 2 5 3Q6.1 4 7.2 3", 1.2, true),
            m_smile: outline("M3.2 2Q5 4 6.8 2", 1.2, true),
            grin: svg("M2.6 1.4H7.4Q7.4 5.2 5 5.2Q2.6 5.2 2.6 1.4Z"),
            grin_tongue: svg("M3.6 4.2Q5 3 6.4 4.2Q5.8 5.1 5 5.1Q4.2 5.1 3.6 4.2Z"),
            sweat: svg("M3 0Q6 4 5.2 5.6Q3 8.4 0.8 5.6Q0 4 3 0Z"),
            spark: svg("M3 0Q3.4 2.6 6 3Q3.4 3.4 3 6Q2.6 3.4 0 3Q2.6 2.6 3 0Z"),
            bolt: svg("M5 0L0.6 5.8H3.8L3 10L7.4 4.2H4.2Z"),
            cat_body: rr(5.0, 10.5, 30.0, 25.5, 12.75),
            cat_foot_l: rr(10.2, 33.6, 6.0, 3.4, 1.7),
            cat_foot_r: rr(23.8, 33.6, 6.0, 3.4, 1.7),
            slime_hl1: rr(0.0, 0.0, 5.5, 2.8, 1.4),
            slime_hl2: rr(16.5, 15.4, 1.8, 1.8, 0.9),
            cheek_l: rr(8.4, 23.4, 4.6, 2.5, 1.25),
            cheek_r: rr(27.0, 23.4, 4.6, 2.5, 1.25),
            hand: rr(0.0, 0.0, 5.0, 4.4, 2.2),
            // Qt draws a Rectangle border inside the item: stroke centred 0.25 in.
            hand_border: stroked(&rr(0.25, 0.25, 4.5, 3.9, 1.95), 0.5, false),
            tongue: rr(4.6, 2.7, 2.0, 2.3, 1.0),
            mouth_o: rr(3.6, 1.2, 2.8, 3.2, 1.4),
            yawn: rr(3.0, 0.6, 4.0, 4.8, 2.0),
            yawn_tongue: rr(0.9, 2.8, 2.2, 1.6, 0.8),
            eye_hl1: rr(0.7, 0.8, 1.6, 1.6, 0.8),
            eye_hl2: rr(2.6, 3.4, 0.8, 0.8, 0.4),
            wide_hl: rr(0.6, 0.6, 1.2, 1.2, 0.6),
            mohawk: svg(
                "M14.6 12.6L15.4 8L17 10.2L18 6L19.2 9.6L20 4.6L20.8 9.6L22 6L23 10.2L24.6 8L25.4 12.6Q20 11.4 14.6 12.6Z",
            ),
            mohawk_root: svg("M15 12.5L15.6 10.2Q20 9 24.4 10.2L25 12.5Q20 11.4 15 12.5Z"),
            studs_l: studs(&[(9.3, 10.4), (8.8, 13.2)], 0.75),
            ring_r: ring(32.1, 12.4, 1.15),
            fangs: svg("M3.3 2.6L4.5 2.6L3.9 4.2Z M5.5 2.6L6.7 2.6L6.1 4.2Z"),
            grin_fangs: svg("M3 1.4L4.2 1.4L3.6 2.9Z M5.8 1.4L7 1.4L6.4 2.9Z"),
        }
    }
}

static PATHS: OnceLock<Paths> = OnceLock::new();

/// The process-wide path cache (built on first use).
pub fn paths() -> &'static Paths {
    PATHS.get_or_init(Paths::build)
}
