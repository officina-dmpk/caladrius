//! The command palette (UX-FR-01): one shortcut and one button open a searchable list of every
//! command of the engine registry (id, title, one-line description, taken from the engine's own
//! descriptions) and of the actions of the interface (new analysis from this worksheet, open an
//! analysis or a worksheet by name, switch the theme, settings). An entry runs with the current
//! selection as context, or opens the page that asks for its parameters. The palette holds no
//! logic of its own: an entry is an [`Action`], the same value a menu or a button returns.

use egui::{Context, Key, KeyboardShortcut, Modifiers, RichText};
use serde::{Deserialize, Serialize};

use crate::app::{Action, Selection, UiApp};
use crate::fmt;
use crate::modelinfo::Compartments;
use crate::theme::Tokens;

/// Ctrl+K (Cmd+K on a Mac).
pub const OPEN: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::K);

/// The longest description shown in the palette, in characters.
const MAX_LINE: usize = 100;

/// The state of the palette: serializable (golden rule 7).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PaletteState {
    pub open: bool,
    pub query: String,
    /// The highlighted row of the ranked list.
    pub selected: usize,
}

/// Where an entry comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    /// An action of the interface (new analysis, theme, settings).
    Interface,
    /// An analysis of the project.
    Analysis,
    /// A worksheet of the project.
    Worksheet,
    /// A command of the engine registry, by its stable id.
    Command,
}

impl Group {
    fn label(self) -> &'static str {
        match self {
            Group::Interface => "action",
            Group::Analysis => "analysis",
            Group::Worksheet => "worksheet",
            Group::Command => "command",
        }
    }
}

/// One line of the palette.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// Unique and stable: `cmd:nca.run`, `ui:new_nca`, `analysis:3`, `worksheet:1`.
    pub key: String,
    /// The command id for a registry entry, else empty.
    pub id: String,
    pub label: String,
    pub description: String,
    pub group: Group,
    /// What running the entry does.
    pub action: Action,
}

/// A command of the registry, as the palette lists it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CommandLine {
    pub id: String,
    pub title: String,
    pub description: String,
}

/// The registry as lines of the palette (id, title, first sentence of the description).
pub(crate) fn registry() -> Vec<CommandLine> {
    caladrius_engine::describe()
        .into_iter()
        .map(|c| CommandLine {
            description: first_sentence(&c.description),
            id: c.id,
            title: c.title,
        })
        .collect()
}

/// The first sentence of a description, without the quoting backticks, at most 100 characters
/// (one line of the palette).
pub fn first_sentence(text: &str) -> String {
    let plain = fmt::plain(text);
    let end = plain.find(". ").unwrap_or(plain.len());
    let sentence = plain.get(..end).unwrap_or(&plain).trim_end_matches('.');
    if sentence.chars().count() > MAX_LINE {
        let cut: String = sentence.chars().take(MAX_LINE - 1).collect();
        format!("{}…", cut.trim_end())
    } else {
        sentence.to_owned()
    }
}

/// The entries that match `query`, best first. Every word of the query must match: as the start
/// of a word of the label, inside the label, inside the id, inside the description, or as letters
/// in order in the label. An empty query keeps the order of the list.
pub fn rank<'a>(entries: &'a [Entry], query: &str) -> Vec<&'a Entry> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    if words.is_empty() {
        return entries.iter().collect();
    }
    let mut scored: Vec<(u32, usize, &Entry)> = Vec::new();
    for (order, entry) in entries.iter().enumerate() {
        let label = entry.label.to_lowercase();
        let id = entry.id.to_lowercase();
        let description = entry.description.to_lowercase();
        // The actions of the interface come before a command of the same quality of match.
        let mut total = u32::from(entry.group != Group::Interface);
        let mut all = true;
        for word in &words {
            let score = if label.starts_with(word.as_str()) {
                Some(0)
            } else if label
                .split(|c: char| !c.is_alphanumeric())
                .any(|w| w.starts_with(word.as_str()))
            {
                Some(1)
            } else if label.contains(word.as_str()) {
                Some(2)
            } else if id.contains(word.as_str()) {
                Some(3)
            } else if description.contains(word.as_str()) {
                Some(5)
            } else if in_order(&label, word) {
                Some(8)
            } else {
                None
            };
            match score {
                Some(s) => total += s,
                None => {
                    all = false;
                    break;
                }
            }
        }
        if all {
            scored.push((total, order, entry));
        }
    }
    scored.sort_by_key(|(score, order, _)| (*score, *order));
    scored.into_iter().map(|(_, _, e)| e).collect()
}

