//! The plots, drawn from data the engine gave: points and lines of a concentration profile on a
//! linear or semi-log axis, and scatter plots of residuals. Every curve is passed in; nothing here
//! evaluates a model. The grid is drawn from the `plot_grid` token, the series from the series tokens.

use std::ops::RangeInclusive;

use egui::{Color32, RichText, Ui};
use egui_plot::{
    GridMark, HLine, Legend, Line, MarkerShape, Plot, PlotBounds, PlotPoint, PlotPoints,
    PlotTransform, Points, VLine,
};

use crate::fmt;
use crate::plotdata::{self, Pt};
use crate::theme::Tokens;

/// Which series colour of the theme a series takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Observed,
    Other,
    Replaced,
    Fit,
    Selected,
    /// The curve of the starting values, apart from the observed points and the chosen range.
    Start,
}

impl Tone {
    fn color(self, tokens: &Tokens) -> Color32 {
        let c = &tokens.colors;
        match self {
            Tone::Observed => c.series_observed.color(),
            Tone::Other => c.series_other.color(),
            Tone::Replaced => c.series_replaced.color(),
            Tone::Fit => c.series_fit.color(),
            Tone::Selected => c.series_selected.color(),
            Tone::Start => c.series_start.color(),
        }
    }
}

/// Which line width of the theme a line takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Weight {
    Thin,
    Medium,
    Thick,
}

/// Which marker size of the theme a point set takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dot {
    Small,
    Medium,
    Large,
}

/// Points drawn as markers: (time, concentration) pairs.
#[derive(Debug, Clone)]
pub struct PointSet {
    pub name: String,
    pub points: Vec<[f64; 2]>,
    pub tone: Tone,
    pub shape: MarkerShape,
    pub dot: Dot,
}

impl PointSet {
    pub fn new(name: &str, points: Vec<[f64; 2]>, tone: Tone, dot: Dot) -> PointSet {
        PointSet {
            name: name.to_owned(),
            points,
            tone,
            shape: MarkerShape::Circle,
            dot,
        }
    }

    pub fn shaped(mut self, shape: MarkerShape) -> PointSet {
        self.shape = shape;
        self
    }
}

/// A polyline: (time, concentration) pairs.
#[derive(Debug, Clone)]
pub struct LineSet {
    pub name: String,
    pub points: Vec<[f64; 2]>,
    pub tone: Tone,
    pub weight: Weight,
}

impl LineSet {
    pub fn new(name: &str, points: Vec<[f64; 2]>, tone: Tone, weight: Weight) -> LineSet {
        LineSet {
            name: name.to_owned(),
            points,
            tone,
            weight,
        }
    }
}

/// The point a click selected: which set, which point of it, and its coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Clicked {
    pub set: usize,
    pub index: usize,
    pub point: [f64; 2],
}

fn pts(points: &[[f64; 2]]) -> impl Iterator<Item = Pt> + '_ {
    points.iter().map(|p| Pt {
        time: p[0],
        conc: p[1],
        replaced: false,
    })
}

/// Axis ranges and the log transform of one plot. The range is the full one: where the plot
/// opens and where a double click brings it back (`paint_grid` sets it; the plot itself keeps the
/// range the person panned or zoomed to). The marks of the grid and of the axes are not
/// stored: they follow the range that is visible (`marks`), so they stay round numbers when the
/// person pans or zooms.
#[derive(Debug, Clone)]
pub struct Axes {
    pub x: (f64, f64),
    pub y: (f64, f64),
    pub log: bool,
    /// A logarithmic horizontal axis too (`x` in decades), for observed against predicted.
    pub x_log: bool,
}

/// Below this many decades visible, a logarithmic axis shows round numbers of its values (1, 2, 5
/// times a power of ten) instead of decades.
const LOG_ZOOMED_SPAN: f64 = 0.5;
/// Below this many decades visible, the multiples 2 and 5 of each decade are labelled too.
const LOG_LABELLED_SPAN: f64 = 1.5;

