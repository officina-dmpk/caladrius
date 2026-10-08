//! Tests of the project file flow: save, open, new, the modified marker, the question before work
//! is lost, and the project tree (names from content, stale results).

use egui::{Key, Modifiers};
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};

use crate::app::{Action, Request, Selection, UiApp};
use crate::projectfile::{Guarded, is_project_file};
use crate::tests::{harness, with_analysis, with_oral};

/// The bytes of the one `SaveProject` request the app made, with its other fields.
fn saved(app: &mut UiApp) -> (String, Vec<u8>, bool) {
    let mut found = None;
    for request in app.take_requests() {
        if let Request::SaveProject {
            suggested,
            bytes,
            ask,
        } = request
        {
            found = Some((suggested, bytes, ask));
        }
    }
    found.expect("a save was requested")
}

fn edit_a_cell(app: &mut UiApp) {
    app.perform(vec![Action::SetCell {
        worksheet: 1,
        row: 4,
        column: "Conc".to_owned(),
        text: "4,2".to_owned(),
    }]);
}

#[test]
fn a_project_is_modified_when_it_differs_from_the_last_saved_one() {
    let mut app = UiApp::new();
    assert!(!app.is_dirty());
    assert_eq!(app.window_title(), "Untitled project - Caladrius");
    app.load_csv("oral.csv", crate::tests::ORAL.as_bytes());
    app.perform(vec![Action::ImportConfirm]);
    assert!(app.is_dirty());
    assert_eq!(app.window_title(), "* Untitled project - Caladrius");
    // A first save has no file yet: it asks where, and suggests the name from the project.
    app.perform(vec![Action::SaveProject]);
    let (suggested, bytes, ask) = saved(&mut app);
    assert_eq!(suggested, "untitled-project.caladrius.json");
    assert!(ask);
    // Nothing is clean until the program says the file was written.
    assert!(app.is_dirty());
    app.project_saved("study.caladrius.json");
    assert!(!app.is_dirty());
    assert_eq!(app.file_name(), Some("study.caladrius.json"));
    assert_eq!(app.window_title(), "Untitled project - Caladrius");
    // The document is the project as `project.save` gives it.
    let doc: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(doc["format"], "caladrius-project");
    assert_eq!(doc["worksheets"].as_array().unwrap().len(), 1);
    // Changing the data makes it modified again; saving a project with a file does not ask.
    edit_a_cell(&mut app);
    assert!(app.is_dirty());
    app.perform(vec![Action::SaveProject]);
    let (suggested, _, ask) = saved(&mut app);
    assert_eq!((suggested.as_str(), ask), ("study.caladrius.json", false));
    app.project_saved("study.caladrius.json");
    assert!(!app.is_dirty());
    // Save as always asks.
    app.perform(vec![Action::SaveProjectAs]);
    assert!(saved(&mut app).2);
}

#[test]
fn a_save_that_fails_or_is_given_up_leaves_the_project_modified() {
    let mut app = with_oral();
    app.perform(vec![Action::SaveProject]);
    let _ = saved(&mut app);
    app.project_save_failed("cannot write study.caladrius.json: access denied");
    assert!(app.is_dirty());
    assert_eq!(
        app.notice_text(),
        Some("cannot write study.caladrius.json: access denied")
    );
    app.perform(vec![Action::SaveProject]);
    let _ = saved(&mut app);
    app.project_save_cancelled();
    assert!(app.is_dirty() && app.file_name().is_none());
}

