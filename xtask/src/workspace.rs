//! Reading the workspace: the members listed in the root manifest and the dependencies each one declares.
//!
//! Parsing works on strings, so it is tested without touching the file system.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use toml::{Table, Value};

use crate::error::{Result, XtaskError};

/// Which dependency table declares a dependency.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DepKind {
    Normal,
    Dev,
    Build,
}

impl DepKind {
    const ALL: [DepKind; 3] = [DepKind::Normal, DepKind::Dev, DepKind::Build];

    fn table_name(self) -> &'static str {
        match self {
            DepKind::Normal => "dependencies",
            DepKind::Dev => "dev-dependencies",
            DepKind::Build => "build-dependencies",
        }
    }
}

/// One dependency declared by a workspace member.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dependency {
    /// Name of the depended-on package (`package = "..."` renames and `workspace = true` are resolved).
    pub package: String,
    pub kind: DepKind,
    /// The `cfg(...)` expression or target triple of a `[target.<..>.dependencies]` table, if any.
    pub target: Option<String>,
}

impl Dependency {
    /// The manifest table that declares the dependency, e.g. `[dev-dependencies]`.
    pub fn table(&self) -> String {
        match &self.target {
            Some(target) => format!("[target.'{target}'.{}]", self.kind.table_name()),
            None => format!("[{}]", self.kind.table_name()),
        }
    }
}

/// A crate of the workspace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Member {
    /// Directory relative to the workspace root, as written in `members`.
    pub dir: String,
    /// Package name.
    pub name: String,
    pub dependencies: Vec<Dependency>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Workspace {
    pub members: Vec<Member>,
}

/// The workspace root: the parent of this crate's directory (`xtask/`).
pub fn root() -> Result<PathBuf> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| {
            XtaskError::new("cannot locate the workspace root from the xtask manifest directory")
        })
}

/// Reads the root manifest and the manifest of every member.
pub fn load(root: &Path) -> Result<Workspace> {
    let root_manifest = read(&root.join("Cargo.toml"))?;
    parse_workspace(&root_manifest, |dir| {
        read(&root.join(dir).join("Cargo.toml"))
    })
}

fn read(path: &Path) -> Result<String> {
    fs::read_to_string(path).map_err(|source| XtaskError::io("read", path, &source))
}

/// Builds the workspace from the root manifest; `read_member` returns the manifest text of a member directory.
pub fn parse_workspace(
    root_manifest: &str,
    mut read_member: impl FnMut(&str) -> Result<String>,
) -> Result<Workspace> {
    let root = parse_toml("the workspace Cargo.toml", root_manifest)?;
    if root.contains_key("package") {
        return Err(XtaskError::new(
            "the workspace Cargo.toml must be a virtual manifest (no [package]): the layer check reads the members only",
        ));
    }
    let workspace = root
        .get("workspace")
        .and_then(Value::as_table)
        .ok_or_else(|| XtaskError::new("the workspace Cargo.toml has no [workspace] table"))?;
    let dirs = member_dirs(workspace)?;
    let renames = workspace_renames(workspace);
    let mut members = Vec::with_capacity(dirs.len());
    for dir in dirs {
        let manifest = read_member(&dir)?;
        members.push(parse_member(&dir, &manifest, &renames)?);
    }
    Ok(Workspace { members })
}

fn parse_toml(label: &str, text: &str) -> Result<Table> {
    text.parse::<Table>()
        .map_err(|source| XtaskError::new(format!("cannot parse {label}: {source}")))
}

/// The `members` list. Globs are refused: each member must be named so that it is registered in the layer table.
fn member_dirs(workspace: &Table) -> Result<Vec<String>> {
    let list = workspace
        .get("members")
        .and_then(Value::as_array)
        .ok_or_else(|| XtaskError::new("[workspace] has no `members` list"))?;
    let mut dirs = Vec::with_capacity(list.len());
    for entry in list {
        let dir = entry.as_str().ok_or_else(|| {
            XtaskError::new(format!("a `members` entry is not a string: {entry:?}"))
        })?;
        if dir.contains(['*', '?', '[']) {
            return Err(XtaskError::new(format!(
                "workspace member `{dir}` is a glob: list each member explicitly so that it gets registered in the layer table"
            )));
        }
        dirs.push(dir.to_owned());
    }
    Ok(dirs)
}

