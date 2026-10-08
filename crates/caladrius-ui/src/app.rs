//! The application: the project tree, the screens, and the one place where the engine is called.
//! The screens draw and return [`Action`]s; the application applies them through `caladrius-engine`
//! commands and then reads the new state back. Nothing here computes a number or reads a file.

use caladrius_engine::Engine;
use egui::{Context, RichText, Ui};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::import::{self, PendingImport};
use crate::model::{Overview, Status, Table, WorksheetInfo, read};
use crate::nca::{self, NcaPage};
use crate::sheet::{self, Rejected};
use crate::theme::{ThemeMode, Tokens};

/// What a screen asks for. Applied after the frame, through engine commands.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Select(Selection),
    OpenCsv,
    NewAnalysis,
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
}

/// What the application asks of the program around it (files are the program's business).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// Ask the person for a CSV file and give it back with [`UiApp::load_csv`].
    PickCsv,
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
}

#[derive(Debug, Clone, PartialEq)]
enum NoticeKind {
    Info,
    Error,
}

#[derive(Debug, Clone, PartialEq)]
struct Notice {
    kind: NoticeKind,
    text: String,
}

/// The worksheet shown: its description and its cells, as the engine gave them.
#[derive(Debug, Clone, Default)]
struct SheetView {
    info: WorksheetInfo,
    table: Table,
}

/// The application.
pub struct UiApp {
    engine: Engine,
    pub state: UiState,
    tokens: Tokens,
    applied: Option<ThemeMode>,
    overview: Overview,
    sheet: Option<SheetView>,
    rejected: Option<Rejected>,
    pending: Option<PendingImport>,
    notice: Option<Notice>,
    requests: Vec<Request>,
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
            theme_error,
        };
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

    /// The import waiting for a decision, for tests.
    pub fn pending_import(&self) -> Option<&PendingImport> {
        self.pending.as_ref()
    }

    // ---- engine access -----------------------------------------------------------------

    /// Runs a command; an error becomes the status-bar message (it says what to fix).
    fn call(&mut self, id: &str, params: Value) -> Result<Value, String> {
        match self.engine.execute(id, params) {
            Ok(v) => Ok(v),
            Err(e) => {
                self.notice = Some(Notice {
                    kind: NoticeKind::Error,
                    text: e.message.clone(),
                });
                Err(e.message)
            }
        }
    }

    fn refresh_overview(&mut self) {
        if let Ok(v) = self.engine.execute("project.describe", Value::Null) {
            self.overview = read(&v);
        }
    }

    fn refresh_sheet(&mut self, id: u64) {
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
        match NcaPage::from_view(&view) {
            Some(page) => {
                self.refresh_sheet(page.worksheet);
                self.state.nca = Some(page);
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
        self.state.nca = Some(NcaPage::new(id, subject));
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
                Action::ToggleTheme => {
                    self.state.mode = self.state.mode.toggled();
                }
                Action::RunNca => self.run_nca(),
                Action::RunAgain(id) => {
                    if let Ok(view) = self.call("analysis.run", json!({ "analysis": id })) {
                        if let Some(page) = self.state.nca.as_mut() {
                            page.adopt(&view);
                        }
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
                        Ok(_) => self.rejected = None,
                        Err(_) => self.rejected = Some(Rejected { row, column, text }),
                    }
                    self.refresh_overview();
                    self.refresh_sheet(worksheet);
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
                self.load_csv(&file.name, &bytes);
            }
        }
        let tokens = self.tokens.clone();
        let mut actions: Vec<Action> = Vec::new();

        egui::TopBottomPanel::top("top-bar")
            .frame(tokens.panel_frame(tokens.colors.panel))
            .show(ctx, |ui| self.top_bar(ui, &mut actions));
        egui::TopBottomPanel::bottom("status-bar")
            .frame(tokens.panel_frame(tokens.colors.panel_alt))
            .show(ctx, |ui| self.status_bar(ui));
        egui::SidePanel::left("project-tree")
            .default_width(260.0)
            .width_range(220.0..=420.0)
            .frame(tokens.panel_frame(tokens.colors.panel_alt))
            .show(ctx, |ui| self.tree(ui, &mut actions));
        let showing_nca =
            matches!(self.state.selection, Selection::Analysis(_)) && self.state.nca.is_some();
        if showing_nca {
            egui::SidePanel::right("plot-panel")
                .default_width(560.0)
                .min_width(380.0)
                .resizable(true)
                .frame(tokens.panel_frame(tokens.colors.panel))
                .show(ctx, |ui| {
                    if let (Some(page), Some(sheet)) =
                        (self.state.nca.as_mut(), self.sheet.as_ref())
                    {
                        nca::plot_panel(
                            ui,
                            &tokens,
                            page,
                            &sheet.info,
                            &mut self.state.log_axis,
                            &mut actions,
                        );
                    }
                });
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
        self.perform(actions);
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
            if ui.button("Open CSV…").clicked() {
                actions.push(Action::OpenCsv);
            }
            let has_sheet = !self.overview.worksheets.is_empty();
            if ui
                .add_enabled(has_sheet, self.tokens.primary_button("New analysis"))
                .on_disabled_hover_text("Open a CSV file first")
                .clicked()
            {
                actions.push(Action::NewAnalysis);
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
            ui.horizontal(|ui| {
                let (dot, color) = match &a.status {
                    Status::Fresh => ("●", c.ok.color()),
                    Status::Stale { .. } => ("●", c.stale.color()),
                    Status::NoResult => ("○", c.text_muted.color()),
                };
                ui.label(RichText::new(dot).color(color));
                if ui.selectable_label(selected, &a.label).clicked() {
                    actions.push(Action::Select(Selection::Analysis(a.id)));
                }
            });
            if a.status.is_stale() {
                ui.label(
                    RichText::new("   out of date")
                        .small()
                        .color(c.stale.color()),
                );
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
            Selection::Analysis(_) => match (self.state.nca.as_mut(), self.sheet.as_ref()) {
                (Some(page), Some(s)) => nca::central(ui, tokens, page, &s.info, &s.table, actions),
                _ => {
                    ui.label("This analysis cannot be shown here yet.");
                }
            },
        }
    }

    fn welcome(&self, ui: &mut Ui, tokens: &Tokens, actions: &mut Vec<Action>) {
        let c = &tokens.colors;
        ui.add_space(tokens.spacing.large * 2.0);
        ui.label(
            RichText::new("Pharmacokinetic analysis")
                .size(tokens.font.heading + 8.0)
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