/// True when the letters of `word` appear in `text` in that order.
fn in_order(text: &str, word: &str) -> bool {
    let mut letters = text.chars();
    word.chars().all(|w| letters.any(|t| t == w))
}

impl UiApp {
    /// Every entry of the palette, for the current selection: the actions of the interface first,
    /// then the analyses and the worksheets by name, then the commands of the registry.
    pub fn palette_entries(&self) -> Vec<Entry> {
        let mut out: Vec<Entry> = Vec::new();
        let worksheet = self
            .selected_worksheet()
            .or_else(|| self.overview.worksheets.first().map(|w| w.id));
        let sheet_name = worksheet
            .and_then(|id| self.overview.worksheets.iter().find(|w| w.id == id))
            .map(|w| w.name.clone());
        let from = match &sheet_name {
            Some(name) => format!(" from {name}"),
            None => String::new(),
        };
        let needs = if sheet_name.is_none() {
            " (open a CSV file first)"
        } else {
            ""
        };
        let ui = |key: &str, label: String, description: String, action: Action| Entry {
            key: format!("ui:{key}"),
            id: String::new(),
            label,
            description,
            group: Group::Interface,
            action,
        };
        out.push(ui(
            "new_nca",
            format!("New NCA{from}"),
            format!("Non-compartmental analysis of this worksheet, run at once with the default options{needs}"),
            Action::NewAnalysis,
        ));
        out.push(ui(
            "new_fit",
            format!("New model fit{from}"),
            format!("Fit a one-compartment model to this worksheet, with starting values from the data{needs}"),
            Action::NewFit,
        ));
        out.push(ui(
            "new_fit_two",
            format!("New two-compartment fit{from}"),
            format!("Fit a two-compartment model (clearances, micro- or macro-constants) to this worksheet; you give every starting value{needs}"),
            Action::NewFitWith(Compartments::Two),
        ));
        out.push(ui(
            "new_simulation",
            "New model simulation".to_owned(),
            "Draw a model's curve for a dose and parameters, no data needed".to_owned(),
            Action::NewSimulation,
        ));
        out.push(ui(
            "new_simulation_two",
            "New two-compartment simulation".to_owned(),
            "Draw the curve of a two-compartment model for a dose and parameters, no data needed"
                .to_owned(),
            Action::NewSimulationWith(Compartments::Two),
        ));
        out.push(ui(
            "theme",
            match self.state.mode {
                crate::theme::ThemeMode::Light => "Switch to the dark theme".to_owned(),
                crate::theme::ThemeMode::Dark => "Switch to the light theme".to_owned(),
            },
            "Colours of the whole interface".to_owned(),
            Action::ToggleTheme,
        ));
        out.push(ui(
            "settings",
            "Open settings".to_owned(),
            "Language, number display, default options of the analyses, plot theme".to_owned(),
            Action::OpenSettings,
        ));
        out.push(ui(
            "save_as",
            "Save project as…".to_owned(),
            "Save the project under another file name".to_owned(),
            Action::SaveProjectAs,
        ));
        out.push(ui(
            "quit",
            "Quit".to_owned(),
            "Close Caladrius (asks first when there are unsaved changes)".to_owned(),
            Action::Quit,
        ));
        for a in &self.overview.analyses {
            let state = match &a.status {
                crate::model::Status::Fresh => "up to date",
                crate::model::Status::Stale { .. } => "out of date",
                crate::model::Status::NoResult => "no result",
            };
            out.push(Entry {
                key: format!("analysis:{}", a.id),
                id: String::new(),
                label: format!("Open analysis: {}", a.label),
                description: format!("{}, {state}", a.kind),
                group: Group::Analysis,
                action: Action::Select(Selection::Analysis(a.id)),
            });
        }
        for w in &self.overview.worksheets {
            out.push(Entry {
                key: format!("worksheet:{}", w.id),
                id: String::new(),
                label: format!("Open worksheet: {}", w.name),
                description: format!("{} rows, {} columns", w.rows, w.columns),
                group: Group::Worksheet,
                action: Action::Select(Selection::Worksheet(w.id)),
            });
        }
        for c in &self.commands {
            out.push(Entry {
                key: format!("cmd:{}", c.id),
                id: c.id.clone(),
                label: c.title.clone(),
                description: c.description.clone(),
                group: Group::Command,
                action: self.command_action(&c.id),
            });
        }
        out
    }

