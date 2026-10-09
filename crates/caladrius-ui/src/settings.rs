//! The settings of the person (UX-FR-14, UX-SET-01): one serializable structure, with a description
//! of every field (what it is, what it affects, its default) that the settings page is drawn from.
//! The operating-system locale is only the first suggestion for the number display: it is kept
//! and shown, and the person's choice replaces it. Nothing here reads the system: the program
//! around the interface hands the locale in ([`crate::UiApp::set_system_locale`]) and stores the
//! settings next to its configuration ([`crate::Request::SaveSettings`]).

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::fit::defaults as fit_defaults;
use crate::theme::ThemeMode;

/// The decimal mark of the numbers on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecimalMark {
    #[default]
    Point,
    Comma,
}

/// How the colours are chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeChoice {
    #[default]
    Light,
    Dark,
    /// Follow the operating system's light or dark setting.
    System,
}

impl ThemeChoice {
    /// The theme to draw now; `system` is what the operating system says, when it says.
    pub fn mode(self, system: Option<ThemeMode>) -> ThemeMode {
        match self {
            ThemeChoice::Light => ThemeMode::Light,
            ThemeChoice::Dark => ThemeMode::Dark,
            ThemeChoice::System => system.unwrap_or(ThemeMode::Light),
        }
    }
}

/// The defaults a new NCA starts from (the options of `nca.run`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NcaDefaults {
    pub auc_method: String,
    pub lambda_z_min_points: u32,
    pub lambda_z_allow_tmax: bool,
}

impl Default for NcaDefaults {
    fn default() -> Self {
        NcaDefaults {
            auc_method: nca_defaults::AUC_METHOD.to_owned(),
            lambda_z_min_points: nca_defaults::MIN_POINTS,
            lambda_z_allow_tmax: nca_defaults::ALLOW_TMAX,
        }
    }
}

/// The engine's own defaults for the options above; a test compares them with what `nca.run` returns.
pub mod nca_defaults {
    pub const AUC_METHOD: &str = "lin_up_log_down";
    pub const MIN_POINTS: u32 = 3;
    pub const ALLOW_TMAX: bool = false;
}

/// The defaults a new fit starts from (the weighting and options of `fit.run`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FitDefaults {
    pub weighting: String,
    pub max_iterations: u32,
    pub convergence: f64,
}

impl Default for FitDefaults {
    fn default() -> Self {
        FitDefaults {
            weighting: "uniform".to_owned(),
            max_iterations: fit_defaults::MAX_ITERATIONS as u32,
            convergence: fit_defaults::CONVERGENCE,
        }
    }
}

/// The operating-system locale, and whether the number display came from it.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LocaleNote {
    /// The locale the system reported at the last start, such as `fr-FR`; shown, never applied
    /// silently.
    pub system: Option<String>,
    /// True while the decimal mark is the one suggested from `system` (the person has not chosen).
    pub mark_from_system: bool,
}

/// Every setting.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// The language of the interface. Placeholder: only English exists for now.
    pub language: String,
    pub decimal_mark: DecimalMark,
    /// Significant digits of the numbers shown in tables and summaries.
    pub significant_digits: u8,
    pub theme: ThemeChoice,
    pub nca: NcaDefaults,
    pub fit: FitDefaults,
    pub locale: LocaleNote,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            language: "en".to_owned(),
            decimal_mark: DecimalMark::Point,
            significant_digits: 4,
            theme: ThemeChoice::Light,
            nca: NcaDefaults::default(),
            fit: FitDefaults::default(),
            locale: LocaleNote::default(),
        }
    }
}

/// How a setting is edited.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    /// One of a few values, each with its label.
    Choice(&'static [(&'static str, &'static str)]),
    /// A whole number in a range.
    Int {
        min: i64,
        max: i64,
    },
    /// A number in a range, shown exactly.
    Number {
        min: f64,
        max: f64,
    },
    Flag,
}

/// The description of one setting: what the page shows around its control.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Field {
    /// The path in the settings structure, `nca.auc_method`.
    pub key: &'static str,
    pub group: &'static str,
    pub title: &'static str,
    /// What the setting changes, in a sentence.
    pub affects: &'static str,
    pub kind: Kind,
}