/// The marks of an axis showing `lo..=hi`: round numbers (1, 2 or 5 times a power of ten) on a
/// linear axis; on a logarithmic one (`lo` and `hi` are exponents) the decades and the multiples 2
/// to 9 of each, or, when less than half a decade is visible, round numbers of the values.
pub(crate) fn marks(log: bool, lo: f64, hi: f64) -> Vec<GridMark> {
    if !(lo.is_finite() && hi.is_finite()) || hi <= lo {
        return Vec::new();
    }
    if !log {
        let ticks = plotdata::nice_ticks(lo, hi, 6);
        let step = match ticks.as_slice() {
            [a, b, ..] => (b - a).abs(),
            _ => hi - lo,
        };
        return ticks
            .into_iter()
            .map(|value| GridMark {
                value,
                step_size: step,
            })
            .collect();
    }
    let span = hi - lo;
    if span < LOG_ZOOMED_SPAN {
        let (a, b) = (10f64.powf(lo), 10f64.powf(hi));
        let ticks = plotdata::nice_ticks(a, b, 6);
        let step = match ticks.as_slice() {
            [first, second, ..] => (second - first).abs(),
            _ => b - a,
        };
        return ticks
            .into_iter()
            .filter(|t| *t > 0.0)
            .map(|t| GridMark {
                value: t.log10(),
                // The distance to the next mark in exponents: it sets how strong the mark is drawn.
                step_size: step / (t * std::f64::consts::LN_10),
            })
            .collect();
    }
    plotdata::log_ticks(lo, hi)
        .into_iter()
        .map(|(value, major)| {
            let labelled = span < LOG_LABELLED_SPAN && is_multiple(value, &[2.0, 5.0]);
            GridMark {
                value,
                step_size: if major || labelled { 1.0 } else { 0.2 },
            }
        })
        .collect()
}

/// Whether `10^exponent` is one of the given multiples of a power of ten.
fn is_multiple(exponent: f64, multiples: &[f64]) -> bool {
    let mantissa = 10f64.powf(exponent - exponent.floor());
    multiples.iter().any(|m| (mantissa - m).abs() < 1e-6)
}

/// The label of a mark of a logarithmic axis, empty where the mark is not labelled: the decades,
/// the multiples 2 and 5 when under one and a half decades are visible, every round number when
/// under half a decade is.
fn log_mark_label(value: f64, span: f64) -> String {
    if (value - value.round()).abs() < 1e-9 {
        plotdata::log_label(value.round())
    } else if span < LOG_ZOOMED_SPAN
        || (span < LOG_LABELLED_SPAN && is_multiple(value, &[2.0, 5.0]))
    {
        fmt::number(10f64.powf(value))
    } else {
        String::new()
    }
}

/// A range the plot can open on: finite, with its end above its start.
fn usable((lo, hi): (f64, f64)) -> (f64, f64) {
    if lo.is_finite() && hi.is_finite() && lo < hi {
        (lo, hi)
    } else if lo.is_finite() {
        (lo, lo + 1.0)
    } else {
        (0.0, 1.0)
    }
}

impl Axes {
    /// Axes for these ranges (on a log axis `y` is in decades).
    pub fn new(x: (f64, f64), y: (f64, f64), log: bool) -> Axes {
        Axes {
            x,
            y,
            log,
            x_log: false,
        }
    }

    /// Axes whose horizontal axis is logarithmic as well (`x` in decades, marks on the decades).
    pub fn log_log(x: (f64, f64), y: (f64, f64)) -> Axes {
        let mut axes = Axes::new(x, y, true);
        axes.x_log = true;
        axes
    }

    /// The value on the horizontal axis for a value of the data.
    pub fn x_of(&self, v: f64) -> f64 {
        if self.x_log { v.log10() } else { v }
    }

    /// The value on the vertical axis for a concentration.
    pub fn y_of(&self, conc: f64) -> f64 {
        if self.log { conc.log10() } else { conc }
    }

