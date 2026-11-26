//! `Pet.qml` drawing: every Item/Shape/Rectangle/Text of the `stage` on the 40x40 grid.

use crate::expr::{Brows, Expr, Eyes, Mouth};
use crate::paths::{paths, Paths};
use crate::Pet;
use foco_base::{path_rrect, theme, Color, Fill, PetKind, Rect, Surface, TextStyle, Weight};
use tiny_skia::Path;

const INK: Color = Color::rgb8(0x1d, 0x1d, 0x1f);
const MOUTH_FILL: Color = Color::rgb8(0x7A, 0x25, 0x33);
const PINK: Color = Color::rgb8(0xFF, 0x7A, 0x90);
const YELLOW: Color = Color::rgb8(0xFF, 0xD6, 0x0A);
const STAR_EDGE: Color = Color::rgb8(0xE0, 0xA8, 0x00);
const HEART_EYE: Color = Color::rgb8(0xFF, 0x37, 0x5F);

// ---- goth-punk cat (Foquinho): black, white linework, green mohawk, silver piercings
/// White lines (closed eyes, mouth) and the ring of the open eyes.
const GOTH_LINE: Color = Color::rgb8(0xF4, 0xF4, 0xF2);
const GOTH_PUPIL: Color = Color::rgb8(0x0B, 0x0B, 0x0D);
const GOTH_MOUTH: Color = Color::rgb8(0x3A, 0x18, 0x20);
const MOHAWK: Color = Color::rgb8(0x6C, 0xA8, 0x2E);
const MOHAWK_ROOT: Color = Color::rgb8(0x3E, 0x6B, 0x1A);
/// Eyebrows: dyed to match the mohawk.
const GOTH_BROW: Color = Color::rgb8(0x8C, 0xC8, 0x3C);
const GOTH_INNER_EAR: Color = Color::rgb8(0xF4, 0xF4, 0xF2);
const SILVER: Color = Color::rgb8(0xC8, 0xCC, 0xD4);
const GOTH_HAND_EDGE: Color = Color::rgb8(0x3A, 0x3A, 0x3E);

fn solid(c: Color) -> Fill {
    Fill::Solid(c)
}

/// Fills `path` translated by (x, y).
fn fill_at(s: &mut dyn Surface, path: &Path, x: f32, y: f32, c: Color) {
    s.save();
    s.translate(x, y);
    s.fill_path(path, &solid(c));
    s.restore();
}

/// Rotates by `deg` around (ox, oy) (QML `Rotation { origin; angle }`); caller saves/restores.
fn rotate_about(s: &mut dyn Surface, ox: f32, oy: f32, deg: f32) {
    s.translate(ox, oy);
    s.rotate_deg(deg);
    s.translate(-ox, -oy);
}

/// Scales by `k` around (ox, oy); caller saves/restores.
fn scale_about(s: &mut dyn Surface, ox: f32, oy: f32, k: f32) {
    s.translate(ox, oy);
    s.scale(k, k);
    s.translate(-ox, -oy);
}