    /// What running a registry command from the palette does with the current selection: it
    /// runs at once when it needs nothing more, or opens the page that asks for its parameters.
    fn command_action(&self, id: &str) -> Action {
        let analysis = match self.state.selection {
            Selection::Analysis(a) => Some(a),
            _ => None,
        };
        let worksheet = self.selected_worksheet();
        let elsewhere = |what: &str| {
            Action::Notice(format!(
                "{what} has no page yet: run it with caladrius-cli or the MCP server."
            ))
        };
        match id {
            "data.import" | "data.preview" => Action::OpenCsv,
            "nca.run" => Action::NewAnalysis,
            "fit.run" | "fit.initial_estimates" | "fit.evaluate" => Action::NewFit,
            "model.simulate" => Action::NewSimulation,
            "project.save" => Action::SaveProject,
            "project.load" => Action::OpenProject,
            "project.new" => Action::NewProject,
            "project.describe" => Action::DescribeProject,
            "analysis.run" => match analysis {
                Some(a) => Action::RunAgain(a),
                None => Action::Notice(
                    "analysis.run runs the analysis in view again: open an analysis first."
                        .to_owned(),
                ),
            },
            "analysis.get" => match analysis {
                Some(a) => Action::Select(Selection::Analysis(a)),
                None => Action::Notice(
                    "analysis.get shows an analysis: pick one in the project tree.".to_owned(),
                ),
            },
            "data.describe" | "data.set_column" | "data.set_cell" => match worksheet {
                Some(w) => Action::Select(Selection::Worksheet(w)),
                None => Action::Notice(format!("{id} works on a worksheet: open one first.")),
            },
            other => elsewhere(other),
        }
    }

    /// The entries that match the query now, best first.
    pub fn palette_results(&self) -> Vec<Entry> {
        let entries = self.palette_entries();
        rank(&entries, &self.state.palette.query)
            .into_iter()
            .cloned()
            .collect()
    }

    /// Opens the palette with an empty query.
    pub(crate) fn open_palette(&mut self) {
        self.state.palette = PaletteState {
            open: true,
            query: String::new(),
            selected: 0,
        };
    }

    pub(crate) fn close_palette(&mut self) {
        self.state.palette.open = false;
    }

    /// Runs the entry `key` (it closes the palette first).
    pub(crate) fn palette_run(&mut self, key: &str) {
        self.close_palette();
        let found = self.palette_entries().into_iter().find(|e| e.key == key);
        if let Some(entry) = found {
            self.perform(vec![entry.action]);
        }
    }

    /// The summary `project.describe` gives, for the status bar.
    pub(crate) fn describe_project(&mut self) {
        let Ok(v) = self.call("project.describe", serde_json::json!({})) else {
            return;
        };
        let count = |key: &str| v.get(key).and_then(|x| x.as_array()).map_or(0, Vec::len);
        let stale = self
            .overview
            .analyses
            .iter()
            .filter(|a| a.status.is_stale())
            .count();
        let history = v
            .get("history_length")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        let text = format!(
            "Project {}: {} worksheet(s), {} analysis(es), {stale} out of date, {history} command(s) in the history.",
            self.overview.name,
            count("worksheets"),
            count("analyses"),
        );
        self.notice = Some(crate::app::Notice {
            kind: crate::app::NoticeKind::Info,
            text,
        });
    }
}