    /// The plot: it opens on the full range, drags to pan, zooms with Ctrl and the wheel or a
    /// pinch, zooms to a box with a drag of the secondary button, and a double click brings the
    /// full range back. The plain wheel is left to the page. The marks follow the visible range.
    pub(crate) fn plot(
        &self,
        id: &str,
        height: f32,
        x_label: String,
        y_label: String,
    ) -> Plot<'static> {
        let log = self.log;
        let x_log = self.x_log;
        let x = usable(self.x);
        let y = usable(self.y);
        // The memory of a plot (what the person zoomed to) belongs to the range it opened on: when
        // the data change, the plot opens on the new range.
        let key = [x.0, x.1, y.0, y.1].map(f64::to_bits);
        Plot::new(format!(
            "{id}:{:x}:{:x}:{:x}:{:x}:{}{}",
            key[0],
            key[1],
            key[2],
            key[3],
            u8::from(x_log),
            u8::from(log)
        ))
        .height(height)
        .x_axis_label(x_label)
        .y_axis_label(y_label)
        .allow_drag(true)
        .allow_zoom(true)
        .allow_scroll(false)
        .allow_boxed_zoom(true)
        .allow_double_click_reset(true)
        .show_grid(false)
        .x_grid_spacer(move |input| marks(x_log, input.bounds.0, input.bounds.1))
        .y_grid_spacer(move |input| marks(log, input.bounds.0, input.bounds.1))
        .x_axis_formatter(move |mark, range: &RangeInclusive<f64>| {
            if x_log {
                log_mark_label(mark.value, range.end() - range.start())
            } else {
                fmt::number(mark.value)
            }
        })
        .y_axis_formatter(move |mark, range: &RangeInclusive<f64>| {
            if log {
                log_mark_label(mark.value, range.end() - range.start())
            } else {
                fmt::number(mark.value)
            }
        })
        .label_formatter(move |_, v| {
            let y = if log { 10f64.powf(v.y) } else { v.y };
            format!("x = {}\ny = {}", fmt::number(v.x), fmt::number(y))
        })
    }

    /// Sets the full range when the plot opens and when the person double clicks it (the plot
    /// keeps what they zoomed to by itself in between), then draws the grid for the range that is
    /// visible now.
    pub(crate) fn paint_grid(&self, plot_ui: &mut egui_plot::PlotUi<'_>, tokens: &Tokens) {
        let response = plot_ui.response().clone();
        let opened = egui::Id::new(("plot-opened", response.id));
        let seen = plot_ui
            .ctx()
            .data(|d| d.get_temp::<bool>(opened))
            .unwrap_or(false);
        let bounds = if seen && !response.double_clicked() {
            plot_ui.plot_bounds()
        } else {
            let (x, y) = (usable(self.x), usable(self.y));
            let full = PlotBounds::from_min_max([x.0, y.0], [x.1, y.1]);
            plot_ui.set_plot_bounds(full);
            plot_ui.ctx().data_mut(|d| d.insert_temp(opened, true));
            full
        };
        let grid = tokens.colors.plot_grid.color();
        // A little beyond the edges: the bounds are those of the start of the frame.
        let reach = |lo: f64, hi: f64| (lo - (hi - lo) * 0.1, hi + (hi - lo) * 0.1);
        let (xl, xh) = reach(bounds.min()[0], bounds.max()[0]);
        let tone = |log: bool, step: f64| {
            if step >= 1.0 || !log {
                grid
            } else {
                grid.gamma_multiply(0.5)
            }
        };
        for m in marks(self.x_log, xl, xh) {
            let color = tone(self.x_log, m.step_size);
            plot_ui.vline(
                VLine::new("", m.value)
                    .color(color)
                    .width(tokens.stroke.thin),
            );
        }
        let (yl, yh) = reach(bounds.min()[1], bounds.max()[1]);
        for m in marks(self.log, yl, yh) {
            let color = tone(self.log, m.step_size);
            plot_ui.hline(
                HLine::new("", m.value)
                    .color(color)
                    .width(tokens.stroke.thin),
            );
        }
    }
}

