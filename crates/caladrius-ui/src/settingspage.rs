//! The settings page: one page, searchable, drawn from the description of the fields in
//! [`crate::settings`]. Each row shows what the setting is, what it affects, its current value as
//! a control and its default (with a way back to it). A change is an [`Action::SetSetting`]: the
//! same value for the page, the palette and tests.

use egui::{RichText, Ui};
use serde_json::{Value, json};

use crate::app::Action;
use crate::settings::{Field, Kind, Settings};
use crate::theme::Tokens;
use crate::widgets::combo;

/// True when `field`, with its current value, matches every word of `query` (in its title, key,
/// group, description or value). An empty query matches everything.
pub fn matches(field: &Field, value_text: &str, query: &str) -> bool {
    let haystack = format!(
        "{} {} {} {} {}",
        field.title, field.key, field.group, field.affects, value_text
    )
    .to_lowercase();
    query
        .split_whitespace()
        .all(|word| haystack.contains(&word.to_lowercase()))
}

/// The fields that match `query`, in the order of the page.
pub fn visible<'a>(settings: &Settings, fields: &'a [Field], query: &str) -> Vec<&'a Field> {
    fields
        .iter()
        .filter(|f| {
            let value = settings.get(f.key).unwrap_or(Value::Null);
            matches(f, &Settings::show(f, &value), query)
        })
        .collect()
}

/// The page body.
pub fn central(
    ui: &mut Ui,
    tokens: &Tokens,
    settings: &Settings,
    query: &mut String,
    actions: &mut Vec<Action>,
) {
    let c = &tokens.colors;
    ui.label(RichText::new("Settings").size(tokens.font.title).strong());
    ui.label(
        RichText::new(
            "Changes apply at once and are kept with the application's configuration. Nothing here changes stored data.",
        )
        .color(c.text_muted.color()),
    );
    ui.add_space(tokens.spacing.small);
    ui.add(
        egui::TextEdit::singleline(query)
            .hint_text("Search settings…")
            .desired_width(tokens.size.settings_label_width + tokens.size.settings_label_width),
    );
    let shown = visible(settings, &crate::settings::FIELDS, query);
    if shown.is_empty() {
        ui.add_space(tokens.spacing.medium);
        ui.label(RichText::new("No setting matches.").color(c.text_muted.color()));
        return;
    }
    let mut group = "";
    for field in &shown {
        if field.group != group {
            group = field.group;
            crate::widgets::section(ui, tokens, group, |_| {});
        }
        row(ui, tokens, settings, field, actions);
    }
}

fn row(
    ui: &mut Ui,
    tokens: &Tokens,
    settings: &Settings,
    field: &Field,
    actions: &mut Vec<Action>,
) {
    let c = &tokens.colors;
    let value = settings.get(field.key).unwrap_or(Value::Null);
    let default = Settings::default_of(field.key).unwrap_or(Value::Null);
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(tokens.size.settings_label_width);
            ui.label(RichText::new(field.title).strong());
            ui.label(
                RichText::new(field.affects)
                    .small()
                    .color(c.text_muted.color()),
            );
        });
        ui.vertical(|ui| {
            if field.key == "locale" {
                locale_control(ui, tokens, settings, actions);
                return;
            }
            let mut next: Option<Value> = None;
            match field.kind {
                Kind::Choice(choices) => {
                    let current = value.as_str().unwrap_or_default().to_owned();
                    if let Some(chosen) = combo(ui, field.key, &current, choices) {
                        next = Some(json!(chosen));
                    }
                }
                Kind::Int { min, max } => {
                    let mut n = value.as_i64().unwrap_or(min);
                    if ui
                        .add(egui::DragValue::new(&mut n).range(min..=max))
                        .changed()
                    {
                        next = Some(json!(n));
                    }
                }
                Kind::Number { min, max } => {
                    let mut x = value.as_f64().unwrap_or(min);
                    let speed = (x.abs() * 0.05).max(1.0e-9);
                    let changed = ui
                        .add(
                            egui::DragValue::new(&mut x)
                                .range(min..=max)
                                .speed(speed)
                                .custom_formatter(|n, _| crate::fmt::exact(n))
                                .custom_parser(crate::fmt::parse_number),
                        )
                        .changed();
                    if changed {
                        next = Some(json!(x));
                    }
                }
                Kind::Flag => {
                    let mut on = value.as_bool().unwrap_or(false);
                    if ui.checkbox(&mut on, "").changed() {
                        next = Some(json!(on));
                    }
                }
            }
            if let Some(v) = next {
                actions.push(Action::SetSetting {
                    key: field.key.to_owned(),
                    value: v,
                });
            }
            let default_text = Settings::show(field, &default);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("Default: {default_text}"))
                        .small()
                        .color(c.text_muted.color()),
                );
                if value != default && ui.small_button("Reset").clicked() {
                    actions.push(Action::ResetSetting(field.key.to_owned()));
                }
            });
        });
    });
    ui.add_space(tokens.spacing.small);
}

/// The system locale: what it is, that it is only a suggestion, and what it suggested.
fn locale_control(ui: &mut Ui, tokens: &Tokens, settings: &Settings, actions: &mut Vec<Action>) {
    let c = &tokens.colors;
    match &settings.locale.system {
        Some(tag) => {
            ui.label(RichText::new(tag).strong());
            let suggestion = crate::settings::decimal_mark_for_locale(tag);
            let word = match suggestion {
                Some(crate::settings::DecimalMark::Comma) => "a comma",
                Some(crate::settings::DecimalMark::Point) => "a point",
                None => "nothing",
            };
            ui.label(
                RichText::new(format!(
                    "Reported by the system; it suggests {word} as the decimal mark."
                ))
                .small()
                .color(c.text_muted.color()),
            );
            if settings.locale.mark_from_system {
                ui.label(
                    RichText::new("The decimal mark in use is this suggestion; change it above if it is wrong.")
                        .small()
                        .color(c.warning.color()),
                );
            } else if suggestion.is_some() && ui.small_button("Use the suggestion").clicked() {
                actions.push(Action::UseSystemSuggestion);
            }
        }
        None => {
            ui.label(
                RichText::new("Not reported by the system: nothing is suggested.")
                    .color(c.text_muted.color()),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::FIELDS;

    #[test]
    fn a_search_matches_title_key_group_description_and_value() {
        let s = Settings::default();
        let keys =
            |q: &str| -> Vec<&str> { visible(&s, &FIELDS, q).iter().map(|f| f.key).collect() };
        assert_eq!(keys("").len(), FIELDS.len());
        // The locale row is about the decimal mark too.
        assert_eq!(keys("decimal"), vec!["decimal_mark", "locale"]);
        assert!(keys("nca").contains(&"nca.auc_method"));
        assert!(keys("fit.max").contains(&"fit.max_iterations"));
        // By what it affects, and by its value.
        assert!(keys("terminal phase").contains(&"nca.lambda_z_min_points"));
        assert!(keys("uniform").contains(&"fit.weighting"));
        // Every word must match.
        assert_eq!(keys("decimal weighting"), Vec::<&str>::new());
        assert_eq!(keys("DECIMAL MARK"), vec!["decimal_mark", "locale"]);
        assert_eq!(keys("decimal point"), vec!["decimal_mark"]);
        assert!(keys("zzz").is_empty());
    }
}
