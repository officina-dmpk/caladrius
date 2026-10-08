//! The plots beside the fit page, always visible: the observed points with the live curve of the
//! starting values and, after a fit, the fitted curve; then the residual plots. The curves are the
//! engine's (`model.simulate`, `fit.run`); this file only draws them.

use egui::{RichText, Ui};

use crate::fit::FitPage;
use crate::model::WorksheetInfo;
use crate::plot::{self, Dot, LineSet, PointSet, Tone, Weight};
use crate::plotdata::Pt;
use crate::theme::Tokens;

/// How many residual plots sit under the fit plot.
const RESIDUAL_PLOTS: usize = 3;

pub fn axis_labels(info: &WorksheetInfo) -> (String, String) {
    (
        format!("Time ({})", info.unit_of("time").unwrap_or("time units")),
        format!(
            "Concentration ({})",
            info.unit_of("concentration")
                .unwrap_or("concentration units")
        ),
    )
}

/// The scale switch shared by the plot headers.
pub fn scale_switch(ui: &mut Ui, tokens: &Tokens, title: &str, log_axis: &mut bool) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).size(tokens.font.heading));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.selectable_value(log_axis, true, "Semi-log");
            ui.selectable_value(log_axis, false, "Linear");
        });
    });
}

/// The panel: the fit plot, then the residual plots.
pub fn plot_panel(
    ui: &mut Ui,
    tokens: &Tokens,
    page: &FitPage,
    info: &WorksheetInfo,
    observed: &[[f64; 2]],
    log_axis: &mut bool,
) {
    let c = &tokens.colors;
    scale_switch(ui, tokens, "Observed and predicted", log_axis);
    if page.status().is_stale() {
        ui.label(
            RichText::new("Out of date: the fitted curve is the previous result. Fit again.")
                .color(c.stale.color()),
        );
    }
    let fit_height = (ui.available_height()
        - tokens.size.residual_plot_height * RESIDUAL_PLOTS as f32
        - tokens.size.plot_reserved_height)
        .max(tokens.size.plot_min_height);
    let data: Vec<Pt> = observed
        .iter()
        .map(|p| Pt {
            time: p[0],
            conc: p[1],
            replaced: false,
        })
        .collect();
    egui::ScrollArea::vertical()
        .id_salt("fit-plots")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if observed.is_empty() {
                ui.label(
                    RichText::new(
                        "This subject has no observation with a time and a concentration.",
                    )
                    .color(c.text_muted.color()),
                );
                return;
            }
            if *log_axis && !plot::log_note(ui, tokens, "fit-observed", &data, log_axis) {
                return;
            }
            let point_sets = vec![PointSet::new(
                "Observed",
                observed.to_vec(),
                Tone::Observed,
                Dot::Medium,
            )];
            let mut line_sets = Vec::new();
            if !page.preview.curve.is_empty() {
                // The live curve is the main line until a fit exists; then it is the reference.
                let (tone, weight) = if page.ok().is_some() {
                    (Tone::Other, Weight::Medium)
                } else {
                    (Tone::Start, Weight::Thick)
                };
                line_sets.push(LineSet::new(
                    "Starting values",
                    page.preview.curve.clone(),
                    tone,
                    weight,
                ));
            }
            if let Some(ok) = page.ok() {
                let curve: Vec<[f64; 2]> = ok.curve.iter().map(|p| [p.time, p.conc]).collect();
                line_sets.push(LineSet::new("Fit", curve, Tone::Fit, Weight::Thick));
            }
            let _ = plot::profile(
                ui,
                tokens,
                "fit-plot",
                axis_labels(info),
                &point_sets,
                &line_sets,
                *log_axis,
                &[],
                fit_height,
            );
            residuals(ui, tokens, page, info);
        });
}

fn residuals(ui: &mut Ui, tokens: &Tokens, page: &FitPage, info: &WorksheetInfo) {
    let c = &tokens.colors;
    ui.add_space(tokens.spacing.medium);
    ui.label(RichText::new("Residuals").size(tokens.font.heading));
    let Some(ok) = page.ok() else {
        ui.label(
            RichText::new("The residual plots appear once the model has been fitted.")
                .color(c.text_muted.color()),
        );
        return;
    };
    let time = format!("Time ({})", info.unit_of("time").unwrap_or("time units"));
    let conc = info
        .unit_of("concentration")
        .unwrap_or("concentration units");
    let pairs = |x: &dyn Fn(&crate::model::FitObservation) -> f64,
                 y: &dyn Fn(&crate::model::FitObservation) -> f64|
     -> Vec<[f64; 2]> { ok.observations.iter().map(|o| [x(o), y(o)]).collect() };
    let height = tokens.size.residual_plot_height;
    ui.label(
        RichText::new("Residual against time")
            .small()
            .color(c.text_muted.color()),
    );
    plot::scatter(
        ui,
        tokens,
        "residual-time",
        (&time, &format!("Residual ({conc})")),
        &pairs(&|o| o.time, &|o| o.residual),
        height,
    );
    ui.label(
        RichText::new("Residual against predicted")
            .small()
            .color(c.text_muted.color()),
    );
    plot::scatter(
        ui,
        tokens,
        "residual-predicted",
        (
            &format!("Predicted ({conc})"),
            &format!("Residual ({conc})"),
        ),
        &pairs(&|o| o.predicted, &|o| o.residual),
        height,
    );
    ui.label(
        RichText::new("Weighted residual against time")
            .small()
            .color(c.text_muted.color()),
    );
    plot::scatter(
        ui,
        tokens,
        "residual-weighted",
        (&time, "Weighted resid."),
        &pairs(&|o| o.time, &|o| o.weighted_residual),
        height,
    );
}