pub(crate) fn paint(pet: &Pet, s: &mut dyn Surface, rect: Rect) {
    if rect.w <= 0.0 {
        return;
    }
    let k = rect.w / 40.0;
    let pa = paths();
    let p = &pet.p;
    let expr = pet.expr();
    let kind = pet.kind;
    let ghost = kind == PetKind::Ghost;

    s.save();
    s.translate(rect.x, rect.y);
    s.scale(k, k);

    let jb = p.jump + p.bob;

    // ---- shadow on the floor (only from 36 px up)
    if rect.w >= 36.0 {
        let w = (if ghost { 16.0 } else { 22.0 }) * (1.0 + jb / 22.0);
        let op = ((if ghost { 0.22 } else { 0.32 }) * (1.0 + jb / 14.0)).clamp(0.0, 1.0);
        if w > 0.0 {
            if let Some(path) = path_rrect(Rect::new(20.0 - w / 2.0, 36.4, w, 2.6), 1.3) {
                s.fill_path(&path, &solid(Color::BLACK.with_alpha(op)));
            }
        }
    }

    // ---- figure
    s.save();
    s.translate(0.0, jb);
    // QML: transform [Scale, Rotation] about (20,37): the scale applies first, then the tilt.
    let sy = p.breathe * (1.0 - p.squash);
    let sx = (1.0 + (1.0 - p.breathe) * if kind == PetKind::Slime { 0.9 } else { 0.5 }) * (1.0 + p.squash * 0.7);
    s.translate(20.0, 37.0);
    s.rotate_deg(p.tilt);
    s.scale(sx, sy);
    s.translate(-20.0, -37.0);

    match kind {
        PetKind::Cat => paint_cat_back(pet, s, pa),
        PetKind::Ghost => s.fill_path(&pa.ghost, &solid(pet.body)),
        PetKind::Slime => {
            s.fill_path(&pa.slime, &solid(pet.body));
            s.save();
            s.translate(10.0, 17.5);
            rotate_about(s, 2.75, 1.4, -38.0);
            s.fill_path(&pa.slime_hl1, &solid(Color::WHITE.with_alpha(0.55)));
            s.restore();
            s.fill_path(&pa.slime_hl2, &solid(Color::WHITE.with_alpha(0.45)));
        }
    }

    paint_face(pet, s, pa, expr);

    // hands
    let border = if kind == PetKind::Cat { GOTH_HAND_EDGE } else { pet.shade.with_alpha(0.55) };
    hand(s, pa, 3.4, 25.5 + p.hand_ly, 0.0, pet.body, border);
    hand(s, pa, 31.6, 25.5 + p.hand_ry, p.hand_rrot, pet.body, border);
    s.restore();

    // ---- shape particles (stage coordinates)
    paint_particles(pet, s, pa, expr);
    s.restore();

    // ---- text particles, in device units so the font size is exactly size/40 * px
    paint_texts(pet, s, rect, expr);
}

fn hand(s: &mut dyn Surface, pa: &Paths, x: f32, y: f32, rot: f32, body: Color, border: Color) {
    s.save();
    s.translate(x, y);
    if rot != 0.0 {
        rotate_about(s, 1.0, 4.0, rot);
    }
    s.fill_path(&pa.hand, &solid(body));
    s.fill_path(&pa.hand_border, &solid(border));
    s.restore();
}

fn paint_cat_back(pet: &Pet, s: &mut dyn Surface, pa: &Paths) {
    let p = &pet.p;
    // tail
    s.save();
    rotate_about(s, 30.0, 32.0, p.tail_a);
    s.fill_path(&pa.tail, &solid(pet.body));
    s.fill_path(&pa.tail_tip, &solid(MOHAWK)); // dyed tip
    s.restore();
    // ears
    s.save();
    rotate_about(s, 12.0, 15.0, p.ear_l);
    s.fill_path(&pa.ear_l, &solid(pet.body));
    s.fill_path(&pa.ear_l_in, &solid(GOTH_INNER_EAR));
    s.fill_path(&pa.studs_l, &solid(SILVER));
    s.restore();
    s.save();
    rotate_about(s, 28.0, 15.0, p.ear_r);
    s.fill_path(&pa.ear_r, &solid(pet.body));
    s.fill_path(&pa.ear_r_in, &solid(GOTH_INNER_EAR));
    s.fill_path(&pa.ring_r, &solid(SILVER));
    s.restore();
    // body
    s.fill_path(&pa.cat_body, &solid(pet.body));
    s.fill_path(&pa.cat_foot_l, &solid(pet.foot));
    s.fill_path(&pa.cat_foot_r, &solid(pet.foot));
    // punk mohawk
    s.fill_path(&pa.mohawk, &solid(MOHAWK));
    s.fill_path(&pa.mohawk_root, &solid(MOHAWK_ROOT));
}

