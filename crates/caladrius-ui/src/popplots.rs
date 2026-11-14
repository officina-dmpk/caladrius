//! The plots of a population fit, drawn from the engine's answer: the observations against the
//! population and the individual predictions with the identity line, the individual weighted
//! residuals against time and against the individual prediction with the zero line, and one small
//! plot per subject. Nothing is evaluated here: the lines join the predictions the engine gives at
//! the observation times. The plots zoom and pan like the others (`plot::Axes`).

use egui::{RichText, Sense, Shape, Stroke, Ui, vec2};
use egui_plot::{AxisHints, Line, LineStyle, MarkerShape, PlotPoints, Points};

use crate::fmt;
use crate::model::WorksheetInfo;
use crate::plot::{self, Axes, Dot, Weight};
use crate::plotdata::{self, Pt};
use crate::pop::Subject;
use crate::theme::Tokens;

/// `Name (unit)`, or the name alone when the worksheet has no unit for it.
fn with_unit(name: &str, info: Option<&WorksheetInfo>, role: &str) -> String {
    match info.and_then(|i| i.unit_of(role)) {
        Some(unit) => format!("{name} ({unit})"),
        None => name.to_owned(),
    }
}

fn finite(points: Vec<[f64; 2]>) -> Vec<[f64; 2]> {
    points
        .into_iter()
        .filter(|p| p[0].is_finite() && p[1].is_finite())
        .collect()
}

fn caption(ui: &mut Ui, tokens: &Tokens, text: &str) {
    ui.label(
        RichText::new(text)
            .small()
            .color(tokens.colors.text_muted.color()),
    );
}

/// Observed against a prediction, with the line y = x: one range on both axes so the identity is
/// the diagonal; the line runs past the range, so it stays in view when the person pans.
fn identity_plot(
    ui: &mut Ui,
    tokens: &Tokens,
    id: &str,
    labels: (String, String),
    points: &[[f64; 2]],
    height: f32,
) {
    let c = &tokens.colors;
    let shown = finite(points.to_vec());
    let lo = shown
        .iter()
        .flat_map(|p| [p[0], p[1]])
        .fold(f64::INFINITY, f64::min);
    let hi = shown
        .iter()
        .flat_map(|p| [p[0], p[1]])
        .fold(f64::NEG_INFINITY, f64::max);
    if !(lo.is_finite() && hi.is_finite()) {
        ui.label(RichText::new("Nothing to plot.").color(c.text_muted.color()));
        return;
    }
    let lo = lo.min(0.0);
    let span = if hi > lo { hi - lo } else { 1.0 };
    let range = (lo, hi + 0.05 * span);
    let axes = Axes::new(range, range, false);
    axes.plot(id, height, labels.0, labels.1)
        .show(ui, |plot_ui| {
            axes.paint_grid(plot_ui, tokens);
            plot_ui.line(
                Line::new(
                    "",
                    PlotPoints::from(vec![[lo - span, lo - span], [hi + span, hi + span]]),
                )
                .color(c.series_other.color())
                .width(tokens.stroke.medium)
                .style(LineStyle::dashed_loose()),
            );
            plot_ui.points(
                Points::new("", PlotPoints::from(shown))
                    .shape(MarkerShape::Circle)
                    .radius(plot::dot_size(tokens, Dot::Medium))
                    .color(c.series_observed.color())
                    .filled(true),
            );
        });
}