#[test]
fn what_is_saved_opens_as_the_same_project() {
    let mut app = with_analysis();
    app.perform(vec![Action::NewFit]);
    app.perform(vec![Action::RunFit]);
    app.perform(vec![Action::NewSimulation, Action::SimSave]);
    let before = app.engine().project().clone();
    assert_eq!(before.analyses().len(), 3);
    app.perform(vec![Action::SaveProject]);
    let (_, bytes, _) = saved(&mut app);
    // Another session opens the file: worksheets, analyses, options and results are the same.
    let mut other = UiApp::new();
    assert!(other.open_project("study.caladrius.json", &bytes));
    assert_eq!(other.engine().project(), &before);
    assert!(!other.is_dirty());
    assert_eq!(other.file_name(), Some("study.caladrius.json"));
    assert_eq!(other.state.selection, Selection::Welcome);
    // The tree lists them, and each one opens as its own page.
    let ids: Vec<u64> = other.overview.analyses.iter().map(|a| a.id).collect();
    assert_eq!(ids.len(), 3);
    for id in ids {
        other.perform(vec![Action::Select(Selection::Analysis(id))]);
        assert!(
            other.nca_page().is_some() || other.fit_page().is_some() || other.sim_page().is_some()
        );
    }
    assert!(other.notice_text().is_some());
}

