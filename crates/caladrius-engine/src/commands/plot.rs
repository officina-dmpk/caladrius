//! `plot.xy` and `export.plot`: XY plots as objects of the workflow (T-110).

use caladrius_project::{
    AnalysisId, AnalysisResult, AnalysisSpec, AnalysisStatus, FitView, PlotLayout, PlotScale,
    PlotSource, PlotSpec, WorksheetId,
};
use serde::Deserialize;
use serde_json::{Value, json};

use super::analysis::{check_target, store, view};
use super::{CommandDef, parse, respond};
use crate::Engine;
use crate::error::CommandError;
use crate::export;
use crate::run;
use crate::schema::{
    array_of, described, integer, nullable, object, one_of_strings, reference, root, string,
};
use crate::svg;

// ---- plot.xy -----------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PlotParams {
    #[serde(default)]
    analysis: Option<AnalysisId>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    worksheet: Option<WorksheetId>,
    #[serde(default)]
    fit: Option<AnalysisId>,
    #[serde(default)]
    view: Option<FitView>,
    #[serde(default, deserialize_with = "super::labels")]
    subjects: Option<Vec<String>>,
    #[serde(default)]
    scale: PlotScale,
    #[serde(default)]
    layout: PlotLayout,
    #[serde(default)]
    title: Option<String>,
}

fn source(p: &PlotParams) -> Result<PlotSource, CommandError> {
    match (p.worksheet, p.fit) {
        (Some(worksheet), None) => {
            if p.view.is_some() {
                return Err(CommandError::invalid(
                    "plot.xy",
                    "`view` chooses a plot of a fit; a worksheet plot has none",
                ));
            }
            Ok(PlotSource::Worksheet { worksheet })
        }
        (None, Some(analysis)) => Ok(PlotSource::Fit {
            analysis,
            view: p.view.unwrap_or_default(),
        }),
        (Some(_), Some(_)) => Err(CommandError::invalid(
            "plot.xy",
            "give `worksheet` or `fit`, not both",
        )),
        (None, None) => Err(CommandError::invalid(
            "plot.xy",
            "give `worksheet` (concentration against time) or `fit` (a plot of a fit result)",
        )),
    }
}

fn plot_xy(engine: &mut Engine, params: Value) -> Result<Value, CommandError> {
    let p: PlotParams = parse("plot.xy", params)?;
    check_target(&engine.project, p.analysis, "plot")?;
    let spec = AnalysisSpec::Plot(PlotSpec {
        source: source(&p)?,
        subjects: p.subjects.clone(),
        scale: p.scale,
        layout: p.layout,
        title: p.title.clone().filter(|t| !t.trim().is_empty()),
    });
    let result = run::run_spec(&engine.project, &spec)?;
    let id = store(
        &mut engine.project,
        p.analysis,
        p.name.as_deref(),
        spec,
        result,
    )?;
    view(&engine.project, id)
}

pub(crate) const PLOT_XY: CommandDef = CommandDef {
    id: "plot.xy",
    title: "Plot data or a fit",
    description: "Draws an XY plot and stores it as an analysis object (new, or `analysis` updated), so it is saved with the project and marked stale when its source changes. Source: `worksheet` (concentration against time, one series per subject, all subjects or `subjects`, overlaid or one panel per subject) or `fit` (a fit analysis already run: `view` profile, observed_predicted, residuals_time or weighted_residuals_time; nothing is fitted again). `scale` linear, log or both (both: a linear and a logarithmic panel; residuals are linear only). The result holds the panels with their axes, units and points as they are in the source (BLQ points flagged); nothing is computed. `export.plot` gives the SVG.",
    mutates: true,
    params: || {
        root(
            "plot.xy parameters",
            object(
                vec![
                    (
                        "analysis",
                        described(
                            reference("Id"),
                            "Update and redraw this plot instead of creating a new one.",
                        ),
                    ),
                    ("name", string()),
                    (
                        "worksheet",
                        described(reference("Id"), "Plot the data of this worksheet."),
                    ),
                    (
                        "fit",
                        described(
                            reference("Id"),
                            "Plot the stored result of this fit analysis.",
                        ),
                    ),
                    ("view", one_of_strings(&FitView::ALL.map(FitView::id))),
                    (
                        "subjects",
                        described(
                            nullable(array_of(crate::schema::subject())),
                            "The subjects drawn, in this order; omitted or null: every subject. Worksheet plots only.",
                        ),
                    ),
                    ("scale", one_of_strings(&["linear", "log", "both"])),
                    ("layout", one_of_strings(&["overlay", "per_subject"])),
                    (
                        "title",
                        described(
                            nullable(string()),
                            "Shown above the plot; omitted: the label of the plot.",
                        ),
                    ),
                ],
                &[],
            ),
        )
    },
    result: || root("plot.xy result", reference("AnalysisView")),
    example: || json!({ "worksheet": 1, "scale": "both", "layout": "per_subject" }),
    run: plot_xy,
};

