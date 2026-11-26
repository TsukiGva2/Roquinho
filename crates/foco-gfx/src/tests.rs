//! Visual and numeric tests; PNGs go to `rs/target/gfx-test/`.

use foco_base::surface::{Fill, Surface, TextStyle, Weight};
use foco_base::{theme, Color, IRect, Point, Rect};

use crate::{icons, Canvas, Renderer};

pub const S: f32 = 25.0 / 24.0;

pub fn out_dir() -> std::path::PathBuf {
    let d = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/gfx-test");
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Saves a BGRA-premultiplied buffer as an opaque PNG over `bg` (no alpha, easy to eyeball).
pub fn save_png(name: &str, buf: &[u8], w: u32, h: u32, bg: Option<[u8; 3]>) {
    let mut rgba = vec![0u8; buf.len()];
    for (s, d) in buf.chunks_exact(4).zip(rgba.chunks_exact_mut(4)) {
        if let Some(b) = bg {
            let inv = 255 - s[3] as u32;
            d[0] = (s[2] as u32 + b[0] as u32 * inv / 255).min(255) as u8;
            d[1] = (s[1] as u32 + b[1] as u32 * inv / 255).min(255) as u8;
            d[2] = (s[0] as u32 + b[2] as u32 * inv / 255).min(255) as u8;
            d[3] = 255;
        } else {
            // un-premultiply for a transparent PNG
            let a = s[3] as u32;
            let un = |v: u8| ((v as u32 * 255 + a / 2).checked_div(a).unwrap_or(0)).min(255) as u8;
            d[0] = un(s[2]);
            d[1] = un(s[1]);
            d[2] = un(s[0]);
            d[3] = s[3];
        }
    }
    let pm = tiny_skia::Pixmap::from_vec(rgba, tiny_skia::IntSize::from_wh(w, h).unwrap()).unwrap();
    pm.save_png(out_dir().join(name)).unwrap();
}

fn full(w: u32, h: u32, r: &mut Renderer, bg: [u8; 3], f: &mut dyn FnMut(&mut Canvas)) -> Vec<u8> {
    let mut buf = vec![0u8; (w * h * 4) as usize];
    r.render(&mut buf, w, h, &[IRect::new(0, 0, w as i32, h as i32)], &mut |c| {
        c.fill_rrect(Rect::new(0.0, 0.0, w as f32, h as f32), 0.0, &Fill::Solid(Color::rgb8(bg[0], bg[1], bg[2])));
        f(c);
    });
    buf
}

#[test]
fn metrics_match_qt() {
    // Ground truth from Qt 6 (Text.implicitWidth/Height, pixelSize N, default font = Noto Sans via qt6ct).
    let mut r = Renderer::new();
    let mut buf = vec![0u8; 4 * 4 * 4];
    let hello = [
        (10.0, 24.234_375, 14.0),
        (11.0, 26.640_625, 15.0),
        (12.0, 29.09375, 17.0),
        (13.0, 31.5, 18.0),
        (14.0, 33.9375, 20.0),
        (15.0, 36.34375, 21.0),
        (23.0, 55.75, 32.0),
    ];
    r.render(&mut buf, 4, 4, &[IRect::new(0, 0, 4, 4)], &mut |c| {
        for (px, w, h) in hello {
            let st = TextStyle::new(px, Weight::Regular, Color::WHITE);
            let mw = c.measure("Hello", &st);
            let mh = c.line_height(&st);
            println!("px {px}: width {mw} (qt {w}) height {mh} (qt {h})");
            assert!((mw - w).abs() < 0.2, "hello width at {px}");
            assert_eq!(mh, h);
        }
        let st = TextStyle::new(13.0, Weight::Regular, Color::WHITE);
        let w1 = c.measure("Ação AVATAR To Wa", &st);
        let w2 = c.measure("00:12:34", &st);
        let w3 = c.measure("Ao", &st.clone().with_spacing(2.0));
        println!("kern: {w1} (qt 118.203125)  digits: {w2} (qt 52.46875)  ls2: {w3} (qt 20.15625)");
        assert!((w1 - 118.203_125).abs() < 0.5);
        // Qt's hinted '4' is 1 px wider than its design advance; everything else matches.
        assert!((w2 - 52.46875).abs() < 1.1);
        assert!((w3 - 20.15625).abs() < 0.1);
    });
}

fn text_sheet(style: crate::FontStyle, name: &str) {
    let mut r = Renderer::with_font_style(style);
    let (w, h) = (640u32, 700u32);
    let buf = full(w, h, &mut r, [0, 0, 0], &mut |c| {
        c.scale(S, S);
        let mut y = 4.0;
        for (wi, wname) in
            [(Weight::Regular, "400"), (Weight::Medium, "500"), (Weight::DemiBold, "600"), (Weight::Bold, "700")]
        {
            for px in [11.0, 12.0, 13.0, 14.0, 15.0, 23.0] {
                let st = TextStyle::new(px, wi, theme::TEXT);
                let adv = c.text(&format!("{wname} {px}px  Foquinho Ação áãçêõ AVATAR To Wa"), 6.0, y, &st);
                let t2 = TextStyle::new(px, wi, theme::TEXT2).tabular();
                c.text("00:12:34", 6.0 + adv + 8.0, y, &t2);
                y += c.line_height(&st) + 1.0;
            }
            y += 3.0;
        }
        // strike, elide, letter spacing, opacity, colours
        let st = TextStyle::new(13.0, Weight::Medium, theme::TEXT).strike();
        c.text("Tarefa concluída riscada", 6.0, y, &st);
        let st2 = TextStyle::new(13.0, Weight::Medium, theme::TEXT);
        let e = c.elide("Uma demanda bem longa que precisa ser elidida no fim", &st2, 150.0);
        c.text(&e, 200.0, y, &st2);
        let wv = c.measure(&e, &st2);
        assert!(wv <= 150.0, "elided width {wv}");
        y += 20.0;
        let sp = TextStyle::new(10.0, Weight::DemiBold, theme::TEXT3).with_spacing(1.2);
        c.text("EM FOCO  A SEGUIR  CONCLUÍDAS HOJE", 6.0, y, &sp);
        c.save();
        c.mul_opacity(0.4);
        c.text("opacity 0.4", 250.0, y, &st2);
        c.restore();
        y += 18.0;
        c.text("Accent", 6.0, y, &TextStyle::new(13.0, Weight::Bold, theme::accent_text(theme::DEFAULT_ACCENT)));
        c.text("Red", 70.0, y, &TextStyle::new(13.0, Weight::Bold, theme::RED));
        c.text("Green", 105.0, y, &TextStyle::new(13.0, Weight::Bold, theme::GREEN));
        c.text("Blue", 150.0, y, &TextStyle::new(13.0, Weight::Bold, theme::BLUE));
        c.text("symbols: … × · • ✓ → ★", 200.0, y, &st2);
        y += 18.0;
        c.save();
        c.translate(40.0, y + 30.0);
        c.rotate_deg(-12.0);
        c.text("rotated (path fallback)", 0.0, 0.0, &st2);
        c.restore();
    });
    save_png(name, &buf, w, h, None);
}

#[test]
fn text_sheets() {
    text_sheet(crate::FontStyle::QtDefault, "text.png");
    text_sheet(crate::FontStyle::Inter, "text_inter.png");
}

#[test]
fn text_tabular_inter() {
    let mut r = Renderer::with_font_style(crate::FontStyle::Inter);
    let mut buf = vec![0u8; 16];
    r.render(&mut buf, 2, 2, &[IRect::new(0, 0, 2, 2)], &mut |c| {
        let st = TextStyle::new(13.0, Weight::Regular, Color::WHITE);
        let t = st.clone().tabular();
        // tabular digits all have the same width in Inter; proportional ones do not
        assert_eq!(c.measure("1111", &t), c.measure("0000", &t));
        assert_ne!(c.measure("1111", &st), c.measure("0000", &st));
        assert_eq!(c.line_height(&st), 16.0);
    });
}

#[test]
fn icon_sheet() {
    let mut r = Renderer::new();
    let (w, h) = (700u32, 260u32);
    let buf = full(w, h, &mut r, [0x1c, 0x1c, 0x1e], &mut |c| {
        c.scale(S, S);
        for (i, n) in icons::names().iter().enumerate() {
            let (col, row) = ((i % 12) as f32, (i / 12) as f32);
            let x = 8.0 + col * 50.0;
            icons::draw(c, n, Rect::new(x, 8.0 + row * 100.0, 32.0, 32.0), theme::TEXT, 1.8);
            icons::draw(
                c,
                n,
                Rect::new(x + 4.0, 48.0 + row * 100.0, 16.0, 16.0),
                theme::accent_text(theme::DEFAULT_ACCENT),
                1.8,
            );
            icons::draw(c, n, Rect::new(x + 26.0, 52.0 + row * 100.0, 11.0, 11.0), theme::TEXT2, 1.6);
            let st = TextStyle::new(8.0, Weight::Regular, theme::TEXT3);
            c.text(n, x, 70.0 + row * 100.0, &st);
        }
    });
    save_png("icons.png", &buf, w, h, None);
    // every icon paints something
    for n in icons::names() {
        let mut b = vec![0u8; 40 * 40 * 4];
        r.render(&mut b, 40, 40, &[IRect::new(0, 0, 40, 40)], &mut |c| {
            icons::draw(c, n, Rect::new(4.0, 4.0, 32.0, 32.0), Color::WHITE, 1.8);
        });
        assert!(b.chunks_exact(4).any(|p| p[3] > 128), "icon {n} empty");
    }
}

/// Alpha of Qt's `RectangularShadow` (blur 34, spread -4, radius 33, offset.y 14, opacity .65) over
/// 0x80 gray, sampled from a real Qt 6 render: shadow item at (100,60) 300x200.
const QT_V: [f32; 21] = [
    0.0, 0.012, 0.012, 0.036, 0.06, 0.096, 0.156, 0.216, 0.3, 0.397, 0.505, 0.613, 0.721, 0.817, 0.901, 0.962, 0.998,
    0.998, 0.998, 0.998, 0.998,
];
const QT_H: [f32; 21] = [
    0.0, 0.0, 0.0, 0.012, 0.024, 0.048, 0.072, 0.12, 0.18, 0.264, 0.349, 0.457, 0.565, 0.673, 0.769, 0.865, 0.938,
    0.986, 0.998, 0.998, 0.998,
];

#[test]
fn shadow_matches_qt() {
    let mut r = Renderer::new();
    let (w, h) = (500u32, 400u32);
    let buf = full(w, h, &mut r, [0x80, 0x80, 0x80], &mut |c| {
        c.shadow(
            Rect::new(100.0, 60.0, 300.0, 200.0),
            33.0,
            34.0,
            -4.0,
            Point::new(0.0, 14.0),
            Color::BLACK.with_alpha(0.65),
        );
    });
    save_png("shadow_qt_ref.png", &buf, w, h, None);
    let alpha = |x: usize, y: usize| (128.0 - buf[(y * w as usize + x) * 4] as f32) / 128.0 / 0.65;
    let mut err = 0.0;
    let mut n = 0.0;
    for (i, q) in QT_V.iter().enumerate() {
        err += (alpha(250, 40 + i * 4) - q).abs();
        n += 1.0;
    }
    for (i, q) in QT_H.iter().enumerate() {
        err += (alpha(60 + i * 4, 160) - q).abs();
        n += 1.0;
    }
    println!("mean abs error vs Qt: {}", err / n);
    assert!(err / n < 0.03, "shadow differs from Qt by {}", err / n);
}

#[test]
fn shadow_sample() {
    let mut r = Renderer::new();
    let (w, h) = (792u32, 792u32);
    let buf = full(w, h, &mut r, [0x80, 0x80, 0x80], &mut |c| {
        c.scale(S, S);
        // Main.qml: x = ear, width = curW, blur 34, spread -4, offset.y 14
        let nr = Rect::new(80.0, 0.0, 560.0, 640.0);
        c.shadow(nr, 33.0, 34.0, -4.0, Point::new(0.0, 14.0), Color::BLACK.with_alpha(0.65));
        c.fill_rrect(nr, 33.0, &Fill::Solid(theme::NOTCH));
        // small alert-like shadow (unsaturated tile)
        let ar = Rect::new(250.0, 660.0, 260.0, 40.0);
        c.shadow(ar, 20.0, 34.0, -4.0, Point::new(0.0, 14.0), Color::BLACK.with_alpha(0.45));
        c.fill_rrect(ar, 20.0, &Fill::Solid(theme::NOTCH));
        // TipBubble: radius 18, blur 26, spread -2, offset 8, #b0000000
        let tr = Rect::new(20.0, 660.0, 180.0, 60.0);
        c.shadow(tr, 18.0, 26.0, -2.0, Point::new(0.0, 8.0), Color::hex("#b0000000").unwrap());
        c.fill_rrect(tr, 18.0, &Fill::Solid(theme::CARD));
    });
    save_png("shadow.png", &buf, w, h, None);
}

#[test]
fn clip_opacity_sample() {
    let mut r = Renderer::new();
    let (w, h) = (360u32, 260u32);
    let buf = full(w, h, &mut r, [0x20, 0x20, 0x28], &mut |c| {
        c.scale(S, S);
        // fractional clip (snapped to whole device px like a Qt scissor)
        c.save();
        c.clip_rect(Rect::new(10.3, 10.3, 100.0, 40.0));
        c.fill_rrect(Rect::new(0.0, 0.0, 200.0, 200.0), 20.0, &Fill::Solid(theme::DEFAULT_ACCENT));
        c.restore();
        // nested clips
        c.save();
        c.clip_rect(Rect::new(130.0, 10.0, 80.0, 60.0));
        c.clip_rect(Rect::new(150.0, 30.0, 80.0, 60.0));
        c.fill_rrect(Rect::new(100.0, 0.0, 200.0, 200.0), 0.0, &Fill::Solid(theme::GREEN));
        c.restore();
        // opacity: overlapping primitives show through each other (no group layer)
        c.save();
        c.mul_opacity(0.5);
        c.fill_rrect(Rect::new(10.0, 80.0, 60.0, 40.0), 10.0, &Fill::Solid(theme::RED));
        c.fill_rrect(Rect::new(40.0, 95.0, 60.0, 40.0), 10.0, &Fill::Solid(theme::BLUE));
        c.text("half", 14.0, 84.0, &TextStyle::new(13.0, Weight::Bold, Color::WHITE));
        c.restore();
        // rotated clip -> mask
        c.save();
        c.translate(200.0, 130.0);
        c.rotate_deg(20.0);
        c.clip_rect(Rect::new(0.0, 0.0, 70.0, 30.0));
        c.fill_rrect(
            Rect::new(-20.0, -20.0, 200.0, 100.0),
            0.0,
            &Fill::Linear {
                p0: Point::new(0.0, 0.0),
                p1: Point::new(70.0, 0.0),
                stops: vec![(0.0, theme::ORANGE), (1.0, theme::BLUE)],
            },
        );
        c.text("clipped text under rotation", 2.0, 5.0, &TextStyle::new(11.0, Weight::Medium, Color::BLACK));
        c.restore();
        // text clipped by a box edge
        c.save();
        c.clip_rect(Rect::new(10.0, 150.0, 60.5, 20.0));
        c.text("Clipped text overflow", 10.0, 152.0, &TextStyle::new(13.0, Weight::Medium, theme::TEXT));
        c.restore();
        // rings / strokes
        c.stroke_rrect(Rect::new(100.0, 150.0, 60.0, 40.0), 12.0, &Fill::Solid(theme::GREEN), 2.0);
        // radial gradient dot
        c.fill_rrect(
            Rect::new(180.0, 160.0, 30.0, 30.0),
            15.0,
            &Fill::Radial {
                center: Point::new(195.0, 175.0),
                radius: 15.0,
                stops: vec![(0.0, Color::WHITE), (1.0, theme::DEFAULT_ACCENT)],
            },
        );
    });
    save_png("clip_opacity.png", &buf, w, h, None);
    let px = |x: usize, y: usize| {
        [
            buf[(y * w as usize + x) * 4],
            buf[(y * w as usize + x) * 4 + 1],
            buf[(y * w as usize + x) * 4 + 2],
            buf[(y * w as usize + x) * 4 + 3],
        ]
    };
    // inside the green double clip, outside it
    assert_eq!(px(190, 60)[3], 255);
    assert_eq!(px(190, 60), [0x3d, 0x8a, 0x24, 255]);
    assert_eq!(px(150, 25), [0x28, 0x20, 0x20, 255]); // outside second clip: still bg (b,g,r order)
}

#[test]
fn damage_rects_match_full_repaint() {
    // painting only a damaged rect gives exactly the same pixels as a full repaint
    let paint = |c: &mut Canvas| {
        c.scale(S, S);
        c.shadow(
            Rect::new(40.0, 20.0, 200.0, 120.0),
            20.0,
            34.0,
            -4.0,
            Point::new(0.0, 14.0),
            Color::BLACK.with_alpha(0.65),
        );
        c.fill_rrect(Rect::new(40.0, 20.0, 200.0, 120.0), 20.0, &Fill::Solid(theme::CARD));
        c.save();
        c.clip_rect(Rect::new(50.0, 30.0, 150.0, 40.0));
        c.text("Foquinho damage test AVATAR 00:12", 55.0, 36.0, &TextStyle::new(13.0, Weight::Medium, theme::TEXT));
        c.restore();
        icons::draw(c, "check", Rect::new(60.0, 90.0, 16.0, 16.0), theme::GREEN, 1.8);
    };
    let mut r = Renderer::new();
    let (w, h) = (300u32, 200u32);
    let mut full_buf = vec![0u8; (w * h * 4) as usize];
    r.render(&mut full_buf, w, h, &[IRect::new(0, 0, w as i32, h as i32)], &mut |c| paint(c));
    for rect in [
        IRect::new(60, 40, 90, 50),
        IRect::new(0, 30, 300, 20),
        IRect::new(70, 100, 28, 28),
        IRect::new(-5, -5, 20, 20),
    ] {
        let mut part = full_buf.clone();
        // poison the rect, then repaint just that area
        let rr = rect.intersect(&IRect::new(0, 0, w as i32, h as i32)).unwrap();
        for y in rr.y..rr.bottom() {
            for x in rr.x..rr.right() {
                let i = ((y as u32 * w + x as u32) * 4) as usize;
                part[i..i + 4].copy_from_slice(&[9, 9, 9, 9]);
            }
        }
        r.render(&mut part, w, h, &[rect], &mut |c| paint(c));
        // tiny-skia flattens curves clipped to the pixmap, so anti-aliased path edges may differ
        // by a few levels between a band and a full repaint; everything else is bit-identical.
        let worst = part.iter().zip(&full_buf).map(|(a, b)| a.abs_diff(*b)).max().unwrap();
        let bad = part.iter().zip(&full_buf).filter(|(a, b)| a != b).count();
        assert!(worst <= 40 && bad < 400, "damage rect {rect:?}: {bad} bytes differ, worst {worst}");
    }
}

fn mock_panel(c: &mut Canvas, icon_names: &[&str]) {
    c.scale(S, S);
    let panel = Rect::new(10.0, 0.0, 530.0, 700.0);
    c.shadow(panel, 33.0, 34.0, -4.0, Point::new(0.0, 14.0), Color::BLACK.with_alpha(0.65));
    c.fill_rrect(panel, 33.0, &Fill::Solid(theme::NOTCH));
    let weights = [Weight::Regular, Weight::Medium, Weight::Bold];
    for i in 0..10 {
        let st = TextStyle::new(13.0, weights[i % 3], theme::TEXT);
        c.text(&format!("Linha {i} Foquinho demanda de teste Ação {i:02}:12"), 30.0, 20.0 + i as f32 * 22.0, &st);
    }
    for i in 0..6 {
        let r = Rect::new(24.0, 260.0 + i as f32 * 62.0, 502.0, 54.0);
        c.fill_rrect(r, 16.0, &Fill::Solid(theme::CARD));
        c.stroke_rrect(r, 16.0, &Fill::Solid(theme::STROKE), 1.0);
        c.text("Cartão da fila", r.x + 16.0, r.y + 9.0, &TextStyle::new(13.0, Weight::Medium, theme::TEXT));
        c.text("00:25:00", r.x + 16.0, r.y + 28.0, &TextStyle::new(11.0, Weight::Regular, theme::TEXT2).tabular());
    }
    for (i, n) in icon_names.iter().enumerate() {
        icons::draw(c, n, Rect::new(30.0 + i as f32 * 28.0, 640.0, 18.0, 18.0), theme::TEXT, 1.8);
    }
}

type Part = (&'static str, fn(&mut Canvas));

#[test]
#[ignore = "perf: run with --release -- --ignored --nocapture"]
fn bench() {
    use std::time::Instant;
    let mut r = Renderer::new();
    let (w, h) = (792u32, 792u32);
    let mut buf = vec![0u8; (w * h * 4) as usize];
    let icon_names = ["plus", "check", "x", "play", "pause", "chevronUp", "trash", "sliders"];
    // the 571x750 expanded panel region (damage rect not at x=0, goes through the scratch copy)
    let panel = IRect::new(110, 0, 571, 750);
    {
        // cold start: font mapping, glyph rasterisation, shadow tile
        let mut cold = Renderer::new();
        let t = Instant::now();
        cold.render(&mut buf, w, h, &[panel], &mut |c| mock_panel(c, &icon_names));
        println!(
            "cold first frame (fonts + {} glyph rasterisations + shadow tile): {:.0} us",
            cold.glyph_misses(),
            t.elapsed().as_secs_f64() * 1e6
        );
        let t = Instant::now();
        cold.render(&mut buf, w, h, &[panel], &mut |c| mock_panel(c, &icon_names));
        println!("second frame: {:.0} us", t.elapsed().as_secs_f64() * 1e6);
    }
    for _ in 0..20 {
        r.render(&mut buf, w, h, &[panel], &mut |c| mock_panel(c, &icon_names));
    }
    let n = 400;
    let t = Instant::now();
    for _ in 0..n {
        r.render(&mut buf, w, h, &[panel], &mut |c| mock_panel(c, &icon_names));
    }
    let us = t.elapsed().as_secs_f64() * 1e6 / n as f64;
    println!("full repaint 571x750 expanded panel: {us:.0} us/frame (glyph misses {})", r.glyph_misses());
    save_png("bench_panel.png", &buf, w, h, None);

    // without the shadow (what a frame costs when only content repaints)
    let t = Instant::now();
    for _ in 0..n {
        r.render(&mut buf, w, h, &[panel], &mut |c| {
            c.scale(S, S);
            c.fill_rrect(Rect::new(10.0, 0.0, 530.0, 700.0), 33.0, &Fill::Solid(theme::NOTCH));
        });
    }
    println!("  of which: black rrect only: {:.0} us", t.elapsed().as_secs_f64() * 1e6 / n as f64);

    // breakdown
    let parts: [Part; 5] = [
        ("empty (clear + copy only)", |_c| {}),
        ("shadow", |c| {
            c.scale(S, S);
            c.shadow(
                Rect::new(10.0, 0.0, 530.0, 700.0),
                33.0,
                34.0,
                -4.0,
                Point::new(0.0, 14.0),
                Color::BLACK.with_alpha(0.65),
            );
        }),
        ("10 text lines", |c| {
            c.scale(S, S);
            let weights = [Weight::Regular, Weight::Medium, Weight::Bold];
            for i in 0..10 {
                let st = TextStyle::new(13.0, weights[i % 3], theme::TEXT);
                c.text(
                    &format!("Linha {i} Foquinho demanda de teste Ação {i:02}:12"),
                    30.0,
                    20.0 + i as f32 * 22.0,
                    &st,
                );
            }
        }),
        ("6 cards (fill+stroke)", |c| {
            c.scale(S, S);
            for i in 0..6 {
                let r = Rect::new(24.0, 260.0 + i as f32 * 62.0, 502.0, 54.0);
                c.fill_rrect(r, 16.0, &Fill::Solid(theme::CARD));
                c.stroke_rrect(r, 16.0, &Fill::Solid(theme::STROKE), 1.0);
            }
        }),
        ("8 icons", |c| {
            c.scale(S, S);
            for (i, n) in ["plus", "check", "x", "play", "pause", "chevronUp", "trash", "sliders"]
                .iter()
                .enumerate()
            {
                icons::draw(c, n, Rect::new(30.0 + i as f32 * 28.0, 640.0, 18.0, 18.0), theme::TEXT, 1.8);
            }
        }),
    ];
    for (name, f) in parts {
        let t = Instant::now();
        for _ in 0..n {
            r.render(&mut buf, w, h, &[panel], &mut |c| f(c));
        }
        println!("  part {name}: {:.0} us", t.elapsed().as_secs_f64() * 1e6 / n as f64);
    }

    let small = IRect::new(300, 300, 28, 28);
    let t = Instant::now();
    let m = 20000;
    for _ in 0..m {
        r.render(&mut buf, w, h, &[small], &mut |c| mock_panel(c, &icon_names));
    }
    println!("28x28 damage repaint (whole scene issued): {:.1} us", t.elapsed().as_secs_f64() * 1e6 / m as f64);
}

/// Renders the strings of the Qt reference grab (`g.qml`) at DPR 1 for ink comparisons.
#[test]
#[ignore = "manual: writes cmp_text.png for visual comparison with the Qt reference grab"]
fn qt_text_compare() {
    let mut r = Renderer::new();
    let (w, h) = (438u32, 271u32);
    let buf = full(w, h, &mut r, [0, 0, 0], &mut |c| {
        c.scale(S, S);
        let mut y = 0.0;
        for wt in [400, 500, 600, 700] {
            let wi = match wt {
                400 => Weight::Regular,
                500 => Weight::Medium,
                600 => Weight::DemiBold,
                _ => Weight::Bold,
            };
            c.text(&format!("Ação AVATAR To Wa 00:12:34 w{wt}"), 0.0, y, &TextStyle::new(13.0, wi, Color::WHITE));
            y += 20.0;
        }
    });
    save_png("cmp_text.png", &buf, w, h, Some([0, 0, 0]));
}

#[test]
#[ignore = "perf: run with --release -- --ignored --nocapture"]
fn bench_shadow_tile() {
    use std::time::Instant;
    let mut r = Renderer::new();
    let (w, h) = (792u32, 792u32);
    let mut buf = vec![0u8; (w * h * 4) as usize];
    let band = IRect::new(110, 0, 571, 750);
    // radius animation: a new quantised radius (and so a new tile) on most frames
    let mut worst = 0.0f64;
    let mut total = 0.0f64;
    let t0 = Instant::now();
    for i in 0..20 {
        let rad = 4.0 + i as f32 * 1.5;
        let t = Instant::now();
        r.render(&mut buf, w, h, &[band], &mut |c| {
            c.scale(S, S);
            c.shadow(
                Rect::new(10.0, 0.0, 530.0, 700.0),
                rad,
                34.0,
                -4.0,
                Point::new(0.0, 14.0),
                Color::BLACK.with_alpha(0.65),
            );
        });
        let us = t.elapsed().as_secs_f64() * 1e6;
        worst = worst.max(us);
        total += us;
    }
    println!(
        "radius animation: 20 frames, mean {:.0} us, worst {:.0} us, tiles built {} ({:.1} ms total)",
        total / 20.0,
        worst,
        r.eng.shadows.tiles_built,
        t0.elapsed().as_secs_f64() * 1e3
    );
}

#[test]
fn renderer_measure_matches_canvas() {
    let mut r = Renderer::new();
    let strings = ["Hello", "Ação AVATAR To Wa", "00:12:34", "Uma demanda bem longa que precisa ser elidida", ""];
    let mut expected = Vec::new();
    let mut buf = vec![0u8; 16];
    for style in [crate::FontStyle::QtDefault, crate::FontStyle::Inter] {
        r.set_font_style(style);
        expected.clear();
        for wt in [Weight::Regular, Weight::Medium, Weight::DemiBold, Weight::Bold] {
            for px in [11.0, 13.0, 23.0] {
                let st = TextStyle::new(px, wt, Color::WHITE).with_spacing(0.5);
                for s in strings {
                    expected.push((s, st.clone(), r.measure(s, &st), r.line_height(&st), r.elide(s, &st, 90.0)));
                }
            }
        }
        r.render(&mut buf, 2, 2, &[IRect::new(0, 0, 2, 2)], &mut |c| {
            c.scale(S, S);
            for (s, st, m, lh, e) in &expected {
                assert_eq!(c.measure(s, st), *m);
                assert_eq!(c.line_height(st), *lh);
                assert_eq!(&c.elide(s, st, 90.0), e);
            }
        });
    }
}

#[test]
fn sprite_cache_stays_under_its_cap_and_counts_its_bytes() {
    let mut r = Renderer::new();
    let (w, h) = (200u32, 100u32);
    let mut buf = vec![0u8; (w * h * 4) as usize];
    let rect = IRect::new(0, 0, w as i32, h as i32);
    // 64x64 sprites (about 17 KB with the margin), all different keys: enough to overflow the cap twice
    let n = 2 * crate::canvas::SPRITE_BYTES / (66 * 66 * 4) + 1;
    for key in 0..n as u64 {
        r.render(&mut buf, w, h, &[rect], &mut |c| {
            c.cached(key, Rect::new(10.0, 10.0, 64.0, 64.0), &mut |s| {
                s.fill_rrect(Rect::new(10.0, 10.0, 64.0, 64.0), 8.0, &Fill::Solid(Color::WHITE));
            });
        });
        let real: usize = r.eng.sprites.values().map(|p| p.data().len()).sum();
        assert_eq!(r.eng.sprite_bytes, real, "counter drifted at key {key}");
        assert!(real <= crate::canvas::SPRITE_BYTES, "{real} bytes cached at key {key}");
    }
    assert!(r.eng.sprites.len() < n, "the cache never emptied");
}

#[test]
fn glyph_cache_stays_under_its_cap_and_a_clear_frees_the_memory() {
    let mut r = Renderer::new();
    let (w, h) = (400u32, 120u32);
    let mut buf = vec![0u8; (w * h * 4) as usize];
    let rect = IRect::new(0, 0, w as i32, h as i32);
    // every size is a new glyph set; the 24 MB-era cap would hold all of them
    let mut peak = 0;
    for i in 0..600 {
        let st = TextStyle::new(30.0 + i as f32 * 0.05, Weight::Regular, Color::WHITE);
        r.render(&mut buf, w, h, &[rect], &mut |c| {
            c.text("Foquinho 0123456789", 4.0, 4.0, &st);
        });
        peak = peak.max(r.eng.glyphs.data.len());
    }
    // a glyph of this size is well under 16 KB
    assert!(peak <= crate::glyphs::MAX_DATA + (16 << 10), "atlas peaked at {peak} bytes");
    assert!(peak > crate::glyphs::MAX_DATA / 2, "the test never filled the atlas");
    r.eng.glyphs.clear();
    assert_eq!(r.eng.glyphs.data.capacity(), 0, "clear kept the doubled capacity");
}

#[test]
fn released_scratch_is_empty_and_the_next_partial_frame_is_identical() {
    let paint = |c: &mut Canvas| {
        c.fill_rrect(Rect::new(0.0, 0.0, 300.0, 200.0), 0.0, &Fill::Solid(theme::CARD));
        c.text("scratch", 20.0, 20.0, &TextStyle::new(13.0, Weight::Medium, theme::TEXT));
    };
    let mut r = Renderer::new();
    let (w, h) = (300u32, 200u32);
    let rect = [IRect::new(40, 10, 120, 80)]; // not full-width: goes through the scratch
    let mut first = vec![0u8; (w * h * 4) as usize];
    r.render(&mut first, w, h, &rect, &mut |c| paint(c));
    assert!(!r.scratch.is_empty());
    r.release_scratch();
    assert_eq!(r.scratch.capacity(), 0);
    let mut second = vec![0u8; (w * h * 4) as usize];
    r.render(&mut second, w, h, &rect, &mut |c| paint(c));
    assert_eq!(first, second);
}
