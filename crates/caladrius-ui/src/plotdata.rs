//! What the profile plot draws, as plain data: the points, what a logarithmic axis can show of
//! them, the axis ranges, and which point a click selects. Display arithmetic only; every
//! concentration and every parameter comes from the engine.

/// A point of the profile.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pt {
    pub time: f64,
    pub conc: f64,
    /// A value that a policy put in the place of a missing or BLQ value.
    pub replaced: bool,
}

/// What a logarithmic concentration axis shows.
#[derive(Debug, Clone, PartialEq)]
pub struct LogView {
    /// Points with a positive concentration, in the order given.
    pub visible: Vec<Pt>,
    /// Points left out of the log view: zero, negative or not a finite number (the data is
    /// unchanged; only this view leaves them out).
    pub hidden: Vec<Pt>,
    /// The axis range as base-10 exponents, whole decades; `None` when nothing remains.
    pub decades: Option<(f64, f64)>,
}

/// The decade range that holds the positive values `low` to `high`: the decade at or below the
/// smallest and at or above the largest; one decade when they fall in a single decade.
pub fn decade_range(low: f64, high: f64) -> (f64, f64) {
    let lo = low.log10().floor();
    let mut hi = high.log10().ceil();
    if hi <= lo {
        hi = lo + 1.0;
    }
    (lo, hi)
}

/// Splits `points` for a logarithmic concentration axis.
pub fn log_view(points: &[Pt]) -> LogView {
    let (visible, hidden): (Vec<Pt>, Vec<Pt>) = points
        .iter()
        .copied()
        .partition(|p| p.conc.is_finite() && p.conc > 0.0 && p.time.is_finite());
    let decades = visible
        .iter()
        .map(|p| p.conc)
        .fold(None, |acc: Option<(f64, f64)>, c| match acc {
            None => Some((c, c)),
            Some((lo, hi)) => Some((lo.min(c), hi.max(c))),
        })
        .map(|(lo, hi)| decade_range(lo, hi));
    LogView {
        visible,
        hidden,
        decades,
    }
}

/// The sentence of the plot's note about hidden points; `None` when none is hidden.
pub fn hidden_note(hidden: usize, total: usize) -> Option<String> {
    (hidden > 0).then(|| {
        format!("{hidden} of {total} points not shown on a log axis (zero or negative values)")
    })
}

/// A plain range for the linear view: from zero (or the first time) to a little past the last time,
/// and from zero to a little past the largest concentration.
pub fn linear_ranges(points: &[Pt]) -> ((f64, f64), (f64, f64)) {
    let finite: Vec<&Pt> = points
        .iter()
        .filter(|p| p.time.is_finite() && p.conc.is_finite())
        .collect();
    let t_min = finite.iter().map(|p| p.time).fold(0.0_f64, f64::min);
    let t_max = finite.iter().map(|p| p.time).fold(1.0_f64, f64::max);
    let c_min = finite.iter().map(|p| p.conc).fold(0.0_f64, f64::min);
    let c_max = finite.iter().map(|p| p.conc).fold(1.0_f64, f64::max);
    let pad = |lo: f64, hi: f64| (lo, hi + 0.05 * (hi - lo));
    (pad(t_min, t_max), pad(c_min, c_max))
}

/// The time axis for the log view: from zero (or earlier) to a little past the last time, over the
/// points that remain on the axis and the points that do not (time is shown for all of them).
pub fn time_range(points: &[Pt]) -> (f64, f64) {
    linear_ranges(points).0
}

