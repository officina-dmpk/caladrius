//! Importing a CSV: a preview before anything is stored (friction 10). The engine reads the file
//! (`data.preview`) and lists every admissible way of reading it, with the checks on each; the
//! person sees the table each reading gives and chooses, or fixes the separator and decimal mark.

use egui::{RichText, Ui};
use serde_json::{Value, json};

use crate::app::Action;
use crate::model::{ColumnInfo, Warning, read};
use crate::sheet::cell_text;
use crate::theme::Tokens;

/// One way of reading the file, as the preview shows it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Reading {
    pub delimiter: String,
    pub decimal_comma: bool,
    pub rows: usize,
    pub columns: Vec<ColumnInfo>,
    pub preview: Vec<Vec<Value>>,
    pub notes: Vec<String>,
    pub checks: Vec<Warning>,
}

impl Reading {
    fn from_value(v: &Value) -> Reading {
        Reading {
            delimiter: v
                .get("delimiter")
                .and_then(Value::as_str)
                .unwrap_or(",")
                .to_owned(),
            decimal_comma: v
                .get("decimal_comma")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            rows: v.get("rows").and_then(Value::as_u64).unwrap_or(0) as usize,
            columns: read(v.get("columns").unwrap_or(&Value::Null)),
            preview: v
                .get("preview")
                .and_then(Value::as_array)
                .map(|rows| {
                    rows.iter()
                        .map(|r| r.as_array().cloned().unwrap_or_default())
                        .collect()
                })
                .unwrap_or_default(),
            notes: read(v.get("notes").unwrap_or(&Value::Null)),
            checks: read(v.get("checks").unwrap_or(&Value::Null)),
        }
    }

    /// `semicolon`, `comma`, `tab` for the delimiter.
    pub fn delimiter_name(&self) -> &'static str {
        match self.delimiter.as_str() {
            ";" => "semicolon",
            "\t" => "tab",
            _ => "comma",
        }
    }

    pub fn decimal_name(&self) -> &'static str {
        if self.decimal_comma {
            "decimal comma"
        } else {
            "decimal point"
        }
    }
}

/// What the person asked of the reader: a separator and a decimal mark, or automatic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ReadOptions {
    pub delimiter: Option<char>,
    pub decimal_comma: Option<bool>,
}

/// A file waiting for the person's decision.
#[derive(Debug, Clone, Default)]
pub struct PendingImport {
    pub name: String,
    pub csv: String,
    pub options: ReadOptions,
    pub readings: Vec<Reading>,
    pub needs_choice: bool,
    pub error: Option<String>,
    pub chosen: usize,
    pub acknowledged: bool,
}

impl PendingImport {
    pub fn new(name: &str, csv: String) -> PendingImport {
        PendingImport {
            name: name.to_owned(),
            csv,
            ..PendingImport::default()
        }
    }

    /// The parameters of `data.preview` for the current options.
    pub fn preview_params(&self) -> Value {
        let mut p = json!({ "csv": self.csv, "rows": 8 });
        if let Some(d) = self.options.delimiter {
            p["delimiter"] = json!(d.to_string());
        }
        if let Some(c) = self.options.decimal_comma {
            p["decimal_comma"] = json!(c);
        }
        p
    }

    /// Takes the answer of `data.preview`, or its error.
    pub fn adopt(&mut self, answer: Result<Value, String>) {
        self.chosen = 0;
        self.acknowledged = false;
        match answer {
            Ok(v) => {
                self.readings = v
                    .get("readings")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().map(Reading::from_value).collect())
                    .unwrap_or_default();
                self.needs_choice = v
                    .get("needs_choice")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                self.error = None;
            }
            Err(message) => {
                self.readings.clear();
                self.needs_choice = false;
                self.error = Some(message);
            }
        }
    }

    pub fn chosen_reading(&self) -> Option<&Reading> {
        self.readings.get(self.chosen)
    }

    /// Whether the chosen reading may be imported now: it has no failed check, or the person
    /// said they looked at it.
    pub fn can_import(&self) -> bool {
        self.chosen_reading()
            .is_some_and(|r| r.checks.is_empty() || self.acknowledged)
    }

    /// The parameters of `data.import` for the chosen reading.
    pub fn import_params(&self) -> Option<Value> {
        let r = self.chosen_reading()?;
        Some(json!({
            "name": self.name,
            "csv": self.csv,
            "delimiter": r.delimiter,
            "decimal_comma": r.decimal_comma,
        }))
    }
}

