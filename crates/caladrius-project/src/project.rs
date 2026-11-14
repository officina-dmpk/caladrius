//! The project: worksheets and analysis objects, with stale marking and save/load as bytes.

use serde::{Deserialize, Serialize};

use crate::analysis::{
    Analysis, AnalysisId, AnalysisResult, AnalysisSpec, AnalysisStatus, StaleReason, StoredResult,
};
use crate::error::{ProjectError, Result};
use crate::plot::{PlotSource, PlotSpec};
use crate::worksheet::{Column, Worksheet, WorksheetId};

/// Version of the saved format; a file with a larger version is refused with a message.
pub const FORMAT_VERSION: u32 = 1;

const FORMAT_NAME: &str = "caladrius-project";

/// A project: everything the user works on, as plain data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    format: String,
    format_version: u32,
    name: String,
    worksheets: Vec<Worksheet>,
    analyses: Vec<Analysis>,
    next_id: u64,
}

impl Default for Project {
    fn default() -> Self {
        Self::new("Untitled project")
    }
}

impl Project {
    /// An empty project.
    pub fn new(name: &str) -> Self {
        Self {
            format: FORMAT_NAME.to_owned(),
            format_version: FORMAT_VERSION,
            name: name.to_owned(),
            worksheets: Vec::new(),
            analyses: Vec::new(),
            next_id: 1,
        }
    }

    /// Name of the project.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Renames the project.
    pub fn rename(&mut self, name: &str) {
        self.name = name.to_owned();
    }

    /// The id after `next_id`. The counter never reaches `u64::MAX` (a loaded file that holds it
    /// is refused), so running out of ids is an error, not a wrap.
    fn following_id(&self) -> Result<u64> {
        self.next_id
            .checked_add(1)
            .filter(|n| *n != u64::MAX)
            .ok_or_else(|| {
                ProjectError::new(
                    "counter_overflow",
                    "this project has run out of identifiers; save its content in a new project",
                )
            })
    }

    fn take_id(&mut self) -> Result<u64> {
        let id = self.next_id;
        self.next_id = self.following_id()?;
        Ok(id)
    }

    // ---- worksheets ----------------------------------------------------------------------

    /// Adds a worksheet and returns its id. The columns are checked (see [`Worksheet`]).
    pub fn add_worksheet(&mut self, name: &str, columns: Vec<Column>) -> Result<WorksheetId> {
        let id = WorksheetId(self.next_id);
        let ws = Worksheet::new(id, name, columns)?;
        self.next_id = self.following_id()?;
        self.worksheets.push(ws);
        Ok(id)
    }

    /// Every worksheet, in creation order.
    pub fn worksheets(&self) -> &[Worksheet] {
        &self.worksheets
    }

    /// The worksheet with this id.
    pub fn worksheet(&self, id: WorksheetId) -> Result<&Worksheet> {
        self.worksheets
            .iter()
            .find(|w| w.id() == id)
            .ok_or_else(|| {
                ProjectError::new(
                    "unknown_worksheet",
                    format!(
                        "there is no worksheet {id}; the worksheets are: {}",
                        list(
                            self.worksheets
                                .iter()
                                .map(|w| format!("{} ({})", w.id(), w.name()))
                        )
                    ),
                )
            })
    }

    /// Changes a worksheet through `edit`. The closure uses the mutating methods of
    /// [`Worksheet`], which bump its revision, so the analyses that read it become stale.
    pub fn edit_worksheet<R>(
        &mut self,
        id: WorksheetId,
        edit: impl FnOnce(&mut Worksheet) -> Result<R>,
    ) -> Result<R> {
        self.worksheet(id)?;
        let ws = self
            .worksheets
            .iter_mut()
            .find(|w| w.id() == id)
            .ok_or_else(|| {
                ProjectError::new("unknown_worksheet", format!("there is no worksheet {id}"))
            })?;
        edit(ws)
    }

