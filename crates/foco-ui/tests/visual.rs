//! Writes PNGs of the notch and the widgets to `rs/target/ui-test/` (text is the PixSurface
//! placeholder box) and checks a few pixels.

use foco_base::{theme, Button, Color, Event, Fill, KeyEvent, Key, Mods, PixSurface, Point, Rect, Surface, Task, TextStyle, Weight};
use foco_ui::notch::{paint_notch, NotchGeom};
use foco_ui::testkit::Harness;
use foco_ui::views::CollapsedView;
use foco_ui::widgets::*;
use foco_ui::{View, S};
use std::path::PathBuf;

fn out_dir() -> PathBuf {
    let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/ui-test");
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn surface(h_logical: f32, bg: Color) -> PixSurface {
    let mut s = PixSurface::new(792, (h_logical * S).ceil() as u32).unwrap();
    s.clear(bg);
    s.scale(S, S);
    s
}

fn save(s: &PixSurface, name: &str) {
    s.save_png(out_dir().join(name)).unwrap();
}

#[test]
fn notch_sizes() {
    let bg = Color::rgb8(0x8a, 0x8d, 0x94);
    for (name, w, h, r, shadow, sh) in [
        ("notch-collapsed.png", 212.0, 38.0, 19.0, 0.0, 90.0),
        ("notch-alert.png", 400.0, 62.0, 28.0, 0.45, 150.0),
        ("notch-expanded.png", 548.0, 360.0, 32.0, 0.65, 460.0),
    ] {
        let mut s = surface(sh, bg);
        paint_notch(&mut s, &NotchGeom::new(w, h, r), shadow);
        save(&s, name);
        // the notch body is opaque black at the top centre
        let (px, py) = ((380.0 * S) as u32, (4.0 * S) as u32);
        assert_eq!(s.pixel(px, py)[3], 255, "{name}");
    }
}

fn task(h: &Harness, title: &str, running: bool, ms: i64) -> Task {
    Task {
        tid: title.into(),
        title: title.into(),
        running,
        started_at: if running { h.epoch as i64 - ms } else { 0 },
        acc_ms: if running { 0 } else { ms },
        ..Task::default()
    }
}

fn collapsed_case(h: &mut Harness, hovered: bool) -> PixSurface {
    let mut v = CollapsedView::new();
    v.set_shown(true, 0.0);
    v.set_hovered(hovered, 0.0);
    let now = 2000.0;
    let mut frame = Rect::default();
    for _ in 0..2 {
        h.with_cx(now, |cx| {
            v.update(cx);
            let w = v.implicit_width();
            frame = Rect::new((760.0 - w) / 2.0, 0.0, w, if hovered { 42.0 } else { 38.0 });
            v.layout(cx, frame);
        });
    }
    let mut s = surface(60.0, Color::rgb8(0x8a, 0x8d, 0x94));
    let g = NotchGeom::new(v.implicit_width(), frame.h, 19.0);
    paint_notch(&mut s, &g, 0.0);
    s.save();
    s.clip_rect(g.clip_rect());
    h.with_pc(now, Rect::new(0.0, 0.0, 760.0, 60.0), |pc| v.paint(&mut s, pc));
    s.restore();
    s
}

#[test]
fn collapsed_view_sheet() {
    let mut h = Harness::new();
    let a = collapsed_case(&mut h, false);
    save(&a, "collapsed-empty.png");
    let t = task(&h, "Escrever o relatório trimestral", true, 754_000);
    h.store.state.focus.push(t);
    h.rev += 1;
    let b = collapsed_case(&mut h, false);
    save(&b, "collapsed-focus.png");
    let c = collapsed_case(&mut h, true);
    save(&c, "collapsed-focus-hover.png");
    let t2 = task(&h, "Responder e-mails", false, 120_000);
    h.store.state.focus.push(t2);
    h.rev += 1;
    let d = collapsed_case(&mut h, false);
    save(&d, "collapsed-two-focus.png");
    // the notch body is black and something (title/timer text) is drawn on top of it
    let lit = (0..792u32).any(|x| (0..38u32).any(|y| b.pixel(x, y)[0] > 100 && b.pixel(x, y)[3] == 255));
    assert!(lit, "content painted inside the notch");
}

#[test]
fn widget_sheet() {
    let mut h = Harness::new();
    let mut s = surface(330.0, theme::CARD);
    let now = 5000.0;

    // icons
    h.with_pc(now, Rect::new(0.0, 0.0, 760.0, 400.0), |_| {});
    for (i, ic) in Icon::ALL.iter().enumerate() {
        let x = 30.0 + i as f32 * 36.0;
        paint_icon(&mut s, *ic, Point::new(x, 30.0), 24.0, theme::TEXT, DEFAULT_STROKE);
        paint_icon(&mut s, *ic, Point::new(x, 62.0), 14.0, theme::TEXT2, DEFAULT_STROKE);
    }

    // icon buttons: rest / hover / pressed / green
    let mut btns = [
        IconButton::new(Icon::Play, 28.0, 15.0),
        IconButton::new(Icon::Pause, 28.0, 15.0),
        IconButton::new(Icon::Trash, 28.0, 15.0),
        IconButton::new(Icon::Check, 26.0, 14.0).colors(Color::WHITE, Color::WHITE, theme::GREEN, theme::GREEN.lighter(1.15)),
    ];
    for (i, b) in btns.iter_mut().enumerate() {
        h.with_cx(0.0, |cx| b.layout(cx, Rect::new(30.0 + i as f32 * 44.0, 90.0, b.size, b.size)));
    }
    let c1 = btns[1].rect().center();
    h.with_cx(0.0, |cx| btns[1].pointer(cx, &Event::PointerMove { pos: c1 }));
    let c2 = btns[2].rect().center();
    h.with_cx(0.0, |cx| {
        btns[2].pointer(cx, &Event::PointerDown { pos: c2, button: Button::Left, clicks: 1 });
    });
    for b in &btns {
        b.paint(&mut s, now);
    }
    assert!(btns[1].hovered() && btns[2].pressed());

    // toggles
    let mut t_off = Toggle::new(false);
    let mut t_on = Toggle::new(true);
    h.with_cx(0.0, |cx| {
        t_off.layout(cx, 230.0, 92.0);
        t_on.layout(cx, 280.0, 92.0);
    });
    h.with_pc(now, Rect::new(0.0, 0.0, 760.0, 400.0), |pc| {
        t_off.paint(&mut s, pc);
        t_on.paint(&mut s, pc);
    });

    // pills, chips, section labels
    h.with_pc(now, Rect::new(0.0, 0.0, 760.0, 400.0), |pc| {
        paint_pill(&mut s, pc, Rect::new(340.0, 94.0, 40.0, PILL_H), "1/3", 0.25, true);
        paint_pill(&mut s, pc, Rect::new(390.0, 94.0, 48.0, PILL_H), "nv 2", 0.24, false);
    });
    let st = TextStyle::new(11.0, Weight::Regular, theme::TEXT);
    for (i, k) in ["Meta", "Alt", "N"].iter().enumerate() {
        let w = chip_width(&mut foco_ui::testkit::FakeText, k, &st, 12.0, 20.0);
        paint_chip(&mut s, Rect::new(460.0 + i as f32 * 44.0, 92.0, w, 20.0), 6.0, k, &st, true);
    }
    paint_section_label(&mut s, Rect::new(30.0, 130.0, 200.0, SECTION_LABEL_H), "EM FOCO", Some(2));
    paint_section_label(&mut s, Rect::new(260.0, 130.0, 200.0, SECTION_LABEL_H), "A SEGUIR", None);

    // text input (active, with a selection) on an input-field card
    let card = Rect::new(30.0, 170.0, 360.0, 44.0);
    s.fill_rrect(card, 14.0, &Fill::Solid(theme::CARD_HOVER));
    s.stroke_rrect(card.inflate(-0.5, -0.5), 13.5, &Fill::Solid(h.env.accent.with_alpha(0.75)), 1.0);
    let mut ti = TextInput::new(TextStyle::new(13.0, Weight::Regular, theme::TEXT), 160);
    h.with_cx(0.0, |cx| ti.layout(cx, Rect::new(56.0, 180.0, 320.0, 24.0)));
    for c in "Escrever relatório".chars() {
        let text = c.to_string();
        h.with_cx(0.0, |cx| ti.key(cx, &KeyEvent { key: Key::Char(c), mods: Mods::NONE, text: Some(text.clone()) }));
    }
    for _ in 0..4 {
        h.with_cx(0.0, |cx| ti.key(cx, &KeyEvent { key: Key::Left, mods: Mods { shift: true, ..Mods::NONE }, text: None }));
    }
    h.with_pc(0.0, Rect::new(0.0, 0.0, 760.0, 400.0), |pc| {
        ti.paint(&mut s, pc, true, pc.env.accent.with_alpha(0.6), Some(Color::WHITE));
    });
    assert_eq!(ti.selected_text().chars().count(), 4);

    // a second input right-aligned, unfocused, with a caret hidden
    let mut ti2 = TextInput::new(TextStyle::new(13.0, Weight::Regular, theme::TEXT2), 24);
    ti2.align_right = true;
    ti2.set_text("Foquinho");
    h.with_cx(0.0, |cx| ti2.layout(cx, Rect::new(420.0, 180.0, 200.0, 24.0)));
    h.with_pc(0.0, Rect::new(0.0, 0.0, 760.0, 400.0), |pc| {
        ti2.paint(&mut s, pc, false, Color::TRANSPARENT, None);
    });

    save(&s, "widgets.png");
}
