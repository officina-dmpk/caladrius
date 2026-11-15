//! The results of a fit, summary first: convergence, the estimates with their precision and both
//! confidence intervals, the goodness of fit, and the flags in words. The matrices, eigenvalues,
//! trace, partial derivatives and predicted values are one click away. Every number is the
//! engine's; this file only lays them out.

use egui::{RichText, Ui};

use crate::fit::FitPage;
use crate::fmt;
use crate::model::{FitOk, FitOutcome, WorksheetInfo};
use crate::modelinfo;
use crate::modelpick;
use crate::theme::Tokens;
use crate::widgets::error_box;

/// The derived parameters, in the order shown.
const DERIVED: [&str; 3] = ["cl", "half_life", "auc_inf"];

/// The goodness-of-fit values of a maximum-likelihood fit: label, engine name. The likelihood
/// forms first; the least-squares forms (uniform weights at the estimate) follow, labelled as such,
/// and are never compared with them (`specs/fit.md` FIT-ML-16).
const GOODNESS_ML: [(&str, &str); 12] = [
    ("-2 log-likelihood", "m2ll"),
    ("Observations", "n"),
    ("AIC", "aic_ml"),
    ("BIC", "bic_ml"),
    ("IWRES mean", "iwres_mean"),
    ("IWRES SD", "iwres_sd"),
    ("|IWRES| above 2", "n_iwres_gt2"),
    ("|IWRES| above 3", "n_iwres_gt3"),
    ("Sum of squares (uniform)", "wrss"),
    ("Degrees of freedom", "df"),
    ("AIC, least-squares form", "aic"),
    ("SBC, least-squares form", "sbc"),
];

/// The goodness-of-fit values: label, engine name.
const GOODNESS: [(&str, &str); 7] = [
    ("Weighted sum of squares", "wrss"),
    ("Residual standard deviation S", "s"),
    ("Degrees of freedom", "df"),
    ("Observations", "n"),
    ("Correlation observed/predicted", "corr_obs_pred"),
    ("AIC", "aic"),
    ("SBC", "sbc"),
];

/// The text with its first letter in capitals.
fn sentence_case(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

fn show(x: Option<f64>) -> String {
    x.map_or_else(|| "-".to_owned(), fmt::number)
}

fn interval(lo: Option<f64>, hi: Option<f64>) -> String {
    match (lo, hi) {
        (Some(a), Some(b)) => format!("{} to {}", fmt::number(a), fmt::number(b)),
        _ => "-".to_owned(),
    }
}

/// The results block of the page.
pub fn results(ui: &mut Ui, tokens: &Tokens, page: &FitPage, info: &WorksheetInfo) {
    let c = &tokens.colors;
    let Some(view) = &page.view else {
        ui.label(
            RichText::new("The results appear here once the model has been fitted.")
                .color(c.text_muted.color()),
        );
        return;
    };
    let ok = match &view.outcome {
        FitOutcome::Ok(ok) => ok,
        FitOutcome::Error(message) => {
            error_box(ui, tokens, message);
            return;
        }
        FitOutcome::Missing => {
            ui.label("This analysis has no result yet.");
            return;
        }
    };
    let stale = page.status().is_stale();
    let body = if stale {
        c.text_muted.color()
    } else {
        c.text.color()
    };

    // Convergence first: whether the numbers below can be trusted.
    let converged = ok.status == "converged";
    let status_color = if converged {
        c.ok.color()
    } else {
        c.warning.color()
    };
    let headline = if converged {
        "Converged".to_owned()
    } else {
        let message = if view.status_message.is_empty() {
            fmt::words(&ok.status)
        } else {
            view.status_message.clone()
        };
        sentence_case(&message)
    };
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(headline).strong().color(status_color));
        let iterations = ok.trace.len().saturating_sub(1);
        ui.label(
            RichText::new(format!(
                "after {iterations} iteration{}, {} observations{}",
                if iterations == 1 { "" } else { "s" },
                view.n_observations,
                match (view.n_missing, view.n_blq) {
                    (0, 0) => String::new(),
                    (m, 0) => format!(" ({m} rows without a concentration left out)"),
                    (0, b) => format!(" ({b} rows below the limit of quantification left out)"),
                    (m, b) => format!(
                        " ({m} rows without a concentration and {b} below the limit of quantification left out)"
                    ),
                }
            ))
            .color(c.text_muted.color()),
        );
    });
    ui.add_space(tokens.spacing.small);

    if !view.flag_messages.is_empty() {
        ui.add_space(tokens.spacing.medium);
        tokens.banner_frame(c.panel_alt, c.warning).show(ui, |ui| {
            ui.label(
                RichText::new("Check these")
                    .strong()
                    .color(c.warning.color()),
            );
            for m in &view.flag_messages {
                ui.label(RichText::new(format!("• {m}")).color(c.text.color()));
            }
        });
    }

    estimates(ui, tokens, page, info, ok, body);
    goodness(ui, tokens, ok, body);

    ui.add_space(tokens.spacing.medium);
    details(ui, tokens, ok);
}

