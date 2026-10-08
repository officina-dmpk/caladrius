//! The worksheet: a table with editable cells and column roles and units. Every edit is an engine
//! command (`data.set_cell`, `data.set_column`); the UI sends the text as typed and the engine
//! reads it (point or comma as the decimal mark).

use egui::{Id, RichText, Sense, Ui};
use serde_json::Value;

use crate::app::Action;
use crate::fmt;
use crate::model::{Table, WorksheetInfo};
use crate::theme::Tokens;

/// A cell entry the engine refused, kept so the person sees what was typed.
#[derive(Debug, Clone, PartialEq)]
pub struct Rejected {
    pub row: usize,
    pub column: String,
    pub text: String,
}

/// The text of a cell as shown: numbers in their shortest exact form, text as it is, missing empty.
pub fn cell_text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Number(n) => n.as_f64().map_or_else(|| n.to_string(), fmt::exact),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// The rows to show: those of `subject` when a subject column exists and one is given.
pub fn visible_rows(table: &Table, info: &WorksheetInfo, subject: Option<&str>) -> Vec<usize> {
    let subject_col = info
        .column_of("subject")
        .and_then(|name| table.column_index(name));
    table
        .rows
        .iter()
        .enumerate()
        .filter(|(_, row)| match (subject, subject_col) {
            (Some(s), Some(col)) => row.get(col).map(cell_text).is_some_and(|t| t == s),
            _ => true,
        })
        .map(|(i, _)| i)
        .collect()
}

#[derive(Clone, Default)]
struct Editing {
    text: String,
    started: bool,
}

/// One editable cell. Click to edit, Enter or leaving the cell to send, Escape to cancel.
fn cell(
    ui: &mut Ui,
    tokens: &Tokens,
    id: Id,
    shown: &str,
    rejected: Option<&Rejected>,
) -> Option<String> {
    let editing: Option<Editing> = ui.data(|d| d.get_temp(id));
    let size = egui::vec2(tokens.size.cell_width, tokens.size.row_height);
    match editing {
        Some(mut state) => {
            let response = ui.add_sized(
                size,
                egui::TextEdit::singleline(&mut state.text).desired_width(tokens.size.cell_width),
            );
            if !state.started {
                response.request_focus();
                state.started = true;
            }
            let escaped = ui.input(|i| i.key_pressed(egui::Key::Escape));
            if escaped {
                ui.data_mut(|d| d.remove::<Editing>(id));
                return None;
            }
            if response.lost_focus() {
                ui.data_mut(|d| d.remove::<Editing>(id));
                return (state.text != shown).then_some(state.text);
            }
            ui.data_mut(|d| d.insert_temp(id, state));
            None
        }
        None => {
            let text = match rejected {
                Some(r) => RichText::new(&r.text).color(tokens.colors.error.color()),
                None => RichText::new(shown),
            };
            let response = ui
                .add_sized(size, egui::Label::new(text).sense(Sense::click()))
                .on_hover_text(match rejected {
                    Some(_) => {
                        "This entry was refused; the message is at the bottom of the window."
                    }
                    None => "Click to edit",
                });
            if response.clicked() {
                let start = rejected.map_or_else(|| shown.to_owned(), |r| r.text.clone());
                ui.data_mut(|d| {
                    d.insert_temp(
                        id,
                        Editing {
                            text: start,
                            started: false,
                        },
                    )
                });
            }
            None
        }
    }
}

/// The data grid: a header and the rows of the table (those of `subject` when given), scrolling,
/// with editable cells.
pub fn grid(
    ui: &mut Ui,
    tokens: &Tokens,
    info: &WorksheetInfo,
    table: &Table,
    subject: Option<&str>,
    visible: usize,
    actions: &mut Vec<Action>,
) {
    grid_with(ui, tokens, info, table, subject, visible, None, actions);
}