/// The index of the point nearest to `click`, within `radius`, among `screen` positions (all in
/// the same units, pixels). `None` when no point is close enough.
pub fn nearest(screen: &[(f64, f64)], click: (f64, f64), radius: f64) -> Option<usize> {
    screen
        .iter()
        .enumerate()
        .map(|(i, p)| (i, (p.0 - click.0).hypot(p.1 - click.1)))
        .filter(|(_, d)| d.is_finite() && *d <= radius)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

/// The set of times after one click: the clicked time is added when it is not in the set and removed
/// when it is. Times are kept sorted and without repeats.
pub fn toggle_time(used: &[f64], clicked: f64) -> Vec<f64> {
    let mut out: Vec<f64> = used.iter().copied().filter(|t| *t != clicked).collect();
    if out.len() == used.len() {
        out.push(clicked);
    }
    out.sort_by(f64::total_cmp);
    out
}

/// Round tick values in `lo..=hi`: about `target` of them, a step of 1, 2 or 5 times a power of ten.
pub fn nice_ticks(lo: f64, hi: f64, target: usize) -> Vec<f64> {
    if !(lo.is_finite() && hi.is_finite()) || hi <= lo || target == 0 {
        return Vec::new();
    }
    let raw = (hi - lo) / target as f64;
    let power = 10f64.powf(raw.log10().floor());
    let step = [1.0, 2.0, 5.0, 10.0]
        .iter()
        .map(|m| m * power)
        .find(|s| *s >= raw)
        .unwrap_or(10.0 * power);
    let first = (lo / step).ceil() as i64;
    let last = (hi / step).floor() as i64;
    (first..=last).take(200).map(|k| k as f64 * step).collect()
}

/// The marks of a logarithmic axis between the exponents `lo` and `hi`: (exponent, major) with the
/// decades major and the multiples 2 to 9 of each decade minor, all inside the range.
pub fn log_ticks(lo: f64, hi: f64) -> Vec<(f64, bool)> {
    if !(lo.is_finite() && hi.is_finite()) || hi <= lo || hi - lo > 40.0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    let first = lo.floor() as i64;
    let last = hi.ceil() as i64;
    for e in first..=last {
        for k in 1..=9 {
            let value = e as f64 + f64::from(k).log10();
            if value >= lo - 1e-9 && value <= hi + 1e-9 {
                out.push((value, k == 1));
            }
        }
    }
    out
}

/// Tick label of a logarithmic axis at the exponent `exponent`: `0.01`, `1`, `100`, `1e5`.
pub fn log_label(exponent: f64) -> String {
    if (-3.0..=4.0).contains(&exponent) {
        format!("{}", 10f64.powf(exponent))
    } else {
        format!("1e{exponent:.0}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(time: f64, conc: f64) -> Pt {
        Pt {
            time,
            conc,
            replaced: false,
        }
    }

    #[test]
    fn a_zero_is_left_out_of_the_log_view_and_counted() {
        let points = [
            pt(0.0, 0.0),
            pt(1.0, 8.0),
            pt(2.0, 4.0),
            pt(4.0, 0.9),
            pt(8.0, -1.0),
        ];
        let v = log_view(&points);
        assert_eq!(v.visible.len(), 3);
        assert_eq!(v.hidden, vec![pt(0.0, 0.0), pt(8.0, -1.0)]);
        // 0.9 is in the decade 0.1-1, 8 in 1-10: the range is whole decades from -1 to 1.
        assert_eq!(v.decades, Some((-1.0, 1.0)));
        assert_eq!(
            hidden_note(v.hidden.len(), points.len()).as_deref(),
            Some("2 of 5 points not shown on a log axis (zero or negative values)")
        );
        assert_eq!(hidden_note(0, 5), None);
    }

    #[test]
    fn nothing_positive_is_an_empty_view_not_a_range() {
        let v = log_view(&[pt(0.0, 0.0), pt(1.0, 0.0)]);
        assert!(v.visible.is_empty());
        assert_eq!(v.decades, None);
        assert_eq!(log_view(&[]).decades, None);
        // Not finite values never reach the axis either.
        let v = log_view(&[
            pt(1.0, f64::NAN),
            pt(f64::INFINITY, 2.0),
            pt(1.0, f64::INFINITY),
        ]);
        assert!(v.visible.is_empty() && v.hidden.len() == 3);
    }

    #[test]
    fn the_range_comes_from_the_remaining_points_in_whole_decades() {
        assert_eq!(decade_range(0.5, 40.0), (-1.0, 2.0));
        assert_eq!(decade_range(3.0, 3.0), (0.0, 1.0), "one point: one decade");
        assert_eq!(decade_range(2.0, 9.0), (0.0, 1.0));
        assert_eq!(decade_range(10.0, 10.0), (1.0, 2.0));
        assert_eq!(decade_range(1.0, 100.0), (0.0, 2.0));
        let v = log_view(&[pt(1.0, 5.0)]);
        assert_eq!(v.decades, Some((0.0, 1.0)));
    }

    #[test]
    fn linear_ranges_start_at_zero_and_leave_room() {
        let ((t0, t1), (c0, c1)) = linear_ranges(&[pt(0.5, 2.0), pt(24.0, 10.0)]);
        assert_eq!((t0, c0), (0.0, 0.0));
        assert!(t1 > 24.0 && c1 > 10.0);
        // Empty or non-finite input still gives a usable range.
        let ((t0, t1), (c0, c1)) = linear_ranges(&[pt(f64::NAN, 1.0)]);
        assert!(t1 > t0 && c1 > c0);
        // A negative concentration is inside the range.
        let (_, (c0, _)) = linear_ranges(&[pt(1.0, -2.0), pt(2.0, 3.0)]);
        assert_eq!(c0, -2.0);
    }

    #[test]
    fn a_click_selects_the_nearest_point_within_the_radius() {
        let screen = [(10.0, 10.0), (50.0, 50.0), (52.0, 49.0)];
        assert_eq!(nearest(&screen, (51.0, 50.0), 8.0), Some(1));
        assert_eq!(nearest(&screen, (52.0, 49.5), 8.0), Some(2));
        assert_eq!(nearest(&screen, (200.0, 200.0), 8.0), None);
        assert_eq!(nearest(&[], (0.0, 0.0), 8.0), None);
        assert_eq!(nearest(&[(f64::NAN, 0.0)], (0.0, 0.0), 8.0), None);
    }

    #[test]
    fn a_click_toggles_a_time_in_the_selection() {
        assert_eq!(toggle_time(&[2.0, 4.0, 8.0], 6.0), vec![2.0, 4.0, 6.0, 8.0]);
        assert_eq!(toggle_time(&[2.0, 4.0, 8.0], 4.0), vec![2.0, 8.0]);
        assert_eq!(toggle_time(&[], 1.0), vec![1.0]);
        assert_eq!(toggle_time(&[1.0], 1.0), Vec::<f64>::new());
    }

    #[test]
    fn ticks_are_round_numbers_inside_the_range() {
        assert_eq!(nice_ticks(0.0, 24.0, 5), vec![0.0, 5.0, 10.0, 15.0, 20.0]);
        assert_eq!(nice_ticks(0.0, 4.2, 5), vec![0.0, 1.0, 2.0, 3.0, 4.0]);
        assert_eq!(
            nice_ticks(-2.0, 3.15, 6),
            vec![-2.0, -1.0, 0.0, 1.0, 2.0, 3.0]
        );
        let t = nice_ticks(0.0, 0.0032, 4);
        assert!(
            t.len() >= 3 && t.iter().all(|v| (0.0..=0.0032).contains(v)),
            "{t:?}"
        );
        for bad in [
            (1.0, 1.0),
            (2.0, 1.0),
            (f64::NAN, 1.0),
            (0.0, f64::INFINITY),
        ] {
            assert!(nice_ticks(bad.0, bad.1, 5).is_empty(), "{bad:?}");
        }
        assert!(nice_ticks(0.0, 1.0, 0).is_empty());
    }

    #[test]
    fn log_ticks_are_decades_and_their_multiples_inside_the_range_only() {
        let ticks = log_ticks(-1.0, 1.0);
        let majors: Vec<f64> = ticks.iter().filter(|t| t.1).map(|t| t.0).collect();
        assert_eq!(majors, vec![-1.0, 0.0, 1.0]);
        assert!(ticks.iter().all(|t| (-1.0..=1.0 + 1e-9).contains(&t.0)));
        assert!(ticks.len() > 15, "minor lines at 2 to 9 in each decade");
        // Nothing outside the range: no stray 0.01 label below 0.1.
        assert!(log_ticks(0.0, 1.0).iter().all(|t| t.0 >= 0.0));
        assert!(log_ticks(1.0, 1.0).is_empty());
        assert!(log_ticks(f64::NAN, 1.0).is_empty());
        assert!(
            log_ticks(-100.0, 100.0).is_empty(),
            "an absurd range draws nothing"
        );
    }

    #[test]
    fn tick_labels_are_plain_numbers_in_the_usual_range() {
        assert_eq!(log_label(0.0), "1");
        assert_eq!(log_label(2.0), "100");
        assert_eq!(log_label(-2.0), "0.01");
        assert_eq!(log_label(-6.0), "1e-6");
        assert_eq!(log_label(7.0), "1e7");
    }
}
