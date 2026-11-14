//! How the population-fit page talks to the engine: open a page, run `pop.fit`, open a stored one.
//! The call is synchronous; the page measures how long it took with the clock the program gives
//! ([`UiApp::set_stopwatch`]; the wall time shown beside the result) and keeps that measure out of
//! the analysis, whose seal hashes the answer only.

use caladrius_engine::{CommandError, Engine};
use serde_json::{Value, json};

use crate::app::{Notice, NoticeKind, Selection, UiApp};
use crate::fmt;
use crate::pop::PopPage;

impl UiApp {
    /// Runs one engine command and says how long it took, in seconds, when the program gave a
    /// clock.
    fn timed(&mut self, id: &str, params: Value) -> (Result<Value, CommandError>, Option<f64>) {
        let watch = self.stopwatch;
        let start = watch.map(|now| now());
        let answer = Engine::execute(&mut self.engine, id, params);
        let wall = match (watch, start) {
            (Some(now), Some(start)) => Some(now() - start).filter(|t| t.is_finite() && *t >= 0.0),
            _ => None,
        };
        (answer, wall)
    }

    /// A new population fit for the worksheet in view, or the first worksheet with several
    /// subjects.
    pub(crate) fn new_pop_fit(&mut self) {
        let worksheet = match &self.state.selection {
            Selection::Worksheet(id) => Some(*id),
            _ => self
                .selected_worksheet()
                .or_else(|| {
                    self.overview
                        .worksheets
                        .iter()
                        .find(|w| w.subjects.len() > 1)
                        .map(|w| w.id)
                })
                .or_else(|| self.overview.worksheets.first().map(|w| w.id)),
        };
        let Some(id) = worksheet else {
            self.notice = Some(Notice {
                kind: NoticeKind::Info,
                text: "Open a CSV file first: a population fit needs a worksheet with a subject column."
                    .to_owned(),
            });
            return;
        };
        self.refresh_sheet(id);
        self.state.pop = Some(Box::new(PopPage::new(id)));
        self.state.selection = Selection::NewPopFit;
        self.notice = None;
    }

    /// The worksheet of the page changed: read it again for its units and subjects.
    pub(crate) fn pop_changed(&mut self) {
        if let Some(id) = self.state.pop.as_ref().map(|p| p.worksheet) {
            self.refresh_sheet(id);
        }
    }

    /// Fits the population model of the page (`pop.fit`).
    pub(crate) fn run_pop_fit(&mut self) {
        let Some(mut page) = self.state.pop.take() else {
            return;
        };
        match page.params() {
            Err(message) => page.error = Some(message),
            Ok(params) => {
                let (answer, wall) = self.timed("pop.fit", params.clone());
                match answer {
                    Ok(answer) => {
                        page.adopt_answer(params, answer, wall);
                        if let Some(run) = page.run.as_mut() {
                            run.seal = self.last_seal();
                        }
                        self.notice = None;
                    }
                    Err(e) => {
                        let message = fmt::plain(&e.message);
                        self.notice = Some(Notice {
                            kind: NoticeKind::Error,
                            text: message.clone(),
                        });
                        page.error = Some(message);
                    }
                }
            }
        }
        let analysis = page.analysis;
        let worksheet = page.worksheet;
        self.state.pop = Some(page);
        self.refresh_overview();
        self.refresh_sheet(worksheet);
        if let Some(id) = analysis {
            self.state.selection = Selection::Analysis(id);
        }
    }

    /// The page of a stored population fit.
    pub(crate) fn open_pop(&mut self, view: &Value) {
        let Some(page) = PopPage::from_view(view) else {
            self.notice = Some(Notice {
                kind: NoticeKind::Info,
                text: "This population fit cannot be read by the page; use caladrius-cli."
                    .to_owned(),
            });
            return;
        };
        self.refresh_sheet(page.worksheet);
        self.state.pop = Some(Box::new(page));
    }

    /// Runs a stored analysis again (`analysis.run`), timing a population fit like a run of its
    /// page.
    pub(crate) fn run_again(&mut self, id: u64) {
        let (answer, wall) = self.timed("analysis.run", json!({ "analysis": id }));
        match answer {
            Ok(view) => {
                if let Some(page) = self.state.pop.as_mut().filter(|p| p.analysis == Some(id)) {
                    page.adopt_view(&view, wall);
                }
                let seal = self.last_seal();
                if let Some(run) = self
                    .state
                    .pop
                    .as_mut()
                    .filter(|p| p.analysis == Some(id))
                    .and_then(|p| p.run.as_mut())
                {
                    run.seal = seal;
                }
                self.adopt_run(&view);
                self.refresh_overview();
            }
            Err(e) => {
                let message = fmt::plain(&e.message);
                if let Some(page) = self.state.pop.as_mut().filter(|p| p.analysis == Some(id)) {
                    page.error = Some(message.clone());
                }
                self.notice = Some(Notice {
                    kind: NoticeKind::Error,
                    text: message,
                });
            }
        }
    }

    /// The seal of the last command, the first digits and the stamp line.
    fn last_seal(&self) -> Option<(String, String)> {
        let entry = self.engine.history().entries().last()?;
        let seal = entry.seal.as_ref()?;
        Some((
            seal.short().to_owned(),
            caladrius_engine::seal::stamp_line(&entry.command, seal),
        ))
    }
}
