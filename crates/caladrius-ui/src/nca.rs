//! The NCA page: one page read from top to bottom (data, route and dose, options, results) with the
//! profile plot always visible beside it. Every control changes the parameters of `nca.run` and the
//! page asks the engine again at once; nothing is computed here.

use egui::{RichText, Ui};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::app::Action;
use crate::fmt;
use crate::model::{
    Candidate, NcaOk, NcaView, Outcome, Status, SubjectResult, Table, WorksheetInfo,
};
use crate::plot::{self, Dot, LineSet, PointSet, Tone, Weight};
use crate::plotdata::{self, Pt};
use crate::theme::Tokens;
use crate::widgets::{combo, error_box, section, threshold, units_line};

/// The route of administration as the page offers it.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteChoice {
    #[default]
    Extravascular,
    IvBolus,
    IvInfusion,
}

impl RouteChoice {
    fn label(self) -> &'static str {
        match self {
            RouteChoice::Extravascular => "Extravascular (oral, other)",
            RouteChoice::IvBolus => "IV bolus",
            RouteChoice::IvInfusion => "IV infusion",
        }
    }
}

/// The state of the NCA page: what the person chose, as data (serializable). The last answer of
/// the engine is a cache and is not saved.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NcaPage {
    pub analysis: Option<u64>,
    pub worksheet: u64,
    pub subject: String,
    pub route: RouteChoice,
    pub infusion_duration: f64,
    pub dose_override: Option<f64>,
    /// The options of the analysis, as the engine returned them after the last run; empty before.
    pub options: Value,
    #[serde(skip)]
    pub view: Option<NcaView>,
    #[serde(skip)]
    pub error: Option<String>,
}

impl NcaPage {
    pub fn new(worksheet: u64, subject: String) -> NcaPage {
        NcaPage {
            analysis: None,
            worksheet,
            subject,
            route: RouteChoice::default(),
            infusion_duration: 1.0,
            dose_override: None,
            options: Value::Null,
            view: None,
            error: None,
        }
    }

    /// The page of an existing analysis, read from its stored options.
    pub fn from_view(view: &Value) -> Option<NcaPage> {
        let v = NcaView::from_value(view)?;
        let spec = &v.spec;
        let route = match spec.get("route") {
            Some(Value::String(s)) if s == "iv_bolus" => (RouteChoice::IvBolus, 1.0),
            Some(r) if r.get("iv_infusion").is_some() => (
                RouteChoice::IvInfusion,
                r.pointer("/iv_infusion/duration")
                    .and_then(Value::as_f64)
                    .unwrap_or(1.0),
            ),
            _ => (RouteChoice::Extravascular, 1.0),
        };
        Some(NcaPage {
            analysis: Some(v.id),
            worksheet: spec.get("worksheet").and_then(Value::as_u64).unwrap_or(0),
            subject: spec
                .get("subject")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            route: route.0,
            infusion_duration: route.1,
            dose_override: spec.get("dose").and_then(Value::as_f64),
            options: spec.get("options").cloned().unwrap_or(Value::Null),
            view: Some(v),
            error: None,
        })
    }

    /// The parameters of `nca.run` for what the page holds.
    pub fn params(&self) -> Value {
        let route = match self.route {
            RouteChoice::Extravascular => json!("extravascular"),
            RouteChoice::IvBolus => json!("iv_bolus"),
            RouteChoice::IvInfusion => {
                json!({ "iv_infusion": { "duration": self.infusion_duration } })
            }
        };
        let mut params = json!({
            "worksheet": self.worksheet,
            "subject": self.subject,
            "route": route,
            "dose": self.dose_override,
        });
        if let Some(id) = self.analysis {
            params["analysis"] = json!(id);
        }
        if self.options.is_object() {
            params["options"] = self.options.clone();
        }
        params
    }

    /// Takes the engine's answer to a run.
    pub fn adopt(&mut self, view: &Value) {
        if let Some(v) = NcaView::from_value(view) {
            self.analysis = Some(v.id);
            if let Some(options) = v.spec.get("options") {
                self.options = options.clone();
            }
            self.view = Some(v);
            self.error = None;
        }
    }

    fn ok(&self) -> Option<(&SubjectResult, &NcaOk)> {
        let s = self.view.as_ref()?.subject.as_ref()?;
        match &s.outcome {
            Outcome::Ok(ok) => Some((s, ok)),
            _ => None,
        }
    }

