//! The population-fit page, read from top to bottom: the setup (worksheet, model, random effects,
//! error model, method, optional starting values, Run), then the results: one status line, the
//! estimates, the second look in R when the fit is a reference case, the goodness-of-fit plots and
//! the individual fits. Every number is a field of the engine's answer (or of the R outputs of the
//! second look), formatted by `fmt`; this file only lays them out.

use egui::{RichText, Ui};

use crate::app::Action;
use crate::fitplots::scale_switch;
use crate::fmt;
use crate::model::{AnalysisEntry, WorksheetEntry, WorksheetInfo};
use crate::modelinfo;
use crate::modelpick;
use crate::pop::{self, Estimate, PopPage, PopRun, SecondLook, Section, Summary};
use crate::popplots;
use crate::theme::Tokens;
use crate::widgets::{combo, error_box, stale_banner};

/// What the page reads from the project besides its own state.
pub struct Context<'a> {
    /// The analysis in the project tree, once run: its name and whether it is up to date.
    pub entry: Option<&'a AnalysisEntry>,
    /// The worksheet of the page (units, subjects), when it is loaded.
    pub info: Option<&'a WorksheetInfo>,
    pub worksheets: &'a [WorksheetEntry],
}

/// The page body.
pub fn central(
    ui: &mut Ui,
    tokens: &Tokens,
    page: &mut PopPage,
    cx: Context,
    log_axis: &mut bool,
    actions: &mut Vec<Action>,
) {
    let title = cx
        .entry
        .map_or_else(|| "New population fit".to_owned(), |e| e.label.clone());
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).size(tokens.font.title).strong());
        validation_tag(ui, tokens);
    });
    let status = cx.entry.map(|e| e.status.clone()).unwrap_or_default();
    stale_banner(ui, tokens, &status, page.analysis, actions);
    setup(ui, tokens, page, &cx, actions);
    if let Some(error) = &page.error {
        ui.add_space(tokens.spacing.small);
        error_box(ui, tokens, error);
    }
    let PopPage { run, jump, .. } = page;
    match run {
        Some(run) => results(ui, tokens, run, jump, cx.info, log_axis),
        None => {
            ui.add_space(tokens.spacing.medium);
            ui.label(
                RichText::new("The estimates, the goodness of fit and the individual fits appear here once the model has been fitted.")
                    .color(tokens.colors.text_muted.color()),
            );
        }
    }
}

/// The small tag beside the title.
fn validation_tag(ui: &mut Ui, tokens: &Tokens) {
    let c = &tokens.colors;
    tokens
        .tag_frame()
        .show(ui, |ui| {
            ui.label(
                RichText::new("in validation")
                    .small()
                    .color(c.text_muted.color()),
            );
        })
        .response
        .on_hover_text(
            "The population fit is new in this version: its validation against the reference tools is in progress.",
        );
}

// ---- setup -------------------------------------------------------------------------------------

/// One line of the setup, as tall as a control (a combo box, a text field), its content centred:
/// the label of a row and the controls beside it sit on one line.
fn line<R>(ui: &mut Ui, body: impl FnOnce(&mut Ui) -> R) -> R {
    let height =
        ui.text_style_height(&egui::TextStyle::Button) + 2.0 * ui.spacing().button_padding.y;
    ui.horizontal(|ui| {
        ui.set_min_height(height);
        body(ui)
    })
    .inner
}

/// The label of a setup row.
fn row_label(ui: &mut Ui, text: &str) {
    line(ui, |ui| ui.label(text));
}