/// The palette over the screen. Returns the actions it asks for.
pub fn draw(
    ctx: &Context,
    tokens: &Tokens,
    state: &mut PaletteState,
    results: &[Entry],
    actions: &mut Vec<Action>,
) {
    let c = &tokens.colors;
    let modal = egui::Modal::new(egui::Id::new("command-palette")).show(ctx, |ui| {
        ui.set_width(tokens.size.palette_width);
        // The keys are taken before the text box sees them.
        let (down, up, enter) = ui.input_mut(|i| {
            (
                i.consume_key(Modifiers::NONE, Key::ArrowDown),
                i.consume_key(Modifiers::NONE, Key::ArrowUp),
                i.consume_key(Modifiers::NONE, Key::Enter),
            )
        });
        let last = results.len().saturating_sub(1);
        if down {
            state.selected = (state.selected + 1).min(last);
        }
        if up {
            state.selected = state.selected.saturating_sub(1);
        }
        let before = state.query.clone();
        let edit = egui::TextEdit::singleline(&mut state.query)
            .hint_text("Type a command, an analysis or a worksheet…")
            .desired_width(f32::INFINITY);
        let response = ui.add(edit);
        response.request_focus();
        if state.query != before {
            state.selected = 0;
        }
        state.selected = state.selected.min(last);
        ui.add_space(tokens.spacing.small);
        if results.is_empty() {
            ui.label(RichText::new("Nothing matches.").color(c.text_muted.color()));
        }
        egui::ScrollArea::vertical()
            .max_height(tokens.size.palette_list_height)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for (i, entry) in results.iter().enumerate() {
                    let selected = i == state.selected;
                    let row = ui.selectable_label(selected, &entry.label);
                    if selected {
                        row.scroll_to_me(None);
                    }
                    let mut detail = entry.group.label().to_owned();
                    if !entry.id.is_empty() {
                        detail = format!("{detail}  {}", entry.id);
                    }
                    ui.label(
                        RichText::new(format!("{detail}: {}", entry.description))
                            .small()
                            .color(c.text_muted.color()),
                    );
                    if row.clicked() {
                        actions.push(Action::PaletteRun(entry.key.clone()));
                    }
                }
            });
        if enter {
            if let Some(entry) = results.get(state.selected) {
                actions.push(Action::PaletteRun(entry.key.clone()));
            }
        }
    });
    if modal.should_close() {
        actions.push(Action::ClosePalette);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(key: &str, id: &str, label: &str, description: &str) -> Entry {
        Entry {
            key: key.to_owned(),
            id: id.to_owned(),
            label: label.to_owned(),
            description: description.to_owned(),
            group: Group::Command,
            action: Action::NewSimulation,
        }
    }

    fn keys<'a>(found: &[&'a Entry]) -> Vec<&'a str> {
        found.iter().map(|e| e.key.as_str()).collect()
    }

    #[test]
    fn the_first_sentence_is_one_short_line_without_backticks() {
        assert_eq!(
            first_sentence("Fits a `model` to data. Stores the result. More."),
            "Fits a model to data"
        );
        assert_eq!(first_sentence("One sentence only."), "One sentence only");
        let long = format!("{} end. next", "word ".repeat(60));
        let line = first_sentence(&long);
        assert!(line.chars().count() <= 100 && line.ends_with('…'), "{line}");
        assert_eq!(first_sentence(""), "");
    }

    #[test]
    fn a_query_matches_label_words_id_and_description_best_first() {
        let list = vec![
            entry("a", "", "New NCA from oral", "Non-compartmental analysis"),
            entry(
                "b",
                "nca.run",
                "Run NCA",
                "Computes the parameters of one subject",
            ),
            entry(
                "c",
                "fit.run",
                "Fit a model",
                "Fits and stores, like nca does not",
            ),
            entry("d", "", "Switch to the dark theme", "Colours"),
        ];
        assert_eq!(keys(&rank(&list, "")), vec!["a", "b", "c", "d"]);
        // Label start beats id, which beats description.
        assert_eq!(keys(&rank(&list, "nca")), vec!["a", "b", "c"]);
        // Every word must match somewhere.
        assert_eq!(keys(&rank(&list, "new nca")), vec!["a"]);
        assert_eq!(keys(&rank(&list, "fit.run")), vec!["c"]);
        // Letters in order in the label are a last resort ("dk" in "dark").
        assert_eq!(keys(&rank(&list, "dk")), vec!["d"]);
        assert!(rank(&list, "zzz").is_empty());
        // Case does not matter.
        assert_eq!(keys(&rank(&list, "DARK")), vec!["d"]);
    }
}
