//! The application: the project tree, the screens, and the one place where the engine is called.
//! The screens draw and return [`Action`]s; the application applies them through `caladrius-engine`
//! commands and then reads the new state back. Nothing here computes a number or reads a file.

use caladrius_engine::Engine;
use egui::{Context, RichText, Ui};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::fit::FitPage;
use crate::fitform;
use crate::fitplots;
use crate::fmt;
use crate::import::{self, PendingImport};
use crate::model::{Overview, Status, Table, WorksheetInfo, read};
use crate::nca::{self, NcaPage};
use crate::palette::{self, CommandLine, PaletteState};
use crate::projectfile::{FileState, Guarded};
use crate::projectmenu;
use crate::settings::{Settings, ThemeChoice};
use crate::settingspage;
use crate::sheet::{self, Rejected};
use crate::sim::{self, SimPage};
use crate::theme::{ThemeMode, Tokens};

/// What a screen asks for. Applied after the frame, through engine commands.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Select(Selection),
    OpenCsv,
    /// A new NCA from the worksheet in view.
    NewAnalysis,
    /// A new model fit from the worksheet in view.
    NewFit,
    /// A new model simulation (no worksheet needed).
    NewSimulation,
    /// The fit page changed: show the live curve again; `regenerate` also asks for new starting
    /// values from the data (the subject, model or dose changed).
    FitChanged {
        regenerate: bool,
    },
    /// Fit the model (`fit.run`).
    RunFit,
    /// The simulation page changed: ask `model.simulate` again.
    SimChanged,
    /// Store the simulation in the project.
    SimSave,
    ToggleTheme,
    /// The NCA page changed: ask `nca.run` again.
    RunNca,
    /// Re-run a stale analysis with its stored options.
    RunAgain(u64),
    SetCell {
        worksheet: u64,
        row: usize,
        column: String,
        text: String,
    },
    SetColumn {
        worksheet: u64,
        column: String,
        role: Option<String>,
        unit: Option<String>,
    },
    ImportReload,
    ImportConfirm,
    ImportCancel,
    /// A message for the status bar.
    Notice(String),
    /// Start a new empty project (asks first when there are unsaved changes).
    NewProject,
    /// Open a project file (asks first when there are unsaved changes).
    OpenProject,
    /// Save the project (asks for a file name when it has none).
    SaveProject,
    /// Save the project under another file name.
    SaveProjectAs,
    /// Close the application (asks first when there are unsaved changes).
    Quit,
    /// The answers to "save the changes?".
    GuardSave,
    GuardDiscard,
    GuardCancel,
    /// Open the command palette (Ctrl+K).
    OpenPalette,
    ClosePalette,
    /// Run the palette entry with this key.
    PaletteRun(String),
    /// Say what the project holds (`project.describe`).
    DescribeProject,
    /// Open the settings page.
    OpenSettings,
    /// Change one setting, by its key (`nca.auc_method`).
    SetSetting {
        key: String,
        value: Value,
    },
    /// Put one setting back to its default.
    ResetSetting(String),
    /// Take the decimal mark the system locale suggests.
    UseSystemSuggestion,
}

/// What the engine understood of a typed cell, when that is not what was typed (`3,25` is 3.25).
fn understood_note(typed: &str, answer: &Value) -> Option<String> {
    let typed = typed.trim();
    match answer.get("understood")? {
        Value::Number(n) => {
            let shown = crate::fmt::exact_point(n.as_f64()?);
            (typed != shown).then(|| format!("Read `{typed}` as {shown}."))
        }
        Value::Null if !typed.is_empty() => Some(format!("Read `{typed}` as a missing value.")),
        _ => None,
    }
}

/// What the main area shows.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Selection {
    #[default]
    Welcome,
    Import,
    Worksheet(u64),
    Analysis(u64),
    /// A model fit being set up, not yet run.
    NewFit,
    /// A model simulation being set up.
    NewSimulation,
    /// The settings page.
    Settings,
}

/// Which analysis page is on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shown {
    Nca,
    Fit,
    Sim,
    Nothing,
}