fn delimiter_choices() -> [(&'static str, Option<char>); 4] {
    [
        ("Automatic", None),
        ("Comma", Some(',')),
        ("Semicolon", Some(';')),
        ("Tab", Some('\t')),
    ]
}

/// The import screen.
pub fn screen(ui: &mut Ui, tokens: &Tokens, p: &mut PendingImport, actions: &mut Vec<Action>) {
    let c = &tokens.colors;
    ui.label(
        RichText::new(format!("Import {}", p.name))
            .size(tokens.font.title)
            .strong(),
    );
    ui.label(
        RichText::new("Nothing is stored until you press Import. Check that the table below is the one you meant.")
            .color(c.text_muted.color()),
    );
    ui.add_space(tokens.spacing.medium);
    ui.horizontal(|ui| {
        let before = p.options;
        ui.label("Separator");
        let shown = delimiter_choices()
            .iter()
            .find(|(_, v)| *v == p.options.delimiter)
            .map_or("Automatic", |(l, _)| *l);
        egui::ComboBox::from_id_salt("import-separator")
            .selected_text(shown)
            .show_ui(ui, |ui| {
                for (label, value) in delimiter_choices() {
                    ui.selectable_value(&mut p.options.delimiter, value, label);
                }
            });
        ui.label("Decimal mark");
        let shown = match p.options.decimal_comma {
            None => "Automatic",
            Some(false) => "Point",
            Some(true) => "Comma",
        };
        egui::ComboBox::from_id_salt("import-decimal")
            .selected_text(shown)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut p.options.decimal_comma, None, "Automatic");
                ui.selectable_value(&mut p.options.decimal_comma, Some(false), "Point");
                ui.selectable_value(&mut p.options.decimal_comma, Some(true), "Comma");
            });
        if p.options != before {
            actions.push(Action::ImportReload);
        }
    });
    if let Some(error) = &p.error {
        ui.add_space(tokens.spacing.medium);
        tokens
            .card_frame()
            .stroke(egui::Stroke::new(tokens.stroke.medium, c.error.color()))
            .show(ui, |ui| {
                ui.label(RichText::new(error).color(c.error.color()));
                ui.label(
                    RichText::new("Change the separator or the decimal mark above, or fix the file and open it again.")
                        .color(c.text_muted.color()),
                );
            });
    }
    if p.needs_choice {
        ui.add_space(tokens.spacing.medium);
        tokens
            .banner_frame(c.stale_background, c.stale)
            .show(ui, |ui| {
                let text = if p.readings.len() > 1 {
                    "This file can be read in more than one way. Pick the table that is right."
                } else {
                    "This reading looks wrong. Look at the table and the checks before importing."
                };
                ui.label(RichText::new(text).strong().color(c.stale.color()));
            });
    }
    ui.add_space(tokens.spacing.medium);
    let count = p.readings.len();
    for (i, reading) in p.readings.clone().iter().enumerate() {
        tokens.card_frame().show(ui, |ui| {
            ui.horizontal(|ui| {
                if (count > 1 || !reading.checks.is_empty()) && ui.radio(p.chosen == i, "").clicked() {
                    p.chosen = i;
                    p.acknowledged = false;
                }
                ui.label(
                    RichText::new(format!(
                        "Read with a {} and a {}: {} rows, {} columns",
                        reading.delimiter_name(),
                        reading.decimal_name(),
                        reading.rows,
                        reading.columns.len()
                    ))
                    .strong(),
                );
            });
            preview_table(ui, tokens, reading, i);
            for check in &reading.checks {
                ui.label(
                    RichText::new(format!("Check: {}", check.message)).color(c.warning.color()),
                );
            }
            if !reading.checks.is_empty() && reading.delimiter == "," {
                ui.label(
                    RichText::new("If the numbers in this file use a decimal comma (0,25 for a quarter), the columns must be separated by a semicolon or a tab: save the file that way and open it again.")
                        .small()
                        .color(c.text_muted.color()),
                );
            }
            for note in &reading.notes {
                ui.label(RichText::new(note).small().color(c.text_muted.color()));
            }
        });
        ui.add_space(tokens.spacing.small);
    }
    ui.add_space(tokens.spacing.medium);
    if p.chosen_reading().is_some_and(|r| !r.checks.is_empty()) {
        ui.checkbox(
            &mut p.acknowledged,
            "I have looked at this reading and want to import it anyway",
        );
    }
    ui.horizontal(|ui| {
        let can = p.can_import();
        if ui
            .add_enabled(can, tokens.primary_button("Import"))
            .on_disabled_hover_text(
                "Tick the box above to import a reading that has checks, or pick another one.",
            )
            .clicked()
        {
            actions.push(Action::ImportConfirm);
        }
        if ui.button("Cancel").clicked() {
            actions.push(Action::ImportCancel);
        }
    });
}