pub(crate) fn dot_size(tokens: &Tokens, dot: Dot) -> f32 {
    match dot {
        Dot::Small => tokens.size.marker_small,
        Dot::Medium => tokens.size.marker_medium,
        Dot::Large => tokens.size.marker_large,
    }
}

pub(crate) fn weight(tokens: &Tokens, w: Weight) -> f32 {
    match w {
        Weight::Thin => tokens.stroke.thin,
        Weight::Medium => tokens.stroke.medium,
        Weight::Thick => tokens.stroke.thick,
    }
}

/// The note about points a logarithmic axis cannot show (UX-LOG-02), with the list on a click
/// and the way back. Returns false when nothing can be drawn on the log axis (an empty state is
/// shown instead of an axis).
pub fn log_note(ui: &mut Ui, tokens: &Tokens, id: &str, data: &[Pt], log_axis: &mut bool) -> bool {
    let c = &tokens.colors;
    let view = plotdata::log_view(data);
    if let Some(note) = plotdata::hidden_note(view.hidden.len(), data.len()) {
        egui::CollapsingHeader::new(RichText::new(note).color(c.warning.color()))
            .id_salt(("hidden-points", id))
            .default_open(false)
            .show(ui, |ui| {
                for p in &view.hidden {
                    ui.label(format!(
                        "time {}, concentration {}",
                        fmt::exact(p.time),
                        fmt::exact(p.conc)
                    ));
                }
                ui.label(
                    RichText::new(
                        "Nothing was changed in the data; these points are only left out of the log view.",
                    )
                    .color(c.text_muted.color()),
                );
                if ui.button("Switch to the linear axis").clicked() {
                    *log_axis = false;
                }
            });
    }
    if view.visible.is_empty() {
        ui.add_space(tokens.spacing.large);
        ui.label(
            RichText::new(
                "No point can be shown on a log axis: every concentration is zero or negative. Switch to the linear axis.",
            )
            .color(c.warning.color()),
        );
        return false;
    }
    true
}

/// The concentration profile: points and lines on a linear or semi-log axis. On a log axis the
/// points and line vertices that are zero or negative are left out of this view only. A click
/// near a point of the set `selectable` is returned.
#[allow(clippy::too_many_arguments)]
pub fn profile(
    ui: &mut Ui,
    tokens: &Tokens,
    id: &str,
    labels: (String, String),
    point_sets: &[PointSet],
    line_sets: &[LineSet],
    log: bool,
    selectable: &[usize],
    height: f32,
) -> Option<Clicked> {
    profile_view(
        ui, tokens, id, labels, point_sets, line_sets, log, selectable, height,
    )
    .0
}