/// What the application asks of the program around it (files are the program's business).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// Ask the person for a CSV file and give it back with [`UiApp::load_csv`].
    PickCsv,
    /// Ask the person for a project file and give it back with [`UiApp::open_project`].
    OpenProject,
    /// Write the project document. `suggested` is the file name to offer; `ask`: let the person
    /// choose where (Save as, or a project that has no file yet), else write over the file the
    /// project came from. Answer with [`UiApp::project_saved`], [`UiApp::project_save_failed`] or
    /// [`UiApp::project_save_cancelled`].
    SaveProject {
        suggested: String,
        bytes: Vec<u8>,
        ask: bool,
    },
    /// Close the window: the person has decided about the unsaved changes.
    Close,
    /// Store the settings (JSON text) with the application's configuration.
    SaveSettings(String),
}

/// The UI state that is worth keeping: serializable (golden rule 7).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct UiState {
    pub mode: ThemeMode,
    pub selection: Selection,
    /// Semi-log concentration axis.
    pub log_axis: bool,
    /// The NCA page being worked on.
    pub nca: Option<NcaPage>,
    /// The fit page being worked on.
    pub fit: Option<FitPage>,
    /// The simulation page being worked on.
    pub sim: Option<SimPage>,
    /// The command palette.
    #[serde(default)]
    pub palette: PaletteState,
    /// What is typed in the search box of the settings page.
    #[serde(default)]
    pub settings_query: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum NoticeKind {
    Info,
    Error,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Notice {
    pub(crate) kind: NoticeKind,
    pub(crate) text: String,
}

/// The worksheet shown: its description and its cells, as the engine gave them.
#[derive(Debug, Clone, Default)]
pub(crate) struct SheetView {
    pub(crate) info: WorksheetInfo,
    pub(crate) table: Table,
}

/// The application.
pub struct UiApp {
    pub(crate) engine: Engine,
    pub state: UiState,
    tokens: Tokens,
    applied: Option<ThemeMode>,
    pub(crate) overview: Overview,
    pub(crate) sheet: Option<SheetView>,
    pub(crate) rejected: Option<Rejected>,
    pub(crate) pending: Option<PendingImport>,
    pub(crate) notice: Option<Notice>,
    pub(crate) requests: Vec<Request>,
    pub(crate) file: FileState,
    /// The registry as the palette lists it (read once).
    pub(crate) commands: Vec<CommandLine>,
    /// The settings of the person.
    pub settings: Settings,
    /// What the program last was asked to store.
    pub(crate) saved_settings: String,
    /// The theme the operating system reports, when it does.
    pub(crate) system_theme: Option<ThemeMode>,
    theme_error: Option<String>,
}

impl Default for UiApp {
    fn default() -> Self {
        Self::new()
    }
}

impl UiApp {
    /// An application with an empty project.
    pub fn new() -> UiApp {
        UiApp::with_engine(Engine::new())
    }

    /// An application over an existing engine (a loaded project).
    pub fn with_engine(engine: Engine) -> UiApp {
        let state = UiState::default();
        let (tokens, theme_error) = match Tokens::embedded(state.mode) {
            Ok(t) => (t, None),
            Err(e) => (Tokens::default(), Some(e)),
        };
        let mut app = UiApp {
            engine,
            state,
            tokens,
            applied: None,
            overview: Overview::default(),
            sheet: None,
            rejected: None,
            pending: None,
            notice: None,
            requests: Vec::new(),
            file: FileState::default(),
            commands: palette::registry(),
            settings: Settings::default(),
            saved_settings: Settings::default().to_json(),
            system_theme: None,
            theme_error,
        };
        app.file.saved = app.engine.project().clone();
        app.refresh_overview();
        app
    }

    /// The engine, for the program that embeds the UI (saving, tests).
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// Mutable access to the engine, for tests that set a project up through commands.
    pub fn engine_mut(&mut self) -> &mut Engine {
        &mut self.engine
    }

    /// What the program around should do now (open a file picker...).
    pub fn take_requests(&mut self) -> Vec<Request> {
        std::mem::take(&mut self.requests)
    }

    /// The last message for the status bar, for tests.
    pub fn notice_text(&self) -> Option<&str> {
        self.notice.as_ref().map(|n| n.text.as_str())
    }

    /// The current NCA page, for tests.
    pub fn nca_page(&self) -> Option<&NcaPage> {
        self.state.nca.as_ref()
    }

    /// The current fit page, for tests.
    pub fn fit_page(&self) -> Option<&FitPage> {
        self.state.fit.as_ref()
    }

    /// The current simulation page, for tests.
    pub fn sim_page(&self) -> Option<&SimPage> {
        self.state.sim.as_ref()
    }

    /// The import waiting for a decision, for tests.
    pub fn pending_import(&self) -> Option<&PendingImport> {
        self.pending.as_ref()
    }

    // ---- engine access -----------------------------------------------------------------

    /// Runs a command; an error becomes the status-bar message (it says what to fix).
    pub(crate) fn call(&mut self, id: &str, params: Value) -> Result<Value, String> {
        match self.engine.execute(id, params) {
            Ok(v) => Ok(v),
            Err(e) => {
                let message = fmt::plain(&e.message);
                self.notice = Some(Notice {
                    kind: NoticeKind::Error,
                    text: message.clone(),
                });
                Err(message)
            }
        }
    }

    pub(crate) fn refresh_overview(&mut self) {
        if let Ok(v) = self.engine.execute("project.describe", Value::Null) {
            self.overview = read(&v);
        }
    }

    pub(crate) fn refresh_sheet(&mut self, id: u64) {
        let described = self.engine.execute(
            "data.describe",
            json!({ "worksheet": id, "preview_rows": 0 }),
        );
        let exported = self.engine.execute(
            "export.table",
            json!({ "table": "worksheet", "worksheet": id }),
        );
        self.sheet = match (described, exported) {
            (Ok(d), Ok(t)) => Some(SheetView {
                info: read(d.get("worksheet").unwrap_or(&Value::Null)),
                table: read(&t),
            }),
            _ => None,
        };
    }

    // ---- loading a file ------------------------------------------------------------------

    /// A CSV file read by the program: show its preview. The bytes must be UTF-8 text.
    pub fn load_csv(&mut self, name: &str, bytes: &[u8]) {
        let stem = name.rsplit(['/', '\\']).next().unwrap_or(name);
        let stem = stem
            .strip_suffix(".csv")
            .or_else(|| stem.strip_suffix(".CSV"))
            .unwrap_or(stem);
        let text = match std::str::from_utf8(bytes) {
            Ok(t) => t.strip_prefix('\u{feff}').unwrap_or(t).to_owned(),
            Err(e) => {
                self.notice = Some(Notice {
                    kind: NoticeKind::Error,
                    text: format!(
                        "{name} is not UTF-8 text (problem near byte {}); save it as CSV UTF-8 and open it again.",
                        e.valid_up_to()
                    ),
                });
                return;
            }
        };
        let mut pending = PendingImport::new(stem, text);
        let answer = self.call("data.preview", pending.preview_params());
        pending.adopt(answer);
        self.pending = Some(pending);
        self.state.selection = Selection::Import;
        self.notice = None;
    }

    // ---- applying actions ------------------------------------------------------------------

    fn select(&mut self, selection: Selection) {
        match &selection {
            Selection::Worksheet(id) => self.refresh_sheet(*id),
            Selection::Analysis(id) => self.open_analysis(*id),
            _ => {}
        }
        self.state.selection = selection;
    }

    fn open_analysis(&mut self, id: u64) {
        let Ok(view) = self.call("analysis.get", json!({ "analysis": id })) else {
            return;
        };
        match view.get("kind").and_then(Value::as_str) {
            Some("fit") => return self.open_fit(&view),
            Some("simulation") => return self.open_simulation(&view),
            _ => {}
        }
        match NcaPage::from_view(&view) {
            Some(page) => {
                self.refresh_sheet(page.worksheet);
                self.state.nca = Some(page);
                self.state.fit = None;
                self.state.sim = None;
            }
            None => {
                self.state.nca = None;
                self.notice = Some(Notice {
                    kind: NoticeKind::Info,
                    text: "This kind of analysis has no page yet; use `caladrius-cli` or the MCP server for it.".to_owned(),
                });
            }
        }
    }

    /// Takes the answer of a run into the page of its kind.
    fn adopt_run(&mut self, view: &Value) {
        // Only the page that shows this analysis takes the answer (the tree can run any of them).
        let id = view.get("id").and_then(Value::as_u64);
        if let Some(page) = self.state.nca.as_mut().filter(|p| p.analysis == id) {
            page.adopt(view);
        }
        if let Some(page) = self.state.fit.as_mut().filter(|p| p.analysis == id) {
            page.adopt(view);
        }
    }

    fn run_nca(&mut self) {
        let Some(mut page) = self.state.nca.take() else {
            return;
        };
        match self.call("nca.run", page.params()) {
            Ok(view) => {
                page.adopt(&view);
                self.notice = None;
            }
            Err(message) => page.error = Some(message),
        }
        let worksheet = page.worksheet;
        let analysis = page.analysis;
        self.state.nca = Some(page);
        self.refresh_overview();
        self.refresh_sheet(worksheet);
        if let Some(id) = analysis {
            self.state.selection = Selection::Analysis(id);
        }
    }

    fn new_analysis(&mut self) {
        let worksheet = match &self.state.selection {
            Selection::Worksheet(id) => Some(*id),
            _ => self.overview.worksheets.first().map(|w| w.id),
        };
        let Some(id) = worksheet else {
            self.notice = Some(Notice {
                kind: NoticeKind::Info,
                text: "Open a CSV file first: an analysis needs a worksheet.".to_owned(),
            });
            return;
        };
        self.refresh_sheet(id);
        let subject = self
            .sheet
            .as_ref()
            .and_then(|s| s.info.subjects.first().cloned())
            .unwrap_or_default();
        self.state.fit = None;
        self.state.sim = None;
        let mut page = NcaPage::new(id, subject);
        // The defaults of the settings, for what differs from the engine's.
        page.options = self.settings.nca_options();
        self.state.nca = Some(page);
        self.run_nca();
    }

    /// Applies actions as if the person had done them (the program around the UI and the tests
    /// use it; the screens return the same actions).
    pub fn perform(&mut self, actions: Vec<Action>) {
        for action in actions {
            match action {
                Action::Select(s) => self.select(s),
                Action::OpenCsv => self.requests.push(Request::PickCsv),
                Action::NewAnalysis => self.new_analysis(),
                Action::NewFit => self.new_fit(),
                Action::NewSimulation => self.new_simulation(),
                Action::FitChanged { regenerate } => self.fit_changed(regenerate),
                Action::RunFit => self.run_fit(),
                Action::SimChanged => self.sim_changed(),
                Action::SimSave => self.sim_save(),
                Action::ToggleTheme => {
                    self.settings.theme = match self.state.mode.toggled() {
                        ThemeMode::Light => ThemeChoice::Light,
                        ThemeMode::Dark => ThemeChoice::Dark,
                    };
                    self.apply_settings();
                    self.remember_settings();
                }
                Action::RunNca => self.run_nca(),
                Action::RunAgain(id) => {
                    if let Ok(view) = self.call("analysis.run", json!({ "analysis": id })) {
                        self.adopt_run(&view);
                        self.refresh_overview();
                    }
                }
                Action::SetCell {
                    worksheet,
                    row,
                    column,
                    text,
                } => {
                    let params = json!({ "worksheet": worksheet, "column": column, "row": row, "value": text });
                    match self.call("data.set_cell", params) {
                        Ok(answer) => {
                            self.rejected = None;
                            // Nothing to say when it was read as typed; no stale message either.
                            self.notice = understood_note(&text, &answer).map(|text| Notice {
                                kind: NoticeKind::Info,
                                text: fmt::plain(&text),
                            });
                        }
                        Err(_) => self.rejected = Some(Rejected { row, column, text }),
                    }
                    self.refresh_overview();
                    self.refresh_sheet(worksheet);
                    if self
                        .state
                        .fit
                        .as_ref()
                        .is_some_and(|p| p.worksheet == worksheet)
                    {
                        self.fit_preview();
                    }
                }
                Action::SetColumn {
                    worksheet,
                    column,
                    role,
                    unit,
                } => {
                    let mut params = json!({ "worksheet": worksheet, "column": column });
                    if let Some(role) = role {
                        params["role"] = json!(role);
                    }
                    if let Some(unit) = unit {
                        params["unit"] = json!(unit);
                    }
                    let _ = self.call("data.set_column", params);
                    self.refresh_overview();
                    self.refresh_sheet(worksheet);
                }
                Action::ImportReload => {
                    if let Some(mut p) = self.pending.take() {
                        let answer = self.call("data.preview", p.preview_params());
                        p.adopt(answer);
                        self.pending = Some(p);
                    }
                }
                Action::ImportConfirm => self.confirm_import(),
                Action::ImportCancel => {
                    self.pending = None;
                    self.state.selection = self
                        .overview
                        .worksheets
                        .last()
                        .map_or(Selection::Welcome, |w| Selection::Worksheet(w.id));
                    if let Selection::Worksheet(id) = self.state.selection {
                        self.refresh_sheet(id);
                    }
                }
                Action::Notice(text) => {
                    self.notice = Some(Notice {
                        kind: NoticeKind::Info,
                        text,
                    });
                }
                Action::NewProject => self.guarded(Guarded::NewProject),
                Action::OpenProject => self.guarded(Guarded::OpenProject),
                Action::SaveProject => self.save_project(false),
                Action::SaveProjectAs => self.save_project(true),
                Action::Quit => self.guarded(Guarded::Close),
                Action::GuardSave => self.guard_save(),
                Action::GuardDiscard => self.guard_discard(),
                Action::GuardCancel => self.guard_cancel(),
                Action::OpenPalette => self.open_palette(),
                Action::ClosePalette => self.close_palette(),
                Action::PaletteRun(key) => self.palette_run(&key),
                Action::DescribeProject => self.describe_project(),
                Action::OpenSettings => self.open_settings(),
                Action::SetSetting { key, value } => self.set_setting(&key, value),
                Action::ResetSetting(key) => self.reset_setting(&key),
                Action::UseSystemSuggestion => self.use_system_suggestion(),
            }
        }
    }

    fn confirm_import(&mut self) {
        let Some(pending) = self.pending.as_ref() else {
            return;
        };
        if !pending.can_import() {
            self.notice = Some(Notice {
                kind: NoticeKind::Info,
                text: "This reading has checks: tick the box to import it anyway, or pick another reading."
                    .to_owned(),
            });
            return;
        }
        let Some(params) = pending.import_params() else {
            return;
        };
        if let Ok(done) = self.call("data.import", params) {
            let id = done
                .pointer("/worksheet/id")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let rows = done
                .pointer("/worksheet/rows")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            self.pending = None;
            self.refresh_overview();
            self.state.selection = Selection::Worksheet(id);
            self.refresh_sheet(id);
            self.notice = Some(Notice {
                kind: NoticeKind::Info,
                text: format!("Imported {rows} rows."),
            });
        }
    }

    // ---- drawing -------------------------------------------------------------------------

    /// Draws the whole window and applies what the person did.
    pub fn ui(&mut self, ctx: &Context) {
        let system = ctx.system_theme().map(|t| match t {
            egui::Theme::Dark => ThemeMode::Dark,
            egui::Theme::Light => ThemeMode::Light,
        });
        self.follow_settings(system);
        if self.applied != Some(self.state.mode) {
            if let Ok(t) = Tokens::embedded(self.state.mode) {
                self.tokens = t;
            }
            self.tokens.apply(ctx, self.state.mode);
            self.applied = Some(self.state.mode);
        }
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for file in dropped {
            if let Some(bytes) = file.bytes {
                if crate::projectfile::is_project_file(&file.name) {
                    self.open_project_guarded(&file.name, bytes.to_vec());
                } else {
                    self.load_csv(&file.name, &bytes);
                }
            }
        }
        let tokens = self.tokens.clone();
        let mut actions: Vec<Action> = Vec::new();
        if self.file.guard.is_none() {
            projectmenu::shortcuts(ctx, &mut actions);
            if ctx.input_mut(|i| i.consume_shortcut(&palette::OPEN)) {
                actions.push(if self.state.palette.open {
                    Action::ClosePalette
                } else {
                    Action::OpenPalette
                });
            }
        }

        egui::TopBottomPanel::top("top-bar")
            .frame(tokens.panel_frame(tokens.colors.panel))
            .show(ctx, |ui| self.top_bar(ui, &mut actions));
        egui::TopBottomPanel::bottom("status-bar")
            .frame(tokens.panel_frame(tokens.colors.panel_alt))
            .show(ctx, |ui| self.status_bar(ui));
        egui::SidePanel::left("project-tree")
            .default_width(tokens.size.tree_width)
            .width_range(tokens.size.tree_min_width..=tokens.size.tree_max_width)
            .frame(tokens.panel_frame(tokens.colors.panel_alt))
            .show(ctx, |ui| self.tree(ui, &mut actions));
        let shown = self.shown();
        if shown != Shown::Nothing {
            egui::SidePanel::right("plot-panel")
                .default_width(tokens.size.plot_panel_width)
                .min_width(tokens.size.plot_panel_min_width)
                .resizable(true)
                .frame(tokens.panel_frame(tokens.colors.panel))
                .show(ctx, |ui| self.plot_panel(ui, &tokens, shown, &mut actions));
        }
        egui::CentralPanel::default()
            .frame(tokens.panel_frame(tokens.colors.background))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        self.main(ui, &tokens, &mut actions);
                    });
            });
        if self.state.palette.open && self.file.guard.is_none() {
            let results = self.palette_results();
            palette::draw(
                ctx,
                &tokens,
                &mut self.state.palette,
                &results,
                &mut actions,
            );
        }
        if let Some(what) = self.file.guard.clone() {
            let name = self.engine.project().name().to_owned();
            projectmenu::guard_dialog(ctx, &tokens, &name, &what, &mut actions);
        }
        self.perform(actions);
    }

    /// Which page the main area shows.
    fn shown(&self) -> Shown {
        let page = match self.state.selection {
            Selection::Analysis(_) => {
                if self.state.nca.is_some() {
                    Shown::Nca
                } else if self.state.fit.is_some() {
                    Shown::Fit
                } else if self.state.sim.is_some() {
                    Shown::Sim
                } else {
                    Shown::Nothing
                }
            }
            Selection::NewFit if self.state.fit.is_some() => Shown::Fit,
            Selection::NewSimulation if self.state.sim.is_some() => Shown::Sim,
            _ => Shown::Nothing,
        };
        // A page that needs a worksheet is shown only when its worksheet is loaded.
        match (page, &self.sheet) {
            (Shown::Nca | Shown::Fit, None) => Shown::Nothing,
            (page, _) => page,
        }
    }

    fn plot_panel(
        &mut self,
        ui: &mut Ui,
        tokens: &Tokens,
        shown: Shown,
        actions: &mut Vec<Action>,
    ) {
        match shown {
            Shown::Nca => {
                if let (Some(page), Some(sheet)) = (self.state.nca.as_mut(), self.sheet.as_ref()) {
                    nca::plot_panel(
                        ui,
                        tokens,
                        page,
                        &sheet.info,
                        &mut self.state.log_axis,
                        actions,
                    );
                }
            }
            Shown::Fit => {
                if let (Some(page), Some(sheet)) = (self.state.fit.as_ref(), self.sheet.as_ref()) {
                    let observed = crate::fit::observed(&sheet.info, &sheet.table, &page.subject);
                    fitplots::plot_panel(
                        ui,
                        tokens,
                        page,
                        &sheet.info,
                        &observed,
                        &mut self.state.log_axis,
                    );
                }
            }
            Shown::Sim => {
                if let Some(page) = self.state.sim.as_ref() {
                    let info = self
                        .sheet
                        .as_ref()
                        .map(|s| &s.info)
                        .filter(|i| Some(i.id) == page.worksheet);
                    sim::plot_panel(ui, tokens, page, info, &mut self.state.log_axis);
                }
            }
            Shown::Nothing => {}
        }
    }

    fn top_bar(&mut self, ui: &mut Ui, actions: &mut Vec<Action>) {
        let c = self.tokens.colors.clone();
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Caladrius")
                    .size(self.tokens.font.heading)
                    .strong()
                    .color(c.accent.color()),
            );
            ui.label(RichText::new(&self.overview.name).color(c.text_muted.color()));
            ui.separator();
            projectmenu::file_menu(ui, actions);
            if ui.button("Open CSV…").clicked() {
                actions.push(Action::OpenCsv);
            }
            let has_sheet = !self.overview.worksheets.is_empty();
            let tokens = self.tokens.clone();
            egui::containers::menu::MenuButton::from_button(tokens.primary_button("New analysis"))
                .ui(ui, |ui| {
                    let nca = egui::Button::new("Non-compartmental analysis (NCA)");
                    if ui
                        .add_enabled(has_sheet, nca)
                        .on_disabled_hover_text("Open a CSV file first")
                        .clicked()
                    {
                        actions.push(Action::NewAnalysis);
                    }
                    if ui
                        .add_enabled(has_sheet, egui::Button::new("Model fit"))
                        .on_disabled_hover_text("Open a CSV file first")
                        .clicked()
                    {
                        actions.push(Action::NewFit);
                    }
                    if ui.button("Model simulation").clicked() {
                        actions.push(Action::NewSimulation);
                    }
                });
            let hint = ui.ctx().format_shortcut(&palette::OPEN);
            if ui
                .button("Commands")
                .on_hover_text(format!("Search every command ({hint})"))
                .clicked()
            {
                actions.push(Action::OpenPalette);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let label = match self.state.mode {
                    ThemeMode::Light => "Dark theme",
                    ThemeMode::Dark => "Light theme",
                };
                if ui.button(label).clicked() {
                    actions.push(Action::ToggleTheme);
                }
            });
        });
    }

    fn status_bar(&self, ui: &mut Ui) {
        let c = &self.tokens.colors;
        match (&self.theme_error, &self.notice) {
            (Some(e), _) => {
                ui.label(RichText::new(e).color(c.error.color()));
            }
            (None, Some(n)) => {
                let color = match n.kind {
                    NoticeKind::Info => c.text_muted.color(),
                    NoticeKind::Error => c.error.color(),
                };
                ui.label(RichText::new(&n.text).color(color));
            }
            (None, None) => {
                ui.label(RichText::new("Ready").color(c.text_muted.color()));
            }
        }
    }

    fn tree(&self, ui: &mut Ui, actions: &mut Vec<Action>) {
        let c = &self.tokens.colors;
        ui.label(
            RichText::new("Project")
                .strong()
                .size(self.tokens.font.heading),
        );
        let marker = if self.is_dirty() { " *" } else { "" };
        let mut title = format!("{}{marker}", self.overview.name);
        if let Some(file) = self.file_name() {
            title = format!("{title}  ({file})");
        }
        ui.label(RichText::new(title).color(c.text_muted.color()));
        let stale = self
            .overview
            .analyses
            .iter()
            .filter(|a| a.status.is_stale())
            .count();
        if stale > 0 {
            ui.label(
                RichText::new(format!("{stale} analysis result(s) out of date"))
                    .small()
                    .color(c.stale.color()),
            );
        }
        ui.add_space(self.tokens.spacing.small);
        ui.label(RichText::new("Worksheets").color(c.text_muted.color()));
        if self.overview.worksheets.is_empty() {
            ui.label(
                RichText::new("none yet")
                    .small()
                    .color(c.text_muted.color()),
            );
        }
        for w in &self.overview.worksheets {
            let selected = self.state.selection == Selection::Worksheet(w.id);
            let text = format!("{}  ({} × {})", w.name, w.rows, w.columns);
            if ui.selectable_label(selected, text).clicked() {
                actions.push(Action::Select(Selection::Worksheet(w.id)));
            }
        }
        ui.add_space(self.tokens.spacing.medium);
        ui.label(RichText::new("Analyses").color(c.text_muted.color()));
        if self.overview.analyses.is_empty() {
            ui.label(
                RichText::new("none yet")
                    .small()
                    .color(c.text_muted.color()),
            );
        }
        for a in &self.overview.analyses {
            let selected = self.state.selection == Selection::Analysis(a.id);
            let stale = a.status.is_stale();
            ui.horizontal(|ui| {
                // A drawn dot (the font has no glyph for one): filled when there is a result,
                // an outline when there is none; green when fresh, amber when out of date.
                let (filled, color) = match &a.status {
                    Status::Fresh => (true, c.ok.color()),
                    Status::Stale { .. } => (true, c.stale.color()),
                    Status::NoResult => (false, c.text_muted.color()),
                };
                let radius = self.tokens.size.marker_medium;
                let (rect, _) = ui
                    .allocate_exact_size(egui::Vec2::splat(radius + radius), egui::Sense::hover());
                if filled {
                    ui.painter().circle_filled(rect.center(), radius, color);
                } else {
                    ui.painter().circle_stroke(
                        rect.center(),
                        radius,
                        egui::Stroke::new(self.tokens.stroke.medium, color),
                    );
                }
                let name = if stale {
                    RichText::new(&a.label).color(c.stale.color())
                } else {
                    RichText::new(&a.label)
                };
                let response = ui.selectable_label(selected, name);
                let response = match a.status.sentence() {
                    Some(sentence) => response.on_hover_text(sentence),
                    None => response,
                };
                if response.clicked() {
                    actions.push(Action::Select(Selection::Analysis(a.id)));
                }
            });
            if stale {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("   out of date")
                            .small()
                            .color(c.stale.color()),
                    );
                    if ui.small_button("Run again").clicked() {
                        actions.push(Action::RunAgain(a.id));
                    }
                });
            }
        }
    }

    fn main(&mut self, ui: &mut Ui, tokens: &Tokens, actions: &mut Vec<Action>) {
        match self.state.selection.clone() {
            Selection::Welcome => self.welcome(ui, tokens, actions),
            Selection::Import => match self.pending.as_mut() {
                Some(p) => import::screen(ui, tokens, p, actions),
                None => self.welcome(ui, tokens, actions),
            },
            Selection::Worksheet(_) => match &self.sheet {
                Some(s) => sheet::screen(
                    ui,
                    tokens,
                    &s.info,
                    &s.table,
                    self.rejected.as_ref(),
                    actions,
                ),
                None => self.welcome(ui, tokens, actions),
            },
            Selection::Settings => settingspage::central(
                ui,
                tokens,
                &self.settings,
                &mut self.state.settings_query,
                actions,
            ),
            Selection::Analysis(_) | Selection::NewFit | Selection::NewSimulation => {
                match (self.shown(), self.sheet.as_ref()) {
                    (Shown::Nca, Some(s)) => {
                        if let Some(page) = self.state.nca.as_mut() {
                            nca::central(ui, tokens, page, &s.info, &s.table, actions);
                        }
                    }
                    (Shown::Fit, Some(s)) => {
                        if let Some(page) = self.state.fit.as_mut() {
                            fitform::central(ui, tokens, page, &s.info, &s.table, actions);
                        }
                    }
                    (Shown::Sim, sheet) => {
                        if let Some(page) = self.state.sim.as_mut() {
                            let info = sheet
                                .map(|s| &s.info)
                                .filter(|i| Some(i.id) == page.worksheet);
                            sim::central(ui, tokens, page, info, actions);
                        }
                    }
                    _ => {
                        ui.label("This analysis cannot be shown here yet.");
                    }
                }
            }
        }
    }

    fn welcome(&self, ui: &mut Ui, tokens: &Tokens, actions: &mut Vec<Action>) {
        let c = &tokens.colors;
        ui.add_space(tokens.spacing.xlarge);
        ui.label(
            RichText::new("Pharmacokinetic analysis")
                .size(tokens.font.display)
                .strong(),
        );
        ui.add_space(tokens.spacing.medium);
        ui.label("Open a CSV file with a time column and a concentration column (and, if you have them, a subject and a dose column), or drop it on this window.");
        ui.label(
            RichText::new("You see the table as it will be read before anything is imported.")
                .color(c.text_muted.color()),
        );
        ui.add_space(tokens.spacing.large);
        if ui
            .button(RichText::new("Open a CSV file…").strong())
            .clicked()
        {
            actions.push(Action::OpenCsv);
        }
    }
}