fn setup(
    ui: &mut Ui,
    tokens: &Tokens,
    page: &mut PopPage,
    cx: &Context,
    actions: &mut Vec<Action>,
) {
    let c = &tokens.colors;
    let muted = c.text_muted.color();
    let extravascular = page.model_info().is_some_and(|m| m.input.extravascular());
    ui.add_space(tokens.spacing.medium);
    egui::Grid::new("pop-setup")
        .num_columns(2)
        .spacing([tokens.spacing.large, tokens.spacing.medium])
        .show(ui, |ui| {
            row_label(ui, "Worksheet");
            line(ui, |ui| {
                let ids: Vec<(String, String)> = cx
                    .worksheets
                    .iter()
                    .map(|w| (w.id.to_string(), w.name.clone()))
                    .collect();
                let choices: Vec<(&str, &str)> = ids
                    .iter()
                    .map(|(id, name)| (id.as_str(), name.as_str()))
                    .collect();
                if let Some(id) = combo(ui, "pop-worksheet", &page.worksheet.to_string(), &choices)
                    .and_then(|id| id.parse::<u64>().ok())
                {
                    page.worksheet = id;
                    actions.push(Action::PopChanged);
                }
                if let Some(w) = cx.worksheets.iter().find(|w| w.id == page.worksheet) {
                    let n = w.subjects.len();
                    ui.label(
                        RichText::new(format!(
                            "{n} subject{}, {} rows",
                            if n == 1 { "" } else { "s" },
                            w.rows
                        ))
                        .color(muted),
                    );
                    if n < 2 {
                        ui.label(
                            RichText::new("a population fit needs several subjects (a subject column)")
                                .color(c.warning.color()),
                        );
                    }
                }
            });
            ui.end_row();

            row_label(ui, "Model");
            line(ui, |ui| {
                let labels: Vec<(&str, String)> =
                    pop::models().map(|m| (m.id, pop::model_label(m))).collect();
                let choices: Vec<(&str, &str)> =
                    labels.iter().map(|(id, l)| (*id, l.as_str())).collect();
                if let Some(id) = combo(ui, "pop-model", &page.model, &choices) {
                    page.set_model(&id);
                }
                ui.label(RichText::new(&page.model).monospace().color(muted));
            });
            ui.end_row();

            row_label(ui, "Random effects");
            line(ui, |ui| {
                for name in page.parameters() {
                    let label = modelinfo::parameter_label(name, extravascular);
                    let mut on = page.random_effects.iter().any(|r| r == name);
                    if !pop::takes_random_effect(name) {
                        // The engine takes no random effect on the lag time or on the duration of
                        // a zero-order input in this version (POP-MOD-02). A stored fit that has
                        // one can still be unticked.
                        let why = if name == "tlag" {
                            "No random effect on the lag time in this version: it is a shared typical value"
                        } else {
                            "No random effect on the duration of the zero-order input in this version: it is a shared typical value"
                        };
                        if ui
                            .add_enabled(on, egui::Checkbox::new(&mut on, label))
                            .on_disabled_hover_text(why)
                            .on_hover_text(why)
                            .changed()
                        {
                            page.set_random_effect(name, on);
                        }
                        continue;
                    }
                    if ui
                        .checkbox(&mut on, label)
                        .on_hover_text(modelinfo::parameter_meaning(name))
                        .changed()
                    {
                        page.set_random_effect(name, on);
                    }
                }
                let shape = if page.kept.get("omega_shape").and_then(|s| s.as_str()) == Some("block") {
                    "log-normal, block Ω (as stored)"
                } else {
                    "log-normal, diagonal Ω"
                };
                ui.label(RichText::new(shape).color(muted));
            });
            ui.end_row();

            row_label(ui, "Error model");
            line(ui, |ui| {
                if let Some(id) = combo(ui, "pop-error-model", &page.error_model, &pop::ERROR_MODELS) {
                    page.set_error_model(&id);
                }
                ui.label(RichText::new(pop::error_formula(&page.error_model)).color(muted));
            });
            ui.end_row();

            row_label(ui, "Method");
            line(ui, |ui| {
                for (id, label) in pop::METHODS {
                    let hover = match id {
                        "foce_i" => "First-order conditional estimation with interaction",
                        "foce" => "First-order conditional estimation, without interaction",
                        _ => "First order: the random effects at 0",
                    };
                    ui.selectable_value(&mut page.method, id.to_owned(), label)
                        .on_hover_text(hover);
                }
            });
            ui.end_row();

            row_label(ui, "Starting values");
            line(ui, |ui| {
                for name in page.parameters() {
                    ui.label(modelinfo::parameter_label(name, extravascular));
                    let text = page.initial.entry(name.to_owned()).or_default();
                    ui.add(
                        egui::TextEdit::singleline(text)
                            .hint_text("auto")
                            .desired_width(tokens.size.short_field_width),
                    );
                    let unit = modelpick::unit_of(name, cx.info);
                    if !unit.is_empty() {
                        ui.label(RichText::new(unit).color(muted));
                    }
                }
                ui.label(RichText::new("optional; empty: the engine's two-stage start").color(muted));
            });
            ui.end_row();
        });
    // Unused fields stay empty in the state, not in the request.
    page.initial.retain(|_, t| !t.is_empty());
    let missing = page.missing_values();
    if !missing.is_empty() {
        let names: Vec<String> = missing
            .iter()
            .map(|n| modelinfo::parameter_label(n, extravascular))
            .collect();
        ui.label(
            RichText::new(format!(
                "Give a starting value for {}: without a random effect, the two-stage start does not compute it.",
                names.join(" and ")
            ))
            .color(c.warning.color()),
        );
    }
    if let Some(kept) = page.kept_summary() {
        ui.label(
            RichText::new(format!(
                "Also sent as stored, set outside this page: {kept}."
            ))
            .color(muted),
        );
    }
    ui.add_space(tokens.spacing.medium);
    ui.horizontal(|ui| {
        let text = if page.run.is_some() {
            "Run again"
        } else {
            "Run the population fit"
        };
        if ui.add(tokens.primary_button(text)).clicked() {
            actions.push(Action::RunPopFit);
        }
        let subjects = cx
            .worksheets
            .iter()
            .find(|w| w.id == page.worksheet)
            .map_or(0, |w| w.subjects.len());
        ui.label(
            RichText::new(format!(
                "Fits the {subjects} subjects together, in one call to pop.fit."
            ))
            .color(muted),
        );
    });
}