    /// Removes a worksheet that no analysis reads.
    pub fn remove_worksheet(&mut self, id: WorksheetId) -> Result<()> {
        self.worksheet(id)?;
        let readers: Vec<String> = self
            .analyses
            .iter()
            .filter(|a| a.spec.worksheet() == Some(id))
            .map(|a| self.label(a))
            .collect();
        if !readers.is_empty() {
            return Err(ProjectError::new(
                "worksheet_in_use",
                format!(
                    "worksheet {id} is read by {}; remove those analyses first",
                    readers.join(", ")
                ),
            ));
        }
        self.worksheets.retain(|w| w.id() != id);
        Ok(())
    }

    // ---- analyses ------------------------------------------------------------------------

    /// Checks that what `spec` reads exists: its worksheet, and for a plot of a fit, a fit
    /// analysis (a plot reads a fit result, never another plot).
    fn check_inputs(&self, spec: &AnalysisSpec) -> Result<()> {
        if let Some(ws) = spec.worksheet() {
            self.worksheet(ws)?;
        }
        if let Some(source) = spec.source_analysis() {
            let kind = self.analysis(source)?.spec.kind();
            if kind != "fit" {
                return Err(ProjectError::new(
                    "wrong_plot_source",
                    format!(
                        "analysis {source} is a {kind} analysis; a plot reads a worksheet or a fit"
                    ),
                ));
            }
        }
        Ok(())
    }

    /// Adds an analysis. The worksheet (or the fit) it reads must exist.
    pub fn add_analysis(&mut self, spec: AnalysisSpec, name: Option<&str>) -> Result<AnalysisId> {
        self.check_inputs(&spec)?;
        let id = AnalysisId(self.take_id()?);
        self.analyses.push(Analysis {
            id,
            name: name.map(str::to_owned),
            spec,
            spec_version: 0,
            runs: 0,
            stored: None,
        });
        Ok(id)
    }

    /// Every analysis, in creation order.
    pub fn analyses(&self) -> &[Analysis] {
        &self.analyses
    }

    /// The analysis with this id.
    pub fn analysis(&self, id: AnalysisId) -> Result<&Analysis> {
        self.analyses.iter().find(|a| a.id == id).ok_or_else(|| {
            ProjectError::new(
                "unknown_analysis",
                format!(
                    "there is no analysis {id}; the analyses are: {}",
                    list(
                        self.analyses
                            .iter()
                            .map(|a| format!("{} ({})", a.id, self.label(a)))
                    )
                ),
            )
        })
    }

    fn analysis_mut(&mut self, id: AnalysisId) -> Result<&mut Analysis> {
        self.analysis(id)?;
        self.analyses
            .iter_mut()
            .find(|a| a.id == id)
            .ok_or_else(|| {
                ProjectError::new("unknown_analysis", format!("there is no analysis {id}"))
            })
    }

    /// Replaces the options of an analysis. The kind cannot change. When the spec differs, the
    /// stored result becomes stale ([`StaleReason::OptionsChanged`]).
    pub fn update_spec(&mut self, id: AnalysisId, spec: AnalysisSpec) -> Result<()> {
        self.check_inputs(&spec)?;
        let a = self.analysis_mut(id)?;
        if a.spec.kind() != spec.kind() {
            return Err(ProjectError::new(
                "analysis_kind_changed",
                format!(
                    "analysis {id} is a {} analysis; create a new analysis for a {}",
                    a.spec.kind(),
                    spec.kind()
                ),
            ));
        }
        if a.spec != spec {
            let next = a
                .spec_version
                .checked_add(1)
                .filter(|v| *v != u64::MAX)
                .ok_or_else(|| {
                    ProjectError::new(
                        "counter_overflow",
                        format!(
                            "analysis {id} has been changed too many times to track further changes"
                        ),
                    )
                })?;
            a.spec = spec;
            a.spec_version = next;
        }
        Ok(())
    }

    /// Renames an analysis; `None` goes back to the label derived from the content.
    pub fn rename_analysis(&mut self, id: AnalysisId, name: Option<&str>) -> Result<()> {
        self.analysis_mut(id)?.name = name.map(str::to_owned);
        Ok(())
    }

