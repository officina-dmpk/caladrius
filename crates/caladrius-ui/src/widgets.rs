//! Small controls shared by the analysis pages: a combo box that reports the new choice, a
//! threshold that can be switched off, a titled section, the units line and the error box.

use egui::{RichText, Ui};

use crate::model::WorksheetInfo;
use crate::theme::Tokens;

pub fn combo(ui: &mut Ui, id: &str, current: &str, choices: &[(&str, &str)]) -> Option<String> {
    let shown = choices
        .iter()
        .find(|(v, _)| *v == current)
        .map_or(current, |(_, l)| *l);
    let mut chosen = None;
    egui::ComboBox::from_id_salt(id)
        .selected_text(shown)
        .show_ui(ui, |ui| {
            for (value, label) in choices {
                if ui.selectable_label(*value == current, *label).clicked() && *value != current {
                    chosen = Some((*value).to_owned());
                }
            }
        });
    chosen
}

/// A threshold that can be switched off: a checkbox and a number. Returns the new setting when it
/// changed (`Some(None)` for off).
pub fn threshold(
    ui: &mut Ui,
    label: &str,
    current: Option<f64>,
    when_on: f64,
    range: std::ops::RangeInclusive<f64>,
    speed: f64,
) -> Option<Option<f64>> {
    let mut changed = None;
    ui.horizontal(|ui| {
        let mut on = current.is_some();
        if ui.checkbox(&mut on, label).changed() {
            changed = Some(on.then_some(current.unwrap_or(when_on)));
        }
        if let Some(mut value) = current {
            if ui
                .add(egui::DragValue::new(&mut value).range(range).speed(speed))
                .changed()
            {
                changed = Some(Some(value));
            }
        }
    });
    changed
}

/// A number field with its unit written in it (`1 h`), so a quantity is never a bare number.
/// The value is shown as typed (exactly); an empty unit shows the number alone.
pub fn unit_field(
    ui: &mut Ui,
    value: &mut f64,
    range: std::ops::RangeInclusive<f64>,
    unit: &str,
) -> bool {
    let speed = (value.abs() * 0.05).max(1.0e-6);
    let mut field = egui::DragValue::new(value)
        .range(range)
        .speed(speed)
        .custom_formatter(|n, _| crate::fmt::exact(n))
        .custom_parser(crate::fmt::parse_number);
    if !unit.is_empty() {
        field = field.suffix(format!(" {unit}"));
    }
    ui.add(field).changed()
}

pub fn section(ui: &mut Ui, tokens: &Tokens, title: &str, body: impl FnOnce(&mut Ui)) {
    ui.add_space(tokens.spacing.medium);
    ui.label(
        RichText::new(title)
            .size(tokens.font.heading)
            .color(tokens.colors.text.color()),
    );
    ui.separator();
    body(ui);
}

pub fn units_line(ui: &mut Ui, tokens: &Tokens, info: &WorksheetInfo) {
    let unit = |role: &str| info.unit_of(role).unwrap_or("not set").to_owned();
    ui.label(
        RichText::new(format!(
            "Units: time {}, concentration {}, dose {}",
            unit("time"),
            unit("concentration"),
            unit("dose")
        ))
        .color(tokens.colors.text_muted.color()),
    );
}

pub fn error_box(ui: &mut Ui, tokens: &Tokens, text: &str) {
    let c = &tokens.colors;
    tokens
        .card_frame()
        .stroke(egui::Stroke::new(tokens.stroke.medium, c.error.color()))
        .show(ui, |ui| {
            ui.label(RichText::new(text).color(c.error.color()));
        });
}

/// The banner of a result that no longer matches its inputs, with the button to run it again.
pub fn stale_banner(
    ui: &mut Ui,
    tokens: &Tokens,
    status: &crate::model::Status,
    analysis: Option<u64>,
    actions: &mut Vec<crate::app::Action>,
) {
    let c = &tokens.colors;
    let Some(sentence) = status.sentence() else {
        return;
    };
    tokens
        .banner_frame(c.stale_background, c.stale)
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(sentence).strong().color(c.stale.color()));
                if let Some(id) = analysis {
                    if ui.add(tokens.primary_button("Run again")).clicked() {
                        actions.push(crate::app::Action::RunAgain(id));
                    }
                }
            });
        });
}
