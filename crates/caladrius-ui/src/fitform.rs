//! The body of the fit page, read from top to bottom: data, model, dose, starting values,
//! weighting and options, the Fit button, then the results (`fitresult`). Every control changes the
//! state of the page and asks for the engine again; the plot beside it always shows the live curve.

use egui::{RichText, Ui};
use serde_json::{Value, json};

use crate::app::Action;
use crate::fit::{CRITERIA, DERIVATIVES, FitPage, WEIGHTINGS, defaults};
use crate::fitresult;
use crate::fmt;
use crate::model::{Table, WorksheetInfo};
use crate::modelpick;
use crate::nca::unit_notes;
use crate::theme::Tokens;
use crate::widgets::{combo, error_box, section, stale_banner, threshold, unit_field, units_line};

fn exact_field(ui: &mut Ui, value: &mut f64, range: std::ops::RangeInclusive<f64>) -> bool {
    let speed = (value.abs() * 0.05).max(1.0e-6);
    ui.add(
        egui::DragValue::new(value)
            .range(range)
            .speed(speed)
            .custom_formatter(|n, _| fmt::exact(n)),
    )
    .changed()
}

/// The page body.
pub fn central(
    ui: &mut Ui,
    tokens: &Tokens,
    page: &mut FitPage,
    info: &WorksheetInfo,
    table: &Table,
    actions: &mut Vec<Action>,
) {
    let c = &tokens.colors;
    let label = page
        .view
        .as_ref()
        .map_or_else(|| "New model fit".to_owned(), |v| v.label.clone());
    ui.label(RichText::new(label).size(tokens.font.title).strong());
    stale_banner(ui, tokens, &page.status(), page.analysis, actions);
    if let Some(error) = &page.error {
        error_box(ui, tokens, error);
    }
    // `regenerate`: the starting values must be generated again (the data, model or dose changed).
    let mut regenerate = false;
    let mut changed = false;

    section(ui, tokens, "Data", |ui| {
        ui.horizontal(|ui| {
            ui.label("Worksheet");
            ui.label(RichText::new(&info.name).strong());
            ui.label(RichText::new(format!("{} rows", info.rows)).color(c.text_muted.color()));
        });
        ui.horizontal(|ui| {
            ui.label("Subject");
            let subjects: Vec<(&str, &str)> = info
                .subjects
                .iter()
                .map(|s| (s.as_str(), s.as_str()))
                .collect();
            if let Some(chosen) = combo(ui, "fit-subject", &page.subject, &subjects) {
                page.subject = chosen;
                regenerate = true;
            }
        });
        if info.unit_warnings.iter().all(|w| w.code != "missing_unit") {
            units_line(ui, tokens, info);
        }
        for line in unit_notes(info) {
            ui.label(RichText::new(line).color(c.warning.color()));
        }
        egui::CollapsingHeader::new(format!("Worksheet data ({} rows, editable)", info.rows))
            .id_salt("fit-worksheet-data")
            .default_open(false)
            .show(ui, |ui| {
                crate::sheet::grid(ui, tokens, info, table, Some(&page.subject), 8, actions);
            });
    });

    section(ui, tokens, "Model and dose", |ui| {
        let mut choice = page.choice();
        if modelpick::picker(ui, tokens, &mut choice, "Fitted") {
            page.set_choice(choice);
            regenerate = true;
        }
        if page.input.has_duration() {
            ui.horizontal(|ui| {
                ui.label("Duration of the input (fixed, not fitted)");
                let time = info.unit_of("time").unwrap_or("");
                if unit_field(ui, &mut page.duration, 1.0e-4..=1.0e6, time) {
                    regenerate = true;
                }
            });
        }
        ui.add_space(tokens.spacing.small);
        ui.horizontal(|ui| {
            ui.label("Dose");
            let from_sheet = page.preview.dose.filter(|_| page.dose_override.is_none());
            let dose_unit = info.unit_of("dose").unwrap_or("");
            let mut override_on = page.dose_override.is_some();
            if let Some(mut dose) = page.dose_override {
                if exact_field(ui, &mut dose, 0.0..=1.0e12) {
                    page.dose_override = Some(dose);
                    regenerate = true;
                }
                ui.label(RichText::new(dose_unit).color(c.text_muted.color()));
            } else if let Some(d) = from_sheet {
                ui.label(RichText::new(format!("{} {dose_unit}", fmt::number(d))).strong());
                ui.label(RichText::new("from the worksheet").color(c.text_muted.color()));
            } else {
                ui.label(
                    RichText::new("none in the worksheet: enter it here to fit a model")
                        .color(c.warning.color()),
                );
            }
            if ui.checkbox(&mut override_on, "enter it here").changed() {
                page.dose_override = override_on.then_some(page.preview.dose.unwrap_or(1.0));
                regenerate = true;
            }
        });
    });

    section(ui, tokens, "Starting values", |ui| {
        starting_values(ui, tokens, page, info, &mut changed, &mut regenerate);
    });

    section(ui, tokens, "Weighting and options", |ui| {
        options_form(ui, tokens, page, &mut changed);
    });

    ui.add_space(tokens.spacing.medium);
    ui.horizontal(|ui| {
        let ready = page.preview.error.is_none() && page.starting_values_set();
        let text = if page.view.is_some() {
            "Fit again"
        } else {
            "Fit the model"
        };
        if ui
            .add_enabled(ready, tokens.primary_button(text))
            .on_disabled_hover_text(
                "Set the starting values first (or fix the message above, if there is one)",
            )
            .clicked()
        {
            actions.push(Action::RunFit);
        }
        ui.label(
            RichText::new("The fit starts from the values above.").color(c.text_muted.color()),
        );
    });

    section(ui, tokens, "Results", |ui| {
        fitresult::results(ui, tokens, page, info);
    });

    if regenerate {
        actions.push(Action::FitChanged { regenerate: true });
    } else if changed {
        actions.push(Action::FitChanged { regenerate: false });
    }
}