#[test]
fn a_file_that_is_not_a_project_changes_nothing_and_says_why() {
    let mut app = with_analysis();
    let before = app.engine().project().clone();
    let dirty = app.is_dirty();
    assert!(!app.open_project("notes.json", b"{ not a project"));
    let message = app.notice_text().unwrap().to_owned();
    assert!(
        message
            .starts_with("notes.json cannot be opened: this is not a readable Caladrius project"),
        "{message}"
    );
    assert!(!app.open_project("binary.json", &[0xff, 0xfe, 0x00]));
    assert!(
        app.notice_text().unwrap().contains("not UTF-8"),
        "{:?}",
        app.notice_text()
    );
    assert!(!app.open_project("other.json", br#"{"format":"something-else"}"#));
    assert_eq!(app.engine().project(), &before);
    assert_eq!(app.is_dirty(), dirty);
    assert!(app.file_name().is_none());
}

#[test]
fn unsaved_changes_are_asked_about_before_a_new_project_replaces_them() {
    let mut app = with_analysis();
    assert!(app.is_dirty());
    app.perform(vec![Action::NewProject]);
    assert_eq!(app.guard(), Some(&Guarded::NewProject));
    // Nothing changed yet; Cancel keeps everything.
    assert_eq!(app.engine().project().worksheets().len(), 1);
    app.perform(vec![Action::GuardCancel]);
    assert!(app.guard().is_none());
    assert_eq!(app.engine().project().worksheets().len(), 1);
    // Don't save: the new project is empty and clean, and the tree is empty.
    app.perform(vec![Action::NewProject, Action::GuardDiscard]);
    assert!(app.engine().project().worksheets().is_empty());
    assert!(!app.is_dirty() && app.file_name().is_none());
    assert!(app.overview.worksheets.is_empty() && app.overview.analyses.is_empty());
    assert_eq!(app.state.selection, Selection::Welcome);
    // A clean project is replaced without a question.
    app.perform(vec![Action::NewProject]);
    assert!(app.guard().is_none());
}

#[test]
fn the_answer_save_writes_first_and_then_does_what_was_asked() {
    let mut app = with_analysis();
    app.perform(vec![Action::OpenProject, Action::GuardSave]);
    // A save request, and no open request until the file is written.
    let requests = app.take_requests();
    assert!(matches!(
        requests.as_slice(),
        [Request::SaveProject { ask: true, .. }]
    ));
    app.project_saved("a.caladrius.json");
    assert_eq!(app.take_requests(), vec![Request::OpenProject]);
    assert!(!app.is_dirty());
    // If the person gives up choosing the file, nothing opens and nothing is lost.
    edit_a_cell(&mut app);
    app.perform(vec![Action::OpenProject, Action::GuardSave]);
    let _ = app.take_requests();
    app.project_save_cancelled();
    assert!(app.take_requests().is_empty());
    assert!(app.is_dirty() && app.guard().is_none());
    // A save that fails stops there too.
    app.perform(vec![Action::NewProject, Action::GuardSave]);
    let _ = app.take_requests();
    app.project_save_failed("cannot write a.caladrius.json: disk full");
    assert!(app.take_requests().is_empty());
    assert_eq!(app.engine().project().worksheets().len(), 1);
}

#[test]
fn a_dropped_project_asks_first_and_replaces_the_project_when_allowed() {
    let mut first = with_analysis();
    first.perform(vec![Action::SaveProject]);
    let (_, bytes, _) = saved(&mut first);
    let wanted = first.engine().project().clone();
    let mut app = with_oral();
    edit_a_cell(&mut app);
    app.open_project_guarded("first.caladrius.json", bytes.clone());
    assert!(matches!(app.guard(), Some(Guarded::OpenBytes { .. })));
    app.perform(vec![Action::GuardDiscard]);
    assert_eq!(app.engine().project(), &wanted);
    assert_eq!(app.file_name(), Some("first.caladrius.json"));
    // Clean: no question.
    app.open_project_guarded("first.caladrius.json", bytes);
    assert!(app.guard().is_none());
}

#[test]
fn closing_the_window_with_unsaved_changes_asks_and_then_closes() {
    let mut app = UiApp::new();
    assert!(app.close_requested());
    let mut app = with_oral();
    assert!(!app.close_requested());
    assert_eq!(app.guard(), Some(&Guarded::Close));
    app.perform(vec![Action::GuardCancel]);
    assert!(app.take_requests().is_empty());
    // Quit from the menu asks the same question.
    app.perform(vec![Action::Quit]);
    assert_eq!(app.guard(), Some(&Guarded::Close));
    app.perform(vec![Action::GuardDiscard]);
    assert_eq!(app.take_requests(), vec![Request::Close]);
    assert!(app.close_requested());
}

#[test]
fn the_question_is_a_sentence_with_three_choices_and_each_one_works() {
    let mut app = with_oral();
    app.perform(vec![Action::NewProject]);
    let mut h = harness(app);
    h.run_steps(3);
    h.get_by_label("Save the changes?");
    h.get_by_label(
        "The project \"Untitled project\" has changes that are not saved. Starting a new project would lose them.",
    );
    h.get_by_label("Save");
    h.get_by_label("Cancel");
    h.get_by_label("Don't save").click();
    h.run_steps(3);
    assert!(h.state().engine().project().worksheets().is_empty());
    assert!(h.state().guard().is_none());
    assert!(h.query_by_label("Save the changes?").is_none());

    // The sentence names what is at stake for the other two requests.
    assert!(
        crate::projectmenu::guard_sentence("study", &Guarded::OpenProject)
            .ends_with("Opening another project would lose them.")
    );
    assert!(
        crate::projectmenu::guard_sentence("study", &Guarded::Close)
            .ends_with("Closing Caladrius would lose them.")
    );
}

#[test]
fn the_file_menu_and_the_shortcuts_reach_the_same_actions() {
    let app = with_oral();
    let mut h = harness(app);
    h.run_steps(3);
    h.get_by_label("File").click();
    h.run_steps(3);
    for entry in ["New project", "Open project…", "Save", "Save as…", "Quit"] {
        assert!(
            h.query_all_by_label_contains(entry).next().is_some(),
            "{entry}"
        );
    }
    // Open CSV is in the menu and on the bar.
    assert_eq!(h.query_all_by_label("Open CSV…").count(), 2);
    h.get_by_label_contains("Save as…").click();
    h.run_steps(3);
    let requests = h.state_mut().take_requests();
    assert!(matches!(
        requests.as_slice(),
        [Request::SaveProject { ask: true, .. }]
    ));
    // Ctrl+S, Ctrl+Shift+S and Ctrl+O.
    h.key_press_modifiers(Modifiers::COMMAND, Key::S);
    h.run_steps(2);
    assert!(matches!(
        h.state_mut().take_requests().as_slice(),
        [Request::SaveProject { .. }]
    ));
    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::S);
    h.run_steps(2);
    assert!(matches!(
        h.state_mut().take_requests().as_slice(),
        [Request::SaveProject { ask: true, .. }]
    ));
    h.state_mut().project_save_cancelled();
    h.key_press_modifiers(Modifiers::COMMAND, Key::O);
    h.run_steps(2);
    // The project is modified: the question comes first.
    assert_eq!(h.state().guard(), Some(&Guarded::OpenProject));
}

#[test]
fn project_files_are_recognised_by_their_name() {
    assert!(is_project_file("study.caladrius.json"));
    assert!(is_project_file("STUDY.JSON"));
    assert!(!is_project_file("study.csv"));
    assert!(!is_project_file("json"));
}

// ---- the project tree ------------------------------------------------------------------------

#[test]
fn analyses_are_named_from_their_content_and_a_new_one_gets_a_name_not_a_number() {
    let mut app = with_analysis();
    app.perform(vec![Action::NewFit]);
    app.perform(vec![Action::RunFit]);
    let labels: Vec<String> = app
        .overview
        .analyses
        .iter()
        .map(|a| a.label.clone())
        .collect();
    assert_eq!(labels.len(), 2);
    assert!(labels[0].starts_with("NCA of oral, subject"), "{labels:?}");
    assert!(
        labels[1].starts_with("Fit pk1.oral_1 to oral"),
        "{labels:?}"
    );
    // The tree shows exactly these names.
    let mut h = harness(app);
    h.run_steps(3);
    for label in &labels {
        assert!(
            h.query_all_by_label_contains(label).next().is_some(),
            "{label}"
        );
    }
}

#[test]
fn an_input_change_marks_every_dependent_analysis_stale_clearly_in_the_tree() {
    let mut app = with_analysis();
    app.perform(vec![Action::NewFit, Action::RunFit]);
    assert!(app.overview.analyses.iter().all(|a| !a.status.is_stale()));
    edit_a_cell(&mut app);
    assert!(app.overview.analyses.iter().all(|a| a.status.is_stale()));
    let mut h = harness(app);
    h.run_steps(3);
    h.get_by_label_contains("2 analysis result(s) out of date");
    assert_eq!(h.get_all_by_label("   out of date").count(), 2);
    // Run again from the tree refreshes that analysis, and only that one.
    h.get_all_by_label("Run again").next().unwrap().click();
    h.run_steps(3);
    let stale: Vec<bool> = h
        .state()
        .overview
        .analyses
        .iter()
        .map(|a| a.status.is_stale())
        .collect();
    assert_eq!(stale, vec![false, true]);
    h.get_by_label_contains("1 analysis result(s) out of date");
}

#[test]
fn the_tree_says_which_file_the_project_is_in_and_whether_it_is_modified() {
    let mut app = with_oral();
    app.perform(vec![Action::SaveProject]);
    let _ = saved(&mut app);
    app.project_saved("study.caladrius.json");
    let mut h = harness(app);
    h.run_steps(3);
    h.get_by_label("Untitled project  (study.caladrius.json)");
    edit_a_cell(h.state_mut());
    h.run_steps(3);
    h.get_by_label("Untitled project *  (study.caladrius.json)");
}

#[test]
fn project_describe_and_the_history_are_untouched_by_the_file_flow() {
    // Save and open are commands: they are in the history, with their ids.
    let mut app = with_oral();
    app.perform(vec![Action::SaveProject]);
    let _ = saved(&mut app);
    let ids: Vec<String> = app
        .engine()
        .history()
        .entries()
        .iter()
        .map(|e| e.command.clone())
        .collect();
    assert!(ids.iter().any(|c| c == "project.save"), "{ids:?}");
    app.perform(vec![Action::NewProject, Action::GuardDiscard]);
    let described = app
        .engine_mut()
        .execute("project.describe", json!({}))
        .unwrap();
    assert_eq!(described["worksheets"], json!([]));
    assert!(
        app.engine()
            .history()
            .entries()
            .iter()
            .any(|e| e.command == "project.new")
    );
}