/// Packages that `[workspace.dependencies]` entries rename (`key = { package = "real-name", .. }`).
fn workspace_renames(workspace: &Table) -> BTreeMap<String, String> {
    let mut renames = BTreeMap::new();
    if let Some(dependencies) = workspace.get("dependencies").and_then(Value::as_table) {
        for (key, spec) in dependencies {
            if let Some(package) = spec
                .as_table()
                .and_then(|t| t.get("package"))
                .and_then(Value::as_str)
            {
                renames.insert(key.clone(), package.to_owned());
            }
        }
    }
    renames
}

fn parse_member(dir: &str, text: &str, renames: &BTreeMap<String, String>) -> Result<Member> {
    let label = format!("{dir}/Cargo.toml");
    let manifest = parse_toml(&label, text)?;
    let name = manifest
        .get("package")
        .and_then(Value::as_table)
        .and_then(|package| package.get("name"))
        .and_then(Value::as_str)
        .ok_or_else(|| XtaskError::new(format!("{label} has no [package] name")))?;
    let mut dependencies = Vec::new();
    collect_dependencies(&manifest, None, renames, &mut dependencies);
    if let Some(targets) = manifest.get("target").and_then(Value::as_table) {
        for (target, table) in targets {
            if let Some(table) = table.as_table() {
                collect_dependencies(table, Some(target), renames, &mut dependencies);
            }
        }
    }
    Ok(Member {
        dir: dir.to_owned(),
        name: name.to_owned(),
        dependencies,
    })
}

/// Appends the dependencies of the three dependency tables found in `table` (a manifest or a `target.<..>` table).
fn collect_dependencies(
    table: &Table,
    target: Option<&str>,
    renames: &BTreeMap<String, String>,
    out: &mut Vec<Dependency>,
) {
    for kind in DepKind::ALL {
        let Some(dependencies) = table.get(kind.table_name()).and_then(Value::as_table) else {
            continue;
        };
        for (key, spec) in dependencies {
            out.push(Dependency {
                package: package_name(key, spec, renames),
                kind,
                target: target.map(str::to_owned),
            });
        }
    }
}

/// The package a dependency entry refers to: `package = "..."` if present, else the workspace rename, else the key.
fn package_name(key: &str, spec: &Value, renames: &BTreeMap<String, String>) -> String {
    let Some(spec) = spec.as_table() else {
        return key.to_owned();
    };
    if let Some(package) = spec.get("package").and_then(Value::as_str) {
        return package.to_owned();
    }
    if spec.get("workspace").and_then(Value::as_bool) == Some(true) {
        if let Some(package) = renames.get(key) {
            return package.clone();
        }
    }
    key.to_owned()
}

