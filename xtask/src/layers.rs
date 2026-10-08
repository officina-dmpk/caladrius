//! `cargo xtask layers`: the layer table and the dependency rules of AGENTS.md section 4.

use std::fmt;

use crate::console;
use crate::error::{Result, XtaskError};
use crate::workspace::{self, DepKind, Dependency, Member, Workspace};

/// A position in the crate stack.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    /// Numerical core: NCA, models, fitting.
    L0,
    /// Project model.
    L1,
    /// Command registry.
    L2,
    /// egui interface.
    L3,
    /// Test tooling: oracles and tolerances. It sits above the stack it tests, which uses it as a dev-dependency only.
    Test,
    /// Binaries that users run.
    App,
    /// Developer tooling (`xtask`).
    Tool,
}

impl Layer {
    pub fn label(self) -> &'static str {
        match self {
            Layer::L0 => "L0",
            Layer::L1 => "L1",
            Layer::L2 => "L2",
            Layer::L3 => "L3",
            Layer::Test => "test",
            Layer::App => "app",
            Layer::Tool => "tool",
        }
    }

    /// Position in the dependency order: a crate may depend on crates of equal or lower rank.
    fn rank(self) -> u8 {
        match self {
            Layer::L0 => 0,
            Layer::L1 => 1,
            Layer::L2 => 2,
            Layer::L3 => 3,
            Layer::Test => 4,
            Layer::App | Layer::Tool => 5,
        }
    }

    /// L0 to L3 compile for WebAssembly (golden rule 9).
    pub fn compiles_to_wasm(self) -> bool {
        matches!(self, Layer::L0 | Layer::L1 | Layer::L2 | Layer::L3)
    }

    /// Only the UI layer and the apps may know the UI toolkit.
    fn may_use_ui_toolkit(self) -> bool {
        matches!(self, Layer::L3 | Layer::App)
    }
}

impl fmt::Display for Layer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// The layer table: every workspace crate is registered here, in the same commit that creates it.
pub const LAYER_TABLE: &[(&str, Layer)] = &[
    ("caladrius-nca", Layer::L0),
    ("caladrius-models", Layer::L0),
    ("caladrius-fit", Layer::L0),
    ("caladrius-project", Layer::L1),
    ("caladrius-engine", Layer::L2),
    ("caladrius-ui", Layer::L3),
    ("caladrius-testkit", Layer::Test),
    ("caladrius", Layer::App),
    ("caladrius-cli", Layer::App),
    ("caladrius-mcp", Layer::App),
    ("xtask", Layer::Tool),
];

const RULES: &str = "\
Rules checked:
  1. a crate depends only on crates of its own layer or a lower one
     (order: L0 < L1 < L2 < L3 < test < app; the one exception is a dev-dependency on a test crate)
  2. only the L3 UI crate and the apps depend on egui, eframe, winit, wgpu or an egui_* crate
  3. every workspace crate is registered in LAYER_TABLE (xtask/src/layers.rs)";

pub fn layer_of(table: &[(&str, Layer)], name: &str) -> Option<Layer> {
    table
        .iter()
        .find(|(candidate, _)| *candidate == name)
        .map(|(_, layer)| *layer)
}

/// UI toolkit crates, which only the UI layer and the apps may use.
fn is_ui_toolkit(package: &str) -> bool {
    matches!(package, "egui" | "eframe" | "winit" | "wgpu")
        || package.starts_with("egui_")
        || package.starts_with("egui-")
}

/// A broken rule.
#[derive(Debug, PartialEq, Eq)]
pub enum Violation {
    /// A workspace crate that is not in the layer table.
    Unregistered { name: String, dir: String },
    /// A dependency on a higher layer.
    HigherLayer {
        name: String,
        layer: Layer,
        dep: String,
        dep_layer: Layer,
        declared_in: String,
    },
    /// A UI toolkit crate used below the UI layer.
    UiToolkit {
        name: String,
        layer: Layer,
        dep: String,
        declared_in: String,
    },
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Violation::Unregistered { name, dir } => write!(
                f,
                "workspace crate `{name}` ({dir}) is not in the layer table: register it in LAYER_TABLE in xtask/src/layers.rs"
            ),
            Violation::HigherLayer {
                name,
                layer,
                dep,
                dep_layer,
                declared_in,
            } => {
                write!(
                    f,
                    "`{name}` ({layer}) depends on `{dep}` ({dep_layer}) in {declared_in}: a crate may only depend on crates of its own layer or a lower one"
                )?;
                if *dep_layer == Layer::Test {
                    write!(f, "; test crates are allowed as [dev-dependencies] only")?;
                }
                Ok(())
            }
            Violation::UiToolkit {
                name,
                layer,
                dep,
                declared_in,
            } => write!(
                f,
                "`{name}` ({layer}) depends on `{dep}` in {declared_in}: UI toolkit crates (egui, eframe, winit, wgpu, egui_*) are reserved for the L3 UI crate and the apps"
            ),
        }
    }
}