fn paint_face(pet: &Pet, s: &mut dyn Surface, pa: &Paths, expr: Expr) {
    let p = &pet.p;
    let eyes = expr.eyes();
    let mouth = expr.mouth(pet.kind);
    let brows = expr.brows();
    let goth = pet.kind == PetKind::Cat;
    let ink = if goth { GOTH_LINE } else { INK };
    let mouth_fill = if goth { GOTH_MOUTH } else { MOUTH_FILL };
    let base_y = match pet.kind {
        PetKind::Slime => 4.0,
        PetKind::Ghost => -0.5,
        PetKind::Cat => 1.0,
    };
    s.save();
    s.translate(p.look_x * 0.6, base_y + p.face_dy + p.look_y * 0.4);

    // eyebrows
    let brow = match brows {
        Brows::None => None,
        Brows::Determined => Some(&pa.brow_determined),
        Brows::Worried => Some(&pa.brow_worried),
        Brows::Tilt => Some(&pa.brow_tilt),
        Brows::Raised => Some(&pa.brow_raised),
    };
    if let Some(b) = brow {
        s.fill_path(b, &solid(if goth { GOTH_BROW } else { INK }));
    }

    // eyes
    for (i, cx) in [14.2f32, 25.8].into_iter().enumerate() {
        let style = if eyes == Eyes::Wink {
            if i == 0 {
                Eyes::Open
            } else {
                Eyes::Happy
            }
        } else {
            eyes
        };
        s.save();
        s.translate(cx - 4.0, 16.5);
        paint_eye(pet, s, pa, style, goth);
        s.restore();
    }

    // cheeks
    let op = if matches!(expr, Expr::Love | Expr::Content) {
        0.85
    } else if pet.mood == foco_base::Mood::Sleep {
        0.3
    } else {
        0.55
    };
    if !goth {
        s.fill_path(&pa.cheek_l, &solid(PINK.with_alpha(op)));
        s.fill_path(&pa.cheek_r, &solid(PINK.with_alpha(op)));
    }

    // mouth
    s.save();
    s.translate(15.0, 22.6);
    let line = |s: &mut dyn Surface, path: &Path| s.fill_path(path, &solid(ink));
    // open mouths: filled, plus a thin white rim on the black cat so they read on the fur
    let open = |s: &mut dyn Surface, path: &Path| {
        s.fill_path(path, &solid(mouth_fill));
        if goth {
            let st = tiny_skia::Stroke { width: 0.7, ..tiny_skia::Stroke::default() };
            s.stroke_path(path, &solid(GOTH_LINE), &st);
        }
    };
    match mouth {
        Mouth::Small => line(s, &pa.m_small),
        Mouth::Cat => line(s, &pa.m_cat),
        Mouth::Flat => line(s, &pa.m_flat),
        Mouth::Wavy => line(s, &pa.m_wavy),
        Mouth::Smile => line(s, &pa.m_smile),
        Mouth::Tongue => {
            line(s, &pa.m_smile);
            s.fill_path(&pa.tongue, &solid(PINK));
        }
        Mouth::O => open(s, &pa.mouth_o),
        Mouth::Grin => {
            open(s, &pa.grin);
            s.fill_path(&pa.grin_tongue, &solid(PINK));
            if goth {
                s.fill_path(&pa.grin_fangs, &solid(Color::WHITE));
            }
        }
        Mouth::Talk => {
            let h = p.talk_h;
            let w = 3.4;
            if let Some(path) = path_rrect(Rect::new(5.0 - w / 2.0, 2.6 - h / 2.0 + 0.6, w, h), w.min(h) / 2.0) {
                open(s, &path);
            }
        }
        Mouth::Yawn => {
            open(s, &pa.yawn);
            fill_at(s, &pa.yawn_tongue, 3.0, 0.6, PINK);
        }
    }
    // vampire fangs under the calm line mouths
    if goth && matches!(mouth, Mouth::Small | Mouth::Cat | Mouth::Smile | Mouth::Flat) {
        s.fill_path(&pa.fangs, &solid(Color::WHITE));
    }
    s.restore();

    s.restore();
}