/// The four goodness-of-fit plots, two by two. After FO there is no individual prediction: the
/// three plots that need one give way to a sentence.
pub fn goodness(ui: &mut Ui, tokens: &Tokens, subjects: &[Subject], info: Option<&WorksheetInfo>) {
    let c = &tokens.colors;
    let obs: Vec<_> = subjects.iter().flat_map(|s| &s.observations).collect();
    let observed = with_unit("Observed", info, "concentration");
    let pred_label = with_unit("PRED", info, "concentration");
    let ipred_label = with_unit("IPRED", info, "concentration");
    let time = with_unit("Time since the dose", info, "time");
    let pred: Vec<[f64; 2]> = obs.iter().map(|o| [o.pred, o.y]).collect();
    let ipred: Vec<[f64; 2]> = obs.iter().filter_map(|o| Some([o.ipred?, o.y])).collect();
    let iwres_time: Vec<[f64; 2]> = obs
        .iter()
        .filter_map(|o| Some([o.time, o.iwres?]))
        .collect();
    let iwres_ipred: Vec<[f64; 2]> = obs
        .iter()
        .filter_map(|o| Some([o.ipred?, o.iwres?]))
        .collect();
    let height = tokens.size.plot_min_height;
    let no_individual = |ui: &mut Ui| {
        ui.add_space(tokens.spacing.large);
        ui.label(
            RichText::new(
                "No individual prediction after FO (it fits with η = 0): run FOCE or FOCEI for this plot.",
            )
            .color(c.text_muted.color()),
        );
    };
    ui.columns(2, |uis| {
        if let [left, right] = uis {
            caption(
                left,
                tokens,
                "Observed against population prediction (PRED)",
            );
            identity_plot(
                left,
                tokens,
                "pop-gof-pred",
                (pred_label.clone(), observed.clone()),
                &pred,
                height,
            );
            caption(
                right,
                tokens,
                "Observed against individual prediction (IPRED)",
            );
            if ipred.is_empty() {
                no_individual(right);
            } else {
                identity_plot(
                    right,
                    tokens,
                    "pop-gof-ipred",
                    (ipred_label.clone(), observed.clone()),
                    &ipred,
                    height,
                );
            }
        }
    });
    ui.add_space(tokens.spacing.medium);
    ui.columns(2, |uis| {
        if let [left, right] = uis {
            caption(
                left,
                tokens,
                "Individual weighted residual (IWRES) against time",
            );
            if iwres_time.is_empty() {
                no_individual(left);
            } else {
                plot::scatter(
                    left,
                    tokens,
                    "pop-gof-iwres-time",
                    (&time, "IWRES"),
                    &iwres_time,
                    height,
                );
            }
            caption(right, tokens, "IWRES against individual prediction");
            if iwres_ipred.is_empty() {
                no_individual(right);
            } else {
                plot::scatter(
                    right,
                    tokens,
                    "pop-gof-iwres-ipred",
                    (&ipred_label, "IWRES"),
                    &iwres_ipred,
                    height,
                );
            }
        }
    });
}

/// A short sample of a marker or a line, for the legend.
fn swatch(ui: &mut Ui, tokens: &Tokens, color: egui::Color32, kind: Swatch) {
    let side = tokens.size.row_height;
    let (rect, _) = ui.allocate_exact_size(vec2(side, side), Sense::hover());
    let painter = ui.painter_at(rect);
    let (a, b) = (
        egui::pos2(rect.left(), rect.center().y),
        egui::pos2(rect.right(), rect.center().y),
    );
    match kind {
        Swatch::Point => {
            painter.circle_filled(rect.center(), plot::dot_size(tokens, Dot::Medium), color);
        }
        Swatch::Line(weight) => {
            painter.line_segment([a, b], Stroke::new(plot::weight(tokens, weight), color));
        }
        Swatch::Dashed(weight) => {
            painter.extend(Shape::dashed_line(
                &[a, b],
                Stroke::new(plot::weight(tokens, weight), color),
                tokens.spacing.small,
                tokens.stroke.thick,
            ));
        }
    }
}

#[derive(Clone, Copy)]
enum Swatch {
    Point,
    Line(Weight),
    Dashed(Weight),
}

/// The one legend of the individual plots.
fn legend(ui: &mut Ui, tokens: &Tokens, individual: bool) {
    let c = &tokens.colors;
    ui.horizontal_wrapped(|ui| {
        let entry = |ui: &mut Ui, color: egui::Color32, kind: Swatch, text: &str| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = tokens.spacing.small;
                swatch(ui, tokens, color, kind);
                ui.label(RichText::new(text).color(c.text.color()));
            });
            ui.add_space(tokens.spacing.medium);
        };
        entry(ui, c.series_observed.color(), Swatch::Point, "Observed");
        if individual {
            entry(
                ui,
                c.series_fit.color(),
                Swatch::Line(Weight::Thick),
                "Individual prediction (IPRED)",
            );
        }
        entry(
            ui,
            c.series_other.color(),
            Swatch::Dashed(Weight::Medium),
            "Population prediction (PRED)",
        );
    });
}

