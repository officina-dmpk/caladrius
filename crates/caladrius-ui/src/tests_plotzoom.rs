//! Zoom and pan on the plots (task "demo/ui-quick"): a drag pans, Ctrl and the wheel or a pinch
//! zooms, a drag of the secondary button zooms to a box, a double click brings the full range back,
//! a click still selects a point, and the marks of the grid follow the range that is visible and
//! stay round numbers (1, 2 or 5 times a power of ten), on linear and on semi-log axes.

use egui::{Event, Modifiers, PointerButton, Pos2, Vec2, pos2};
use egui_kittest::Harness;

use crate::plot::{self, Clicked, Dot, PointSet, Tone, marks};
use crate::theme::{ThemeMode, Tokens};

/// What the plot said at the last frame.
#[derive(Default)]
struct Seen {
    transform: Option<egui_plot::PlotTransform>,
    clicked: Vec<Clicked>,
}

impl Seen {
    fn bounds(&self) -> ([f64; 2], [f64; 2]) {
        let b = self
            .transform
            .as_ref()
            .map(|t| *t.bounds())
            .unwrap_or(egui_plot::PlotBounds::NOTHING);
        (b.min(), b.max())
    }
}

fn plot_harness(log: bool) -> Harness<'static, Seen> {
    let tokens = Tokens::embedded(ThemeMode::Light).unwrap();
    let points = vec![
        [0.5, 4.0],
        [1.0, 8.0],
        [2.0, 6.0],
        [4.0, 3.5],
        [8.0, 1.5],
        [24.0, 0.2],
    ];
    let sets = vec![PointSet::new(
        "observed",
        points,
        Tone::Observed,
        Dot::Medium,
    )];
    let mut h = Harness::builder()
        .with_size(Vec2::new(640.0, 420.0))
        .build_ui_state(
            move |ui, seen: &mut Seen| {
                let (clicked, transform) = plot::profile_view(
                    ui,
                    &tokens,
                    "zoom-test",
                    ("Time".to_owned(), "Concentration".to_owned()),
                    &sets,
                    &[],
                    log,
                    &[0],
                    300.0,
                );
                seen.clicked.extend(clicked);
                seen.transform = Some(transform);
            },
            Seen::default(),
        );
    h.run_steps(3);
    h
}

fn centre(h: &Harness<'_, Seen>) -> Pos2 {
    h.state()
        .transform
        .as_ref()
        .map(|t| t.frame().center())
        .unwrap_or(pos2(300.0, 150.0))
}