fn preview_table(ui: &mut Ui, tokens: &Tokens, reading: &Reading, index: usize) {
    let c = &tokens.colors;
    egui::Grid::new(("import-preview", index))
        .striped(true)
        .show(ui, |ui| {
            for col in &reading.columns {
                let unit = col
                    .unit
                    .as_deref()
                    .map(|u| format!(" ({u})"))
                    .unwrap_or_default();
                ui.label(RichText::new(format!("{}{unit}", col.name)).strong());
            }
            ui.end_row();
            for col in &reading.columns {
                let role = match col.role.as_str() {
                    "other" => "not used".to_owned(),
                    r => r.to_owned(),
                };
                ui.label(RichText::new(role).small().color(c.text_muted.color()));
            }
            ui.end_row();
            for row in &reading.preview {
                for value in row {
                    ui.label(cell_text(value));
                }
                ui.end_row();
            }
        });
    if reading.rows > reading.preview.len() {
        ui.label(
            RichText::new(format!(
                "… {} more rows",
                reading.rows - reading.preview.len()
            ))
            .small()
            .color(c.text_muted.color()),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answer() -> Value {
        json!({
            "readings": [
                { "delimiter": ";", "decimal_comma": true, "rows": 2,
                  "columns": [{ "name": "time", "role": "time", "unit": null, "type": "number", "missing": 0 }],
                  "preview": [[0.25], [0.5]], "notes": [], "checks": [] },
                { "delimiter": ",", "decimal_comma": false, "rows": 2,
                  "columns": [{ "name": "time", "role": "time", "unit": null, "type": "number", "missing": 0 }],
                  "preview": [[0], [0]], "notes": [],
                  "checks": [{ "code": "duplicate_times", "message": "time 0 appears twice" }] },
            ],
            "recommended": 0, "ambiguous": true, "needs_choice": true,
        })
    }

    #[test]
    fn the_preview_is_read_and_a_reading_with_checks_needs_a_yes() {
        let mut p = PendingImport::new("x", "time;conc\n".to_owned());
        p.adopt(Ok(answer()));
        assert_eq!(p.readings.len(), 2);
        assert!(p.needs_choice);
        assert_eq!(p.readings[0].delimiter_name(), "semicolon");
        assert_eq!(p.readings[0].decimal_name(), "decimal comma");
        // The first reading is plausible and can be imported at once.
        assert!(p.can_import());
        let params = p.import_params().unwrap();
        assert_eq!(params["delimiter"], ";");
        assert_eq!(params["decimal_comma"], true);
        // The second has a failed check: not without the person's yes.
        p.chosen = 1;
        assert!(!p.can_import());
        p.acknowledged = true;
        assert!(p.can_import());
        // Choosing again takes the yes back.
        p.adopt(Ok(answer()));
        assert!(!p.acknowledged);
    }

    #[test]
    fn an_error_leaves_nothing_to_import() {
        let mut p = PendingImport::new("x", String::new());
        p.adopt(Err("line 3 has 1 fields but the header has 2".to_owned()));
        assert!(p.readings.is_empty() && !p.can_import() && p.import_params().is_none());
        assert!(p.error.as_deref().unwrap().contains("line 3"));
    }

    #[test]
    fn the_options_become_the_parameters_of_the_preview() {
        let mut p = PendingImport::new("x", "a".to_owned());
        assert_eq!(p.preview_params(), json!({ "csv": "a", "rows": 8 }));
        p.options = ReadOptions {
            delimiter: Some(';'),
            decimal_comma: Some(true),
        };
        let v = p.preview_params();
        assert_eq!(v["delimiter"], ";");
        assert_eq!(v["decimal_comma"], true);
    }
}