    /// Stores the result of a run of analysis `id`, computed from the current spec and the
    /// current state of its worksheet (or of the analysis it reads).
    pub fn set_result(&mut self, id: AnalysisId, result: AnalysisResult) -> Result<()> {
        let spec = &self.analysis(id)?.spec;
        let revision = match spec.worksheet() {
            Some(ws) => Some(self.worksheet(ws)?.revision()),
            None => None,
        };
        let source_run = match spec.source_analysis() {
            Some(source) => Some(self.analysis(source)?.runs),
            None => None,
        };
        let a = self.analysis_mut(id)?;
        let runs = a
            .runs
            .checked_add(1)
            .filter(|r| *r != u64::MAX)
            .ok_or_else(|| {
                ProjectError::new(
                    "counter_overflow",
                    format!("analysis {id} has been run too many times to track further runs"),
                )
            })?;
        a.runs = runs;
        a.stored = Some(StoredResult {
            result,
            worksheet_revision: revision,
            spec_version: a.spec_version,
            source_run,
        });
        Ok(())
    }

    /// Removes an analysis that no other analysis reads (a fit shown by a plot is kept until the
    /// plot is removed).
    pub fn remove_analysis(&mut self, id: AnalysisId) -> Result<()> {
        self.analysis(id)?;
        let readers: Vec<String> = self
            .analyses
            .iter()
            .filter(|a| a.spec.source_analysis() == Some(id))
            .map(|a| self.label(a))
            .collect();
        if !readers.is_empty() {
            return Err(ProjectError::new(
                "analysis_in_use",
                format!(
                    "analysis {id} is read by {}; remove those plots first",
                    readers.join(", ")
                ),
            ));
        }
        self.analyses.retain(|a| a.id != id);
        Ok(())
    }

    /// Whether the stored result of analysis `id` matches the current data and options.
    pub fn status(&self, id: AnalysisId) -> Result<AnalysisStatus> {
        let a = self.analysis(id)?;
        let Some(stored) = &a.stored else {
            return Ok(AnalysisStatus::NoResult);
        };
        let mut reasons = Vec::new();
        if let (Some(ws), Some(ran)) = (a.spec.worksheet(), stored.worksheet_revision) {
            match self.worksheet(ws) {
                Ok(w) if w.revision() != ran => reasons.push(StaleReason::InputChanged {
                    ran_on_revision: ran,
                    current_revision: w.revision(),
                }),
                Ok(_) => {}
                Err(_) => reasons.push(StaleReason::WorksheetMissing),
            }
        }
        if let Some(source) = a.spec.source_analysis() {
            match self.analysis(source) {
                Ok(s) => {
                    if stored.source_run != Some(s.runs) {
                        reasons.push(StaleReason::SourceChanged);
                    }
                    // The source is a fit (checked when the plot is added, updated or loaded),
                    // which reads no other analysis: this does not recurse further.
                    if s.spec.source_analysis().is_none()
                        && matches!(self.status(source), Ok(AnalysisStatus::Stale { .. }))
                    {
                        reasons.push(StaleReason::SourceStale);
                    }
                }
                Err(_) => reasons.push(StaleReason::SourceMissing),
            }
        }
        if stored.spec_version != a.spec_version {
            reasons.push(StaleReason::OptionsChanged);
        }
        Ok(if reasons.is_empty() {
            AnalysisStatus::Fresh
        } else {
            AnalysisStatus::Stale { reasons }
        })
    }

    /// A label named from the content (friction 7): the user's name when there is one, else the
    /// kind, the model or subject, and the worksheet.
    pub fn label_of(&self, id: AnalysisId) -> Result<String> {
        Ok(self.label(self.analysis(id)?))
    }

    fn label(&self, a: &Analysis) -> String {
        match &a.name {
            Some(n) => n.clone(),
            None => self.describe_spec(&a.spec),
        }
    }