fn starting_values(
    ui: &mut Ui,
    tokens: &Tokens,
    page: &mut FitPage,
    info: &WorksheetInfo,
    changed: &mut bool,
    regenerate: &mut bool,
) {
    let c = &tokens.colors;
    if let Some(message) = &page.start_error {
        error_box(ui, tokens, message);
    }
    let automatic = page.automatic_estimates();
    if !automatic {
        // One sentence, and every value is asked for (specs/fit.md OF-08).
        ui.label(
            RichText::new(
                "Starting values cannot be generated from the data for two compartments: set a value for every parameter below.",
            )
            .color(c.text.color()),
        );
    }
    if page.initial.is_empty() && page.start_error.is_none() {
        ui.label(
            RichText::new("No starting values yet: they are generated from the data.")
                .color(c.text_muted.color()),
        );
    } else {
        let rows = modelpick::Rows::of("starting", &page.choice(), Some(info));
        let FitPage {
            initial, pending, ..
        } = &mut *page;
        if modelpick::parameter_rows(ui, tokens, &rows, initial, Some(pending)) {
            *changed = true;
        }
    }
    ui.horizontal(|ui| {
        if automatic {
            if ui.button("Generate from the data").clicked() {
                *regenerate = true;
            }
            ui.label(
                RichText::new("Edit a value and the curve on the plot follows.")
                    .color(c.text_muted.color()),
            );
        } else {
            let waiting = page.pending.len();
            if waiting > 0
                && ui
                    .button("Use the values shown")
                    .on_hover_text(
                        "They come from the worked example of the specification, not from your data",
                    )
                    .clicked()
            {
                page.accept_placeholders();
                *changed = true;
            }
            let hint = if waiting > 0 {
                format!(
                    "{waiting} still to set (shown muted); the curve follows each edit."
                )
            } else {
                "Edit a value and the curve on the plot follows.".to_owned()
            };
            ui.label(RichText::new(hint).color(c.text_muted.color()));
        }
    });
    if let Some(message) = page
        .preview
        .error
        .as_ref()
        .filter(|_| page.start_error.is_none())
    {
        ui.label(RichText::new(message).color(c.warning.color()));
    }
    let mut notes: Vec<String> = Vec::new();
    if let Some(wrss) = page.preview.wrss {
        notes.push(format!(
            "Weighted sum of squares at these values: {}",
            fmt::number(wrss)
        ));
    }
    let time = info.unit_of("time").unwrap_or("");
    if let Some(t) = page.preview.secondary.get("half_life") {
        notes.push(format!("half-life {} {time}", fmt::number(*t)));
    }
    if let (Some(t), Some(cm)) = (
        page.preview.secondary.get("tmax_pred"),
        page.preview.secondary.get("cmax_pred"),
    ) {
        notes.push(format!(
            "peak {} at {} {time}",
            fmt::number(*cm),
            fmt::number(*t)
        ));
    }
    if !notes.is_empty() {
        ui.label(RichText::new(notes.join(";  ")).color(c.text.color()));
    }
}