/// One eye in its 8x8 item (origin at the item's top-left).
fn paint_eye(pet: &Pet, s: &mut dyn Surface, pa: &Paths, style: Eyes, goth: bool) {
    let p = &pet.p;
    let ink = if goth { GOTH_LINE } else { INK };
    match style {
        Eyes::Open | Eyes::Half => {
            let half = style == Eyes::Half;
            let w = if half { 4.4 } else { 4.2 };
            let h = ((if half { 2.4 } else { 5.4 }) * p.blink).max(0.8);
            let x = 4.0 - w / 2.0 + p.look_x * 0.55;
            let y = (if half { 4.8 } else { 4.0 }) - h / 2.0 + p.look_y * 0.5;
            if goth {
                // white ring eye with a black centre (no highlights)
                let (w, h) = (w + 0.8, h);
                let x = x - 0.4;
                if let Some(path) = path_rrect(Rect::new(x, y, w, h), w.min(h) / 2.0) {
                    s.fill_path(&path, &solid(GOTH_LINE));
                }
                let t = 1.0;
                let (iw, ih) = (w - 2.0 * t, h - 2.0 * t);
                if ih > 0.6 {
                    let ix = x + t + p.look_x * 0.2;
                    if let Some(path) = path_rrect(Rect::new(ix, y + t, iw, ih), iw.min(ih) / 2.0) {
                        s.fill_path(&path, &solid(GOTH_PUPIL));
                    }
                }
                return;
            } else if let Some(path) = path_rrect(Rect::new(x, y, w, h), w.min(h) / 2.0) {
                s.fill_path(&path, &solid(INK));
            }
            if h > 3.0 {
                fill_at(s, &pa.eye_hl1, x, y, Color::WHITE);
            }
            if h > 4.0 {
                fill_at(s, &pa.eye_hl2, x, y, Color::WHITE.with_alpha(0.8));
            }
        }
        Eyes::Wide => {
            let (w, h) = (6.6, 6.8 * p.blink);
            let (x, y) = (4.0 - w / 2.0, 4.0 - h / 2.0);
            if let Some(path) = path_rrect(Rect::new(x, y, w, h), w / 2.0) {
                s.fill_path(&path, &solid(Color::WHITE));
            }
            let (pw, ph) = (3.4, 3.6);
            let px = x + (w - pw) / 2.0 + p.look_x * 0.4;
            let py = y + (h - ph) / 2.0 + 0.3;
            if let Some(path) = path_rrect(Rect::new(px, py, pw, ph), 1.7) {
                s.fill_path(&path, &solid(if goth { GOTH_PUPIL } else { INK }));
            }
            fill_at(s, &pa.wide_hl, px, py, Color::WHITE);
        }
        Eyes::Closed => s.fill_path(&pa.eye_closed, &solid(ink)),
        Eyes::Happy => s.fill_path(&pa.eye_happy, &solid(ink)),
        Eyes::Star => {
            s.save();
            // rotation (1 - sparkT) * 72 and scale 1.15 about the item centre
            rotate_about(s, 4.0, 4.0, (1.0 - p.spark_t) * 72.0);
            scale_about(s, 4.0, 4.0, 1.15);
            s.fill_path(&pa.star, &solid(YELLOW));
            s.fill_path(&pa.star_edge, &solid(STAR_EDGE));
            s.restore();
        }
        Eyes::Heart => {
            s.save();
            let sc = 1.1 + (p.hearts_t * std::f32::consts::PI * 4.0).sin() * 0.12;
            scale_about(s, 4.0, 4.0, sc);
            s.fill_path(&pa.heart, &solid(HEART_EYE));
            s.restore();
        }
        // handled by the caller (one eye open, the other happy)
        Eyes::Wink => {}
    }
}