/// [`grid`] that also shows one refused entry.
#[allow(clippy::too_many_arguments)]
pub fn grid_with(
    ui: &mut Ui,
    tokens: &Tokens,
    info: &WorksheetInfo,
    table: &Table,
    subject: Option<&str>,
    visible: usize,
    rejected: Option<&Rejected>,
    actions: &mut Vec<Action>,
) {
    let rows = visible_rows(table, info, subject);
    ui.horizontal(|ui| {
        ui.add_sized(
            [tokens.size.row_number_width, tokens.size.row_height],
            egui::Label::new(
                RichText::new("row")
                    .small()
                    .color(tokens.colors.text_muted.color()),
            ),
        );
        for (i, name) in table.columns.iter().enumerate() {
            let role = info
                .columns
                .iter()
                .find(|c| &c.name == name)
                .map_or("", |c| c.role.as_str());
            let unit = info
                .columns
                .iter()
                .find(|c| &c.name == name)
                .and_then(|c| c.unit.as_deref())
                .map(|u| format!(" ({u})"))
                .unwrap_or_default();
            let _ = i;
            ui.add_sized(
                [tokens.size.cell_width, tokens.size.row_height],
                egui::Label::new(RichText::new(format!("{name}{unit}")).strong().color(
                    if role == "other" {
                        tokens.colors.text_muted.color()
                    } else {
                        tokens.colors.text.color()
                    },
                )),
            );
        }
    });
    let height = tokens.size.row_height * visible.min(rows.len().max(1)) as f32;
    egui::ScrollArea::vertical()
        .id_salt(("sheet-rows", info.id))
        .max_height(height + tokens.spacing.small)
        .auto_shrink([false, true])
        .show_rows(ui, tokens.size.row_height, rows.len(), |ui, range| {
            for pos in range {
                let Some(&row_index) = rows.get(pos) else {
                    continue;
                };
                let Some(row) = table.rows.get(row_index) else {
                    continue;
                };
                ui.horizontal(|ui| {
                    ui.add_sized(
                        [tokens.size.row_number_width, tokens.size.row_height],
                        egui::Label::new(
                            RichText::new((row_index + 1).to_string())
                                .small()
                                .color(tokens.colors.text_muted.color()),
                        ),
                    );
                    for (col, name) in table.columns.iter().enumerate() {
                        let shown = row.get(col).map(cell_text).unwrap_or_default();
                        let id = Id::new(("cell", info.id, row_index, col));
                        let bad = rejected.filter(|r| r.row == row_index && &r.column == name);
                        if let Some(text) = cell(ui, tokens, id, &shown, bad) {
                            actions.push(Action::SetCell {
                                worksheet: info.id,
                                row: row_index,
                                column: name.clone(),
                                text,
                            });
                        }
                    }
                });
            }
        });
}

const ROLES: [(&str, &str); 6] = [
    ("time", "Time"),
    ("concentration", "Concentration"),
    ("subject", "Subject"),
    ("dose", "Dose"),
    ("route", "Route"),
    ("other", "Not used"),
];

