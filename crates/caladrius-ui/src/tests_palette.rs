//! Tests of the command palette: the list holds every command with its sentence, a query ranks,
//! an entry runs with the selection as context, and an NCA starts in two actions.

use egui::{Key, Modifiers};
use egui_kittest::kittest::{NodeT, Queryable};

use crate::app::{Action, Selection, UiApp, UiState};
use crate::palette::{Group, PaletteState};
use crate::tests::{harness, with_analysis, with_oral};
use crate::theme::ThemeMode;

fn first_key(app: &UiApp) -> Option<String> {
    app.palette_results().first().map(|e| e.key.clone())
}

fn type_query(app: &mut UiApp, query: &str) {
    app.state.palette.query = query.to_owned();
    app.state.palette.selected = 0;
}

#[test]
fn every_registered_command_is_listed_with_its_id_title_and_one_line() {
    let app = UiApp::new();
    let entries = app.palette_entries();
    let listed: Vec<&str> = entries
        .iter()
        .filter(|e| e.group == Group::Command)
        .map(|e| e.id.as_str())
        .collect();
    assert_eq!(listed, caladrius_engine::command_ids());
    for e in entries.iter().filter(|e| e.group == Group::Command) {
        assert!(!e.label.is_empty() && !e.description.is_empty(), "{}", e.id);
        assert!(!e.description.contains('`'), "{}", e.id);
        assert!(e.description.chars().count() <= 100, "{}", e.id);
        assert!(!e.description.contains('\n'), "{}", e.id);
    }
    // Keys are unique, so an entry can be found again by its key.
    let mut keys: Vec<&str> = entries.iter().map(|e| e.key.as_str()).collect();
    keys.sort_unstable();
    keys.dedup();
    assert_eq!(keys.len(), entries.len());
    // The description is the engine's own, shortened to its first sentence.
    let info = caladrius_engine::describe()
        .into_iter()
        .find(|c| c.id == "fit.run")
        .unwrap();
    let fit = entries.iter().find(|e| e.id == "fit.run").unwrap();
    assert!(
        crate::fmt::plain(&info.description).starts_with(fit.description.trim_end_matches('…')),
        "{}",
        fit.description
    );
}

#[test]
fn an_nca_is_created_from_an_open_worksheet_in_two_actions() {
    let mut app = with_oral();
    assert_eq!(app.state.selection, Selection::Worksheet(1));
    assert!(app.engine().project().analyses().is_empty());
    // One: open the palette. Two: pick the entry (the query "nca" puts it first).
    app.perform(vec![Action::OpenPalette]);
    assert!(app.state.palette.open);
    type_query(&mut app, "nca");
    let key = first_key(&app).unwrap();
    assert_eq!(key, "ui:new_nca");
    app.perform(vec![Action::PaletteRun(key)]);
    assert!(!app.state.palette.open);
    assert_eq!(app.engine().project().analyses().len(), 1);
    assert!(app.nca_page().unwrap().view.is_some());
    let id = app.overview.analyses[0].id;
    assert_eq!(app.state.selection, Selection::Analysis(id));
}

#[test]
fn the_keyboard_alone_starts_an_nca_the_palette_is_one_shortcut_and_one_entry() {
    let mut h = harness(with_oral());
    h.run_steps(3);
    h.key_press_modifiers(Modifiers::COMMAND, Key::K);
    h.run_steps(3);
    assert!(h.state().state.palette.open);
    // The text box has the focus as soon as the palette opens: the person just types.
    // The palette's box is the empty one (the others, behind it, hold units and cell values).
    h.get_all_by_role(egui::accesskit::Role::TextInput)
        .find(|n| n.accesskit_node().value().is_none_or(|v| v.is_empty()))
        .unwrap()
        .type_text("new nca");
    h.run_steps(3);
    h.get_by_label("New NCA from oral");
    h.key_press(Key::Enter);
    h.run_steps(4);
    assert_eq!(h.state().engine().project().analyses().len(), 1);
    assert!(!h.state().state.palette.open);
}