// ---- results -----------------------------------------------------------------------------------

/// A section heading the links can scroll to.
fn heading(ui: &mut Ui, tokens: &Tokens, title: &str, me: Section, jump: &mut Option<Section>) {
    ui.add_space(tokens.spacing.large);
    let response = ui.label(
        RichText::new(title)
            .size(tokens.font.heading)
            .color(tokens.colors.text.color()),
    );
    scroll_if(ui, &response, me, jump);
    ui.separator();
}

fn scroll_if(ui: &Ui, response: &egui::Response, me: Section, jump: &mut Option<Section>) {
    if *jump == Some(me) {
        ui.scroll_to_rect(response.rect, Some(egui::Align::TOP));
        *jump = None;
    }
}

/// `-` for a value the engine did not give (null: not calculated, fixed, or after FO).
fn show(x: Option<f64>) -> String {
    x.map_or_else(|| "-".to_owned(), fmt::number)
}

/// The wall time of the call: milliseconds under a second.
fn wall(seconds: f64) -> String {
    if seconds < 1.0 {
        format!("{} ms", (seconds * 1000.0).round())
    } else {
        format!("{} s", fmt::number((seconds * 100.0).round() / 100.0))
    }
}

fn sentence_case(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

fn results(
    ui: &mut Ui,
    tokens: &Tokens,
    run: &PopRun,
    jump: &mut Option<Section>,
    info: Option<&WorksheetInfo>,
    log_axis: &mut bool,
) {
    let c = &tokens.colors;
    let model = modelinfo::by_id(run.model());
    let order = model.map(pop::structural).unwrap_or_default();
    let extravascular = model.is_some_and(|m| m.input.extravascular());
    let s = pop::summary(&run.answer, &order);
    ui.add_space(tokens.spacing.large);
    status_line(ui, tokens, run, &s);
    // Links to the parts below.
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = tokens.spacing.large;
        let mut parts = vec![(Section::Estimates, "Estimates")];
        if run.second_look.is_some() {
            parts.push((Section::SecondLook, "Second look in R"));
        }
        parts.push((Section::Goodness, "Goodness of fit"));
        parts.push((Section::Individual, "Individual fits"));
        for (section, text) in parts {
            if ui
                .link(RichText::new(text).small().color(c.accent.color()))
                .clicked()
            {
                *jump = Some(section);
            }
        }
    });
    for (subject, message) in &s.refused {
        ui.label(
            RichText::new(format!("Subject {subject} left out: {message}"))
                .color(c.warning.color()),
        );
    }
    for note in &s.notes {
        ui.label(RichText::new(note).small().color(c.text_muted.color()));
    }

    heading(ui, tokens, "Estimates", Section::Estimates, jump);
    estimates(ui, tokens, &s, info, extravascular);

    if let Some(look) = &run.second_look {
        heading(ui, tokens, "Second look in R", Section::SecondLook, jump);
        second_look(ui, tokens, &s, look, extravascular);
    }

    heading(ui, tokens, "Goodness of fit", Section::Goodness, jump);
    let subjects = pop::subjects(&run.answer);
    popplots::goodness(ui, tokens, &subjects, info);

    ui.add_space(tokens.spacing.large);
    let anchor = ui.allocate_response(egui::Vec2::ZERO, egui::Sense::hover());
    scroll_if(ui, &anchor, Section::Individual, jump);
    scale_switch(ui, tokens, "Individual fits", log_axis);
    ui.separator();
    popplots::individual(ui, tokens, &subjects, info, *log_axis);
}

