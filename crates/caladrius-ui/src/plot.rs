//! The plots, drawn from data the engine gave: points and lines of a concentration profile on a
//! linear or semi-log axis, and scatter plots of residuals. Every curve is passed in; nothing here
//! evaluates a model. The grid is drawn from the `plot_grid` token, the series from the series tokens.

use egui::{Color32, RichText, Ui};
use egui_plot::{
    GridMark, HLine, Legend, Line, MarkerShape, Plot, PlotBounds, PlotPoint, PlotPoints, Points,
    VLine,
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

/// Axis ranges, ticks and the log transform of one plot.
#[derive(Debug, Clone)]
pub struct Axes {
    pub x: (f64, f64),
    pub y: (f64, f64),
    pub log: bool,
    x_ticks: Vec<f64>,
    y_marks: Vec<(f64, bool)>,
    x_step: f64,
    y_step: f64,
}

fn step_of(ticks: &[f64]) -> f64 {
    match ticks {
        [a, b, ..] => (b - a).abs(),
        _ => 1.0,
    }
}

impl Axes {
    /// Axes for these ranges (on a log axis `y` is in decades).
    pub fn new(x: (f64, f64), y: (f64, f64), log: bool) -> Axes {
        let x_ticks = plotdata::nice_ticks(x.0, x.1, 6);
        let y_marks: Vec<(f64, bool)> = if log {
            plotdata::log_ticks(y.0, y.1)
        } else {
            plotdata::nice_ticks(y.0, y.1, 6)
                .into_iter()
                .map(|v| (v, true))
                .collect()
        };
        let y_step = if log {
            1.0
        } else {
            step_of(&y_marks.iter().map(|m| m.0).collect::<Vec<f64>>())
        };
        Axes {
            x_step: step_of(&x_ticks),
            x_ticks,
            y_marks,
            y_step,
            x,
            y,
            log,
        }
    }

    /// The value on the vertical axis for a concentration.
    pub fn y_of(&self, conc: f64) -> f64 {
        if self.log { conc.log10() } else { conc }
    }

    fn plot(&self, id: &str, height: f32, x_label: String, y_label: String) -> Plot<'static> {
        let log = self.log;
        let x_marks: Vec<GridMark> = self
            .x_ticks
            .iter()
            .map(|v| GridMark {
                value: *v,
                step_size: self.x_step,
            })
            .collect();
        let y_grid: Vec<GridMark> = self
            .y_marks
            .iter()
            .map(|(v, major)| GridMark {
                value: *v,
                step_size: if *major {
                    self.y_step
                } else {
                    self.y_step / 5.0
                },
            })
            .collect();
        Plot::new(id.to_owned())
            .height(height)
            .x_axis_label(x_label)
            .y_axis_label(y_label)
            .allow_drag(false)
            .allow_zoom(false)
            .allow_scroll(false)
            .allow_boxed_zoom(false)
            .allow_double_click_reset(false)
            .show_grid(false)
            .x_grid_spacer(move |_| x_marks.clone())
            .y_grid_spacer(move |_| y_grid.clone())
            .x_axis_formatter(|mark, _| fmt::number(mark.value))
            .y_axis_formatter(move |mark, _| {
                if log {
                    if (mark.value - mark.value.round()).abs() < 1e-9 {
                        plotdata::log_label(mark.value.round())
                    } else {
                        String::new()
                    }
                } else {
                    fmt::number(mark.value)
                }
            })
            .label_formatter(move |_, v| {
                let y = if log { 10f64.powf(v.y) } else { v.y };
                format!("x = {}\ny = {}", fmt::number(v.x), fmt::number(y))
            })
    }

    fn paint_grid(&self, plot_ui: &mut egui_plot::PlotUi<'_>, tokens: &Tokens) {
        plot_ui.set_plot_bounds(PlotBounds::from_min_max(
            [self.x.0, self.y.0],
            [self.x.1, self.y.1],
        ));
        let grid = tokens.colors.plot_grid.color();
        for v in &self.x_ticks {
            plot_ui.vline(VLine::new("", *v).color(grid).width(tokens.stroke.thin));
        }
        for (v, major) in &self.y_marks {
            let color = if *major {
                grid
            } else {
                grid.gamma_multiply(0.5)
            };
            plot_ui.hline(HLine::new("", *v).color(color).width(tokens.stroke.thin));
        }
    }
}

fn dot_size(tokens: &Tokens, dot: Dot) -> f32 {
    match dot {
        Dot::Small => tokens.size.marker_small,
        Dot::Medium => tokens.size.marker_medium,
        Dot::Large => tokens.size.marker_large,
    }
}

fn weight(tokens: &Tokens, w: Weight) -> f32 {
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
    plot.show(ui, |plot_ui| {
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
    clicked
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