const LANGUAGES: [(&str, &str); 1] = [("en", "English")];
const MARKS: [(&str, &str); 2] = [("point", "Point (3.25)"), ("comma", "Comma (3,25)")];
const THEMES: [(&str, &str); 3] = [
    ("light", "Light"),
    ("dark", "Dark"),
    ("system", "Follow the system"),
];
const AUC_METHODS: [(&str, &str); 3] = [
    ("linear", "Linear trapezoid"),
    ("lin_up_log_down", "Linear up, log down"),
    ("lin_log", "Linear to Tmax, log after"),
];
const WEIGHTINGS: [(&str, &str); 5] = [
    ("uniform", "Uniform (every point counts the same)"),
    ("inv_y", "1/y (observed)"),
    ("inv_y2", "1/y² (observed)"),
    ("inv_yhat", "1/ŷ (predicted)"),
    ("inv_yhat2", "1/ŷ² (predicted)"),
];

/// Every setting, in the order of the page.
pub const FIELDS: [Field; 11] = [
    Field {
        key: "language",
        group: "Interface",
        title: "Language",
        affects: "The language of menus and messages. A placeholder for now: only English exists.",
        kind: Kind::Choice(&LANGUAGES),
    },
    Field {
        key: "theme",
        group: "Interface",
        title: "Plot and interface theme",
        affects: "The colours of the whole window and of the plots. \"Follow the system\" takes the light or dark setting of the operating system.",
        kind: Kind::Choice(&THEMES),
    },
    Field {
        key: "decimal_mark",
        group: "Numbers",
        title: "Decimal mark",
        affects: "How every number is written on screen (tables, fields, axes). It never changes stored data or what a CSV import understands.",
        kind: Kind::Choice(&MARKS),
    },
    Field {
        key: "significant_digits",
        group: "Numbers",
        title: "Significant digits",
        affects: "The digits of the numbers in result tables and summaries. A value you type is shown as you typed it, and nothing stored is rounded.",
        kind: Kind::Int { min: 2, max: 10 },
    },
    Field {
        key: "nca.auc_method",
        group: "New NCA (options of nca.run)",
        title: "AUC method",
        affects: "How the area under the curve is added up in every new NCA. An analysis already made keeps its own.",
        kind: Kind::Choice(&AUC_METHODS),
    },
    Field {
        key: "nca.lambda_z_min_points",
        group: "New NCA (options of nca.run)",
        title: "Fewest points in the terminal phase",
        affects: "The smallest number of points the automatic terminal-phase choice may use in a new NCA.",
        kind: Kind::Int { min: 2, max: 30 },
    },
    Field {
        key: "nca.lambda_z_allow_tmax",
        group: "New NCA (options of nca.run)",
        title: "Terminal phase may include Tmax",
        affects: "Whether the automatic terminal-phase choice may start at the highest concentration, in a new NCA.",
        kind: Kind::Flag,
    },
    Field {
        key: "fit.weighting",
        group: "New fit (options of fit.run)",
        title: "Weighting",
        affects: "The weights of the least-squares fit in every new model fit. A fit already made keeps its own.",
        kind: Kind::Choice(&WEIGHTINGS),
    },
    Field {
        key: "fit.max_iterations",
        group: "New fit (options of fit.run)",
        title: "Maximum iterations",
        affects: "How many iterations a new fit may take before it stops without converging.",
        kind: Kind::Int { min: 1, max: 1000 },
    },
    Field {
        key: "fit.convergence",
        group: "New fit (options of fit.run)",
        title: "Convergence criterion",
        affects: "The fit stops when the relative decrease of the sum of squares falls below this number, in every new fit.",
        kind: Kind::Number { min: 0.0, max: 0.1 },
    },
    Field {
        key: "locale",
        group: "System",
        title: "Operating-system locale",
        affects: "Only the first suggestion for the decimal mark. It is shown here and never applied without being shown.",
        kind: Kind::Flag,
    },
];

/// The languages whose usual decimal mark is the comma (the primary language subtag).
const COMMA_LANGUAGES: [&str; 30] = [
    "af", "bg", "ca", "cs", "da", "de", "el", "es", "et", "eu", "fi", "fr", "gl", "hr", "hu", "id",
    "is", "it", "lt", "lv", "nb", "nl", "no", "pl", "pt", "ro", "ru", "sk", "sl", "sv",
];
const COMMA_LANGUAGES_MORE: [&str; 3] = ["sr", "tr", "uk"];