    fn status(&self) -> Status {
        self.view
            .as_ref()
            .map(|v| v.status.clone())
            .unwrap_or_default()
    }

    fn option_str(&self, path: &str) -> Option<&str> {
        self.options.pointer(path).and_then(Value::as_str)
    }

    /// Sets a value in the options, creating the objects on the way.
    fn set_option(&mut self, path: &[&str], value: Value) {
        if !self.options.is_object() {
            self.options = json!({});
        }
        let mut node = &mut self.options;
        for (i, key) in path.iter().enumerate() {
            if !node.is_object() {
                *node = json!({});
            }
            let Some(map) = node.as_object_mut() else {
                return;
            };
            if i + 1 == path.len() {
                map.insert((*key).to_owned(), value);
                return;
            }
            node = map.entry((*key).to_owned()).or_insert_with(|| json!({}));
        }
    }

    /// The sample times of the terminal phase the engine used, or the ones chosen by hand.
    pub fn used_times(&self) -> Vec<f64> {
        if let Some(times) = self
            .options
            .pointer("/lambda_z_selection/manual/times")
            .and_then(Value::as_array)
        {
            return times.iter().filter_map(Value::as_f64).collect();
        }
        let Some((_, ok)) = self.ok() else {
            return Vec::new();
        };
        let Some(sel) = ok.selected() else {
            return Vec::new();
        };
        ok.profile
            .iter()
            .filter(|p| {
                p.is_sample()
                    && p.conc > 0.0
                    && !p.is_replaced()
                    && p.time >= sel.time_first
                    && p.time <= sel.time_last
            })
            .map(|p| p.time)
            .collect()
    }

    /// A click on the point at `time`: it joins the hand-picked terminal phase or leaves it. The
    /// terminal phase keeps at least two points; the error says so and nothing changes.
    pub fn toggle_point(&mut self, time: f64) -> Result<(), String> {
        let next = plotdata::toggle_time(&self.used_times(), time);
        if next.len() < 2 {
            return Err(
                "The terminal phase needs at least 2 points: keep one more selected before removing this one."
                    .to_owned(),
            );
        }
        self.set_option(&["lambda_z_selection", "manual"], json!({ "times": next }));
        Ok(())
    }

    /// True when the terminal phase was chosen by hand.
    pub fn manual(&self) -> bool {
        self.options
            .pointer("/lambda_z_selection/manual")
            .is_some_and(|m| !m.is_null())
    }
}

// ---- small controls ----------------------------------------------------------------------

// ---- the page ----------------------------------------------------------------------------