fn estimates(
    ui: &mut Ui,
    tokens: &Tokens,
    page: &FitPage,
    info: &WorksheetInfo,
    ok: &FitOk,
    body: egui::Color32,
) {
    let c = &tokens.colors;
    let level = page.option_f64("/confidence_level", crate::fit::defaults::CONFIDENCE_LEVEL);
    let percent = fmt::number(level * 100.0);
    let extravascular = page.input.extravascular();
    let flagged: Vec<&str> = ok.flags.iter().map(|f| f.concerns()).collect();
    egui::Grid::new("estimates-grid")
        .num_columns(6)
        .spacing([tokens.spacing.large, tokens.spacing.small])
        .striped(true)
        .show(ui, |ui| {
            for h in [
                "Parameter".to_owned(),
                "Estimate".to_owned(),
                "SE".to_owned(),
                "CV%".to_owned(),
                format!("{percent}% interval"),
                format!("{percent}% planar"),
            ] {
                ui.label(RichText::new(h).small().strong());
            }
            ui.end_row();
            let names = ok
                .parameters
                .iter()
                .map(String::as_str)
                .chain(DERIVED.into_iter().filter(|n| {
                    ok.value(&format!("estimate.{n}")).is_some()
                        && !ok.parameters.iter().any(|p| p == n)
                }));
            for (i, name) in names.enumerate() {
                if i == ok.parameters.len() {
                    // A thin break between what was fitted and what follows from it.
                    ui.label(RichText::new("Derived").small().color(c.text_muted.color()));
                    for _ in 0..5 {
                        ui.label("");
                    }
                    ui.end_row();
                }
                let unit = modelpick::unit_of(name, Some(info));
                let label = modelinfo::parameter_label(name, extravascular);
                let first = if unit.is_empty() {
                    label
                } else {
                    format!("{label} ({unit})")
                };
                let bad = flagged.contains(&name);
                let value = |kind: &str| ok.value(&format!("{kind}.{name}"));
                ui.label(RichText::new(first).color(body));
                ui.label(RichText::new(show(value("estimate"))).strong().color(body));
                ui.label(RichText::new(show(value("se"))).color(body));
                let cv_color = if bad { c.warning.color() } else { body };
                ui.label(RichText::new(show(value("cv_percent"))).color(cv_color));
                ui.label(RichText::new(interval(value("ci_lo"), value("ci_hi"))).color(body));
                ui.label(
                    RichText::new(interval(value("planar_lo"), value("planar_hi"))).color(body),
                );
                ui.end_row();
            }
        });
}