#[test]
fn escape_and_the_button_close_and_open_the_palette() {
    let mut h = harness(with_oral());
    h.run_steps(3);
    h.get_by_label("Commands").click();
    h.run_steps(3);
    assert!(h.state().state.palette.open);
    h.get_by_label("Open settings");
    h.key_press(Key::Escape);
    h.run_steps(3);
    assert!(!h.state().state.palette.open);
    // The shortcut toggles it.
    h.key_press_modifiers(Modifiers::COMMAND, Key::K);
    h.run_steps(3);
    assert!(h.state().state.palette.open);
    h.key_press_modifiers(Modifiers::COMMAND, Key::K);
    h.run_steps(3);
    assert!(!h.state().state.palette.open);
}

#[test]
fn the_selection_is_the_context_and_without_a_worksheet_the_entry_says_what_to_do() {
    let mut app = with_oral();
    let label = |app: &UiApp, key: &str| {
        app.palette_entries()
            .into_iter()
            .find(|e| e.key == key)
            .map(|e| (e.label, e.description))
    };
    let (name, _) = label(&app, "ui:new_fit").unwrap();
    assert_eq!(name, "New model fit from oral");
    // No worksheet: the entry is there and says why it cannot run, and running it is a sentence.
    let mut empty = UiApp::new();
    let (name, description) = label(&empty, "ui:new_nca").unwrap();
    assert_eq!(name, "New NCA");
    assert!(
        description.contains("open a CSV file first"),
        "{description}"
    );
    empty.perform(vec![
        Action::OpenPalette,
        Action::PaletteRun("ui:new_nca".to_owned()),
    ]);
    assert!(
        empty
            .notice_text()
            .unwrap()
            .contains("Open a CSV file first")
    );
    assert!(empty.engine().project().analyses().is_empty());
    // A command that is not in the list does nothing, quietly.
    app.perform(vec![Action::PaletteRun("cmd:no.such".to_owned())]);
    assert!(app.engine().project().analyses().is_empty());
}

#[test]
fn analyses_and_worksheets_open_by_name_and_the_theme_switches() {
    let mut app = with_analysis();
    let id = app.overview.analyses[0].id;
    app.perform(vec![
        Action::Select(Selection::Welcome),
        Action::OpenPalette,
    ]);
    type_query(&mut app, "oral");
    let keys: Vec<String> = app.palette_results().into_iter().map(|e| e.key).collect();
    assert!(keys.contains(&format!("analysis:{id}")), "{keys:?}");
    assert!(keys.contains(&"worksheet:1".to_owned()), "{keys:?}");
    app.perform(vec![Action::PaletteRun(format!("analysis:{id}"))]);
    assert_eq!(app.state.selection, Selection::Analysis(id));
    assert!(app.nca_page().is_some());
    app.perform(vec![Action::OpenPalette]);
    type_query(&mut app, "worksheet");
    app.perform(vec![Action::PaletteRun("worksheet:1".to_owned())]);
    assert_eq!(app.state.selection, Selection::Worksheet(1));
    // Theme.
    assert_eq!(app.state.mode, ThemeMode::Light);
    app.perform(vec![Action::OpenPalette]);
    type_query(&mut app, "dark");
    assert_eq!(first_key(&app).as_deref(), Some("ui:theme"));
    app.perform(vec![Action::PaletteRun("ui:theme".to_owned())]);
    assert_eq!(app.state.mode, ThemeMode::Dark);
    type_query(&mut app, "light");
    assert_eq!(first_key(&app).as_deref(), Some("ui:theme"));
}