/// The page body: data, route and dose, options, results.
pub fn central(
    ui: &mut Ui,
    tokens: &Tokens,
    page: &mut NcaPage,
    info: &WorksheetInfo,
    table: &Table,
    actions: &mut Vec<Action>,
) {
    let c = &tokens.colors;
    let label = page
        .view
        .as_ref()
        .map_or_else(|| "New NCA".to_owned(), |v| v.label.clone());
    ui.label(RichText::new(label).size(tokens.font.title).strong());
    let status = page.status();
    if let Some(sentence) = status.sentence() {
        tokens
            .banner_frame(c.stale_background, c.stale)
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(sentence).strong().color(c.stale.color()));
                    if let Some(id) = page.analysis {
                        if ui.add(tokens.primary_button("Run again")).clicked() {
                            actions.push(Action::RunAgain(id));
                        }
                    }
                });
            });
    }
    if let Some(error) = &page.error {
        error_box(ui, tokens, error);
    }
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
            if let Some(chosen) = combo(ui, "subject", &page.subject, &subjects) {
                page.subject = chosen;
                // A hand-picked terminal phase belongs to one subject.
                page.set_option(&["lambda_z_selection", "manual"], Value::Null);
                changed = true;
            }
        });
        if info.unit_warnings.iter().all(|w| w.code != "missing_unit") {
            units_line(ui, tokens, info);
        }
        for line in unit_notes(info) {
            ui.label(RichText::new(line).color(c.warning.color()));
        }
        egui::CollapsingHeader::new(format!("Worksheet data ({} rows, editable)", info.rows))
            .id_salt("worksheet-data")
            .default_open(false)
            .show(ui, |ui| {
                crate::sheet::grid(ui, tokens, info, table, Some(&page.subject), 8, actions);
            });
    });

    section(ui, tokens, "Route and dose", |ui| {
        ui.horizontal(|ui| {
            ui.label("Route");
            let before = page.route;
            egui::ComboBox::from_id_salt("route")
                .selected_text(page.route.label())
                .show_ui(ui, |ui| {
                    for r in [
                        RouteChoice::Extravascular,
                        RouteChoice::IvBolus,
                        RouteChoice::IvInfusion,
                    ] {
                        ui.selectable_value(&mut page.route, r, r.label());
                    }
                });
            if page.route != before {
                changed = true;
            }
            if page.route == RouteChoice::IvInfusion {
                ui.label("duration");
                let time = info.unit_of("time").unwrap_or("time units");
                if ui
                    .add(
                        egui::DragValue::new(&mut page.infusion_duration)
                            .range(0.0001..=1.0e6)
                            .speed(0.05)
                            .suffix(format!(" {time}")),
                    )
                    .changed()
                {
                    changed = true;
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label("Dose");
            let from_sheet = page
                .view
                .as_ref()
                .and_then(|v| v.subject.as_ref())
                .and_then(|s| s.dose);
            let mut override_on = page.dose_override.is_some();
            let dose_unit = info.unit_of("dose").unwrap_or("");
            if override_on {
                let mut dose = page.dose_override.unwrap_or(0.0);
                if ui
                    .add(
                        egui::DragValue::new(&mut dose)
                            .range(0.0..=1.0e12)
                            .speed(0.5)
                            .suffix(format!(" {dose_unit}")),
                    )
                    .changed()
                {
                    page.dose_override = Some(dose);
                    changed = true;
                }
            } else if let Some(d) = from_sheet {
                ui.label(RichText::new(format!("{} {dose_unit}", fmt::number(d))).strong());
                ui.label(RichText::new("from the worksheet").color(c.text_muted.color()));
            } else {
                ui.label(
                    RichText::new(
                        "none in the worksheet: clearance and volumes are not calculated",
                    )
                    .color(c.warning.color()),
                );
            }
            if ui.checkbox(&mut override_on, "enter it here").changed() {
                page.dose_override = override_on.then_some(from_sheet.unwrap_or(1.0));
                changed = true;
            }
        });
    });

    section(ui, tokens, "Options", |ui| {
        options_form(ui, tokens, page, &mut changed);
    });

    section(ui, tokens, "Results", |ui| {
        results(ui, tokens, page, info, actions)
    });

    if changed {
        actions.push(Action::RunNca);
    }
}

/// The unit warnings as sentences: the columns without a unit are named in one line.
pub fn unit_notes(info: &WorksheetInfo) -> Vec<String> {
    let mut missing: Vec<&str> = Vec::new();
    let mut lines = Vec::new();
    for w in &info.unit_warnings {
        if w.code == "missing_unit" {
            let what = ["time", "concentration", "dose"]
                .into_iter()
                .find(|what| w.message.starts_with(&format!("the {what} ")));
            if let Some(what) = what {
                missing.push(what);
                continue;
            }
        }
        lines.push(format!("Check the units: {}", w.message));
    }
    if !missing.is_empty() {
        lines.insert(
            0,
            format!(
                "Units are not set for the {} column{}: give them in the worksheet so that AUC, clearance and volume get their units.",
                missing.join(", "),
                if missing.len() == 1 { "" } else { "s" }
            ),
        );
    }
    lines
}

fn options_form(ui: &mut Ui, tokens: &Tokens, page: &mut NcaPage, changed: &mut bool) {
    let c = &tokens.colors;
    if !page.options.is_object() {
        ui.label(
            RichText::new("The options appear after the first run.").color(c.text_muted.color()),
        );
        return;
    }
    egui::Grid::new("options-grid")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label("AUC method");
            let current = page
                .option_str("/auc_method")
                .unwrap_or("linear")
                .to_owned();
            if let Some(v) = combo(
                ui,
                "auc_method",
                &current,
                &[
                    ("linear", "Linear trapezoid"),
                    ("lin_up_log_down", "Linear up, log down"),
                    ("lin_log", "Linear to Tmax, log after"),
                ],
            ) {
                page.set_option(&["auc_method"], json!(v));
                *changed = true;
            }
            ui.end_row();

            ui.label("Terminal phase");
            ui.horizontal(|ui| {
                let min_points = page
                    .options
                    .pointer("/lambda_z/min_points")
                    .and_then(Value::as_u64)
                    .unwrap_or(3);
                let mut n = min_points as i64;
                ui.label("at least");
                if ui.add(egui::DragValue::new(&mut n).range(2..=30)).changed() {
                    page.set_option(&["lambda_z", "min_points"], json!(n));
                    *changed = true;
                }
                ui.label("points");
                let mut allow = page
                    .options
                    .pointer("/lambda_z/allow_tmax")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                if ui.checkbox(&mut allow, "may include Tmax").changed() {
                    page.set_option(&["lambda_z", "allow_tmax"], json!(allow));
                    *changed = true;
                }
            });
            ui.end_row();
        });
    ui.label(
        RichText::new("Quality flags (switch one off to stop checking it):")
            .color(c.text_muted.color()),
    );
    let get = |page: &NcaPage, key: &str| {
        page.options
            .pointer(&format!("/quality/{key}"))
            .and_then(Value::as_f64)
    };
    let limits = [
        (
            "min_adj_r_squared",
            "adjusted R² at least",
            0.9,
            0.0..=1.0,
            0.01,
        ),
        ("min_span_ratio", "span of at least", 2.0, 0.0..=20.0, 0.1),
        (
            "max_extrapolated_percent",
            "extrapolated AUC at most (%)",
            20.0,
            0.0..=100.0,
            0.5,
        ),
        ("min_points", "points of at least", 3.0, 2.0..=30.0, 1.0),
    ];
    for (key, label, when_on, range, speed) in limits {
        if let Some(new) = threshold(ui, label, get(page, key), when_on, range, speed) {
            let value = new.map_or(Value::Null, |v| {
                if key == "min_points" {
                    json!(v.round() as u64)
                } else {
                    json!(v)
                }
            });
            page.set_option(&["quality", key], value);
            *changed = true;
        }
    }
    egui::CollapsingHeader::new("More options")
        .id_salt("more-options")
        .default_open(false)
        .show(ui, |ui| {
            egui::Grid::new("more-grid").num_columns(2).show(ui, |ui| {
                ui.label("Concentration at the dose time");
                let current = page.option_str("/start").unwrap_or("c0").to_owned();
                if let Some(v) = combo(
                    ui,
                    "start",
                    &current,
                    &[
                        ("c0", "Estimate it (C0 for IV bolus, 0 otherwise)"),
                        ("zero", "Zero"),
                        ("none", "Leave it out"),
                    ],
                ) {
                    page.set_option(&["start"], json!(v));
                    *changed = true;
                }
                ui.end_row();
                ui.label("Negative concentrations");
                let current = page.option_str("/negative").unwrap_or("error").to_owned();
                if let Some(v) = combo(
                    ui,
                    "negative",
                    &current,
                    &[
                        ("error", "Stop with an error"),
                        ("allow", "Keep them"),
                        ("set_zero", "Replace by zero"),
                    ],
                ) {
                    page.set_option(&["negative"], json!(v));
                    *changed = true;
                }
                ui.end_row();
                for (key, label) in [
                    (
                        "first",
                        "Values below the limit before the first quantified one",
                    ),
                    ("middle", "Values below the limit between quantified ones"),
                    (
                        "last",
                        "Values below the limit after the last quantified one",
                    ),
                ] {
                    ui.label(label);
                    let path = format!("/blq/position/{key}");
                    match page
                        .options
                        .pointer(&path)
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                    {
                        Some(current) => {
                            if let Some(v) = combo(
                                ui,
                                &format!("blq-{key}"),
                                &current,
                                &[("keep", "Keep as zero"), ("drop", "Leave out")],
                            ) {
                                page.set_option(&["blq", "position", key], json!(v));
                                *changed = true;
                            }
                        }
                        None => {
                            ui.label(
                                RichText::new("set to a value (see the command's options)")
                                    .color(c.text_muted.color()),
                            );
                        }
                    }
                    ui.end_row();
                }
            });
            let mut exclude = page
                .options
                .pointer("/lambda_z_selection/exclude_replaced")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if ui
                .checkbox(
                    &mut exclude,
                    "Keep replaced values out of the terminal phase",
                )
                .changed()
            {
                page.set_option(&["lambda_z_selection", "exclude_replaced"], json!(exclude));
                *changed = true;
            }
        });
}