fn goodness(ui: &mut Ui, tokens: &Tokens, ok: &FitOk, body: egui::Color32) {
    let c = &tokens.colors;
    ui.add_space(tokens.spacing.medium);
    ui.label(RichText::new("Goodness of fit").strong());
    if ok.maximum_likelihood() {
        let model = ok.error_model.as_deref().unwrap_or_default();
        let name = crate::pop::ERROR_MODELS
            .iter()
            .find(|(id, _)| *id == model)
            .map_or(model, |(_, label)| *label);
        ui.label(
            RichText::new(format!(
                "Maximum likelihood, {} error model ({}). The least-squares forms are shown for reference and are not comparable with the likelihood ones.",
                name.to_lowercase(),
                crate::pop::error_formula(model).replace("IPRED", "ŷ")
            ))
            .small()
            .color(c.text_muted.color()),
        );
    }
    egui::Grid::new("goodness-grid")
        .num_columns(4)
        .spacing([tokens.spacing.large, tokens.spacing.small])
        .show(ui, |ui| {
            let rows: &[(&str, &str)] = if ok.maximum_likelihood() {
                &GOODNESS_ML
            } else {
                &GOODNESS
            };
            for (i, (label, key)) in rows.iter().enumerate() {
                if let Some(x) = ok.value(key) {
                    ui.label(RichText::new(*label).color(c.text_muted.color()));
                    let shown = if *key == "corr_obs_pred" {
                        fmt::fit_quality(x)
                    } else {
                        fmt::number(x)
                    };
                    ui.label(RichText::new(shown).strong().color(body));
                    if i % 2 == 1 {
                        ui.end_row();
                    }
                } else if i % 2 == 1 {
                    ui.end_row();
                }
            }
        });
}

// ---- details on demand --------------------------------------------------------------------

fn details(ui: &mut Ui, tokens: &Tokens, ok: &FitOk) {
    for (title, prefix) in [
        ("Variance-covariance matrix", "covariance"),
        ("Correlation matrix", "correlation"),
    ] {
        egui::CollapsingHeader::new(title)
            .id_salt(prefix)
            .default_open(false)
            .show(ui, |ui| matrix(ui, tokens, prefix, ok));
    }
    egui::CollapsingHeader::new("Eigenvalues and condition number")
        .id_salt("eigenvalues")
        .default_open(false)
        .show(ui, |ui| eigenvalues(ui, tokens, ok));
    egui::CollapsingHeader::new(format!("Minimization trace ({} rows)", ok.trace.len()))
        .id_salt("trace")
        .default_open(false)
        .show(ui, |ui| trace(ui, tokens, ok));
    egui::CollapsingHeader::new("Partial derivatives")
        .id_salt("partials")
        .default_open(false)
        .show(ui, |ui| partials(ui, tokens, ok));
    egui::CollapsingHeader::new(format!("Predicted values ({})", ok.observations.len()))
        .id_salt("predicted")
        .default_open(false)
        .show(ui, |ui| predicted(ui, tokens, ok));
}

fn header(ui: &mut Ui, names: &[String]) {
    for h in names {
        ui.label(RichText::new(h).small().strong());
    }
    ui.end_row();
}

fn matrix(ui: &mut Ui, tokens: &Tokens, prefix: &str, ok: &FitOk) {
    if ok.parameters.is_empty() {
        ui.label("Not calculated.");
        return;
    }
    egui::Grid::new(("matrix", prefix))
        .striped(true)
        .spacing([tokens.spacing.large, tokens.spacing.small])
        .show(ui, |ui| {
            let mut head = vec![String::new()];
            head.extend(ok.parameters.iter().cloned());
            header(ui, &head);
            for a in &ok.parameters {
                ui.label(RichText::new(a).small().strong());
                for b in &ok.parameters {
                    ui.label(show(ok.value(&format!("{prefix}.{a}.{b}"))));
                }
                ui.end_row();
            }
        });
}