fn button(h: &mut Harness<'_, Seen>, at: Pos2, button: PointerButton, pressed: bool) {
    h.input_mut().events.push(Event::PointerButton {
        pos: at,
        button,
        pressed,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(1);
}

fn move_to(h: &mut Harness<'_, Seen>, at: Pos2) {
    h.input_mut().events.push(Event::PointerMoved(at));
    h.run_steps(1);
}

/// Two clicks in one frame, which is how fast a double click is.
fn double_click(h: &mut Harness<'_, Seen>, at: Pos2) {
    move_to(h, at);
    for pressed in [true, false, true, false] {
        h.input_mut().events.push(Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    h.run_steps(3);
}

/// A press at `from`, a move in a few steps to `to`, a release.
fn drag(h: &mut Harness<'_, Seen>, from: Pos2, to: Pos2, which: PointerButton) {
    move_to(h, from);
    button(h, from, which, true);
    for i in 1..=4 {
        let f = i as f32 / 4.0;
        move_to(h, from + (to - from) * f);
    }
    button(h, to, which, false);
    h.run_steps(2);
}

fn span(b: ([f64; 2], [f64; 2])) -> (f64, f64) {
    (b.1[0] - b.0[0], b.1[1] - b.0[1])
}

#[test]
fn a_drag_pans_and_a_double_click_brings_the_full_range_back() {
    let mut h = plot_harness(false);
    let full = h.state().bounds();
    assert!(full.0[0] < 0.5 && full.1[0] > 24.0, "{full:?}");
    let c = centre(&h);
    drag(
        &mut h,
        c,
        c - Vec2::new(120.0, 40.0),
        PointerButton::Primary,
    );
    let panned = h.state().bounds();
    // The view moved to the right and up (the data followed the hand), by the same span.
    assert!(panned.0[0] > full.0[0] + 1.0, "{panned:?} against {full:?}");
    let (sx, sy) = (span(full), span(panned));
    assert!(
        (sx.0 - sy.0).abs() < 1e-9 * sx.0.abs().max(1.0),
        "{sx:?} {sy:?}"
    );
    // A drag is not a click: no point was selected.
    assert!(h.state().clicked.is_empty());
    // A double click restores the full range exactly.
    let c = centre(&h);
    double_click(&mut h, c);
    assert_eq!(h.state().bounds(), full);
}

#[test]
fn a_pinch_and_a_box_zoom_narrow_the_view_and_a_double_click_restores_it() {
    let mut h = plot_harness(false);
    let full = h.state().bounds();
    let c = centre(&h);
    move_to(&mut h, c);
    h.input_mut().events.push(Event::Zoom(2.0));
    h.run_steps(3);
    let zoomed = h.state().bounds();
    assert!(span(zoomed).0 < span(full).0 * 0.75, "{zoomed:?} {full:?}");
    // The plain wheel is left to the page: no zoom, no pan.
    let before = h.state().bounds();
    h.input_mut().events.push(Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: Vec2::new(0.0, -80.0),
        modifiers: Modifiers::NONE,
    });
    h.run_steps(3);
    assert_eq!(h.state().bounds(), before);
    // A drag of the secondary button draws a box and zooms to it.
    let c = centre(&h);
    drag(
        &mut h,
        c - Vec2::new(60.0, 40.0),
        c + Vec2::new(60.0, 40.0),
        PointerButton::Secondary,
    );
    let boxed = h.state().bounds();
    assert!(span(boxed).0 < span(before).0 * 0.8, "{boxed:?} {before:?}");
    double_click(&mut h, c);
    assert_eq!(h.state().bounds(), full);
}

#[test]
fn a_click_on_a_point_still_selects_it() {
    let mut h = plot_harness(false);
    let (screen, plot_point) = {
        let t = h.state().transform.as_ref().unwrap();
        let p = egui_plot::PlotPoint::new(4.0, 3.5);
        (t.position_from_point(&p), p)
    };
    move_to(&mut h, screen);
    button(&mut h, screen, PointerButton::Primary, true);
    button(&mut h, screen, PointerButton::Primary, false);
    h.run_steps(2);
    let clicked = h.state().clicked.last().copied();
    let clicked = clicked.expect("the click selected a point");
    assert_eq!((clicked.set, clicked.index), (0, 3));
    assert_eq!(clicked.point, [plot_point.x, plot_point.y]);
}

#[test]
fn the_semi_log_plot_pans_and_zooms_too() {
    let mut h = plot_harness(true);
    let full = h.state().bounds();
    let c = centre(&h);
    drag(&mut h, c, c - Vec2::new(100.0, 0.0), PointerButton::Primary);
    assert_ne!(h.state().bounds(), full);
    move_to(&mut h, c);
    h.input_mut().events.push(Event::Zoom(3.0));
    h.run_steps(3);
    let zoomed = h.state().bounds();
    // The vertical range is in decades: a zoom narrows it, whatever the marks are.
    assert!(span(zoomed).1 < span(full).1, "{zoomed:?} {full:?}");
}

/// A tick is a round number when it is a whole multiple of a step of 1, 2 or 5 times a power of ten.
fn is_round(value: f64, step: f64) -> bool {
    let k = value / step;
    (k - k.round()).abs() < 1e-6 && {
        let mantissa = step / 10f64.powf((step.log10() + 1e-9).floor());
        [1.0, 2.0, 5.0].iter().any(|m| (mantissa - m).abs() < 1e-6)
    }
}

#[test]
fn the_marks_of_a_zoomed_linear_range_are_round_numbers() {
    for (lo, hi) in [
        (0.0, 24.0),
        (3.7, 9.1),
        (12.345, 12.789),
        (-0.0021, 0.0043),
        (1.0e6, 1.0e6 + 50.0),
        (-5.0, 1.0e-3),
    ] {
        let marks = marks(false, lo, hi);
        assert!(marks.len() >= 2, "{lo}..{hi}: {marks:?}");
        for m in &marks {
            assert!(m.value >= lo && m.value <= hi, "{lo}..{hi}: {m:?}");
            assert!(is_round(m.value, m.step_size), "{lo}..{hi}: {m:?}");
        }
        let n = marks.len();
        assert!((3..=12).contains(&n), "{lo}..{hi}: {n} marks");
    }
    assert!(marks(false, 2.0, 2.0).is_empty());
    assert!(marks(false, f64::NAN, 2.0).is_empty());
}

#[test]
fn the_marks_of_a_zoomed_semi_log_range_are_round_numbers_of_the_values() {
    // Decades and their multiples when several are visible.
    let wide = marks(true, -1.0, 2.0);
    assert!(wide.iter().any(|m| (m.value - 1.0).abs() < 1e-12));
    assert!(wide.iter().all(|m| m.value >= -1.0 && m.value <= 2.0));
    // Under half a decade: round numbers of the values (here 1.1 to 1.9 in steps of 0.2 or 0.1).
    for (lo, hi) in [(0.04, 0.27), (0.9, 1.1), (-1.32, -1.1)] {
        let zoomed = marks(true, lo, hi);
        assert!(zoomed.len() >= 2, "{lo}..{hi}: {zoomed:?}");
        let values: Vec<f64> = zoomed.iter().map(|m| 10f64.powf(m.value)).collect();
        let step = values[1] - values[0];
        for v in &values {
            assert!(is_round(*v, step), "{lo}..{hi}: {v} on a step of {step}");
        }
    }
    assert!(marks(true, 1.0, 1.0).is_empty());
}
