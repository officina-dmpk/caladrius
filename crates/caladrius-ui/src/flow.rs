//! How the fit and simulation pages talk to the engine: open a page, generate the starting values,
//! redraw the live curve and the objective at every change, run, store. Each step is a command
//! (`fit.initial_estimates`, `model.simulate`, `fit.evaluate`, `fit.run`); nothing is computed here.
//! The live calls are quiet: a refused value shows its sentence on the page, not in the status bar.

use serde_json::Value;

use crate::app::{Notice, NoticeKind, Selection, UiApp};
use crate::fit::{self, FitPage};
use crate::sim::SimPage;

impl UiApp {
    /// A command that is asked at every change: its refusal is a sentence for the page.
    fn ask(&mut self, id: &str, params: Value) -> Result<Value, String> {
        self.engine.execute(id, params).map_err(|e| e.message)
    }

    fn info_notice(&mut self, text: &str) {
        self.notice = Some(Notice {
            kind: NoticeKind::Info,
            text: text.to_owned(),
        });
    }

    // ---- fit -------------------------------------------------------------------------------

    /// A new fit page for the worksheet in view, with starting values generated from its data.
    pub(crate) fn new_fit(&mut self) {
        let worksheet = match &self.state.selection {
            Selection::Worksheet(id) => Some(*id),
            _ => self.overview.worksheets.first().map(|w| w.id),
        };
        let Some(id) = worksheet else {
            self.info_notice("Open a CSV file first: a fit needs a worksheet.");
            return;
        };
        self.refresh_sheet(id);
        let subject = self
            .sheet
            .as_ref()
            .and_then(|s| s.info.subjects.first().cloned())
            .unwrap_or_default();
        self.state.nca = None;
        self.state.sim = None;
        self.state.fit = Some(FitPage::new(id, subject));
        self.state.selection = Selection::NewFit;
        self.notice = None;
        self.fit_changed(true);
    }

    /// The page of a stored fit. Its starting values are the ones it was run from.
    pub(crate) fn open_fit(&mut self, view: &Value) {
        let Some(mut page) = FitPage::from_view(view) else {
            self.state.fit = None;
            self.info_notice("This fit uses a model the page does not know; use `caladrius-cli`.");
            return;
        };
        self.refresh_sheet(page.worksheet);
        page.preview.dose = page.view.as_ref().and_then(|v| v.dose);
        if page.initial.is_empty() {
            if let Some(row) = page.ok().and_then(|ok| ok.trace.first()) {
                let names = page.model().parameters;
                page.initial = names
                    .iter()
                    .zip(&row.estimates)
                    .map(|(n, x)| ((*n).to_owned(), *x))
                    .collect();
            }
        }
        let generate = page.initial.is_empty();
        self.state.nca = None;
        self.state.sim = None;
        self.state.fit = Some(page);
        if generate {
            self.fit_changed(true);
        } else {
            self.fit_preview();
        }
    }

    /// The page changed: generate the starting values again when asked, then redraw the live curve.
    pub(crate) fn fit_changed(&mut self, regenerate: bool) {
        if regenerate {
            if let Some(page) = self.state.fit.as_ref() {
                let answer = self.ask("fit.initial_estimates", page.initial_params());
                if let Some(page) = self.state.fit.as_mut() {
                    match answer {
                        Ok(v) => {
                            page.adopt_initial(&v);
                            page.start_error = None;
                        }
                        Err(message) => {
                            page.initial.clear();
                            page.start_error = Some(message);
                        }
                    }
                }
            }
        }
        self.fit_preview();
    }

    /// The live curve of the starting values and their objective, from the engine.
    pub(crate) fn fit_preview(&mut self) {
        let Some(page) = self.state.fit.as_ref() else {
            return;
        };
        let observed = self
            .sheet
            .as_ref()
            .map(|s| fit::observed(&s.info, &s.table, &page.subject))
            .unwrap_or_default();
        let simulate = fit::last_time(&observed).and_then(|end| page.simulate_params(end));
        let complete = page
            .model()
            .parameters
            .iter()
            .all(|n| page.initial.contains_key(*n));
        let evaluate = page.evaluate_params();
        let simulated = simulate.map(|p| self.ask("model.simulate", p));
        let evaluated = if complete {
            self.ask("fit.evaluate", evaluate)
        } else {
            Err(
                "Give a starting value for every parameter, or generate them from the data."
                    .to_owned(),
            )
        };
        if let Some(page) = self.state.fit.as_mut() {
            page.adopt_preview(simulated, evaluated);
        }
    }

    /// Fits the model from the starting values on the page.
    pub(crate) fn run_fit(&mut self) {
        let Some(mut page) = self.state.fit.take() else {
            return;
        };
        match self.call("fit.run", page.run_params()) {
            Ok(view) => {
                page.adopt(&view);
                self.notice = None;
            }
            Err(message) => page.error = Some(message),
        }
        let analysis = page.analysis;
        let worksheet = page.worksheet;
        self.state.fit = Some(page);
        self.refresh_overview();
        self.refresh_sheet(worksheet);
        if let Some(id) = analysis {
            self.state.selection = Selection::Analysis(id);
        }
    }

    // ---- simulation ------------------------------------------------------------------------

    /// A new simulation page, drawn at once.
    pub(crate) fn new_simulation(&mut self) {
        self.state.nca = None;
        self.state.fit = None;
        self.state.sim = Some(SimPage::new());
        self.state.selection = Selection::NewSimulation;
        self.notice = None;
        self.sim_changed();
    }

    /// The page of a stored simulation, drawn again from its parameters.
    pub(crate) fn open_simulation(&mut self, view: &Value) {
        let Some(page) = SimPage::from_view(view) else {
            self.state.sim = None;
            self.info_notice("This simulation uses a model the page does not know.");
            return;
        };
        self.state.nca = None;
        self.state.fit = None;
        self.state.sim = Some(page);
        self.sim_changed();
    }

    /// The curve of the engine for the parameters on the page.
    pub(crate) fn sim_changed(&mut self) {
        let Some(page) = self.state.sim.as_ref() else {
            return;
        };
        let answer = self.ask("model.simulate", page.params(false));
        if let Some(page) = self.state.sim.as_mut() {
            page.adopt(answer);
        }
    }

    /// Stores the simulation as an analysis of the project.
    pub(crate) fn sim_save(&mut self) {
        let Some(page) = self.state.sim.as_ref() else {
            return;
        };
        let params = page.params(true);
        let Ok(answer) = self.call("model.simulate", params) else {
            return;
        };
        if let Some(page) = self.state.sim.as_mut() {
            page.adopt(Ok(answer));
        }
        self.refresh_overview();
        if let Some(id) = self.state.sim.as_ref().and_then(|p| p.analysis) {
            self.state.selection = Selection::Analysis(id);
        }
        self.info_notice("Saved as an analysis in the project tree.");
    }
}