/// `profile`, and the transform of the plot: the range it shows (on a log axis the vertical one
/// is in decades), which is what the person panned or zoomed to, or the full range.
#[allow(clippy::too_many_arguments)]
pub(crate) fn profile_view(
    ui: &mut Ui,
    tokens: &Tokens,
    id: &str,
    labels: (String, String),
    point_sets: &[PointSet],
    line_sets: &[LineSet],
    log: bool,
    selectable: &[usize],
    height: f32,
) -> (Option<Clicked>, PlotTransform) {
    let keep = |p: &[f64; 2]| p[0].is_finite() && p[1].is_finite() && (!log || p[1] > 0.0);
    let all: Vec<[f64; 2]> = point_sets
        .iter()
        .map(|s| &s.points)
        .chain(line_sets.iter().map(|s| &s.points))
        .flatten()
        .copied()
        .filter(&keep)
        .collect();
    let all_pts: Vec<Pt> = pts(&all).collect();
    let (x_range, lin) = plotdata::linear_ranges(&all_pts);
    let y_range = match (log, plotdata::log_view(&all_pts).decades) {
        (true, Some(d)) => d,
        _ => lin,
    };
    let axes = Axes::new(x_range, y_range, log);
    let mut clicked: Option<Clicked> = None;
    let plot = axes
        .plot(id, height, labels.0, labels.1)
        .legend(Legend::default());
    let shown = plot.show(ui, |plot_ui| {
        axes.paint_grid(plot_ui, tokens);
        for l in line_sets {
            let line: Vec<[f64; 2]> = l
                .points
                .iter()
                .filter(|p| keep(p))
                .map(|p| [p[0], axes.y_of(p[1])])
                .collect();
            plot_ui.line(
                Line::new(l.name.clone(), PlotPoints::from(line))
                    .color(l.tone.color(tokens))
                    .width(weight(tokens, l.weight)),
            );
        }
        for s in point_sets {
            let shown: Vec<[f64; 2]> = s
                .points
                .iter()
                .filter(|p| keep(p))
                .map(|p| [p[0], axes.y_of(p[1])])
                .collect();
            plot_ui.points(
                Points::new(s.name.clone(), PlotPoints::from(shown))
                    .shape(s.shape)
                    .radius(dot_size(tokens, s.dot))
                    .color(s.tone.color(tokens))
                    .filled(true),
            );
        }
        let response = plot_ui.response().clone();
        if !response.clicked() {
            return;
        }
        let Some(pos) = response.interact_pointer_pos() else {
            return;
        };
        // The clickable points of every selectable set, with where they are on the screen.
        let mut candidates: Vec<(usize, usize, [f64; 2])> = Vec::new();
        for set_index in selectable {
            if let Some(set) = point_sets.get(*set_index) {
                for (index, p) in set.points.iter().copied().enumerate() {
                    if keep(&p) {
                        candidates.push((*set_index, index, p));
                    }
                }
            }
        }
        let screen: Vec<(f64, f64)> = candidates
            .iter()
            .map(|(_, _, p)| {
                let s = plot_ui.screen_from_plot(PlotPoint::new(p[0], axes.y_of(p[1])));
                (f64::from(s.x), f64::from(s.y))
            })
            .collect();
        if let Some(i) = plotdata::nearest(
            &screen,
            (f64::from(pos.x), f64::from(pos.y)),
            f64::from(tokens.size.hit_radius),
        ) {
            if let Some((set, index, point)) = candidates.get(i) {
                clicked = Some(Clicked {
                    set: *set,
                    index: *index,
                    point: *point,
                });
            }
        }
    });
    (clicked, shown.transform)
}

/// A scatter plot of residuals (or any quantity) with a line at zero, on linear axes.
pub fn scatter(
    ui: &mut Ui,
    tokens: &Tokens,
    id: &str,
    labels: (&str, &str),
    points: &[[f64; 2]],
    height: f32,
) {
    let finite: Vec<[f64; 2]> = points
        .iter()
        .copied()
        .filter(|p| p[0].is_finite() && p[1].is_finite())
        .collect();
    if finite.is_empty() {
        ui.label(RichText::new("Nothing to plot.").color(tokens.colors.text_muted.color()));
        return;
    }
    let lo = finite.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
    let hi = finite
        .iter()
        .map(|p| p[0])
        .fold(f64::NEG_INFINITY, f64::max);
    let span = hi - lo;
    let pad_x = if span > 0.0 { span * 0.05 } else { 1.0 };
    let reach = finite
        .iter()
        .map(|p| p[1].abs())
        .fold(0.0_f64, f64::max)
        .max(f64::MIN_POSITIVE);
    let axes = Axes::new(
        (lo - pad_x, hi + pad_x),
        (-reach * 1.25, reach * 1.25),
        false,
    );
    let plot = axes.plot(id, height, labels.0.to_owned(), labels.1.to_owned());
    plot.show(ui, |plot_ui| {
        axes.paint_grid(plot_ui, tokens);
        plot_ui.hline(
            HLine::new("", 0.0)
                .color(Tone::Other.color(tokens))
                .width(tokens.stroke.medium),
        );
        plot_ui.points(
            Points::new("", PlotPoints::from(finite.clone()))
                .shape(MarkerShape::Circle)
                .radius(dot_size(tokens, Dot::Medium))
                .color(Tone::Observed.color(tokens))
                .filled(true),
        );
    });
}