/// The decimal mark a locale such as `fr-FR` or `de_DE.UTF-8` usually uses; `None` for a tag
/// that does not name a language.
pub fn decimal_mark_for_locale(tag: &str) -> Option<DecimalMark> {
    let language: String = tag
        .split(['-', '_', '.', '@'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    if language.len() < 2 || language.len() > 3 || !language.chars().all(|c| c.is_ascii_lowercase())
    {
        return None;
    }
    let comma = COMMA_LANGUAGES.contains(&language.as_str())
        || COMMA_LANGUAGES_MORE.contains(&language.as_str());
    Some(if comma {
        DecimalMark::Comma
    } else {
        DecimalMark::Point
    })
}

fn pointer(key: &str) -> String {
    format!("/{}", key.replace('.', "/"))
}

impl Settings {
    /// The settings as JSON text, to be stored by the program.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_owned())
    }

    /// Reads stored settings. Fields that are missing take their defaults, so a file written by
    /// an older version still reads; a value that does not fit gives a sentence and nothing is read.
    pub fn from_json(text: &str) -> Result<Settings, String> {
        let settings: Settings = serde_json::from_str(text)
            .map_err(|e| format!("the settings file is not readable: {e}"))?;
        settings.checked()
    }

    fn checked(self) -> Result<Settings, String> {
        for field in &FIELDS {
            if let Some(value) = self.get(field.key) {
                check(field, &value)?;
            }
        }
        Ok(self)
    }

    /// The value of a setting by its key, as JSON.
    pub fn get(&self, key: &str) -> Option<Value> {
        serde_json::to_value(self)
            .ok()?
            .pointer(&pointer(key))
            .cloned()
    }

    /// The default of a setting.
    pub fn default_of(key: &str) -> Option<Value> {
        Settings::default().get(key)
    }

    /// Changes a setting. A value outside what the setting accepts is refused with a sentence
    /// that says what is accepted, and nothing changes.
    pub fn set(&mut self, key: &str, value: Value) -> Result<(), String> {
        let field = FIELDS
            .iter()
            .find(|f| f.key == key)
            .ok_or_else(|| format!("there is no setting {key}"))?;
        if key == "locale" {
            return Err(
                "the operating-system locale is read from the system, not set here".to_owned(),
            );
        }
        check(field, &value)?;
        let mut tree = serde_json::to_value(&*self)
            .map_err(|e| format!("the settings cannot be changed: {e}"))?;
        let slot = tree
            .pointer_mut(&pointer(key))
            .ok_or_else(|| format!("there is no setting {key}"))?;
        *slot = value;
        let changed: Settings = serde_json::from_value(tree)
            .map_err(|e| format!("{} does not accept that value: {e}", field.title))?;
        if key == "decimal_mark" {
            // The person chose it: it no longer comes from the system.
            let mut changed = changed;
            changed.locale.mark_from_system = false;
            *self = changed;
        } else {
            *self = changed;
        }
        Ok(())
    }

    /// The text that shows a value of a field: the label of a choice, the number, yes or no.
    pub fn show(field: &Field, value: &Value) -> String {
        match (field.kind, value) {
            (Kind::Choice(choices), Value::String(s)) => choices
                .iter()
                .find(|(v, _)| v == s)
                .map_or_else(|| s.clone(), |(_, l)| (*l).to_owned()),
            (Kind::Flag, Value::Bool(b)) => if *b { "yes" } else { "no" }.to_owned(),
            (_, Value::Number(n)) => match n.as_f64() {
                Some(x) => crate::fmt::exact(x),
                None => n.to_string(),
            },
            (_, other) => other.to_string(),
        }
    }

    /// The system locale was reported: keep it for display.
    pub fn note_system_locale(&mut self, tag: Option<&str>) {
        self.locale.system = tag.map(str::to_owned);
        if self.locale.system.is_none() {
            self.locale.mark_from_system = false;
        }
    }

    /// Takes the decimal mark the system locale suggests, and remembers that it came from there.
    /// Returns false when there is no locale or it names no language.
    pub fn suggest_from_system(&mut self) -> bool {
        let Some(mark) = self
            .locale
            .system
            .as_deref()
            .and_then(decimal_mark_for_locale)
        else {
            return false;
        };
        self.decimal_mark = mark;
        self.locale.mark_from_system = true;
        true
    }

    /// The options a new NCA starts with: only what differs from the engine's defaults (null when
    /// nothing does), so the engine's own defaults stay the source of truth.
    pub fn nca_options(&self) -> Value {
        let d = NcaDefaults::default();
        let mut options = json!({});
        if self.nca.auc_method != d.auc_method {
            options["auc_method"] = json!(self.nca.auc_method);
        }
        if self.nca.lambda_z_min_points != d.lambda_z_min_points
            || self.nca.lambda_z_allow_tmax != d.lambda_z_allow_tmax
        {
            options["lambda_z"] = json!({
                "min_points": self.nca.lambda_z_min_points,
                "allow_tmax": self.nca.lambda_z_allow_tmax,
            });
        }
        if options.as_object().is_some_and(serde_json::Map::is_empty) {
            Value::Null
        } else {
            options
        }
    }

    /// The options a new fit starts with, as `nca_options` does for an NCA.
    pub fn fit_options(&self) -> Value {
        let d = FitDefaults::default();
        let mut options = json!({});
        if self.fit.max_iterations != d.max_iterations {
            options["max_iterations"] = json!(self.fit.max_iterations);
        }
        if self.fit.convergence != d.convergence {
            options["convergence"] = json!(self.fit.convergence);
        }
        if options.as_object().is_some_and(serde_json::Map::is_empty) {
            Value::Null
        } else {
            options
        }
    }
}

/// Checks a value against what a field accepts.
fn check(field: &Field, value: &Value) -> Result<(), String> {
    let refuse = |what: String| Err(format!("{}: {what}", field.title));
    match (field.kind, value) {
        (Kind::Choice(choices), Value::String(s)) => {
            if choices.iter().any(|(v, _)| v == s) {
                Ok(())
            } else {
                let names: Vec<&str> = choices.iter().map(|(v, _)| *v).collect();
                refuse(format!("`{s}` is not one of {}", names.join(", ")))
            }
        }
        (Kind::Int { min, max }, v) => match v.as_i64() {
            Some(n) if (min..=max).contains(&n) => Ok(()),
            _ => refuse(format!("give a whole number from {min} to {max}")),
        },
        (Kind::Number { min, max }, v) => match v.as_f64() {
            Some(x) if x.is_finite() && x >= min && x <= max => Ok(()),
            _ => refuse(format!(
                "give a number from {} to {}",
                crate::fmt::exact_point(min),
                crate::fmt::exact_point(max)
            )),
        },
        (Kind::Flag, Value::Bool(_)) => Ok(()),
        (Kind::Flag, _) if field.key == "locale" => Ok(()),
        _ => refuse("that kind of value is not accepted".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_the_documented_ones_and_round_trip_as_json() {
        let s = Settings::default();
        assert_eq!(s.significant_digits, 4);
        assert_eq!(s.decimal_mark, DecimalMark::Point);
        assert_eq!(s.fit.max_iterations, 50);
        assert_eq!(Settings::from_json(&s.to_json()).unwrap(), s);
        // A file from an older version (a field missing) still reads.
        let older = Settings::from_json(r#"{ "significant_digits": 6 }"#).unwrap();
        assert_eq!(older.significant_digits, 6);
        assert_eq!(older.fit, FitDefaults::default());
    }

    #[test]
    fn every_field_is_a_setting_of_the_structure_with_a_default_and_a_description() {
        let s = Settings::default();
        for field in &FIELDS {
            assert!(s.get(field.key).is_some(), "{}", field.key);
            assert!(Settings::default_of(field.key).is_some(), "{}", field.key);
            assert!(
                !field.title.is_empty() && field.affects.ends_with('.'),
                "{}",
                field.key
            );
            // The default is accepted by its own field.
            let value = s.get(field.key).unwrap();
            assert!(check(field, &value).is_ok(), "{}", field.key);
        }
        // And every setting of the structure has a field (nothing is hidden), apart from the
        // note that records the locale's origin.
        let tree = serde_json::to_value(&s).unwrap();
        let mut leaves = Vec::new();
        fn walk(prefix: &str, v: &Value, out: &mut Vec<String>) {
            match v {
                Value::Object(map) => {
                    for (k, x) in map {
                        let key = if prefix.is_empty() {
                            k.clone()
                        } else {
                            format!("{prefix}.{k}")
                        };
                        walk(&key, x, out);
                    }
                }
                _ => out.push(prefix.to_owned()),
            }
        }
        walk("", &tree, &mut leaves);
        for leaf in leaves {
            if leaf.starts_with("locale.") {
                continue;
            }
            assert!(FIELDS.iter().any(|f| f.key == leaf), "no field for {leaf}");
        }
    }

    #[test]
    fn a_value_outside_what_a_setting_accepts_is_refused_with_a_sentence() {
        let mut s = Settings::default();
        let e = s.set("significant_digits", json!(1)).unwrap_err();
        assert!(e.contains("whole number from 2 to 10"), "{e}");
        let e = s.set("significant_digits", json!(2.5)).unwrap_err();
        assert!(e.contains("whole number"), "{e}");
        let e = s.set("fit.weighting", json!("wild")).unwrap_err();
        assert!(e.contains("not one of uniform, inv_y"), "{e}");
        let e = s.set("fit.convergence", json!(5.0)).unwrap_err();
        assert!(e.contains("from 0 to 0.1"), "{e}");
        assert!(s.set("fit.convergence", json!(f64::NAN)).is_err());
        assert!(s.set("nca.lambda_z_allow_tmax", json!("yes")).is_err());
        assert!(
            s.set("no.such", json!(1))
                .unwrap_err()
                .contains("no setting")
        );
        assert!(
            s.set("locale", json!(true))
                .unwrap_err()
                .contains("read from the system")
        );
        assert_eq!(s, Settings::default());
        s.set("significant_digits", json!(6)).unwrap();
        s.set("fit.weighting", json!("inv_y")).unwrap();
        s.set("nca.auc_method", json!("linear")).unwrap();
        assert_eq!(
            (s.significant_digits, s.fit.weighting.as_str()),
            (6, "inv_y")
        );
        // A stored file with a value that does not fit is refused as a whole.
        let e = Settings::from_json(r#"{ "significant_digits": 99 }"#).unwrap_err();
        assert!(e.contains("Significant digits"), "{e}");
        assert!(
            Settings::from_json("not json")
                .unwrap_err()
                .contains("not readable")
        );
    }

    #[test]
    fn the_locale_is_a_suggestion_that_is_kept_shown_and_replaced_by_the_choice() {
        assert_eq!(decimal_mark_for_locale("fr-FR"), Some(DecimalMark::Comma));
        assert_eq!(
            decimal_mark_for_locale("de_DE.UTF-8"),
            Some(DecimalMark::Comma)
        );
        assert_eq!(decimal_mark_for_locale("pt-BR"), Some(DecimalMark::Comma));
        assert_eq!(decimal_mark_for_locale("en-US"), Some(DecimalMark::Point));
        assert_eq!(decimal_mark_for_locale("ja_JP"), Some(DecimalMark::Point));
        assert_eq!(decimal_mark_for_locale("C"), None);
        assert_eq!(decimal_mark_for_locale(""), None);
        assert_eq!(decimal_mark_for_locale("POSIX"), None);
        let mut s = Settings::default();
        // Without a locale nothing is suggested.
        assert!(!s.suggest_from_system());
        s.note_system_locale(Some("fr-FR"));
        // Noting the locale alone changes no setting but the note.
        assert_eq!(s.decimal_mark, DecimalMark::Point);
        assert!(!s.locale.mark_from_system);
        assert!(s.suggest_from_system());
        assert_eq!(s.decimal_mark, DecimalMark::Comma);
        assert!(s.locale.mark_from_system);
        // The person's own choice replaces it and says so.
        s.set("decimal_mark", json!("point")).unwrap();
        assert_eq!(s.decimal_mark, DecimalMark::Point);
        assert!(!s.locale.mark_from_system);
        assert_eq!(s.locale.system.as_deref(), Some("fr-FR"));
    }

    #[test]
    fn new_analyses_start_from_the_defaults_that_differ_and_only_those() {
        let mut s = Settings::default();
        assert_eq!(s.nca_options(), Value::Null);
        assert_eq!(s.fit_options(), Value::Null);
        s.set("nca.lambda_z_min_points", json!(4)).unwrap();
        s.set("nca.auc_method", json!("linear")).unwrap();
        s.set("fit.max_iterations", json!(80)).unwrap();
        assert_eq!(
            s.nca_options(),
            json!({ "auc_method": "linear", "lambda_z": { "min_points": 4, "allow_tmax": false } })
        );
        assert_eq!(s.fit_options(), json!({ "max_iterations": 80 }));
    }

    #[test]
    fn the_theme_choice_follows_the_system_only_when_asked() {
        assert_eq!(
            ThemeChoice::Dark.mode(Some(ThemeMode::Light)),
            ThemeMode::Dark
        );
        assert_eq!(
            ThemeChoice::Light.mode(Some(ThemeMode::Dark)),
            ThemeMode::Light
        );
        assert_eq!(
            ThemeChoice::System.mode(Some(ThemeMode::Dark)),
            ThemeMode::Dark
        );
        assert_eq!(ThemeChoice::System.mode(None), ThemeMode::Light);
    }
}