/// Test helper: a workspace from in-memory manifests, as `(directory, manifest text)` pairs.
#[cfg(test)]
pub fn from_strs(root_manifest: &str, members: &[(&str, &str)]) -> Result<Workspace> {
    parse_workspace(root_manifest, |dir| {
        members
            .iter()
            .find(|(candidate, _)| *candidate == dir)
            .map(|(_, text)| (*text).to_owned())
            .ok_or_else(|| XtaskError::new(format!("test has no manifest for {dir}")))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROOT: &str = r#"
[workspace]
members = ["crates/a", "crates/b"]

[workspace.dependencies]
inherited = { package = "real-inherited", path = "crates/b" }
plain = "1"
"#;

    fn dep(package: &str, kind: DepKind, target: Option<&str>) -> Dependency {
        Dependency {
            package: package.to_owned(),
            kind,
            target: target.map(str::to_owned),
        }
    }

    #[test]
    fn members_and_all_dependency_tables_are_read() -> Result<()> {
        let a = r#"
[package]
name = "crate-a"

[dependencies]
crate-b = { path = "../b" }

[dev-dependencies]
serde = "1"

[build-dependencies]
cc = "1"

[target.'cfg(windows)'.dependencies]
winapi = "0.3"

[target.'cfg(unix)'.dev-dependencies]
libc = "0.2"
"#;
        let b = "[package]\nname = \"crate-b\"\n";
        let workspace = from_strs(ROOT, &[("crates/a", a), ("crates/b", b)])?;

        let names: Vec<&str> = workspace
            .members
            .iter()
            .map(|member| member.name.as_str())
            .collect();
        assert_eq!(names, ["crate-a", "crate-b"]);
        let dirs: Vec<&str> = workspace
            .members
            .iter()
            .map(|member| member.dir.as_str())
            .collect();
        assert_eq!(dirs, ["crates/a", "crates/b"]);
        let dependencies: Vec<&Dependency> = workspace
            .members
            .iter()
            .filter(|member| member.name == "crate-a")
            .flat_map(|member| member.dependencies.iter())
            .collect();
        let expected = [
            dep("crate-b", DepKind::Normal, None),
            dep("serde", DepKind::Dev, None),
            dep("cc", DepKind::Build, None),
            dep("winapi", DepKind::Normal, Some("cfg(windows)")),
            dep("libc", DepKind::Dev, Some("cfg(unix)")),
        ];
        // The order of the tables is not part of the contract, only their content.
        assert_eq!(dependencies.len(), expected.len(), "{dependencies:?}");
        for wanted in &expected {
            assert!(
                dependencies.contains(&wanted),
                "missing {wanted:?} in {dependencies:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn renamed_and_workspace_inherited_dependencies_resolve_to_package_names() -> Result<()> {
        let a = r#"
[package]
name = "crate-a"

[dependencies]
alias = { package = "caladrius-engine", path = "../engine" }
inherited.workspace = true
plain = { workspace = true }
untouched = "1"
"#;
        let b = "[package]\nname = \"crate-b\"\n";
        let workspace = from_strs(ROOT, &[("crates/a", a), ("crates/b", b)])?;

        let mut packages: Vec<&str> = workspace
            .members
            .iter()
            .flat_map(|member| member.dependencies.iter())
            .map(|dependency| dependency.package.as_str())
            .collect();
        packages.sort_unstable();
        assert_eq!(
            packages,
            ["caladrius-engine", "plain", "real-inherited", "untouched"]
        );
        Ok(())
    }

    #[test]
    fn dependency_table_names_the_declaring_table() {
        assert_eq!(dep("x", DepKind::Normal, None).table(), "[dependencies]");
        assert_eq!(dep("x", DepKind::Dev, None).table(), "[dev-dependencies]");
        assert_eq!(
            dep("x", DepKind::Build, Some("cfg(windows)")).table(),
            "[target.'cfg(windows)'.build-dependencies]"
        );
    }

    fn error_text<T>(result: Result<T>) -> String {
        match result {
            Ok(_) => "no error".to_owned(),
            Err(error) => error.to_string(),
        }
    }

    #[test]
    fn glob_members_are_refused() {
        let root = "[workspace]\nmembers = [\"crates/*\"]\n";
        assert!(error_text(from_strs(root, &[])).contains("is a glob"));
    }

    #[test]
    fn a_root_manifest_with_a_package_is_refused() {
        let root = "[package]\nname = \"root\"\n[workspace]\nmembers = []\n";
        assert!(error_text(from_strs(root, &[])).contains("virtual manifest"));
    }

    #[test]
    fn a_missing_workspace_table_or_members_list_is_reported() {
        assert!(error_text(from_strs("", &[])).contains("no [workspace] table"));
        assert!(error_text(from_strs("[workspace]\n", &[])).contains("no `members` list"));
    }

    #[test]
    fn a_member_without_a_package_name_is_reported_with_its_path() {
        let root = "[workspace]\nmembers = [\"crates/a\"]\n";
        let text = error_text(from_strs(root, &[("crates/a", "[dependencies]\n")]));
        assert!(
            text.contains("crates/a/Cargo.toml has no [package] name"),
            "{text}"
        );
    }

    #[test]
    fn invalid_toml_is_reported_with_its_label() {
        let root = "[workspace]\nmembers = [\"crates/a\"]\n";
        let text = error_text(from_strs(root, &[("crates/a", "[package\nname = ")]));
        assert!(text.contains("cannot parse crates/a/Cargo.toml"), "{text}");
    }

    #[test]
    fn an_unreadable_member_is_reported() {
        let root = "[workspace]\nmembers = [\"crates/missing\"]\n";
        assert!(error_text(from_strs(root, &[])).contains("crates/missing"));
    }
}