/// One small plot per subject, on the same axes, with one legend above them.
pub fn individual(
    ui: &mut Ui,
    tokens: &Tokens,
    subjects: &[Subject],
    info: Option<&WorksheetInfo>,
    log: bool,
) {
    let c = &tokens.colors;
    let keep = |p: &[f64; 2]| p[0].is_finite() && p[1].is_finite() && (!log || p[1] > 0.0);
    let individual = subjects
        .iter()
        .flat_map(|s| &s.observations)
        .any(|o| o.ipred.is_some());
    legend(ui, tokens, individual);
    caption(
        ui,
        tokens,
        &format!(
            "{} against {}; one panel per subject, all on the same axes. Drag to pan, Ctrl and the wheel to zoom, a double click to come back.",
            with_unit("Concentration", info, "concentration"),
            with_unit("time since the dose", info, "time").to_lowercase()
        ),
    );
    let all: Vec<Pt> = subjects
        .iter()
        .flat_map(|s| &s.observations)
        .flat_map(|o| {
            let mut v = vec![[o.time, o.y], [o.time, o.pred]];
            if let Some(i) = o.ipred {
                v.push([o.time, i]);
            }
            v
        })
        .filter(|p| keep(p))
        .map(|p| Pt {
            time: p[0],
            conc: p[1],
            replaced: false,
        })
        .collect();
    if all.is_empty() {
        ui.label(
            RichText::new(if log {
                "No value above zero: nothing to show on a log axis. Switch to the linear axis."
            } else {
                "Nothing to plot."
            })
            .color(c.text_muted.color()),
        );
        return;
    }
    let (x_range, lin) = plotdata::linear_ranges(&all);
    let y_range = match (log, plotdata::log_view(&all).decades) {
        (true, Some(d)) => d,
        _ => lin,
    };
    let axes = Axes::new(x_range, y_range, log);
    let width = tokens.size.subject_plot_min_width.max(1.0);
    let columns = ((ui.available_width() / width).floor() as usize).max(1);
    let height = tokens.size.small_plot_height;
    for chunk in subjects.chunks(columns) {
        ui.columns(columns, |uis| {
            for (subject, col) in chunk.iter().zip(uis.iter_mut()) {
                subject_plot(col, tokens, &axes, subject, log, height);
            }
        });
        ui.add_space(tokens.spacing.small);
    }
}

fn subject_plot(
    ui: &mut Ui,
    tokens: &Tokens,
    axes: &Axes,
    subject: &Subject,
    log: bool,
    height: f32,
) {
    let c = &tokens.colors;
    let keep = |p: &[f64; 2]| p[0].is_finite() && p[1].is_finite() && (!log || p[1] > 0.0);
    let series = |f: &dyn Fn(&crate::pop::Observation) -> Option<f64>| -> Vec<[f64; 2]> {
        subject
            .observations
            .iter()
            .filter_map(|o| Some([o.time, f(o)?]))
            .filter(|p| keep(p))
            .map(|p| [p[0], axes.y_of(p[1])])
            .collect()
    };
    let observed = series(&|o| Some(o.y));
    let pred = series(&|o| Some(o.pred));
    let ipred = series(&|o| o.ipred);
    ui.label(
        RichText::new(format!("Subject {}", subject.id))
            .small()
            .strong()
            .color(c.text.color()),
    );
    ui.add_space(tokens.spacing.small);
    // A narrow plot: the time marks are labelled closer together than egui_plot's default allows.
    let x_axis = AxisHints::new_x()
        .formatter(|mark, _| fmt::number(mark.value))
        .label_spacing(tokens.spacing.xlarge..=tokens.spacing.xlarge + tokens.spacing.large);
    axes.plot(
        &format!("pop-subject-{}", subject.id),
        height,
        String::new(),
        String::new(),
    )
    .custom_x_axes(vec![x_axis])
    .show(ui, |plot_ui| {
        axes.paint_grid(plot_ui, tokens);
        plot_ui.line(
            Line::new("PRED", PlotPoints::from(pred))
                .color(c.series_other.color())
                .width(plot::weight(tokens, Weight::Medium))
                .style(LineStyle::dashed_loose()),
        );
        if !ipred.is_empty() {
            plot_ui.line(
                Line::new("IPRED", PlotPoints::from(ipred))
                    .color(c.series_fit.color())
                    .width(plot::weight(tokens, Weight::Thick)),
            );
        }
        plot_ui.points(
            Points::new("Observed", PlotPoints::from(observed))
                .shape(MarkerShape::Circle)
                .radius(plot::dot_size(tokens, Dot::Medium))
                .color(c.series_observed.color())
                .filled(true),
        );
    });
}
