use super::*;
use foco_base::{Mood, PetKind, PixSurface, Rect, Wake};
use std::path::PathBuf;

const MOODS: [Mood; 5] = [Mood::Sleep, Mood::Idle, Mood::Focus, Mood::Tired, Mood::Happy];
/// (reaction, time after `react` at which the contact sheet samples it)
const REACTIONS: [(&str, f64); 5] =
    [("celebrate", 280.0), ("wave", 520.0), ("surprised", 360.0), ("love", 420.0), ("wink", 300.0)];

fn out_dir() -> PathBuf {
    let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/pet-test");
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn advance(p: &mut Pet, from: Ms, to: Ms) {
    let mut t = from;
    while t < to {
        t = (t + 16.0).min(to);
        p.update(t);
    }
}

fn mood_pet(kind: PetKind, mood: Mood, seed: u64, size: f32) -> (Pet, Ms) {
    let mut p = Pet::new(kind, seed);
    p.set_size(size);
    p.update(0.0);
    p.set_mood(mood, 0.0);
    advance(&mut p, 0.0, 700.0);
    (p, 700.0)
}

fn react_pet(kind: PetKind, name: &str, at: f64, seed: u64, size: f32) -> Pet {
    let mut p = Pet::new(kind, seed);
    p.set_size(size);
    p.set_mood(Mood::Idle, 0.0);
    p.update(0.0);
    advance(&mut p, 0.0, 100.0);
    p.react(name, 100.0);
    advance(&mut p, 100.0, 100.0 + at);
    p
}

fn upscale(src: &PixSurface, f: u32) -> PixSurface {
    let (w, h) = (src.width(), src.height());
    let mut out = PixSurface::new(w * f, h * f).unwrap();
    let d = src.data();
    let o = out.pixmap_mut().data_mut();
    for y in 0..h * f {
        for x in 0..w * f {
            let si = (((y / f) * w + x / f) * 4) as usize;
            let di = ((y * w * f + x) * 4) as usize;
            o[di..di + 4].copy_from_slice(&d[si..si + 4]);
        }
    }
    out
}

fn sheet(size: f32, cell: u32, name: &str) -> PixSurface {
    let cols = MOODS.len() + REACTIONS.len();
    let mut surf = PixSurface::new(cell * cols as u32, cell * 3).unwrap();
    surf.clear(Color::rgb8(0xec, 0xec, 0xf0));
    let off = (cell as f32 - size) / 2.0;
    for (r, kind) in PetKind::ALL.into_iter().enumerate() {
        for (c, mood) in MOODS.into_iter().enumerate() {
            let (pet, t) = mood_pet(kind, mood, 7, size);
            pet.paint(&mut surf, Rect::new(c as f32 * cell as f32 + off, r as f32 * cell as f32 + off, size, size), t);
        }
        for (i, (name, at)) in REACTIONS.into_iter().enumerate() {
            let c = MOODS.len() + i;
            let pet = react_pet(kind, name, at, 7, size);
            pet.paint(
                &mut surf,
                Rect::new(c as f32 * cell as f32 + off, r as f32 * cell as f32 + off, size, size),
                0.0,
            );
        }
    }
    surf.save_png(out_dir().join(name)).unwrap();
    surf
}

#[test]
fn contact_sheets() {
    let big = sheet(66.0, 96, "pet-sheet-66.png");
    let small = sheet(26.0, 40, "pet-sheet-26.png");
    upscale(&small, 4).save_png(out_dir().join("pet-sheet-26-x4.png")).unwrap();
    // each cell has ink
    for (s, cell) in [(&big, 96u32), (&small, 40u32)] {
        for r in 0..3 {
            for c in 0..10 {
                let mut any = false;
                for y in 0..cell {
                    for x in 0..cell {
                        any |= s.pixel(c * cell + x, r * cell + y)[3] > 0;
                    }
                }
                assert!(any, "empty cell {r},{c}");
            }
        }
    }
}

#[test]
fn breathing_strip() {
    let size = 66.0;
    let cell = 80u32;
    let frames = 13;
    let mut surf = PixSurface::new(cell * frames, cell).unwrap();
    surf.clear(Color::rgb8(0xec, 0xec, 0xf0));
    let mut pet = Pet::new(PetKind::Cat, 3);
    pet.set_size(size);
    pet.set_mood(Mood::Idle, 0.0);
    let mut t = 0.0;
    let mut heights = Vec::new();
    for f in 0..frames {
        let target = f as f64 * 125.0;
        while t < target {
            t = (t + 16.0).min(target);
            pet.update(t);
        }
        pet.update(target);
        heights.push(pet.p.breathe);
        pet.paint(&mut surf, Rect::new(f as f32 * cell as f32 + 7.0, 7.0, size, size), target);
    }
    surf.save_png(out_dir().join("pet-breathe-strip.png")).unwrap();
    let (mn, mx) = heights.iter().fold((f32::MAX, f32::MIN), |(a, b), &v| (a.min(v), b.max(v)));
    assert!(mx > 0.999 && mn < 0.98, "breathe range {mn}..{mx}");
}

fn run_script(seed: u64) -> Vec<(bool, u64)> {
    let mut p = Pet::new(PetKind::Cat, seed);
    p.set_size(26.0);
    let mut out = Vec::new();
    let mut t = 0.0;
    while t < 40_000.0 {
        t += 16.0;
        match t as u64 {
            2000 => p.set_mood(Mood::Idle, t),
            9000 => p.react("wave", t),
            15_000 => p.set_talking(true, t),
            17_000 => p.set_talking(false, t),
            20_000 => p.set_mood(Mood::Tired, t),
            25_000 => p.react("celebrate", t),
            _ => {}
        }
        let ch = p.update(t);
        let mut h = p.p.breathe.to_bits() as u64;
        for v in [p.p.tail_a, p.p.jump, p.p.blink, p.p.look_x, p.p.hand_ry, p.p.squash, p.p.tilt] {
            h = h.wrapping_mul(1_000_003).wrapping_add(v.to_bits() as u64);
        }
        out.push((ch, h));
    }
    out
}

#[test]
fn deterministic_for_a_seed() {
    let a = run_script(42);
    let b = run_script(42);
    assert_eq!(a, b);
    let c = run_script(43);
    assert_ne!(a, c, "different seeds should differ (blink/fidget timing)");
    // paint is deterministic too
    let mut s1 = PixSurface::new(80, 80).unwrap();
    let mut s2 = PixSurface::new(80, 80).unwrap();
    let p1 = react_pet(PetKind::Slime, "love", 400.0, 5, 66.0);
    let p2 = react_pet(PetKind::Slime, "love", 400.0, 5, 66.0);
    p1.paint(&mut s1, Rect::new(7.0, 7.0, 66.0, 66.0), 0.0);
    p2.paint(&mut s2, Rect::new(7.0, 7.0, 66.0, 66.0), 0.0);
    assert_eq!(s1.data(), s2.data());
}

#[test]
fn inactive_is_idle_and_inert() {
    let mut p = Pet::new(PetKind::Cat, 1);
    p.update(0.0);
    p.set_active(false, 10.0);
    assert!(p.update(20.0), "one final update after the change");
    assert_eq!(p.next_wake(20.0), Wake::Idle);
    p.react("celebrate", 30.0);
    assert!(!p.reacting(), "react ignored while inactive");
    assert!(!p.update(40.0));
    assert!(!p.update(100_000.0));
    assert_eq!(p.next_wake(100_000.0), Wake::Idle);
    p.set_active(true, 200_000.0);
    assert!(p.update(200_000.0));
    assert_ne!(p.next_wake(200_000.0), Wake::Idle);
}

#[test]
fn reaction_runs_at_frame_rate_then_settles() {
    let mut p = Pet::new(PetKind::Cat, 9);
    p.set_size(26.0);
    p.set_mood(Mood::Idle, 0.0);
    p.update(0.0);
    advance(&mut p, 0.0, 200.0);
    p.react("celebrate", 200.0);
    assert_eq!(p.next_wake(200.0), Wake::At(200.0 + 1000.0 / 30.0), "react starts a 30 Hz tween");
    p.update(216.0);
    // jump/hearts/sparkles are moving: its own tweens tick at 30 Hz, not at the display rate
    assert_eq!(p.next_wake(216.0), Wake::At(216.0 + 1000.0 / 30.0), "tweens run at 30 Hz");
    // after the animations end but before the reaction timer ends
    advance(&mut p, 216.0, 2200.0);
    assert!(p.reacting());
    match p.next_wake(2200.0) {
        Wake::At(t) => assert!(t > 2200.0 && t <= 3200.0 + 1.0, "wake inside reaction: {t}"),
        w => assert_eq!(w, Wake::Frame, "unexpected {w:?}"),
    }
    advance(&mut p, 2200.0, 3300.0);
    assert!(!p.reacting());
    advance(&mut p, 3300.0, 3900.0);
    assert!(p.p.jump.abs() < 1e-6 && p.p.squash.abs() < 1e-6);
}

fn simulate(kind: PetKind, mood: Mood, secs: f64) -> (u32, u32, u32, u32) {
    let mut p = Pet::new(kind, 11);
    p.set_size(26.0);
    p.set_mood(mood, 0.0);
    let mut t = 0.0;
    assert!(p.update(t));
    let (mut wakes, mut repaints, mut at_wakes, mut at_hits) = (0u32, 0u32, 0u32, 0u32);
    while t < secs * 1000.0 {
        let w = p.next_wake(t);
        let next = match w {
            Wake::Idle => panic!("active pet must not be idle"),
            Wake::Frame => t + 16.7,
            Wake::Ambient => t + 33.3,
            Wake::At(x) => {
                assert!(x > t, "At in the past: {x} <= {t}");
                x
            }
        };
        t = next;
        let ch = p.update(t);
        wakes += 1;
        repaints += ch as u32;
        if matches!(w, Wake::At(_)) {
            at_wakes += 1;
            at_hits += ch as u32;
        }
    }
    (wakes, repaints, at_wakes, at_hits)
}

#[test]
fn breathing_gives_precise_at_wakes_and_update_agrees() {
    // wakes per 20 s at 26 px; a fixed 30 Hz ambient tick would be 600
    for (kind, mood, max_wakes) in [
        (PetKind::Cat, Mood::Sleep, 450),
        (PetKind::Cat, Mood::Focus, 450),
        (PetKind::Ghost, Mood::Idle, 450),
        (PetKind::Slime, Mood::Tired, 400),
    ] {
        let (wakes, repaints, at_wakes, at_hits) = simulate(kind, mood, 20.0);
        eprintln!("{kind:?} {mood:?}, 20 s @26px: {wakes} wakes, {repaints} repaints ({at_hits}/{at_wakes} At wakes repainted)");
        assert!(wakes < max_wakes, "{kind:?} {mood:?}: {wakes}");
        assert!(repaints * 100 >= wakes * 50, "{repaints}/{wakes}");
        assert!(at_hits * 100 >= at_wakes * 60, "{at_hits}/{at_wakes}");
    }
}

#[test]
fn update_is_quiet_when_nothing_changes() {
    let mut p = Pet::new(PetKind::Ghost, 2);
    p.set_size(26.0);
    p.update(0.0);
    advance(&mut p, 0.0, 1000.0);
    // same instant twice: nothing new
    p.update(1000.0);
    assert!(!p.update(1000.0));
    assert!(!p.update(1000.5));
}

#[test]
fn blink_fidget_deadlines_are_reported() {
    let mut p = Pet::new(PetKind::Cat, 4);
    p.set_size(26.0);
    p.set_mood(Mood::Focus, 0.0);
    p.update(0.0);
    // first blink at 3200
    let mut seen = false;
    let mut t = 0.0;
    while t < 3300.0 {
        t = match p.next_wake(t) {
            Wake::At(x) => x,
            _ => t + 16.7,
        };
        p.update(t);
        if p.p.blink < 0.99 {
            seen = true;
            assert!(t >= 3200.0, "blink early at {t}");
            break;
        }
    }
    assert!(seen, "blink must fire at 3200 ms");
    // closed eyes (sleep) have no blink timer
    let mut s = Pet::new(PetKind::Cat, 4);
    s.set_mood(Mood::Sleep, 0.0);
    s.update(0.0);
    assert!(s.blink_at.is_none());
    assert!(s.fidget_at.is_some());
    // talking suspends the fidget, hover suspends it too
    s.set_talking(true, 10.0);
    assert!(s.fidget_at.is_none());
    s.set_talking(false, 20.0);
    assert!(s.fidget_at.is_some());
    s.set_hover(true, 30.0);
    assert_eq!(s.expr(), Expr::Content);
    assert!(s.fidget_at.is_none());
}

#[test]
fn expressions_follow_qml_priority() {
    let mut p = Pet::new(PetKind::Slime, 1);
    p.set_mood(Mood::Tired, 0.0);
    assert_eq!(p.expr(), Expr::Tired);
    p.set_hover(true, 1.0);
    assert_eq!(p.expr(), Expr::Content);
    p.react("love", 2.0);
    assert_eq!(p.expr(), Expr::Love);
    p.set_talking(true, 3.0);
    assert_eq!(p.expr(), Expr::Talk);
    p.set_talking(false, 4.0);
    p.update(2000.0);
    assert!(!p.reacting());
    assert_eq!(p.expr(), Expr::Content);
    assert_eq!(Expr::Focus.mouth(PetKind::Cat), expr::Mouth::Cat);
    assert_eq!(Expr::Focus.mouth(PetKind::Ghost), expr::Mouth::Smile);
    assert_eq!(Expr::Celebrate.eyes(), expr::Eyes::Star);
}

#[test]
fn paint_stays_inside_overdraw() {
    let size = 40.0f32;
    let pad = 30u32;
    let mut states: Vec<(PetKind, Option<&str>, f64)> = Vec::new();
    for kind in PetKind::ALL {
        for (name, _) in REACTIONS {
            for t in [100.0, 300.0, 600.0, 1000.0] {
                states.push((kind, Some(name), t));
            }
        }
        for name in ["ready", "curious", "hum", "yawn", "sigh"] {
            for t in [150.0, 500.0, 1200.0] {
                states.push((kind, Some(name), t));
            }
        }
    }
    for (kind, name, t) in states {
        let mut p = Pet::new(kind, 1);
        p.set_size(size);
        p.set_mood(Mood::Idle, 0.0);
        p.update(0.0);
        advance(&mut p, 0.0, 100.0);
        p.react(name.unwrap(), 100.0);
        advance(&mut p, 100.0, 100.0 + t);
        let mut s = PixSurface::new(40 + 2 * pad, 40 + 2 * pad).unwrap();
        p.paint(&mut s, Rect::new(pad as f32, pad as f32, size, size), 0.0);
        let m = (p.overdraw() * size).ceil() as u32;
        for y in 0..s.height() {
            for x in 0..s.width() {
                let inside = x + m >= pad && x < pad + 40 + m && y + m >= pad && y < pad + 40 + m;
                if !inside {
                    assert_eq!(s.pixel(x, y)[3], 0, "{kind:?} {name:?} t={t}: ink at ({x},{y}) beyond overdraw {m}");
                }
            }
        }
    }
}

#[test]
fn slime_uses_accent() {
    let mut a = Pet::new(PetKind::Slime, 1);
    let mut b = Pet::new(PetKind::Slime, 1);
    b.set_accent(Color::rgb8(0x30, 0xd1, 0x58));
    a.update(0.0);
    b.update(0.0);
    let mut sa = PixSurface::new(40, 40).unwrap();
    let mut sb = PixSurface::new(40, 40).unwrap();
    a.paint(&mut sa, Rect::new(0.0, 0.0, 40.0, 40.0), 0.0);
    b.paint(&mut sb, Rect::new(0.0, 0.0, 40.0, 40.0), 0.0);
    assert_ne!(sa.pixel(20, 30), sb.pixel(20, 30));
    // cat ignores the accent
    let mut c = Pet::new(PetKind::Cat, 1);
    let before = c.body;
    c.set_accent(Color::rgb8(1, 2, 3));
    assert_eq!(before, c.body);
    assert!(c.update(0.0));
}

#[test]
#[ignore = "perf: run with --release -- --ignored --nocapture"]
fn perf_paint() {
    use std::hint::black_box;
    use std::time::Instant;
    for size in [26.0f32, 66.0] {
        let mut surf = PixSurface::new(100, 100).unwrap();
        let states: [(&str, Pet); 3] = [
            ("cat idle", mood_pet(PetKind::Cat, Mood::Idle, 1, size).0),
            ("slime focus", mood_pet(PetKind::Slime, Mood::Focus, 1, size).0),
            ("cat celebrate", react_pet(PetKind::Cat, "celebrate", 280.0, 1, size)),
        ];
        for (name, pet) in &states {
            let rect = Rect::new(10.0, 10.0, size, size);
            for _ in 0..200 {
                pet.paint(&mut surf, rect, 0.0);
            }
            let n = 20_000;
            let t0 = Instant::now();
            for _ in 0..n {
                surf.clear(Color::rgb8(0xec, 0xec, 0xf0));
                pet.paint(black_box(&mut surf), rect, 0.0);
            }
            let us = t0.elapsed().as_secs_f64() * 1e6 / n as f64;
            eprintln!("paint {name:14} size {size:>4}: {us:7.2} us (incl. clear of 100x100)");
        }
        let mut pet = Pet::new(PetKind::Cat, 1);
        pet.set_size(size);
        pet.set_mood(Mood::Idle, 0.0);
        let n = 100_000;
        let t0 = Instant::now();
        let mut t = 0.0;
        let mut ch = 0u32;
        for _ in 0..n {
            t += 33.0;
            ch += pet.update(t) as u32;
        }
        eprintln!(
            "update size {size:>4}: {:6.3} us/call ({ch} of {n} repainted at 30 Hz steps)",
            t0.elapsed().as_secs_f64() * 1e6 / n as f64
        );
        let t0 = Instant::now();
        let mut acc = 0.0;
        for i in 0..n {
            if let Wake::At(x) = pet.next_wake(t + i as f64 * 0.01) {
                acc += x;
            }
        }
        black_box(acc);
        eprintln!("next_wake size {size:>4}: {:6.3} us/call", t0.elapsed().as_secs_f64() * 1e6 / n as f64);
    }
}