#[test]
fn registry_commands_run_with_the_selection_or_open_their_page() {
    let mut app = with_analysis();
    // fit.run opens the fit page for the worksheet in view; model.simulate opens its page.
    app.perform(vec![Action::Select(Selection::Worksheet(1))]);
    app.perform(vec![Action::PaletteRun("cmd:fit.run".to_owned())]);
    assert!(app.fit_page().is_some());
    app.perform(vec![Action::PaletteRun("cmd:model.simulate".to_owned())]);
    assert!(app.sim_page().is_some());
    // analysis.run with the analysis in view refreshes a stale result.
    let id = app.overview.analyses[0].id;
    app.perform(vec![Action::Select(Selection::Analysis(id))]);
    app.perform(vec![Action::SetCell {
        worksheet: 1,
        row: 4,
        column: "Conc".to_owned(),
        text: "4,2".to_owned(),
    }]);
    assert!(app.overview.analyses[0].status.is_stale());
    app.perform(vec![Action::PaletteRun("cmd:analysis.run".to_owned())]);
    assert!(!app.overview.analyses[0].status.is_stale());
    // Without an analysis in view the entry says what to do instead of guessing.
    app.perform(vec![Action::Select(Selection::Welcome)]);
    app.perform(vec![Action::PaletteRun("cmd:analysis.run".to_owned())]);
    assert!(
        app.notice_text()
            .unwrap()
            .contains("open an analysis first")
    );
    // A command without a page says where to run it.
    app.perform(vec![Action::PaletteRun("cmd:history.list".to_owned())]);
    let said = app.notice_text().unwrap();
    assert!(
        said.contains("history.list") && said.contains("caladrius-cli"),
        "{said}"
    );
    // project.describe says what the project holds.
    app.perform(vec![Action::PaletteRun("cmd:project.describe".to_owned())]);
    let said = app.notice_text().unwrap();
    assert!(said.contains("1 worksheet(s), 1 analysis(es)"), "{said}");
    // The project commands are the same actions as the File menu.
    app.perform(vec![Action::PaletteRun("cmd:project.save".to_owned())]);
    assert!(matches!(
        app.take_requests().as_slice(),
        [crate::app::Request::SaveProject { .. }]
    ));
}

#[test]
fn the_palette_state_is_data() {
    let state = UiState {
        palette: PaletteState {
            open: true,
            query: "fit".to_owned(),
            selected: 2,
        },
        ..UiState::default()
    };
    let text = serde_json::to_string(&state).unwrap();
    assert_eq!(serde_json::from_str::<UiState>(&text).unwrap(), state);
    // An older saved state without the palette still reads.
    let older = r#"{"mode":"light","selection":"welcome","log_axis":false,"nca":null,"fit":null,"sim":null}"#;
    assert!(!serde_json::from_str::<UiState>(older).unwrap().palette.open);
}

#[test]
fn the_arrow_keys_move_the_highlight_and_enter_runs_it() {
    let mut app = with_oral();
    app.perform(vec![Action::OpenPalette]);
    let mut h = harness(app);
    h.run_steps(3);
    // The list starts on the first entry; Down moves to the second (the model fit).
    h.key_press(Key::ArrowDown);
    h.run_steps(2);
    assert_eq!(h.state().state.palette.selected, 1);
    h.key_press(Key::ArrowUp);
    h.run_steps(2);
    assert_eq!(h.state().state.palette.selected, 0);
    h.key_press(Key::ArrowDown);
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(4);
    assert!(h.state().fit_page().is_some());
    assert!(!h.state().state.palette.open);
}

#[test]
fn typing_a_verb_puts_the_action_before_the_command_of_the_same_name() {
    let mut app = with_oral();
    app.perform(vec![Action::OpenPalette]);
    type_query(&mut app, "fit");
    assert_eq!(first_key(&app).as_deref(), Some("ui:new_fit"));
    assert!(app.palette_results().iter().any(|e| e.key == "cmd:fit.run"));
    type_query(&mut app, "fit.run");
    assert_eq!(first_key(&app).as_deref(), Some("cmd:fit.run"));
}