/// Checks every member of the workspace against the table; an empty result means the rules hold.
pub fn check(workspace: &Workspace, table: &[(&str, Layer)]) -> Vec<Violation> {
    let mut violations = Vec::new();
    for member in &workspace.members {
        let Some(layer) = layer_of(table, &member.name) else {
            violations.push(Violation::Unregistered {
                name: member.name.clone(),
                dir: member.dir.clone(),
            });
            continue;
        };
        for dependency in &member.dependencies {
            if is_ui_toolkit(&dependency.package) && !layer.may_use_ui_toolkit() {
                violations.push(Violation::UiToolkit {
                    name: member.name.clone(),
                    layer,
                    dep: dependency.package.clone(),
                    declared_in: dependency.table(),
                });
            }
            let Some(dep_layer) = layer_of(table, &dependency.package) else {
                continue;
            };
            let test_tooling = dependency.kind == DepKind::Dev && dep_layer == Layer::Test;
            if dep_layer.rank() > layer.rank() && !test_tooling {
                violations.push(Violation::HigherLayer {
                    name: member.name.clone(),
                    layer,
                    dep: dependency.package.clone(),
                    dep_layer,
                    declared_in: dependency.table(),
                });
            }
        }
    }
    violations
}

/// The dependencies of a member that are workspace crates according to the table.
fn internal_dependencies<'a>(member: &'a Member, table: &[(&str, Layer)]) -> Vec<&'a Dependency> {
    member
        .dependencies
        .iter()
        .filter(|dependency| layer_of(table, &dependency.package).is_some())
        .collect()
}

fn describe(dependency: &Dependency) -> String {
    match dependency.kind {
        DepKind::Normal => dependency.package.clone(),
        DepKind::Dev => format!("{} (dev)", dependency.package),
        DepKind::Build => format!("{} (build)", dependency.package),
    }
}

/// The layer table as aligned text: layer, crate, directory and the workspace crates it depends on.
pub fn render_table(workspace: &Workspace, table: &[(&str, Layer)]) -> String {
    const HEADERS: [&str; 4] = [
        "layer",
        "crate",
        "directory",
        "depends on (workspace crates)",
    ];
    let rows: Vec<[String; 4]> = workspace
        .members
        .iter()
        .map(|member| {
            let depends_on = internal_dependencies(member, table)
                .into_iter()
                .map(describe)
                .collect::<Vec<_>>()
                .join(", ");
            [
                layer_of(table, &member.name)
                    .map_or("?", Layer::label)
                    .to_owned(),
                member.name.clone(),
                member.dir.clone(),
                if depends_on.is_empty() {
                    "-".to_owned()
                } else {
                    depends_on
                },
            ]
        })
        .collect();
    let mut widths = HEADERS.map(str::len);
    for row in &rows {
        for (width, cell) in widths.iter_mut().zip(row) {
            *width = (*width).max(cell.chars().count());
        }
    }
    let separator = widths.map(|width| "-".repeat(width));
    let mut lines = vec![
        format_row(&HEADERS, &widths),
        format_row(&separator.each_ref().map(String::as_str), &widths),
    ];
    lines.extend(
        rows.iter()
            .map(|row| format_row(&row.each_ref().map(String::as_str), &widths)),
    );
    lines.join("\n")
}

fn format_row(cells: &[&str; 4], widths: &[usize; 4]) -> String {
    let line = cells
        .iter()
        .zip(widths)
        .map(|(cell, &width)| format!("{cell:<width$}"))
        .collect::<Vec<_>>()
        .join("  ");
    line.trim_end().to_owned()
}