    /// The label an analysis with this spec and no name gets: the kind, the model or subject,
    /// and the worksheet (or the analysis it reads).
    pub fn describe_spec(&self, spec: &AnalysisSpec) -> String {
        let sheet = |id: WorksheetId| {
            self.worksheet(id)
                .map(|w| w.name().to_owned())
                .unwrap_or_else(|_| format!("worksheet {id}"))
        };
        match spec {
            AnalysisSpec::Nca(s) => format!(
                "NCA of {}, {}",
                sheet(s.worksheet),
                s.subject
                    .as_deref()
                    .map_or("all subjects".to_owned(), |x| format!("subject {x}"))
            ),
            AnalysisSpec::Fit(s) => format!(
                "Fit {} to {}{}",
                s.model.id(),
                sheet(s.worksheet),
                s.subject
                    .as_deref()
                    .map_or(String::new(), |x| format!(", subject {x}"))
            ),
            AnalysisSpec::PopFit(s) => {
                format!("Population fit {} to {}", s.model.id(), sheet(s.worksheet))
            }
            AnalysisSpec::Simulation(s) => format!("Simulation of {}", s.input.model.id()),
            AnalysisSpec::Plot(PlotSpec {
                source: PlotSource::Worksheet { worksheet },
                subjects,
                ..
            }) => format!(
                "Plot of {}, {}",
                sheet(*worksheet),
                match subjects.as_deref() {
                    None => "all subjects".to_owned(),
                    Some([one]) => format!("subject {one}"),
                    Some(many) => format!("{} subjects", many.len()),
                }
            ),
            AnalysisSpec::Plot(PlotSpec {
                source: PlotSource::Fit { analysis, view },
                ..
            }) => {
                let fit = self
                    .analysis(*analysis)
                    .map(|f| self.label(f))
                    .unwrap_or_else(|_| format!("analysis {analysis}"));
                format!("{} of {fit}", view.title())
            }
        }
    }

    // ---- save and load ---------------------------------------------------------------------

    /// The project as UTF-8 JSON bytes, to be written by the caller.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        serde_json::to_vec_pretty(self).map_err(|e| {
            ProjectError::new(
                "save_failed",
                format!("the project cannot be serialized: {e}"),
            )
        })
    }

    /// Reads a project from the bytes written by [`Project::to_bytes`], and checks it: format
    /// name and version, worksheets, unique ids, analyses that read existing worksheets.
    pub fn from_bytes(bytes: &[u8]) -> Result<Project> {
        let project: Project = serde_json::from_slice(bytes).map_err(|e| {
            ProjectError::new(
                "load_failed",
                format!("this is not a readable Caladrius project: {e}"),
            )
        })?;
        project.check()?;
        Ok(project)
    }

    fn check(&self) -> Result<()> {
        if self.format != FORMAT_NAME {
            return Err(ProjectError::new(
                "load_wrong_format",
                format!(
                    "the file declares the format `{}`, not `{FORMAT_NAME}`",
                    self.format
                ),
            ));
        }
        if self.format_version > FORMAT_VERSION {
            return Err(ProjectError::new(
                "load_newer_version",
                format!(
                    "the project was saved by a newer version (format {}, this one reads up to {FORMAT_VERSION}); update Caladrius",
                    self.format_version
                ),
            ));
        }
        let mut ids: Vec<u64> = self
            .worksheets
            .iter()
            .map(|w| w.id().0)
            .chain(self.analyses.iter().map(|a| a.id.0))
            .collect();
        ids.sort_unstable();
        if ids.windows(2).any(|p| p.first() == p.get(1)) {
            return Err(ProjectError::new(
                "load_duplicate_id",
                "two objects of the project share an id",
            ));
        }
        if self.next_id == u64::MAX || ids.last().is_some_and(|&m| m >= self.next_id) {
            return Err(ProjectError::new(
                "load_bad_counter",
                "the id counter of the project is behind its objects",
            ));
        }
        for w in &self.worksheets {
            w.validate()?;
            if w.revision() == u64::MAX {
                return Err(ProjectError::new(
                    "load_bad_counter",
                    format!(
                        "worksheet `{}` has an impossible revision counter",
                        w.name()
                    ),
                ));
            }
        }
        if self
            .analyses
            .iter()
            .any(|a| a.spec_version == u64::MAX || a.runs == u64::MAX)
        {
            return Err(ProjectError::new(
                "load_bad_counter",
                "an analysis has an impossible version counter",
            ));
        }
        for a in &self.analyses {
            self.check_inputs(&a.spec)?;
        }
        Ok(())
    }
}

fn list(items: impl Iterator<Item = String>) -> String {
    let v: Vec<String> = items.collect();
    if v.is_empty() {
        "none".to_owned()
    } else {
        v.join(", ")
    }
}
