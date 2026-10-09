//! The presets of the fit options (task T-044): named sets of the five iteration settings
//! (`derivatives`, `increment`, `criterion`, `convergence`, `max_iterations`). The names come from
//! the schema of `fit.run` and the values from the engine itself: a scratch engine fits a small
//! profile once per name with `{"preset": name}` and the options it echoes are the preset. Nothing
//! is written twice (golden rule 5), so a preset changed in the engine changes the page.

use std::sync::OnceLock;

use serde_json::{Map, Value, json};

use caladrius_engine::Engine;

use crate::fit::CRITERIA;

/// The five settings a preset sets, in the order the page shows them.
pub const KEYS: [&str; 5] = [
    "derivatives",
    "increment",
    "criterion",
    "convergence",
    "max_iterations",
];

/// The id of the preset that holds when no option is set.
const DEFAULT_ID: &str = "default";

/// What the choice shows when the options match no preset.
pub const CUSTOM: &str = "Custom";

/// A named set of options, as the engine returns it.
#[derive(Debug, Clone, PartialEq)]
pub struct Preset {
    /// The id the engine takes in `options.preset`.
    pub id: String,
    /// The name the person reads.
    pub label: String,
    /// The five settings, by key.
    pub options: Map<String, Value>,
}

/// A small profile with a little noise: the scratch fit that makes the engine echo a preset.
const PROFILE: &str = "Time (h),Conc (mg/L),Dose (mg)\n0.5,8.6,100\n1,7.5,100\n2,5.6,100\n4,3.1,100\n6,1.7,100\n8,0.95,100\n12,0.31,100\n";

/// Every preset of the engine, in the order of its schema.
pub fn all() -> &'static [Preset] {
    static PRESETS: OnceLock<Vec<Preset>> = OnceLock::new();
    PRESETS.get_or_init(load)
}

fn load() -> Vec<Preset> {
    ids().into_iter().filter_map(|id| read(&id)).collect()
}

/// The preset ids of the schema of `fit.run`.
fn ids() -> Vec<String> {
    caladrius_engine::describe()
        .into_iter()
        .find(|c| c.id == "fit.run")
        .and_then(|c| {
            c.params_schema
                .pointer("/$defs/FitOptions/properties/preset/enum")
                .and_then(Value::as_array)
                .map(|ids| {
                    ids.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
        })
        .unwrap_or_default()
}

/// The options the engine echoes for `{"preset": id}`.
fn read(id: &str) -> Option<Preset> {
    let mut engine = Engine::new();
    let imported = engine.import_csv("preset", PROFILE.as_bytes()).ok()?;
    let worksheet = imported.pointer("/worksheet/id")?.as_u64()?;
    let fitted = engine
        .execute(
            "fit.run",
            json!({ "worksheet": worksheet, "model": "pk1.iv_bolus", "options": { "preset": id } }),
        )
        .ok()?;
    let echoed = fitted.pointer("/spec/options")?.as_object()?;
    let mut options = Map::new();
    for key in KEYS {
        options.insert(key.to_owned(), echoed.get(key)?.clone());
    }
    Some(Preset {
        id: id.to_owned(),
        label: label_of(id),
        options,
    })
}

/// The name of a preset: the two the engine has are named, a new one is its id spelled out.
fn label_of(id: &str) -> String {
    match id {
        "default" => "Caladrius default".to_owned(),
        "reference_conventions" => "Reference conventions".to_owned(),
        other => {
            let spaced = other.replace('_', " ");
            let mut chars = spaced.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(chars).collect()
            })
        }
    }
}

/// The value of a setting in the options of a page; a setting not set is the default preset's.
fn effective(options: &Value, key: &str) -> Option<Value> {
    options.get(key).cloned().or_else(|| {
        all()
            .iter()
            .find(|p| p.id == DEFAULT_ID)
            .and_then(|p| p.options.get(key).cloned())
    })
}

/// The preset the options of a page are: all five settings equal (a setting not set counts as the
/// default preset's). `None` when they are none: the page then says "custom".
pub fn matching(options: &Value) -> Option<&'static Preset> {
    all().iter().find(|p| {
        KEYS.iter()
            .all(|key| effective(options, key) == p.options.get(*key).cloned())
    })
}

/// Writes the five settings of a preset into the options of a page, leaving the rest alone.
/// Returns whether the preset exists.
pub fn apply(options: &mut Value, id: &str) -> bool {
    let Some(preset) = all().iter().find(|p| p.id == id) else {
        return false;
    };
    if !options.is_object() {
        *options = json!({});
    }
    if let Some(map) = options.as_object_mut() {
        for (key, value) in &preset.options {
            map.insert(key.clone(), value.clone());
        }
    }
    true
}

/// A number the way a threshold is written (`1e-10`, `0.001` as `1e-3`).
fn short(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_f64)
        .map_or_else(String::new, |x| format!("{x:e}"))
}

/// What one preset means, from its values.
fn meaning(preset: &Preset) -> String {
    let get = |key: &str| preset.options.get(key);
    let increment = short(get("increment"));
    let derivatives = match get("derivatives").and_then(Value::as_str) {
        Some("auto") => format!(
            "closed forms when the model has them, else forward differences of increment {increment}"
        ),
        Some("forward_difference") => format!("forward differences of increment {increment}"),
        Some("analytic") => "closed forms".to_owned(),
        _ => String::new(),
    };
    let criterion = get("criterion")
        .and_then(Value::as_str)
        .and_then(|id| CRITERIA.iter().find(|(c, _)| *c == id))
        .map_or_else(String::new, |(_, label)| label.to_lowercase());
    let iterations = get("max_iterations")
        .and_then(Value::as_u64)
        .map_or_else(String::new, |n| format!("{n} iterations at most"));
    format!(
        "{}: {derivatives}, stop when the {criterion} falls below {}, {iterations}",
        preset.label,
        short(get("convergence")),
    )
}

/// One sentence saying what each preset means, written from the values the engine returned.
pub fn sentence() -> String {
    let parts: Vec<String> = all().iter().map(meaning).collect();
    if parts.is_empty() {
        String::new()
    } else {
        format!("{}.", parts.join("; "))
    }
}