/// Prints the layer table, then fails with exit code 1 if a rule is broken.
pub fn run() -> Result<()> {
    let root = workspace::root()?;
    let workspace = workspace::load(&root)?;
    console::out(&render_table(&workspace, LAYER_TABLE));
    console::out("");
    console::out(RULES);
    let violations = check(&workspace, LAYER_TABLE);
    for violation in &violations {
        console::err(&format!("error: {violation}"));
    }
    if violations.is_empty() {
        let edges: usize = workspace
            .members
            .iter()
            .map(|member| internal_dependencies(member, LAYER_TABLE).len())
            .sum();
        console::out(&format!(
            "layers: ok ({} crates, {edges} dependencies between them)",
            workspace.members.len()
        ));
        Ok(())
    } else {
        Err(XtaskError::new(format!(
            "layers: {} violation(s)",
            violations.len()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::from_strs;

    /// A `[<kind>]` table with one path dependency per name.
    fn deps(kind: &str, names: &[&str]) -> String {
        let mut text = format!("[{kind}]\n");
        for name in names {
            text.push_str(&format!("{name} = {{ path = \"../{name}\" }}\n"));
        }
        text
    }

    /// Violations of a workspace whose members are `(package name, rest of the manifest)`, each in `crates/<name>`.
    fn violations(members: &[(&str, String)]) -> Result<Vec<Violation>> {
        let dirs: Vec<String> = members
            .iter()
            .map(|(name, _)| format!("crates/{name}"))
            .collect();
        let listed = dirs
            .iter()
            .map(|dir| format!("\"{dir}\""))
            .collect::<Vec<_>>()
            .join(", ");
        let root = format!("[workspace]\nmembers = [{listed}]\n");
        let manifests: Vec<String> = members
            .iter()
            .map(|(name, rest)| format!("[package]\nname = \"{name}\"\n{rest}"))
            .collect();
        let pairs: Vec<(&str, &str)> = dirs
            .iter()
            .map(String::as_str)
            .zip(manifests.iter().map(String::as_str))
            .collect();
        Ok(check(&from_strs(&root, &pairs)?, LAYER_TABLE))
    }

    fn higher(
        name: &str,
        layer: Layer,
        dep: &str,
        dep_layer: Layer,
        declared_in: &str,
    ) -> Violation {
        Violation::HigherLayer {
            name: name.to_owned(),
            layer,
            dep: dep.to_owned(),
            dep_layer,
            declared_in: declared_in.to_owned(),
        }
    }

    #[test]
    fn the_table_registers_each_crate_once_and_six_crates_compile_to_wasm() {
        let mut names: Vec<&str> = LAYER_TABLE.iter().map(|(name, _)| *name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), LAYER_TABLE.len());
        let wasm = LAYER_TABLE
            .iter()
            .filter(|(_, layer)| layer.compiles_to_wasm())
            .count();
        assert_eq!(wasm, 6);
    }

    #[test]
    fn a_clean_stack_has_no_violation() -> Result<()> {
        let found = violations(&[
            (
                "caladrius-nca",
                deps("dev-dependencies", &["caladrius-testkit"]),
            ),
            ("caladrius-models", String::new()),
            ("caladrius-fit", deps("dependencies", &["caladrius-models"])),
            (
                "caladrius-project",
                deps("dependencies", &["caladrius-nca"]),
            ),
            (
                "caladrius-engine",
                deps("dependencies", &["caladrius-project", "caladrius-fit"]),
            ),
            (
                "caladrius-ui",
                format!(
                    "{}[dependencies.egui]\nversion = \"0.30\"\n",
                    deps("dependencies", &["caladrius-engine"])
                ),
            ),
            (
                "caladrius-testkit",
                deps("dependencies", &["caladrius-nca", "caladrius-models"]),
            ),
            (
                "caladrius-cli",
                deps("dependencies", &["caladrius-engine", "caladrius-testkit"]),
            ),
            (
                "caladrius",
                format!(
                    "{}eframe = \"0.30\"\negui_plot = \"0.30\"\n",
                    deps("dependencies", &["caladrius-ui"])
                ),
            ),
            ("xtask", "[dependencies]\ntoml = \"1\"\n".to_owned()),
        ])?;
        assert!(found.is_empty(), "{found:?}");
        Ok(())
    }

    #[test]
    fn a_lower_layer_cannot_depend_on_a_higher_one() -> Result<()> {
        let found = violations(&[("caladrius-nca", deps("dependencies", &["caladrius-engine"]))])?;
        assert_eq!(
            found,
            [higher(
                "caladrius-nca",
                Layer::L0,
                "caladrius-engine",
                Layer::L2,
                "[dependencies]"
            )]
        );
        Ok(())
    }

    #[test]
    fn crates_of_the_same_layer_may_depend_on_each_other() -> Result<()> {
        let found = violations(&[(
            "caladrius-fit",
            deps("dependencies", &["caladrius-models", "caladrius-nca"]),
        )])?;
        assert!(found.is_empty(), "{found:?}");
        Ok(())
    }

    #[test]
    fn a_dev_dependency_on_the_test_layer_is_allowed_from_every_layer() -> Result<()> {
        let dev = deps("dev-dependencies", &["caladrius-testkit"]);
        let found = violations(&[
            ("caladrius-nca", dev.clone()),
            ("caladrius-project", dev.clone()),
            ("caladrius-engine", dev.clone()),
            ("caladrius-ui", dev.clone()),
            ("caladrius-cli", dev),
        ])?;
        assert!(found.is_empty(), "{found:?}");
        Ok(())
    }

    #[test]
    fn the_test_layer_is_refused_outside_dev_dependencies_below_the_apps() -> Result<()> {
        let found = violations(&[
            (
                "caladrius-nca",
                deps("dependencies", &["caladrius-testkit"]),
            ),
            (
                "caladrius-engine",
                deps("build-dependencies", &["caladrius-testkit"]),
            ),
            (
                "caladrius-cli",
                deps("dependencies", &["caladrius-testkit"]),
            ),
        ])?;
        assert_eq!(
            found,
            [
                higher(
                    "caladrius-nca",
                    Layer::L0,
                    "caladrius-testkit",
                    Layer::Test,
                    "[dependencies]"
                ),
                higher(
                    "caladrius-engine",
                    Layer::L2,
                    "caladrius-testkit",
                    Layer::Test,
                    "[build-dependencies]"
                ),
            ]
        );
        Ok(())
    }

    #[test]
    fn a_dev_dependency_on_a_higher_non_test_layer_is_refused() -> Result<()> {
        let found = violations(&[(
            "caladrius-nca",
            deps("dev-dependencies", &["caladrius-engine"]),
        )])?;
        assert_eq!(
            found,
            [higher(
                "caladrius-nca",
                Layer::L0,
                "caladrius-engine",
                Layer::L2,
                "[dev-dependencies]"
            )]
        );
        Ok(())
    }

    #[test]
    fn nothing_depends_on_an_app() -> Result<()> {
        let found = violations(&[("caladrius-engine", deps("dependencies", &["caladrius-cli"]))])?;
        assert_eq!(
            found,
            [higher(
                "caladrius-engine",
                Layer::L2,
                "caladrius-cli",
                Layer::App,
                "[dependencies]"
            )]
        );
        Ok(())
    }

    #[test]
    fn renamed_dependencies_are_still_checked() -> Result<()> {
        let rest =
            "[dependencies]\nalias = { package = \"caladrius-engine\", path = \"../engine\" }\n";
        let found = violations(&[("caladrius-nca", rest.to_owned())])?;
        assert_eq!(
            found,
            [higher(
                "caladrius-nca",
                Layer::L0,
                "caladrius-engine",
                Layer::L2,
                "[dependencies]"
            )]
        );
        Ok(())
    }

    #[test]
    fn target_specific_dependencies_are_checked() -> Result<()> {
        let rest =
            "[target.'cfg(windows)'.dependencies]\ncaladrius-ui = { path = \"../caladrius-ui\" }\n";
        let found = violations(&[("caladrius-project", rest.to_owned())])?;
        assert_eq!(
            found,
            [higher(
                "caladrius-project",
                Layer::L1,
                "caladrius-ui",
                Layer::L3,
                "[target.'cfg(windows)'.dependencies]"
            )]
        );
        Ok(())
    }

    #[test]
    fn ui_toolkit_crates_are_refused_below_the_ui_layer() -> Result<()> {
        let toolkit = |name: &str| format!("[dependencies]\n{name} = \"1\"\n");
        let found = violations(&[
            ("caladrius-nca", toolkit("egui")),
            ("caladrius-models", toolkit("eframe")),
            ("caladrius-fit", toolkit("egui_plot")),
            ("caladrius-project", toolkit("egui-wgpu")),
            ("caladrius-engine", toolkit("winit")),
            ("caladrius-testkit", toolkit("wgpu")),
            ("xtask", toolkit("egui")),
        ])?;
        assert_eq!(found.len(), 7, "{found:?}");
        assert!(
            found
                .iter()
                .all(|violation| matches!(violation, Violation::UiToolkit { .. })),
            "{found:?}"
        );
        Ok(())
    }

    #[test]
    fn ui_toolkit_crates_are_allowed_in_the_ui_layer_and_the_apps() -> Result<()> {
        let toolkit = "[dependencies]\negui = \"1\"\neframe = \"1\"\nwinit = \"1\"\nwgpu = \"1\"\negui_plot = \"1\"\n";
        let found = violations(&[
            ("caladrius-ui", toolkit.to_owned()),
            ("caladrius", toolkit.to_owned()),
            ("caladrius-mcp", toolkit.to_owned()),
        ])?;
        assert!(found.is_empty(), "{found:?}");
        Ok(())
    }

    #[test]
    fn an_unregistered_crate_is_reported() -> Result<()> {
        let found = violations(&[(
            "caladrius-newcomer",
            deps("dependencies", &["caladrius-engine"]),
        )])?;
        assert_eq!(
            found,
            [Violation::Unregistered {
                name: "caladrius-newcomer".to_owned(),
                dir: "crates/caladrius-newcomer".to_owned(),
            }]
        );
        Ok(())
    }

    #[test]
    fn the_rendered_table_lists_layers_crates_and_workspace_dependencies() -> Result<()> {
        // `serde` lands in the last table, [dev-dependencies]: it is not a workspace crate, so it is not listed.
        let rest = format!(
            "{}{}serde = \"1\"\n",
            deps("dependencies", &["caladrius-models"]),
            deps("dev-dependencies", &["caladrius-testkit"]),
        );
        let manifests = [
            (
                "crates/caladrius-nca",
                format!("[package]\nname = \"caladrius-nca\"\n{rest}"),
            ),
            (
                "crates/caladrius-models",
                "[package]\nname = \"caladrius-models\"\n".to_owned(),
            ),
            ("crates/stray", "[package]\nname = \"stray\"\n".to_owned()),
        ];
        let root = "[workspace]\nmembers = [\"crates/caladrius-nca\", \"crates/caladrius-models\", \"crates/stray\"]\n";
        let pairs: Vec<(&str, &str)> = manifests
            .iter()
            .map(|(dir, text)| (*dir, text.as_str()))
            .collect();
        let text = render_table(&from_strs(root, &pairs)?, LAYER_TABLE);

        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 5, "{text}");
        assert!(
            lines
                .iter()
                .any(|line| line.starts_with("layer") && line.contains("depends on")),
            "{text}"
        );
        assert!(
            lines.iter().any(|line| line.starts_with("L0")
                && line.contains("caladrius-nca")
                && line.contains("crates/caladrius-nca")
                && line.ends_with("caladrius-models, caladrius-testkit (dev)")),
            "{text}"
        );
        assert!(
            lines.iter().any(|line| line.starts_with("L0")
                && line.contains("caladrius-models")
                && line.ends_with('-')),
            "{text}"
        );
        assert!(
            lines
                .iter()
                .any(|line| line.starts_with('?') && line.contains("stray")),
            "{text}"
        );
        Ok(())
    }

    #[test]
    fn violation_messages_name_the_crate_the_dependency_and_what_to_do() {
        let message = higher(
            "caladrius-nca",
            Layer::L0,
            "caladrius-engine",
            Layer::L2,
            "[dependencies]",
        )
        .to_string();
        assert_eq!(
            message,
            "`caladrius-nca` (L0) depends on `caladrius-engine` (L2) in [dependencies]: a crate may only depend on crates of its own layer or a lower one"
        );
        let test_crate = higher(
            "caladrius-nca",
            Layer::L0,
            "caladrius-testkit",
            Layer::Test,
            "[dependencies]",
        )
        .to_string();
        assert!(
            test_crate.ends_with("test crates are allowed as [dev-dependencies] only"),
            "{test_crate}"
        );
        let toolkit = Violation::UiToolkit {
            name: "caladrius-engine".to_owned(),
            layer: Layer::L2,
            dep: "egui".to_owned(),
            declared_in: "[dependencies]".to_owned(),
        }
        .to_string();
        assert!(
            toolkit.contains("`egui`") && toolkit.contains("reserved for the L3 UI crate"),
            "{toolkit}"
        );
        let unregistered = Violation::Unregistered {
            name: "caladrius-newcomer".to_owned(),
            dir: "crates/caladrius-newcomer".to_owned(),
        }
        .to_string();
        assert!(unregistered.contains("LAYER_TABLE"), "{unregistered}");
    }
}