fn paint_particles(pet: &Pet, s: &mut dyn Surface, pa: &Paths, expr: Expr) {
    let p = &pet.p;

    // sweat drop
    if matches!(expr, Expr::Tired | Expr::Sigh) {
        s.save();
        s.mul_opacity(1.0 - p.sweat * 0.7);
        s.translate(31.5, 9.0 + p.sweat * 4.0);
        s.fill_path(&pa.sweat, &solid(theme::BLUE));
        s.restore();
    }

    // hearts
    if p.hearts_t < 1.0 {
        for (x, size, delay) in [(30.0f32, 9.0f32, 0.0f32), (5.0, 7.0, 0.15), (20.0, 6.0, 0.3)] {
            let t = ((p.hearts_t - delay) / (1.0 - delay)).clamp(0.0, 1.0);
            if t <= 0.0 {
                continue;
            }
            let op = if t < 0.55 { 1.0 } else { (1.0 - t) / 0.45 };
            let sc = (0.5 + (t * 5.0).min(1.0) * 0.6) * size / 8.0;
            let col = if delay == 0.15 { Color::rgb8(0xFF, 0x9F, 0xB0) } else { Color::rgb8(0xFF, 0x64, 0x82) };
            s.save();
            s.mul_opacity(op);
            s.translate(x + (t * std::f32::consts::TAU).sin() * 1.6, 12.0 - t * 16.0);
            scale_about(s, 4.0, 4.0, sc);
            s.fill_path(&pa.heart, &solid(col));
            s.restore();
        }
    }

    // sparkles
    if p.spark_t < 1.0 {
        for (x, y, delay) in [(3.0f32, 8.0f32, 0.0f32), (34.0, 14.0, 0.2), (30.0, 2.0, 0.35), (7.0, 28.0, 0.5)] {
            let t = ((p.spark_t - delay) / 0.5).clamp(0.0, 1.0);
            if t <= 0.0 || t >= 1.0 {
                continue;
            }
            s.save();
            s.translate(x - 3.0, y - 3.0);
            scale_about(s, 3.0, 3.0, (t * std::f32::consts::PI).sin());
            s.fill_path(&pa.spark, &solid(YELLOW));
            s.restore();
        }
    }

    // bolt (ready to focus)
    if expr == Expr::Ready {
        s.save();
        s.translate(32.0, 2.0);
        scale_about(s, 4.0, 5.0, p.pop_s);
        s.fill_path(&pa.bolt, &solid(YELLOW));
        s.restore();
    }
}

fn paint_texts(pet: &Pet, s: &mut dyn Surface, rect: Rect, expr: Expr) {
    let p = &pet.p;
    let k = rect.w / 40.0;
    // stage point -> device
    let dx = |x: f32| rect.x + x * k;
    let dy = |y: f32| rect.y + y * k;

    // sleeping z's
    if expr == Expr::Sleep {
        for i in 0..3 {
            let op = p.z[i];
            if op <= 0.002 {
                continue;
            }
            let fi = i as f32;
            let style = TextStyle::new((6.0 + fi * 1.6) * k, Weight::Bold, theme::TEXT2);
            s.save();
            s.mul_opacity(op);
            s.text("z", dx(31.0 + fi * 2.6), dy(9.0 - fi * 4.4), &style);
            s.restore();
        }
    }

    // "!" / "?"
    if matches!(expr, Expr::Surprised | Expr::Curious) && p.pop_s > 0.001 {
        let surprised = expr == Expr::Surprised;
        let col = if surprised { theme::ORANGE } else { theme::accent_text(pet.accent) };
        let style = TextStyle::new(11.0 * k, Weight::Bold, col);
        let txt = if surprised { "!" } else { "?" };
        let (w, h) = (s.measure(txt, &style), s.line_height(&style));
        s.save();
        s.translate(dx(32.0) + w / 2.0, dy(0.0) + h / 2.0);
        if !surprised {
            s.rotate_deg(12.0);
        }
        s.scale(p.pop_s, p.pop_s);
        s.text(txt, -w / 2.0, -h / 2.0, &style);
        s.restore();
    }

    // music notes while humming
    if expr == Expr::Hum {
        for (x, delay, glyph) in [(31.0f32, 0.0f32, "♪"), (35.0, 0.4, "♫")] {
            let t = ((p.notes_t - delay) / 0.6).clamp(0.0, 1.0);
            if t <= 0.0 || t >= 1.0 {
                continue;
            }
            let style = TextStyle::new(8.0 * k, Weight::Regular, theme::accent_text(pet.accent));
            s.save();
            s.mul_opacity(1.0 - t);
            s.text(glyph, dx(x + (t * std::f32::consts::TAU).sin() * 1.5), dy(12.0 - t * 13.0), &style);
            s.restore();
        }
    }
}