fn status_line(ui: &mut Ui, tokens: &Tokens, run: &PopRun, s: &Summary) {
    let c = &tokens.colors;
    let muted = c.text_muted.color();
    let converged = s.status == "converged";
    let (headline, color) = if converged {
        ("Converged".to_owned(), c.ok.color())
    } else {
        (sentence_case(&fmt::words(&s.status)), c.warning.color())
    };
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(headline).strong().color(color));
        let count = |n: Option<u64>, one: &str, many: &str| {
            n.map(|n| format!("{n} {}", if n == 1 { one } else { many }))
        };
        let parts: Vec<String> = [
            Some(pop::method_label(&s.method).to_owned()),
            count(s.n_subjects, "subject", "subjects"),
            count(s.n_observations, "observation", "observations"),
            count(s.iterations, "iteration", "iterations"),
        ]
        .into_iter()
        .flatten()
        .collect();
        ui.label(RichText::new(parts.join("  ·  ")).color(muted));
        if let Some(seconds) = run.wall_seconds {
            ui.label(RichText::new(format!("·  {}", wall(seconds))).color(muted))
                .on_hover_text("How long the call took, measured by this window. It is not part of the analysis, whose seal covers the answer only.");
        }
        if let Some((short, stamp)) = &run.seal {
            ui.label(
                RichText::new(format!("seal {short}"))
                    .small()
                    .monospace()
                    .color(muted),
            )
            .on_hover_text(stamp);
        }
    });
}

/// A grid with a caption, its header row, and its rows.
fn table(
    ui: &mut Ui,
    tokens: &Tokens,
    id: &str,
    caption: &str,
    header: &[&str],
    body: impl FnOnce(&mut Ui),
) {
    ui.label(RichText::new(caption).strong());
    egui::Grid::new(id)
        .num_columns(header.len())
        .spacing([tokens.spacing.large, tokens.spacing.small])
        .striped(true)
        .show(ui, |ui| {
            for h in header {
                ui.label(
                    RichText::new(*h)
                        .small()
                        .color(tokens.colors.text_muted.color()),
                );
            }
            ui.end_row();
            body(ui);
        });
}

/// The label of a parameter with its unit.
fn parameter(name: &str, info: Option<&WorksheetInfo>, extravascular: bool) -> String {
    let label = modelinfo::parameter_label(name, extravascular);
    let unit = modelpick::unit_of(name, info);
    if unit.is_empty() {
        label
    } else {
        format!("{label} ({unit})")
    }
}

/// The label of an error parameter.
fn residual_label(e: &Estimate, info: Option<&WorksheetInfo>) -> String {
    match e.name.as_str() {
        "a" => match info.and_then(|i| i.unit_of("concentration")) {
            Some(unit) => format!("a, additive ({unit})"),
            None => "a, additive".to_owned(),
        },
        "b" => "b, proportional".to_owned(),
        other => other.to_owned(),
    }
}

