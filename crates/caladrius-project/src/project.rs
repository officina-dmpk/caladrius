//! The project: worksheets and analysis objects, with stale marking and save/load as bytes.

use serde::{Deserialize, Serialize};

use crate::analysis::{
    Analysis, AnalysisId, AnalysisResult, AnalysisSpec, AnalysisStatus, StaleReason, StoredResult,
};
use crate::error::{ProjectError, Result};
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

    fn take_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    // ---- worksheets ----------------------------------------------------------------------

    /// Adds a worksheet and returns its id. The columns are checked (see [`Worksheet`]).
    pub fn add_worksheet(&mut self, name: &str, columns: Vec<Column>) -> Result<WorksheetId> {
        let id = WorksheetId(self.next_id);
        let ws = Worksheet::new(id, name, columns)?;
        self.next_id += 1;
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

    /// Adds an analysis. The worksheet it reads must exist.
    pub fn add_analysis(&mut self, spec: AnalysisSpec, name: Option<&str>) -> Result<AnalysisId> {
        if let Some(ws) = spec.worksheet() {
            self.worksheet(ws)?;
        }
        let id = AnalysisId(self.take_id());
        self.analyses.push(Analysis {
            id,
            name: name.map(str::to_owned),
            spec,
            spec_version: 0,
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
        if let Some(ws) = spec.worksheet() {
            self.worksheet(ws)?;
        }
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
            a.spec = spec;
            a.spec_version += 1;
        }
        Ok(())
    }

    /// Renames an analysis; `None` goes back to the label derived from the content.
    pub fn rename_analysis(&mut self, id: AnalysisId, name: Option<&str>) -> Result<()> {
        self.analysis_mut(id)?.name = name.map(str::to_owned);
        Ok(())
    }

    /// Stores the result of a run of analysis `id`, computed from the current spec and the
    /// current state of its worksheet.
    pub fn set_result(&mut self, id: AnalysisId, result: AnalysisResult) -> Result<()> {
        let revision = match self.analysis(id)?.spec.worksheet() {
            Some(ws) => Some(self.worksheet(ws)?.revision()),
            None => None,
        };
        let a = self.analysis_mut(id)?;
        a.stored = Some(StoredResult {
            result,
            worksheet_revision: revision,
            spec_version: a.spec_version,
        });
        Ok(())
    }

    /// Removes an analysis.
    pub fn remove_analysis(&mut self, id: AnalysisId) -> Result<()> {
        self.analysis(id)?;
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
        if let Some(n) = &a.name {
            return n.clone();
        }
        let sheet = |id: WorksheetId| {
            self.worksheet(id)
                .map(|w| w.name().to_owned())
                .unwrap_or_else(|_| format!("worksheet {id}"))
        };
        match &a.spec {
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
            AnalysisSpec::Simulation(s) => format!("Simulation of {}", s.input.model.id()),
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
        if ids.last().is_some_and(|&m| m >= self.next_id) {
            return Err(ProjectError::new(
                "load_bad_counter",
                "the id counter of the project is behind its objects",
            ));
        }
        for w in &self.worksheets {
            w.validate()?;
        }
        for a in &self.analyses {
            if let Some(ws) = a.spec.worksheet() {
                self.worksheet(ws)?;
            }
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