// ---- results -----------------------------------------------------------------------------

const SUMMARY: [&str; 12] = [
    "cmax",
    "tmax",
    "auclast",
    "aucinf.obs",
    "aucpext.obs",
    "lambda.z",
    "half.life",
    "adj.r.squared",
    "span.ratio",
    "cl.obs",
    "vz.obs",
    "mrt.obs",
];

fn results(
    ui: &mut Ui,
    tokens: &Tokens,
    page: &mut NcaPage,
    info: &WorksheetInfo,
    actions: &mut Vec<Action>,
) {
    let c = &tokens.colors;
    let stale = page.status().is_stale();
    let Some(view) = &page.view else {
        ui.label("Nothing to show yet.");
        return;
    };
    let Some(subject) = &view.subject else {
        ui.label("This analysis has no result yet.");
        return;
    };
    let ok = match &subject.outcome {
        Outcome::Ok(ok) => ok,
        Outcome::Error(message) => {
            error_box(ui, tokens, message);
            if page.manual() {
                ui.label(
                    RichText::new("The terminal phase is chosen by hand; the choice may be what the engine refuses.")
                        .color(c.text_muted.color()),
                );
                if ui.button("Back to automatic selection").clicked() {
                    page.set_option(&["lambda_z_selection", "manual"], Value::Null);
                    actions.push(Action::RunNca);
                }
            }
            return;
        }
        Outcome::Missing => {
            ui.label("No result.");
            return;
        }
    };
    let body_color = if stale {
        c.text_muted.color()
    } else {
        c.text.color()
    };
    let extravascular = page.route == RouteChoice::Extravascular;
    let time_unit = info.unit_of("time");
    let conc_unit = info.unit_of("concentration");

    // The terminal phase window.
    if let Some(sel) = ok.selected() {
        let how = if page.manual() {
            "chosen by hand"
        } else {
            "chosen automatically"
        };
        ui.label(
            RichText::new(format!(
                "Terminal phase: {} points from t = {} to {}{}, {how}.",
                sel.n_points,
                fmt::number(sel.time_first),
                fmt::number(sel.time_last),
                time_unit.map(|u| format!(" {u}")).unwrap_or_default(),
            ))
            .color(body_color),
        );
    } else {
        ui.label(
            RichText::new(
                "No terminal phase could be chosen: λz and what depends on it are not calculated.",
            )
            .color(c.warning.color()),
        );
    }
    ui.add_space(tokens.spacing.small);

    // The summary: the headline parameters, each with its flag marker.
    let flagged: Vec<&str> = ok.flags.iter().map(|f| f.concerns()).collect();
    egui::Grid::new("summary-grid")
        .num_columns(3)
        .spacing([tokens.spacing.large, tokens.spacing.small])
        .striped(true)
        .show(ui, |ui| {
            for name in SUMMARY.iter().chain(if page.route == RouteChoice::IvBolus {
                ["c0", "vss.iv.obs"].iter()
            } else {
                [].iter()
            }) {
                let Some(value) = ok.get(name) else { continue };
                let label = fmt::parameter_label(name, extravascular);
                let flag = flagged.contains(name);
                ui.label(RichText::new(label).color(body_color));
                match (value.value, &value.not_calculated) {
                    (Some(x), _) => {
                        let shown = if name.contains("r.squared") {
                            fmt::fit_quality(x)
                        } else {
                            fmt::number(x)
                        };
                        let text = RichText::new(shown).strong().color(if flag {
                            c.warning.color()
                        } else {
                            body_color
                        });
                        ui.label(text);
                        let unit =
                            fmt::parameter_unit(name, time_unit, conc_unit, &info.derived_units);
                        ui.label(
                            RichText::new(unit.unwrap_or_default()).color(c.text_muted.color()),
                        );
                    }
                    (None, reason) => {
                        let reason = reason.clone().unwrap_or_default();
                        let sentence = subject
                            .not_calculated_messages
                            .get(*name)
                            .cloned()
                            .unwrap_or_else(|| fmt::words(&reason));
                        ui.label(RichText::new("not calculated").color(c.text_muted.color()))
                            .on_hover_text(sentence.clone());
                        ui.label(
                            RichText::new(fmt::words(&reason))
                                .small()
                                .color(c.text_muted.color()),
                        )
                        .on_hover_text(sentence);
                    }
                }
                ui.end_row();
            }
        });

    // Flags: what to check, in words.
    if !subject.flag_messages.is_empty() {
        ui.add_space(tokens.spacing.medium);
        tokens.banner_frame(c.panel_alt, c.warning).show(ui, |ui| {
            ui.label(
                RichText::new("Check these")
                    .strong()
                    .color(c.warning.color()),
            );
            for m in &subject.flag_messages {
                ui.label(RichText::new(format!("• {}", fmt::with_labels(m))).color(c.text.color()));
            }
        });
    }

    // Details on demand.
    ui.add_space(tokens.spacing.medium);
    egui::CollapsingHeader::new("All parameters")
        .id_salt("all-params")
        .default_open(false)
        .show(ui, |ui| {
            egui::Grid::new("all-params-grid")
                .striped(true)
                .num_columns(3)
                .show(ui, |ui| {
                    for p in &ok.parameters {
                        ui.label(&p.name);
                        match (p.value.value, &p.value.not_calculated) {
                            (Some(x), _) => ui.label(fmt::number(x)),
                            (None, r) => ui.label(
                                RichText::new(fmt::words(&r.clone().unwrap_or_default()))
                                    .color(c.text_muted.color()),
                            ),
                        };
                        ui.label(
                            RichText::new(
                                fmt::parameter_unit(
                                    &p.name,
                                    time_unit,
                                    conc_unit,
                                    &info.derived_units,
                                )
                                .unwrap_or_default(),
                            )
                            .color(c.text_muted.color()),
                        );
                        ui.end_row();
                    }
                });
        });
    egui::CollapsingHeader::new(format!("Points of the profile ({})", ok.profile.len()))
        .id_salt("profile-points")
        .default_open(false)
        .show(ui, |ui| {
            egui::Grid::new("profile-grid")
                .striped(true)
                .show(ui, |ui| {
                    ui.label(RichText::new("time").strong());
                    ui.label(RichText::new("concentration").strong());
                    ui.label(RichText::new("origin").strong());
                    ui.end_row();
                    for p in &ok.profile {
                        ui.label(fmt::exact(p.time));
                        ui.label(fmt::exact(p.conc));
                        ui.label(fmt::words(&p.origin));
                        ui.end_row();
                    }
                });
        });
    if !ok.removed.is_empty() {
        egui::CollapsingHeader::new(format!("Points left out ({})", ok.removed.len()))
            .id_salt("removed-points")
            .default_open(false)
            .show(ui, |ui| {
                for r in &ok.removed {
                    ui.label(format!(
                        "row {} at time {}: {}",
                        r.index + 1,
                        fmt::exact(r.time),
                        fmt::words(&r.reason)
                    ));
                }
            });
    }
}