fn options_form(ui: &mut Ui, tokens: &Tokens, page: &mut FitPage, changed: &mut bool) {
    let c = &tokens.colors;
    ui.horizontal(|ui| {
        ui.label("Weighting");
        if let Some(v) = combo(ui, "weighting", &page.weighting.clone(), &WEIGHTINGS) {
            page.weighting = v;
            *changed = true;
        }
    });
    let predicted = page.weighting.contains("yhat");
    if predicted {
        ui.label(
            RichText::new("Weights from the predictions are recomputed at every iteration.")
                .small()
                .color(c.text_muted.color()),
        );
    }
    egui::CollapsingHeader::new("More options")
        .id_salt("fit-more-options")
        .default_open(false)
        .show(ui, |ui| {
            egui::Grid::new("fit-options-grid")
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label("Maximum iterations");
                    let mut n =
                        page.option_f64("/max_iterations", defaults::MAX_ITERATIONS as f64) as i64;
                    if ui
                        .add(egui::DragValue::new(&mut n).range(1..=1000))
                        .changed()
                    {
                        page.set_option(&["max_iterations"], json!(n));
                        *changed = true;
                    }
                    ui.end_row();

                    ui.label("Convergence criterion");
                    let current = page.option_str("/criterion", "relative_decrease");
                    if let Some(v) = combo(ui, "criterion", &current, &CRITERIA) {
                        page.set_option(&["criterion"], json!(v));
                        *changed = true;
                    }
                    ui.end_row();

                    ui.label("Stop when it falls below");
                    let mut x = page.option_f64("/convergence", defaults::CONVERGENCE);
                    if exact_field(ui, &mut x, 0.0..=0.1) {
                        page.set_option(&["convergence"], json!(x));
                        *changed = true;
                    }
                    ui.end_row();

                    ui.label("Partial derivatives");
                    let current = page.option_str("/derivatives", "forward_difference");
                    if let Some(v) = combo(ui, "derivatives", &current, &DERIVATIVES) {
                        page.set_option(&["derivatives"], json!(v));
                        *changed = true;
                    }
                    ui.end_row();

                    ui.label("Increment of the differences");
                    let mut x = page.option_f64("/increment", defaults::INCREMENT);
                    if exact_field(ui, &mut x, 1.0e-9..=0.1) {
                        page.set_option(&["increment"], json!(x));
                        *changed = true;
                    }
                    ui.end_row();

                    ui.label("Confidence level");
                    let mut x = page.option_f64("/confidence_level", defaults::CONFIDENCE_LEVEL);
                    if exact_field(ui, &mut x, 0.5..=0.9999) {
                        page.set_option(&["confidence_level"], json!(x));
                        *changed = true;
                    }
                    ui.end_row();
                });
            ui.label(
                RichText::new("Quality flags (switch one off to stop checking it):")
                    .color(c.text_muted.color()),
            );
            let limits = [
                (
                    "max_cv_percent",
                    "CV% above",
                    defaults::MAX_CV_PERCENT,
                    0.0..=1000.0,
                    1.0,
                ),
                (
                    "max_abs_correlation",
                    "correlation above",
                    defaults::MAX_ABS_CORRELATION,
                    0.0..=1.0,
                    0.01,
                ),
                (
                    "max_condition_number",
                    "condition number above",
                    defaults::MAX_CONDITION_NUMBER,
                    1.0..=1.0e12,
                    1000.0,
                ),
                (
                    "min_degrees_of_freedom",
                    "fewer degrees of freedom than",
                    defaults::MIN_DEGREES_OF_FREEDOM,
                    0.0..=100.0,
                    1.0,
                ),
            ];
            for (key, label, when_on, range, speed) in limits {
                if let Some(new) = threshold(
                    ui,
                    label,
                    page.threshold(key, when_on),
                    when_on,
                    range,
                    speed,
                ) {
                    let value = new.map_or(Value::Null, |v| {
                        if key == "min_degrees_of_freedom" {
                            json!(v.round() as u64)
                        } else {
                            json!(v)
                        }
                    });
                    page.set_option(&["flags", key], value);
                    *changed = true;
                }
            }
        });
}