/// The worksheet screen: column roles and units, unit warnings, the grid.
pub fn screen(
    ui: &mut Ui,
    tokens: &Tokens,
    info: &WorksheetInfo,
    table: &Table,
    rejected: Option<&Rejected>,
    actions: &mut Vec<Action>,
) {
    let c = &tokens.colors;
    ui.horizontal(|ui| {
        ui.label(RichText::new(&info.name).size(tokens.font.title).strong());
        ui.label(
            RichText::new(format!(
                "{} rows, {} subject{}",
                info.rows,
                info.subjects.len(),
                if info.subjects.len() == 1 { "" } else { "s" }
            ))
            .color(c.text_muted.color()),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(tokens.primary_button("New NCA analysis from this worksheet"))
                .clicked()
            {
                actions.push(Action::NewAnalysis);
            }
        });
    });
    ui.add_space(tokens.spacing.medium);
    tokens.card_frame().show(ui, |ui| {
        ui.label(RichText::new("Columns").strong());
        ui.label(
            RichText::new("Say which column is the time, the concentration, the subject and the dose, and give the units.")
                .small()
                .color(c.text_muted.color()),
        );
        egui::Grid::new(("columns", info.id)).num_columns(5).striped(true).show(ui, |ui| {
            for h in ["name", "role", "unit", "type", "missing"] {
                ui.label(RichText::new(h).small().strong());
            }
            ui.end_row();
            for col in &info.columns {
                ui.label(&col.name);
                let mut role = col.role.clone();
                let shown = ROLES
                    .iter()
                    .find(|(v, _)| *v == role)
                    .map_or(role.as_str(), |(_, l)| *l)
                    .to_owned();
                egui::ComboBox::from_id_salt(("role", info.id, &col.name))
                    .selected_text(shown)
                    .show_ui(ui, |ui| {
                        for (value, label) in ROLES {
                            ui.selectable_value(&mut role, value.to_owned(), label);
                        }
                    });
                if role != col.role {
                    actions.push(Action::SetColumn {
                        worksheet: info.id,
                        column: col.name.clone(),
                        role: Some(role),
                        unit: None,
                    });
                }
                let id = Id::new(("unit", info.id, &col.name));
                let mut text: String = ui
                    .data(|d| d.get_temp::<String>(id))
                    .unwrap_or_else(|| col.unit.clone().unwrap_or_default());
                let response = ui.add(egui::TextEdit::singleline(&mut text).desired_width(tokens.size.text_box_width));
                if response.changed() {
                    ui.data_mut(|d| d.insert_temp(id, text.clone()));
                }
                if response.lost_focus() {
                    ui.data_mut(|d| d.remove::<String>(id));
                    if text.trim() != col.unit.clone().unwrap_or_default() {
                        actions.push(Action::SetColumn {
                            worksheet: info.id,
                            column: col.name.clone(),
                            role: None,
                            unit: Some(text.trim().to_owned()),
                        });
                    }
                }
                ui.label(RichText::new(&col.kind).color(c.text_muted.color()));
                ui.label(col.missing.to_string());
                ui.end_row();
            }
        });
        for line in crate::nca::unit_notes(info) {
            ui.label(RichText::new(line).color(c.warning.color()));
        }
        if !info.derived_units.is_empty() {
            let derived: Vec<String> = ["auc", "cl", "v"]
                .iter()
                .filter_map(|k| info.derived_units.get(*k).map(|u| format!("{} in {u}", k.to_uppercase())))
                .collect();
            if !derived.is_empty() {
                ui.label(
                    RichText::new(format!("Derived units: {}", derived.join(", ")))
                        .color(c.text_muted.color()),
                );
            }
        }
    });
    ui.add_space(tokens.spacing.medium);
    grid_with(ui, tokens, info, table, None, 30, rejected, actions);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ColumnInfo;
    use serde_json::json;

    fn info() -> WorksheetInfo {
        let col = |name: &str, role: &str| ColumnInfo {
            name: name.to_owned(),
            role: role.to_owned(),
            unit: None,
            kind: "number".to_owned(),
            missing: 0,
        };
        WorksheetInfo {
            id: 1,
            name: "w".to_owned(),
            rows: 4,
            subjects: vec!["1".to_owned(), "2".to_owned()],
            columns: vec![
                col("id", "subject"),
                col("t", "time"),
                col("c", "concentration"),
            ],
            ..WorksheetInfo::default()
        }
    }

    fn table() -> Table {
        Table {
            columns: vec!["id".to_owned(), "t".to_owned(), "c".to_owned()],
            rows: vec![
                vec![json!(1), json!(0), json!(1.5)],
                vec![json!(1), json!(1), json!(null)],
                vec![json!(2), json!(0), json!(2)],
                vec![json!(2), json!(1), json!(0.25)],
            ],
        }
    }

    #[test]
    fn cells_show_numbers_exactly_and_missing_as_empty() {
        assert_eq!(cell_text(&json!(0.1)), "0.1");
        assert_eq!(cell_text(&json!(3)), "3");
        assert_eq!(cell_text(&json!(null)), "");
        assert_eq!(cell_text(&json!("x")), "x");
    }

    #[test]
    fn the_rows_of_one_subject_are_picked_by_the_subject_column() {
        let (info, table) = (info(), table());
        assert_eq!(visible_rows(&table, &info, None), vec![0, 1, 2, 3]);
        assert_eq!(visible_rows(&table, &info, Some("2")), vec![2, 3]);
        assert_eq!(visible_rows(&table, &info, Some("9")), Vec::<usize>::new());
        // Without a subject column every row is shown.
        let mut no_subject = info.clone();
        no_subject.columns.retain(|c| c.role != "subject");
        assert_eq!(visible_rows(&table, &no_subject, Some("2")).len(), 4);
    }
}