// ---- the plot ----------------------------------------------------------------------------

fn profile_points(ok: &NcaOk) -> Vec<Pt> {
    ok.profile
        .iter()
        .filter(|p| p.is_sample())
        .map(|p| Pt {
            time: p.time,
            conc: p.conc,
            replaced: p.is_replaced(),
        })
        .collect()
}

/// The plot panel: the profile (linear or semi-log), the terminal-phase line, the points used,
/// the note about points a log axis cannot show, and the candidate fits. Always visible.
pub fn plot_panel(
    ui: &mut Ui,
    tokens: &Tokens,
    page: &mut NcaPage,
    info: &WorksheetInfo,
    log_axis: &mut bool,
    actions: &mut Vec<Action>,
) {
    let c = &tokens.colors;
    ui.horizontal(|ui| {
        ui.label(RichText::new("Concentration over time").size(tokens.font.heading));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.selectable_value(log_axis, true, "Semi-log");
            ui.selectable_value(log_axis, false, "Linear");
        });
    });
    if page.status().is_stale() {
        ui.label(
            RichText::new(
                "Out of date: this plot shows the previous result. Run the analysis again.",
            )
            .color(c.stale.color()),
        );
    }
    let Some((_, ok)) = page.ok().map(|(s, o)| (s.clone(), o.clone())) else {
        ui.add_space(tokens.spacing.large);
        let text = match page
            .view
            .as_ref()
            .and_then(|v| v.subject.as_ref())
            .map(|s| &s.outcome)
        {
            Some(Outcome::Error(message)) => {
                format!("The analysis could not run, so there is no profile to draw: {message}")
            }
            _ => "The profile appears here once the analysis has run.".to_owned(),
        };
        ui.label(RichText::new(text).color(c.text_muted.color()));
        return;
    };
    let points = profile_points(&ok);
    let used = page.used_times();
    let labels = (
        format!("Time ({})", info.unit_of("time").unwrap_or("time units")),
        format!(
            "Concentration ({})",
            info.unit_of("concentration")
                .unwrap_or("concentration units")
        ),
    );
    if *log_axis && !plot::log_note(ui, tokens, "profile", &points, log_axis) {
        return;
    }
    let log = *log_axis;
    let as_pairs = |keep: &dyn Fn(&Pt) -> bool| -> Vec<[f64; 2]> {
        points
            .iter()
            .filter(|p| keep(p))
            .map(|p| [p.time, p.conc])
            .collect()
    };
    let point_sets = vec![
        PointSet::new(
            "Observed",
            as_pairs(&|p| !p.replaced && !used.contains(&p.time)),
            Tone::Observed,
            Dot::Small,
        ),
        PointSet::new(
            "Replaced value",
            as_pairs(&|p| p.replaced),
            Tone::Replaced,
            Dot::Medium,
        )
        .shaped(egui_plot::MarkerShape::Diamond),
        PointSet::new(
            "Used for λz",
            as_pairs(&|p| used.contains(&p.time)),
            Tone::Selected,
            Dot::Large,
        ),
    ];
    let mut line_sets = vec![LineSet::new(
        "Profile",
        as_pairs(&|_| true),
        Tone::Other,
        Weight::Medium,
    )];
    // The terminal-phase line: the engine's intercept and λz, drawn from the first point of the
    // phase to the last time (a rendering of two engine values).
    if let Some(sel) = ok.selected() {
        let tlast = points.iter().map(|p| p.time).fold(0.0_f64, f64::max);
        let span = (tlast - sel.time_first).max(f64::MIN_POSITIVE);
        let steps = 60;
        let curve: Vec<[f64; 2]> = (0..=steps)
            .map(|i| {
                let t = sel.time_first + span * f64::from(i) / f64::from(steps);
                [t, (sel.intercept - sel.lambda_z * t).exp()]
            })
            .collect();
        line_sets.push(LineSet::new("λz fit", curve, Tone::Fit, Weight::Thick));
    }
    let height =
        (ui.available_height() - tokens.size.plot_reserved_height).max(tokens.size.plot_min_height);
    let clicked = plot::profile(
        ui,
        tokens,
        "profile-plot",
        labels,
        &point_sets,
        &line_sets,
        log,
        &[0, 1, 2],
        height,
    );
    let clicked_time = clicked.map(|c| c.point[0]);
    ui.label(
        RichText::new("Click a point to add it to the terminal phase or take it out.")
            .small()
            .color(c.text_muted.color()),
    );
    if let Some(t) = clicked_time {
        match page.toggle_point(t) {
            Ok(()) => actions.push(Action::RunNca),
            Err(message) => actions.push(Action::Notice(message)),
        }
    }
    candidates(ui, tokens, page, &ok.lambda_z_candidates, actions);
}