fn eigenvalues(ui: &mut Ui, tokens: &Tokens, ok: &FitOk) {
    egui::Grid::new("eigen-grid")
        .spacing([tokens.spacing.large, tokens.spacing.small])
        .show(ui, |ui| {
            for i in 1..=ok.parameters.len() {
                ui.label(format!("Eigenvalue {i}"));
                ui.label(show(ok.value(&format!("eigenvalue.{i}"))));
                ui.end_row();
            }
            ui.label("Condition number (correlation matrix)");
            ui.label(show(ok.value("condition_number")));
            ui.end_row();
            ui.label("Condition number (Jacobian)");
            ui.label(show(ok.value("kappa_jacobian")));
            ui.end_row();
        });
}

fn trace(ui: &mut Ui, tokens: &Tokens, ok: &FitOk) {
    egui::ScrollArea::vertical()
        .id_salt("trace-scroll")
        .max_height(tokens.size.list_max_height)
        .show(ui, |ui| {
            egui::Grid::new("trace-grid")
                .striped(true)
                .spacing([tokens.spacing.large, tokens.spacing.small])
                .show(ui, |ui| {
                    let ml = ok.maximum_likelihood();
                    let objective = if ml { "-2LL" } else { "WRSS" };
                    let mut head = vec!["iteration".to_owned(), objective.to_owned()];
                    head.extend(ok.parameters.iter().cloned());
                    head.extend(["damping".to_owned(), "step".to_owned()]);
                    header(ui, &head);
                    for row in &ok.trace {
                        ui.label(row.iteration.to_string());
                        if ml {
                            ui.label(show(row.m2ll));
                        } else {
                            ui.label(fmt::number(row.wrss));
                        }
                        for j in 0..ok.parameters.len() {
                            ui.label(show(row.estimates.get(j).copied()));
                        }
                        ui.label(fmt::number(row.lambda));
                        ui.label(show(row.step));
                        ui.end_row();
                    }
                });
        });
}

fn partials(ui: &mut Ui, tokens: &Tokens, ok: &FitOk) {
    egui::ScrollArea::vertical()
        .id_salt("partials-scroll")
        .max_height(tokens.size.list_max_height)
        .show(ui, |ui| {
            egui::Grid::new("partials-grid")
                .striped(true)
                .spacing([tokens.spacing.large, tokens.spacing.small])
                .show(ui, |ui| {
                    let mut head = vec!["time".to_owned()];
                    // One column per model parameter: the error parameters have none.
                    head.extend(
                        ok.parameters
                            .iter()
                            .take(ok.partials.len())
                            .map(|p| format!("d/d {p}")),
                    );
                    header(ui, &head);
                    for (i, o) in ok.observations.iter().enumerate() {
                        ui.label(fmt::number(o.time));
                        for column in &ok.partials {
                            ui.label(show(column.get(i).copied()));
                        }
                        ui.end_row();
                    }
                });
        });
}

fn predicted(ui: &mut Ui, tokens: &Tokens, ok: &FitOk) {
    egui::ScrollArea::vertical()
        .id_salt("predicted-scroll")
        .max_height(tokens.size.list_max_height)
        .show(ui, |ui| {
            egui::Grid::new("predicted-grid")
                .striped(true)
                .spacing([tokens.spacing.large, tokens.spacing.small])
                .show(ui, |ui| {
                    let ml = ok.maximum_likelihood();
                    let (fifth, sixth) = if ml {
                        ("σ", "IWRES")
                    } else {
                        ("weighted residual", "weight")
                    };
                    header(
                        ui,
                        &[
                            "time".to_owned(),
                            "observed".to_owned(),
                            "predicted".to_owned(),
                            "residual".to_owned(),
                            fifth.to_owned(),
                            sixth.to_owned(),
                        ],
                    );
                    for o in &ok.observations {
                        for x in [o.time, o.observed, o.predicted, o.residual] {
                            ui.label(fmt::number(x));
                        }
                        if ml {
                            ui.label(show(o.sigma));
                            ui.label(show(o.iwres));
                        } else {
                            ui.label(fmt::number(o.weighted_residual));
                            ui.label(fmt::number(o.weight));
                        }
                        ui.end_row();
                    }
                });
        });
}
