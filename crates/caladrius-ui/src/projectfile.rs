//! Project files: new, open, save, save as, the modified marker and the question asked before work
//! is lost. The interface never touches the disk: a save is a [`Request::SaveProject`] with the
//! bytes of the document (from the `project.save` command) for the program around to write, and
//! an open is bytes handed back with [`UiApp::open_project`]. A project is modified when it
//! differs from the one last saved or opened.

use std::cell::Cell;

use caladrius_engine::Project;
use serde_json::{Value, json};

use crate::app::{Notice, NoticeKind, Request, Selection, UiApp};
use crate::fmt;

/// What waits for the answer to "save the changes?".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Guarded {
    /// Start a new, empty project.
    NewProject,
    /// Ask for a project file to open.
    OpenProject,
    /// Open a project file that is already read (dropped on the window, or given on the command
    /// line). `token` is the program's own mark for the file; it is given back by
    /// [`UiApp::take_opened_token`] once the file has really been opened.
    OpenBytes {
        name: String,
        bytes: Vec<u8>,
        token: Option<u64>,
    },
    /// Close the application.
    Close,
}

/// The state around the project file. It belongs to the program that runs the interface, not to
/// the saved UI state.
#[derive(Debug, Clone, Default)]
pub(crate) struct FileState {
    /// The name of the file the project was opened from or saved to.
    pub name: Option<String>,
    /// The project as last saved or opened: what "modified" is measured against.
    pub saved: Project,
    /// Changes whenever `saved` is replaced.
    pub saved_generation: u64,
    /// The token of the file opened last through [`Guarded::OpenBytes`], until it is taken.
    pub opened_token: Option<u64>,
    /// `(engine revision, saved generation, modified)`: the last answer of [`UiApp::is_dirty`].
    pub dirty_cache: Cell<Option<(u64, u64, bool)>>,
    /// The question on screen, if any.
    pub guard: Option<Guarded>,
    /// What to do once the save that was asked for has been written.
    pub after_save: Option<Guarded>,
    /// The project as it was when the save was requested; it becomes `saved` when it is written.
    pub pending_save: Option<Project>,
    /// The person said the application may close.
    pub may_close: bool,
}

/// True for the name of a project file (`study.caladrius.json`). A file dropped on the window or
/// given on the command line is a project only if it is named so; the Open dialog also offers
/// any `.json` file, because the person chose it.
pub fn is_project_file(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".caladrius.json")
}

impl UiApp {
    /// True when the project differs from the one last saved or opened.
    pub fn is_dirty(&self) -> bool {
        // The project can only change through a command, which moves the engine's revision, so the
        // comparison is made again only when that number or the saved project has changed.
        let key = (self.engine.revision(), self.file.saved_generation);
        if let Some((revision, generation, dirty)) = self.file.dirty_cache.get() {
            if (revision, generation) == key {
                return dirty;
            }
        }
        let dirty = self.engine.project() != &self.file.saved;
        self.file.dirty_cache.set(Some((key.0, key.1, dirty)));
        dirty
    }

    /// Takes the project as it is now for the saved one (the program wrote it, or it was opened).
    pub(crate) fn mark_saved(&mut self) {
        self.file.saved = self.engine.project().clone();
        self.file.saved_generation = self.file.saved_generation.wrapping_add(1);
    }

    /// The token given with a dropped project file, once that file has been opened (after the
    /// question, if there was one). `None` while it waits, or when it was never opened.
    pub fn take_opened_token(&mut self) -> Option<u64> {
        self.file.opened_token.take()
    }

    /// The name of the file the project is saved in, if it has one.
    pub fn file_name(&self) -> Option<&str> {
        self.file.name.as_deref()
    }

    /// What the window title says: the project name, a marker when it has unsaved changes.
    pub fn window_title(&self) -> String {
        let marker = if self.is_dirty() { "* " } else { "" };
        format!("{marker}{} - Caladrius", self.engine.project().name())
    }

    /// The question on screen, for tests.
    pub fn guard(&self) -> Option<&Guarded> {
        self.file.guard.as_ref()
    }

    /// The program asks whether the window may close now. When there are unsaved changes the
    /// answer is no and the question appears; the program asks again after the person decided.
    pub fn close_requested(&mut self) -> bool {
        if self.file.may_close || !self.is_dirty() {
            return true;
        }
        self.file.guard = Some(Guarded::Close);
        false
    }

    fn info(&mut self, text: String) {
        self.notice = Some(Notice {
            kind: NoticeKind::Info,
            text,
        });
    }

    fn failure(&mut self, text: String) {
        self.notice = Some(Notice {
            kind: NoticeKind::Error,
            text,
        });
    }

    // ---- asking before work is lost ----------------------------------------------------------

    /// Does what was asked, after asking first if the project has unsaved changes.
    pub(crate) fn guarded(&mut self, what: Guarded) {
        if self.is_dirty() {
            self.file.guard = Some(what);
        } else {
            self.proceed(what);
        }
    }