fn estimates(
    ui: &mut Ui,
    tokens: &Tokens,
    s: &Summary,
    info: Option<&WorksheetInfo>,
    extravascular: bool,
) {
    let c = &tokens.colors;
    let body = c.text.color();
    let typical = |ui: &mut Ui| {
        table(
            ui,
            tokens,
            "pop-typical",
            "Typical values",
            &["Parameter", "Estimate", "SE"],
            |ui| {
                for e in &s.typical {
                    ui.label(parameter(&e.name, info, extravascular));
                    ui.label(RichText::new(show(e.value)).strong().color(body));
                    ui.label(show(e.se));
                    ui.end_row();
                }
            },
        );
    };
    let variability = |ui: &mut Ui| {
        table(
            ui,
            tokens,
            "pop-variability",
            "Between-subject variability",
            &["Random effect", "ω²", "SE", "ω (SD)", "SE", "η-shrinkage"],
            |ui| {
                for v in &s.variability {
                    ui.label(modelinfo::parameter_label(&v.name, extravascular));
                    ui.label(RichText::new(show(v.variance)).strong().color(body));
                    ui.label(show(v.variance_se));
                    ui.label(RichText::new(show(v.sd)).color(body));
                    ui.label(show(v.sd_se));
                    ui.label(show(v.shrinkage));
                    ui.end_row();
                }
            },
        );
    };
    let residual = |ui: &mut Ui| {
        table(
            ui,
            tokens,
            "pop-residual",
            "Residual error",
            &["Parameter", "Estimate", "SE", "ε-shrinkage"],
            |ui| {
                for (i, e) in s.residual.iter().enumerate() {
                    ui.label(residual_label(e, info));
                    ui.label(RichText::new(show(e.value)).strong().color(body));
                    ui.label(show(e.se));
                    ui.label(if i == 0 {
                        show(s.eps_shrinkage)
                    } else {
                        String::new()
                    });
                    ui.end_row();
                }
            },
        );
    };
    let objective = |ui: &mut Ui| {
        let convention = if s.ofv_convention == "no_2pi" {
            "−2 log-likelihood without the constant N·ln(2π)"
        } else {
            "−2 log-likelihood, the constant N·ln(2π) included"
        };
        table(
            ui,
            tokens,
            "pop-objective",
            "Objective",
            &["", "Value"],
            |ui| {
                ui.label("OFV").on_hover_text(convention);
                ui.label(RichText::new(show(s.ofv)).strong().color(body));
                ui.end_row();
                ui.label("AIC");
                ui.label(RichText::new(show(s.aic)).color(body));
                ui.end_row();
                ui.label("BIC");
                ui.label(RichText::new(show(s.bic)).color(body));
                ui.end_row();
            },
        );
    };
    // Side by side when the page is wide enough, else one under the other.
    let wide = ui.available_width() >= tokens.size.plot_panel_min_width * 2.0;
    if wide {
        ui.horizontal_top(|ui| {
            ui.vertical(typical);
            ui.add_space(tokens.spacing.xlarge);
            ui.vertical(variability);
        });
        ui.add_space(tokens.spacing.medium);
        ui.horizontal_top(|ui| {
            ui.vertical(residual);
            ui.add_space(tokens.spacing.xlarge);
            ui.vertical(objective);
        });
    } else {
        typical(ui);
        ui.add_space(tokens.spacing.medium);
        variability(ui);
        ui.add_space(tokens.spacing.medium);
        residual(ui);
        ui.add_space(tokens.spacing.medium);
        objective(ui);
    }
}

fn second_look(ui: &mut Ui, tokens: &Tokens, s: &Summary, look: &SecondLook, extravascular: bool) {
    let c = &tokens.colors;
    let body = c.text.color();
    let ours = format!(
        "Caladrius {} ({})",
        crate::VERSION,
        pop::method_label(&s.method)
    );
    egui::Grid::new("pop-second-look")
        .num_columns(4)
        .spacing([tokens.spacing.xlarge, tokens.spacing.small])
        .striped(true)
        .show(ui, |ui| {
            for h in [
                "",
                ours.as_str(),
                look.nlme.heading.as_str(),
                look.saemix.heading.as_str(),
            ] {
                ui.label(RichText::new(h).small().color(c.text_muted.color()));
            }
            ui.end_row();
            for e in &s.typical {
                ui.label(modelinfo::parameter_label(&e.name, extravascular));
                ui.label(RichText::new(show(e.value)).strong().color(body));
                ui.label(show(look.nlme.theta.get(&e.name).copied()));
                ui.label(show(look.saemix.theta.get(&e.name).copied()));
                ui.end_row();
            }
            for v in &s.variability {
                ui.label(format!(
                    "ω² {}",
                    modelinfo::parameter_label(&v.name, extravascular)
                ));
                ui.label(RichText::new(show(v.variance)).strong().color(body));
                ui.label(show(look.nlme.omega.get(&v.name).copied()));
                ui.label(show(look.saemix.omega.get(&v.name).copied()));
                ui.end_row();
            }
            if let Some(e) = s.residual.first() {
                ui.label(residual_label(e, None));
                ui.label(RichText::new(show(e.value)).strong().color(body));
                ui.label(show(look.nlme.residual));
                ui.label(show(look.saemix.residual));
                ui.end_row();
            }
        });
    ui.label(
        RichText::new(
            "The three use different approximations, and their −2LL are not comparable, so they are not shown.",
        )
        .small()
        .color(c.text_muted.color()),
    )
    .on_hover_text(format!(
        "Reference case {} of oracle/expected/pop/second_look (same model, error model, random effects, subjects and observations).",
        look.case
    ));
}