fn candidates(
    ui: &mut Ui,
    tokens: &Tokens,
    page: &mut NcaPage,
    list: &[Candidate],
    actions: &mut Vec<Action>,
) {
    let c = &tokens.colors;
    ui.add_space(tokens.spacing.small);
    ui.horizontal(|ui| {
        ui.label(RichText::new("Terminal-phase candidates").strong());
        if page.manual() && ui.button("Back to automatic").clicked() {
            page.set_option(&["lambda_z_selection", "manual"], Value::Null);
            actions.push(Action::RunNca);
        }
    });
    if list.is_empty() {
        ui.label(
            RichText::new("No candidate: too few points after Tmax.").color(c.text_muted.color()),
        );
        return;
    }
    egui::ScrollArea::vertical()
        .id_salt("candidates")
        .max_height(tokens.size.list_max_height)
        .show(ui, |ui| {
            egui::Grid::new("candidates-grid")
                .striped(true)
                .show(ui, |ui| {
                    for h in ["points", "from", "to", "λz", "adj. R²", ""] {
                        ui.label(RichText::new(h).small().strong());
                    }
                    ui.end_row();
                    for cand in list {
                        let text = |s: String| {
                            if cand.selected {
                                RichText::new(s).strong().color(c.accent.color())
                            } else if !cand.valid {
                                RichText::new(s).color(c.text_muted.color())
                            } else {
                                RichText::new(s)
                            }
                        };
                        let mut row_clicked = false;
                        row_clicked |= ui
                            .selectable_label(cand.selected, text(cand.n_points.to_string()))
                            .clicked();
                        row_clicked |= ui.label(text(fmt::number(cand.time_first))).clicked();
                        row_clicked |= ui.label(text(fmt::number(cand.time_last))).clicked();
                        row_clicked |= ui.label(text(fmt::number(cand.lambda_z))).clicked();
                        row_clicked |= ui
                            .label(text(
                                cand.adj_r_squared
                                    .map_or_else(|| "-".to_owned(), fmt::fit_quality),
                            ))
                            .clicked();
                        ui.label(
                            RichText::new(if cand.selected {
                                "selected"
                            } else if cand.valid {
                                ""
                            } else {
                                "rising"
                            })
                            .small()
                            .color(c.text_muted.color()),
                        );
                        ui.end_row();
                        if row_clicked && !cand.selected {
                            page.set_option(
                            &["lambda_z_selection", "manual"],
                            json!({ "range": { "start": cand.time_first, "end": cand.time_last } }),
                        );
                            actions.push(Action::RunNca);
                        }
                    }
                });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_builds_the_parameters_of_nca_run() {
        let mut page = NcaPage::new(3, "A".to_owned());
        let p = page.params();
        assert_eq!(p["worksheet"], 3);
        assert_eq!(p["subject"], "A");
        assert_eq!(p["route"], "extravascular");
        assert!(p.get("analysis").is_none() && p.get("options").is_none());
        page.analysis = Some(7);
        page.route = RouteChoice::IvInfusion;
        page.infusion_duration = 2.5;
        page.dose_override = Some(50.0);
        page.set_option(&["lambda_z", "min_points"], json!(4));
        page.set_option(
            &["lambda_z_selection", "manual"],
            json!({ "times": [4.0, 6.0] }),
        );
        let p = page.params();
        assert_eq!(p["analysis"], 7);
        assert_eq!(p["route"]["iv_infusion"]["duration"], 2.5);
        assert_eq!(p["dose"], 50.0);
        assert_eq!(p["options"]["lambda_z"]["min_points"], 4);
        assert_eq!(page.used_times(), [4.0, 6.0]);
        assert!(page.manual());
        page.set_option(&["lambda_z_selection", "manual"], Value::Null);
        assert!(!page.manual());
    }

    #[test]
    fn the_state_of_the_page_round_trips_as_data() {
        let mut page = NcaPage::new(1, "2".to_owned());
        page.route = RouteChoice::IvBolus;
        page.set_option(&["auc_method"], json!("linear"));
        let text = serde_json::to_string(&page).unwrap();
        let back: NcaPage = serde_json::from_str(&text).unwrap();
        assert_eq!(back, page);
    }

    #[test]
    fn a_stored_analysis_gives_back_its_page() {
        let view = json!({
            "id": 5, "label": "NCA", "kind": "nca", "status": { "state": "fresh" },
            "spec": { "kind": "nca", "worksheet": 2, "subject": "7",
                      "route": { "iv_infusion": { "duration": 0.5 } }, "dose": null,
                      "options": { "auc_method": "linear" } },
            "result": null,
        });
        let page = NcaPage::from_view(&view).unwrap();
        assert_eq!(
            (page.analysis, page.worksheet, page.subject.as_str()),
            (Some(5), 2, "7")
        );
        assert_eq!(page.route, RouteChoice::IvInfusion);
        assert_eq!(page.infusion_duration, 0.5);
        assert_eq!(page.dose_override, None);
        assert_eq!(page.option_str("/auc_method"), Some("linear"));
        assert!(NcaPage::from_view(&json!({ "kind": "fit" })).is_none());
    }
}