// ---- export.plot -------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExportPlotParams {
    analysis: AnalysisId,
    #[serde(default = "svg_format")]
    format: String,
    #[serde(default)]
    width: Option<u32>,
    #[serde(default)]
    height: Option<u32>,
}

fn svg_format() -> String {
    "svg".to_owned()
}

fn side(value: Option<u32>, default: f64, what: &str) -> Result<f64, CommandError> {
    match value {
        None => Ok(default),
        Some(v) if (200..=2000).contains(&v) => Ok(f64::from(v)),
        Some(v) => Err(CommandError::invalid(
            "export.plot",
            format!("`{what}` is {v}; give a panel {what} between 200 and 2000 pixels"),
        )),
    }
}

fn export_plot(engine: &mut Engine, params: Value) -> Result<Value, CommandError> {
    let p: ExportPlotParams = parse("export.plot", params)?;
    if p.format != "svg" {
        return Err(CommandError::invalid(
            "export.plot",
            format!(
                "the format `{}` is not available; the formats are: svg",
                p.format
            ),
        ));
    }
    let size = svg::Size {
        width: side(p.width, svg::PANEL.width, "width")?,
        height: side(p.height, svg::PANEL.height, "height")?,
    };
    let project = &engine.project;
    let analysis = project.analysis(p.analysis)?;
    let run = match analysis.result() {
        Some(AnalysisResult::Plot(run)) => run,
        Some(_) => {
            return Err(CommandError::new(
                "not_a_plot",
                format!(
                    "analysis {} is a {} analysis; export.plot exports a plot (make one with plot.xy)",
                    p.analysis,
                    analysis.spec().kind()
                ),
            ));
        }
        None => {
            return Err(CommandError::new(
                "no_result",
                format!(
                    "plot {} has not been drawn; run it before exporting it",
                    p.analysis
                ),
            ));
        }
    };
    let status = match project.status(p.analysis)? {
        AnalysisStatus::Fresh => "fresh",
        AnalysisStatus::Stale { .. } => "stale",
        AnalysisStatus::NoResult => "no_result",
    };
    let label = project.label_of(p.analysis)?;
    respond(&json!({
        "format": "svg",
        "file_name": format!("{}.svg", export::slug(&label)),
        "status": status,
        "panels": run.panels.len(),
        "svg": svg::render(run, size),
    }))
}

pub(crate) const EXPORT_PLOT: CommandDef = CommandDef {
    id: "export.plot",
    title: "Export a plot as SVG",
    description: "The last drawing of a plot analysis as an SVG document (text; the caller writes the file), with a suggested file name. `status` says whether the plot is fresh or stale. `width` and `height` are the size of one panel in pixels (default 420 by 300); the panels are laid out in a grid with one legend. Colours are those of the light theme.",
    mutates: false,
    params: || {
        root(
            "export.plot parameters",
            object(
                vec![
                    ("analysis", reference("Id")),
                    ("format", one_of_strings(&["svg"])),
                    (
                        "width",
                        json!({ "type": "integer", "minimum": 200, "maximum": 2000 }),
                    ),
                    (
                        "height",
                        json!({ "type": "integer", "minimum": 200, "maximum": 2000 }),
                    ),
                ],
                &["analysis"],
            ),
        )
    },
    result: || {
        root(
            "export.plot result",
            object(
                vec![
                    ("format", one_of_strings(&["svg"])),
                    ("file_name", string()),
                    ("status", one_of_strings(&["fresh", "stale", "no_result"])),
                    ("panels", integer()),
                    ("svg", string()),
                ],
                &["format", "file_name", "status", "panels", "svg"],
            ),
        )
    },
    // The plot of the examples script is analysis 5, after the fit (3) and the population fit (4).
    example: || json!({ "analysis": 5 }),
    run: export_plot,
};