    fn proceed(&mut self, what: Guarded) {
        self.file.guard = None;
        match what {
            Guarded::NewProject => self.new_project(),
            Guarded::OpenProject => self.requests.push(Request::OpenProject),
            Guarded::OpenBytes { name, bytes, token } => {
                if self.open_project(&name, &bytes) {
                    self.file.opened_token = token;
                }
            }
            Guarded::Close => {
                self.file.may_close = true;
                self.requests.push(Request::Close);
            }
        }
    }

    /// The answer "Save": write the project, then do what was asked.
    pub(crate) fn guard_save(&mut self) {
        if let Some(what) = self.file.guard.take() {
            self.file.after_save = Some(what);
            self.save_project(false);
        }
    }

    /// The answer "Don't save": do what was asked and lose the changes.
    pub(crate) fn guard_discard(&mut self) {
        if let Some(what) = self.file.guard.take() {
            self.proceed(what);
        }
    }

    /// The answer "Cancel".
    pub(crate) fn guard_cancel(&mut self) {
        self.file.guard = None;
        self.file.after_save = None;
    }

    // ---- save --------------------------------------------------------------------------------

    /// Asks the program to write the project (`project.save`). `ask_name`: always ask where
    /// (Save as); a project without a file asks too.
    pub(crate) fn save_project(&mut self, ask_name: bool) {
        let Ok(saved) = self.call("project.save", json!({})) else {
            self.file.after_save = None;
            return;
        };
        let suggested = saved
            .get("file_name")
            .and_then(Value::as_str)
            .unwrap_or("project.caladrius.json")
            .to_owned();
        let Some(document) = saved.get("project") else {
            self.file.after_save = None;
            return;
        };
        match serde_json::to_vec_pretty(document) {
            Ok(bytes) => {
                self.file.pending_save = Some(self.engine.project().clone());
                let ask = ask_name || self.file.name.is_none();
                self.requests.push(Request::SaveProject {
                    suggested: self.file.name.clone().unwrap_or(suggested),
                    bytes,
                    ask,
                });
            }
            Err(e) => {
                self.file.after_save = None;
                self.failure(format!("The project cannot be saved: {e}"));
            }
        }
    }

    /// The program wrote the file `name`: the project is no longer modified.
    pub fn project_saved(&mut self, name: &str) {
        match self.file.pending_save.take() {
            Some(project) => {
                self.file.saved = project;
                self.file.saved_generation = self.file.saved_generation.wrapping_add(1);
            }
            None => self.mark_saved(),
        }
        self.file.name = Some(name.to_owned());
        self.info(format!("Saved {name}."));
        if let Some(what) = self.file.after_save.take() {
            self.proceed(what);
        }
    }

    /// The program could not write the file: the sentence says why. Nothing else happens.
    pub fn project_save_failed(&mut self, message: &str) {
        self.file.pending_save = None;
        self.file.after_save = None;
        self.failure(message.to_owned());
    }

    /// The person gave up choosing a file: the project stays modified and nothing is lost.
    pub fn project_save_cancelled(&mut self) {
        self.file.pending_save = None;
        self.file.after_save = None;
    }

    // ---- open and new --------------------------------------------------------------------------

    /// The program read the project file `name` (`project.load`); true when it was opened. A file
    /// that cannot be read changes nothing and says why.
    pub fn open_project(&mut self, name: &str, bytes: &[u8]) -> bool {
        match self.engine.load_bytes(bytes) {
            Ok(done) => {
                self.reset_views();
                self.mark_saved();
                self.file.name = Some(name.to_owned());
                let note = done
                    .get("history_note")
                    .and_then(Value::as_str)
                    .map(|n| format!(" ({})", fmt::plain(n)))
                    .unwrap_or_default();
                let sheets = self.overview.worksheets.len();
                let analyses = self.overview.analyses.len();
                self.info(format!(
                    "Opened {name}: {sheets} worksheet(s), {analyses} analysis(es){note}."
                ));
                true
            }
            Err(e) => {
                self.failure(format!(
                    "{name} cannot be opened: {}",
                    fmt::plain(&e.message)
                ));
                false
            }
        }
    }

    /// A project file that is already read (dropped on the window): asks first if work would be
    /// lost. `token` comes back from [`UiApp::take_opened_token`] when, and only when, the file
    /// was opened.
    pub fn open_project_guarded(&mut self, name: &str, bytes: Vec<u8>, token: Option<u64>) {
        self.guarded(Guarded::OpenBytes {
            name: name.to_owned(),
            bytes,
            token,
        });
    }

    fn new_project(&mut self) {
        if self.call("project.new", json!({})).is_ok() {
            self.reset_views();
            self.mark_saved();
            self.file.name = None;
            self.info("New project.".to_owned());
        }
    }

    /// Shows the welcome screen over the project that was just loaded or started.
    fn reset_views(&mut self) {
        self.state.selection = Selection::Welcome;
        self.state.nca = None;
        self.state.fit = None;
        self.state.sim = None;
        self.sheet = None;
        self.rejected = None;
        self.pending = None;
        self.refresh_overview();
    }
}
