//! `cargo xtask review`: the numbers the repository states, confronted with the files that own them.
//!
//! `lint` checks the mechanical rules of `AGENTS.md`; `conformance` computes the values. This task
//! answers a third question, the one an outside reader asks: *do the figures written in the README
//! and in the release notes still agree with the files that produce them?* It changes nothing.
//!
//! Rules, each with a stable id:
//!
//! - `review-conformance-total`: the overall line of `docs/conformance.md` equals the sum of its
//!   three sections.
//! - `review-conformance-floors`: every case in the conformance tables has a floor line, and every
//!   floor line names a case (the generated file must stay whole).
//! - `review-model-ids`: every model id of the registry has a conformance case, and every conformance
//!   case of the `pk1.`/`pk2.` form names a model of the registry.
//! - `review-versions`: the version of `Cargo.toml` and the one of `CITATION.cff` agree.
//! - `review-specs-index`: every `specs/*.md` is named by `specs/README.md`.
//! - `review-claims-checked`: the case and value counts the README states for its one- and
//!   two-compartment model rows are checked against `docs/conformance.md`, the file the section
//!   points at.
//! - `review-pk2-readme`: when the registry holds `pk2.` models, the README has a two-compartment
//!   row in its coverage table and no clause that still says they are not implemented.
//! - `review-specs-counts` (note): the "Which rules are settled" table of the README against the
//!   `- Status:` lines of each `specs/*.md`. The README counts them by grep and the tagging
//!   convention is the reader's, so a difference is worth reading, not a contradiction.
//! - `review-release-drift` (note): what changed under `docs/`, `specs/`, `oracle/scripts/`,
//!   `oracle/README.md` or `README.md` since the last tag, so that a claim covered by that tag is not
//!   read as a claim about `main`.
//!
//! A failure exits 1 (with `--strict`, a note exits 1 too). A rule that finds the claim correct says
//! nothing, so a consistent tree has only the real notes. The output is meant to be read in a
//! terminal and pasted into a review note.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use crate::console;
use crate::error::{Result, XtaskError};
use crate::lint::Tree;
use crate::workspace;

/// The file that owns the conformance numbers, and the reference the README coverage table is read
/// after.
const CONFORMANCE: &str = "docs/conformance.md";
/// The file that owns the model ids.
const REGISTRY: &str = "crates/caladrius-models/src/model.rs";
/// The reference the README points at, and after which its coverage rows are read.
const REFERENCE: &str = "docs/conformance.md";

/// Where one check stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// Contradicts a source file: the report exits 1.
    Fail,
    /// Worth reading, not a contradiction: the report exits 1 only with `--strict`.
    Note,
}

impl Kind {
    fn label(self) -> &'static str {
        match self {
            Kind::Fail => "fail",
            Kind::Note => "note",
        }
    }
}

/// One result of one rule.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Finding {
    rule: &'static str,
    kind: Kind,
    /// Where the claim is written (relative path, `/` separators).
    file: String,
    /// 1-based line of the claim, when the rule knows it.
    line: usize,
    message: String,
}

impl Finding {
    fn fail(rule: &'static str, file: &str, line: usize, message: impl Into<String>) -> Self {
        Self {
            rule,
            kind: Kind::Fail,
            file: file.to_owned(),
            line,
            message: message.into(),
        }
    }

    fn note(rule: &'static str, file: &str, line: usize, message: impl Into<String>) -> Self {
        Self {
            rule,
            kind: Kind::Note,
            file: file.to_owned(),
            line,
            message: message.into(),
        }
    }

    fn render(&self) -> String {
        let location = if self.line == 0 {
            self.file.clone()
        } else {
            format!("{}:{}", self.file, self.line)
        };
        format!(
            "{location} {} [{}] {}",
            self.rule,
            self.kind.label(),
            self.message
        )
    }
}

/// Running the whole review of one tree.
fn checks(tree: &Tree, root: &Path) -> Result<Vec<Finding>> {
    let mut findings = Vec::new();
    findings.extend(conformance_total(tree));
    findings.extend(conformance_floors(tree));
    findings.extend(model_ids(tree));
    findings.extend(versions(tree));
    findings.extend(specs_index(tree));
    findings.extend(claims_checked(tree));
    findings.extend(two_compartment_readme(tree));
    findings.extend(specs_counts(tree));
    findings.extend(release_drift(root));
    findings.sort_by(|a, b| {
        (&a.file, a.line, a.rule, a.message.as_str()).cmp(&(
            &b.file,
            b.line,
            b.rule,
            b.message.as_str(),
        ))
    });
    Ok(findings)
}

/// The totals an `<n> of <n> values validated` line carries, and its case count when it has one.
fn counts_of(line: &str) -> Option<((usize, usize), Option<usize>)> {
    let (before, rest) = line.split_once(" of ")?;
    let start = before
        .rsplit(|c: char| !c.is_ascii_digit())
        .next()
        .and_then(|number| number.parse::<usize>().ok())?;
    let (expected, rest) = rest.split_once(" values validated")?;
    let expected = expected.trim().parse::<usize>().ok()?;
    let cases = rest.split_once(" on ").and_then(|(_, after)| {
        after
            .split_ascii_whitespace()
            .next()
            .and_then(|number| number.parse::<usize>().ok())
    });
    Some(((start, expected), cases))
}

/// The counts of the conformance file: the overall line (with its line number) and the total of each
/// section.
#[derive(Debug, Default, PartialEq, Eq)]
struct Conformance {
    overall: Option<(usize, usize)>,
    /// 1-based line of the overall line, 0 when there is none.
    overall_line: usize,
    sections: Vec<usize>,
}

/// Reads `docs/conformance.md`: the overall line and the total of each section.
fn parse_conformance(text: &str) -> Conformance {
    let mut out = Conformance::default();
    for (index, line) in text.lines().enumerate() {
        let Some((values, cases)) = counts_of(line) else {
            continue;
        };
        if line.starts_with("**Overall:") {
            out.overall = Some(values);
            out.overall_line = index + 1;
        } else if cases.is_some() {
            out.sections.push(values.0);
        }
    }
    out
}

/// The overall count is the sum of what the sections validated, and both halves agree.
fn conformance_total(tree: &Tree) -> Vec<Finding> {
    let Some(text) = tree.files.get(CONFORMANCE) else {
        return vec![Finding::fail(
            "review-conformance-total",
            CONFORMANCE,
            0,
            "the file is missing: `cargo xtask conformance` has not been run, or the path moved",
        )];
    };
    let parsed = parse_conformance(text);
    let Some((overall, expected)) = parsed.overall else {
        return vec![Finding::fail(
            "review-conformance-total",
            CONFORMANCE,
            0,
            "no `**Overall: <n> of <n> values validated**` line",
        )];
    };
    let line = parsed.overall_line;
    let mut findings = Vec::new();
    if overall != expected {
        findings.push(Finding::fail(
            "review-conformance-total",
            CONFORMANCE,
            line,
            format!("the overall line says {overall} of {expected}: the two halves differ"),
        ));
    }
    let sum: usize = parsed.sections.iter().sum();
    if sum != overall {
        findings.push(Finding::fail(
            "review-conformance-total",
            CONFORMANCE,
            line,
            format!(
                "the overall line says {overall}, the {} sections total {sum}",
                parsed.sections.len()
            ),
        ));
    }
    findings
}

/// Every case of the tables has a floor line, and every floor line names a case.
fn conformance_floors(tree: &Tree) -> Vec<Finding> {
    let Some(text) = tree.files.get(CONFORMANCE) else {
        return Vec::new();
    };
    // Name -> 1-based line of its heading or of its floor line.
    let mut cases: BTreeMap<String, usize> = BTreeMap::new();
    let mut floors: BTreeMap<String, usize> = BTreeMap::new();
    for (index, line) in text.lines().enumerate() {
        if let Some(name) = line.strip_prefix("### ") {
            cases.entry(name.trim().to_owned()).or_insert(index + 1);
        } else if let Some(name) = line
            .trim_start()
            .strip_prefix("<!-- floor ")
            .and_then(|rest| rest.split_ascii_whitespace().next())
        {
            floors.entry(name.to_owned()).or_insert(index + 1);
        }
    }
    let mut findings = Vec::new();
    for (name, line) in &cases {
        if !floors.contains_key(name) {
            findings.push(Finding::fail(
                "review-conformance-floors",
                CONFORMANCE,
                *line,
                format!("case `{name}` has no floor line"),
            ));
        }
    }
    for (name, line) in &floors {
        if !cases.contains_key(name) {
            findings.push(Finding::fail(
                "review-conformance-floors",
                CONFORMANCE,
                *line,
                format!("floor line `{name}` names no case in the file"),
            ));
        }
    }
    findings
}

/// The model ids of the registry, as the string literals of `model.rs`, each with the 1-based line
/// of its first appearance. Only the `pk1.` and `pk2.` families are read.
fn registry_ids(text: &str) -> BTreeMap<String, usize> {
    let mut ids = BTreeMap::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim_start().starts_with("//") {
            continue;
        }
        let mut rest = line;
        while let Some((_, after)) = rest.split_once("\"pk") {
            let Some((tail, next)) = after.split_once('"') else {
                break;
            };
            let candidate = format!("pk{tail}");
            if candidate.starts_with("pk1.") || candidate.starts_with("pk2.") {
                ids.entry(candidate).or_insert(index + 1);
            }
            rest = next;
        }
    }
    ids
}

/// The model ids a conformance table names in its second column, with the line of each. The row
/// that lists the inputs the engine must refuse names them as a pattern
/// (`pk2.* (inputs to refuse)`), which is not a model id.
fn conformance_models(text: &str) -> BTreeMap<String, usize> {
    let mut covered = BTreeMap::new();
    for (index, line) in text.lines().enumerate() {
        let Some(cell) = line.split('|').nth(2) else {
            continue;
        };
        let cell = cell.trim();
        if !cell.contains(' ') && (cell.starts_with("pk1.") || cell.starts_with("pk2.")) {
            covered.entry(cell.to_owned()).or_insert(index + 1);
        }
    }
    covered
}

/// Every model of the registry is covered by a conformance case, and the reverse.
fn model_ids(tree: &Tree) -> Vec<Finding> {
    let Some(registry) = tree.files.get(REGISTRY) else {
        return vec![Finding::fail(
            "review-model-ids",
            REGISTRY,
            0,
            "the registry file is missing: the path moved",
        )];
    };
    let ids = registry_ids(registry);
    if ids.is_empty() {
        return vec![Finding::fail(
            "review-model-ids",
            REGISTRY,
            0,
            "no `pk1.`/`pk2.` id found: the registry no longer looks as expected",
        )];
    }
    let Some(text) = tree.files.get(CONFORMANCE) else {
        return Vec::new();
    };
    let covered = conformance_models(text);
    let mut findings = Vec::new();
    for (id, line) in &ids {
        if !covered.contains_key(id) {
            findings.push(Finding::fail(
                "review-model-ids",
                REGISTRY,
                *line,
                format!("model `{id}` has no conformance case: the oracle does not cover it"),
            ));
        }
    }
    for (id, line) in &covered {
        if !ids.contains_key(id) {
            findings.push(Finding::fail(
                "review-model-ids",
                CONFORMANCE,
                *line,
                format!("a conformance case names `{id}`, absent from the registry"),
            ));
        }
    }
    findings
}

/// A version as a file declares it.
#[derive(Debug, PartialEq, Eq)]
enum Declared {
    /// The version and its 1-based line.
    Version(String, usize),
    /// The manifest takes the version of the workspace (`version.workspace = true`).
    Inherited,
    /// No readable version.
    Missing,
}

/// The value of a `key = "x.y.z"` (TOML) or `key: x.y.z` (YAML) line, without a trailing comment.
fn value_of(line: &str, key: &str, separator: char) -> Option<String> {
    let rest = line.strip_prefix(key)?.trim_start();
    let rest = rest.strip_prefix(separator)?;
    let value = rest.split('#').next().unwrap_or_default();
    let value = value.trim().trim_matches('"').trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_owned())
    }
}

/// The version of the manifest: the `version` key of `[workspace.package]` or `[package]`.
fn manifest_version(text: &str) -> Declared {
    let mut in_package = false;
    let mut inherited = false;
    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            let name = trimmed.split('#').next().unwrap_or_default().trim();
            in_package = name == "[workspace.package]" || name == "[package]";
            continue;
        }
        if !in_package {
            continue;
        }
        if let Some(value) = value_of(trimmed, "version", '=') {
            return Declared::Version(value, index + 1);
        }
        if trimmed.starts_with("version.workspace") && trimmed.contains("true") {
            inherited = true;
        }
    }
    if inherited {
        Declared::Inherited
    } else {
        Declared::Missing
    }
}

/// The version of `CITATION.cff`: the first `version:` at the top level (an indented one belongs to a
/// nested entry, such as a reference).
fn citation_version(text: &str) -> Declared {
    for (index, line) in text.lines().enumerate() {
        if let Some(value) = value_of(line, "version", ':') {
            return Declared::Version(value, index + 1);
        }
    }
    Declared::Missing
}

/// `Cargo.toml` and `CITATION.cff` state the same version, and both state one.
fn versions(tree: &Tree) -> Vec<Finding> {
    let (Some(cargo), Some(citation)) =
        (tree.files.get("Cargo.toml"), tree.files.get("CITATION.cff"))
    else {
        return Vec::new();
    };
    let Declared::Version(cite, line) = citation_version(citation) else {
        return vec![Finding::fail(
            "review-versions",
            "CITATION.cff",
            0,
            "no top-level `version:` line: the version of the citation is unreadable",
        )];
    };
    match manifest_version(cargo) {
        Declared::Version(manifest, _) if manifest == cite => Vec::new(),
        Declared::Version(manifest, _) => vec![Finding::fail(
            "review-versions",
            "CITATION.cff",
            line,
            format!("CITATION.cff says {cite}, Cargo.toml says {manifest}"),
        )],
        Declared::Inherited => Vec::new(),
        Declared::Missing => vec![Finding::fail(
            "review-versions",
            "Cargo.toml",
            0,
            "no `version` in `[workspace.package]` or `[package]`: the version of the manifest is unreadable",
        )],
    }
}

/// Every `specs/*.md` is named by `specs/README.md`.
fn specs_index(tree: &Tree) -> Vec<Finding> {
    let Some(index) = tree.files.get("specs/README.md") else {
        return Vec::new();
    };
    let mut findings = Vec::new();
    for path in tree.files.keys() {
        let Some(name) = path.strip_prefix("specs/") else {
            continue;
        };
        if name == "README.md" || !name.ends_with(".md") {
            continue;
        }
        if !index.contains(name) {
            findings.push(Finding::fail(
                "review-specs-index",
                "specs/README.md",
                0,
                format!("`{name}` exists but the index does not name it"),
            ));
        }
    }
    findings
}

/// The first number of a string.
fn first_number(text: &str) -> Option<usize> {
    number_at(text, 0)
}

/// The second number of a string.
fn second_number(text: &str) -> Option<usize> {
    number_at(text, 1)
}

fn number_at(text: &str, index: usize) -> Option<usize> {
    let mut found = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        if ch.is_ascii_digit() {
            current.push(ch);
        } else if !current.is_empty() {
            found.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        found.push(current);
    }
    found.get(index)?.parse::<usize>().ok()
}

/// The model family a coverage row speaks about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Family {
    One,
    Two,
}

/// The case and value counts the README states for one model family, and the line of the row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Claim {
    family: Family,
    cases: usize,
    values: usize,
    line: usize,
}

/// The case and value counts the README states for each model family in its coverage table.
fn readme_claims(text: &str) -> Option<Vec<Claim>> {
    let lines: Vec<&str> = text.lines().collect();
    let reference = lines.iter().position(|line| line.contains(REFERENCE))?;
    let mut claims = Vec::new();
    for (index, line) in lines.iter().enumerate().skip(reference) {
        let row: Vec<&str> = line.split('|').map(str::trim).collect();
        // A model row: | description | checked against | tolerance | cases and values | not validated |
        // (the first and the last cell of the split are the empty outsides of the row).
        if row.len() < 6 {
            continue;
        }
        let (Some(description), Some(counts)) = (row.get(1), row.get(4)) else {
            continue;
        };
        let family = if description.contains("two-compartment models") {
            Family::Two
        } else if description.contains("one-compartment models") {
            Family::One
        } else {
            continue;
        };
        if !counts.contains("case") {
            continue;
        }
        let (Some(cases), Some(values)) = (first_number(counts), second_number(counts)) else {
            continue;
        };
        claims.push(Claim {
            family,
            cases,
            values,
            line: index + 1,
        });
    }
    Some(claims)
}

/// The conformance section that holds the one- and two-compartment model cases, and the heading
/// that ends it (the fit section repeats the model ids in the other direction).
const MODELS_HEADING: &str = "## Models against exact closed-form values";
const NEXT_HEADING: &str = "## Weighted least-squares fits";

/// What the conformance file covers, per model family, inside the models section only.
fn coverage(text: &str) -> BTreeMap<String, (usize, usize)> {
    let mut per_model: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut inside = false;
    for line in text.lines() {
        if line.starts_with(NEXT_HEADING) {
            break;
        }
        if line.starts_with(MODELS_HEADING) {
            inside = true;
            continue;
        }
        if !inside {
            continue;
        }
        let row: Vec<&str> = line.split('|').map(str::trim).collect();
        if row.len() < 7 {
            continue;
        }
        let (Some(model), Some(validated)) = (row.get(2), row.get(4)) else {
            continue;
        };
        if !model.starts_with("pk") {
            continue;
        }
        let family = model.split('.').next().unwrap_or_default().to_owned();
        let values = validated
            .split_once('/')
            .and_then(|(_, after)| after.trim().parse::<usize>().ok())
            .unwrap_or(0);
        let entry = per_model.entry(family).or_insert((0, 0));
        entry.0 += 1;
        entry.1 += values;
    }
    per_model
}

/// The model counts the README states are checked against `docs/conformance.md`. Agreement is
/// silent: a confirmation is not something to read, so `--strict` can pass.
fn claims_checked(tree: &Tree) -> Vec<Finding> {
    let Some(readme) = tree.files.get("README.md") else {
        return Vec::new();
    };
    let Some(conformance) = tree.files.get(CONFORMANCE) else {
        return Vec::new();
    };
    let Some(claims) = readme_claims(readme) else {
        return vec![Finding::note(
            "review-claims-checked",
            "README.md",
            0,
            format!("the README does not name `{REFERENCE}`: nothing checked"),
        )];
    };
    if claims.is_empty() {
        return vec![Finding::note(
            "review-claims-checked",
            "README.md",
            0,
            "the README names the conformance file but states no model case/value counts",
        )];
    }
    let per_model = coverage(conformance);
    let mut findings = Vec::new();
    for claim in claims {
        let key = match claim.family {
            Family::One => "pk1",
            Family::Two => "pk2",
        };
        let Some((actual_cases, actual_values)) = per_model.get(key) else {
            findings.push(Finding::fail(
                "review-claims-checked",
                "README.md",
                claim.line,
                format!("the README claims {key} coverage, `{CONFORMANCE}` has no {key} case"),
            ));
            continue;
        };
        if claim.cases != *actual_cases || claim.values != *actual_values {
            findings.push(Finding::fail(
                "review-claims-checked",
                "README.md",
                claim.line,
                format!(
                    "the README claims {} cases and {} values for {key}; \
                     `{CONFORMANCE}` holds {actual_cases} cases and {actual_values} values",
                    claim.cases, claim.values
                ),
            ));
        }
    }
    findings
}

/// The README speaks about two-compartment models when the registry has them: a coverage row with
/// counts, and no clause that still says they are not implemented.
fn two_compartment_readme(tree: &Tree) -> Vec<Finding> {
    let Some(registry) = tree.files.get(REGISTRY) else {
        return Vec::new();
    };
    if !registry_ids(registry)
        .keys()
        .any(|id| id.starts_with("pk2."))
    {
        return Vec::new();
    }
    let Some(readme) = tree.files.get("README.md") else {
        return Vec::new();
    };
    let mut findings = Vec::new();
    let has_row = readme_claims(readme)
        .is_some_and(|claims| claims.iter().any(|claim| claim.family == Family::Two));
    if !has_row {
        findings.push(Finding::fail(
            "review-pk2-readme",
            "README.md",
            0,
            "the registry has `pk2.` models but the coverage table has no two-compartment row \
             with case and value counts",
        ));
    }
    for (index, line) in readme.lines().enumerate() {
        // A clause is what lies between two separators, so that "user-written models (not
        // implemented); the two-compartment fit" is not read as one statement.
        let stale = line
            .to_ascii_lowercase()
            .split([';', '|', '.'])
            .any(|clause| {
                (clause.contains("two-compartment") || clause.contains("two compartment"))
                    && clause.contains("not implemented")
            });
        if stale {
            findings.push(Finding::fail(
                "review-pk2-readme",
                "README.md",
                index + 1,
                "the README still says two-compartment models are not implemented, \
                 the registry has `pk2.` models",
            ));
        }
    }
    findings
}

/// The counts of the status table of the README, against the `- Status: <tag>` lines of each spec
/// (`**Status:**` is also accepted, for a rule that spells the tag differently).
fn specs_counts(tree: &Tree) -> Vec<Finding> {
    let Some(readme) = tree.files.get("README.md") else {
        return Vec::new();
    };
    let mut actual: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (path, text) in &tree.files {
        let Some(name) = path.strip_prefix("specs/") else {
            continue;
        };
        if !name.ends_with(".md") || name == "README.md" {
            continue;
        }
        let mut confirmed = 0_usize;
        let mut untested = 0_usize;
        let mut assumed = 0_usize;
        let mut observed = 0_usize;
        for line in text.lines() {
            let trimmed = line.trim_start();
            let trimmed = trimmed.strip_prefix("- ").unwrap_or(trimmed);
            let rest = trimmed
                .strip_prefix("**Status:**")
                .or_else(|| trimmed.strip_prefix("Status:"));
            let Some(rest) = rest else {
                continue;
            };
            let status = rest
                .trim_start_matches(|c: char| c == ':' || c.is_whitespace())
                .trim_start_matches('`')
                .to_ascii_lowercase();
            if status.starts_with("confirmed") {
                confirmed += 1;
            } else if status.starts_with("assumed") {
                assumed += 1;
            } else if status.starts_with("observed") {
                observed += 1;
            } else if status.starts_with("documented") {
                untested += 1;
            }
        }
        actual.insert(
            name.to_owned(),
            vec![confirmed, untested, assumed, observed],
        );
    }
    let mut findings = Vec::new();
    for (index, line) in readme.lines().enumerate() {
        let row: Vec<&str> = line.split('|').map(str::trim).collect();
        if row.len() < 7 {
            continue;
        }
        let (Some(first), Some(cells)) = (row.get(1), row.get(2..6)) else {
            continue;
        };
        if !first.starts_with("`specs/") {
            continue;
        }
        let name = first
            .trim_matches('`')
            .trim_start_matches("specs/")
            .to_owned();
        let Some(counts) = actual.get(&name) else {
            findings.push(Finding::note(
                "review-specs-counts",
                "README.md",
                index + 1,
                format!("`{name}` is listed in the table but is not in the tree"),
            ));
            continue;
        };
        let claimed: Vec<usize> = cells.iter().filter_map(|cell| cell.parse().ok()).collect();
        if claimed.len() == counts.len() && claimed != *counts {
            findings.push(Finding::note(
                "review-specs-counts",
                "README.md",
                index + 1,
                format!(
                    "{name}: the README table says {claimed:?}, the `Status:` lines count {counts:?}"
                ),
            ));
        }
    }
    findings
}

/// What the drift note counts: the files whose change a reader of the tag does not see.
const DRIFT_SCOPE: &str = "docs/, specs/, oracle/scripts/, oracle/README.md or README.md";

/// Whether a changed path is one the drift note counts.
fn is_tracked_for_drift(path: &str) -> bool {
    ["docs/", "specs/", "oracle/scripts/"]
        .iter()
        .any(|prefix| path.starts_with(prefix))
        || path == "oracle/README.md"
        || path == "README.md"
}

/// What changed under `docs/`, `specs/`, `oracle/scripts/`, `oracle/README.md` or `README.md` since
/// the last tag. Git is asked here; the reading of its answers is [`drift_findings`].
fn release_drift(root: &Path) -> Vec<Finding> {
    let tag = git(root, &["describe", "--tags", "--abbrev=0"]).map(|text| text.trim().to_owned());
    let changed = tag
        .as_deref()
        .and_then(|tag| git(root, &["diff", "--name-only", tag, "--"]));
    drift_findings(tag.as_deref(), changed.as_deref())
}

/// The note about the drift since `tag`, from the answers of git (`None` when git had none).
fn drift_findings(tag: Option<&str>, changed: Option<&str>) -> Vec<Finding> {
    let Some(tag) = tag else {
        return vec![Finding::note(
            "review-release-drift",
            "README.md",
            0,
            "no tag found: the drift of the published release cannot be reported",
        )];
    };
    let Some(changed) = changed else {
        return vec![Finding::note(
            "review-release-drift",
            "README.md",
            0,
            format!(
                "`git diff` against {tag} failed: the drift of the published release cannot be reported"
            ),
        )];
    };
    let mut tracked: Vec<&str> = changed
        .lines()
        .map(str::trim)
        .filter(|path| is_tracked_for_drift(path))
        .collect();
    tracked.sort_unstable();
    if tracked.is_empty() {
        return vec![Finding::note(
            "review-release-drift",
            "README.md",
            0,
            format!("nothing under {DRIFT_SCOPE} changed since {tag}"),
        )];
    }
    vec![Finding::note(
        "review-release-drift",
        "README.md",
        0,
        format!(
            "{} file(s) under {DRIFT_SCOPE} changed since {tag}: a reader who opens the tag does \
             not see them (first three: {})",
            tracked.len(),
            tracked
                .iter()
                .take(3)
                .copied()
                .collect::<Vec<_>>()
                .join(", ")
        ),
    )]
}

/// Runs one git command in the repository; `None` when git or the command is unavailable.
fn git(root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

/// `cargo xtask review [--strict]`: prints the findings and exits 1 on a failure (or on a note with
/// `--strict`).
pub fn run(strict: bool) -> Result<()> {
    let root = workspace::root()?;
    let tree = crate::lint::load_tree(&root)?;
    let findings = checks(&tree, &root)?;
    let failures = findings.iter().filter(|f| f.kind == Kind::Fail).count();
    let notes = findings.len() - failures;
    for finding in &findings {
        console::out(&finding.render());
    }
    console::out(&format!(
        "review: {failures} failure(s), {notes} note(s), {} file(s) read",
        tree.files.len()
    ));
    if failures > 0 || (strict && notes > 0) {
        return Err(XtaskError::new(format!(
            "review: {failures} failure(s), {notes} note(s)"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(files: &[(&str, &str)]) -> Tree {
        Tree {
            files: files
                .iter()
                .map(|(path, text)| ((*path).to_owned(), (*text).to_owned()))
                .collect(),
            tracked: files.iter().map(|(path, _)| (*path).to_owned()).collect(),
            members: Vec::new(),
        }
    }

    const CONFORMANCE_OK: &str = "\
# Conformance

**Overall: 300 of 300 values validated (100.0 %).**

## NCA against PKNCA

100 of 100 values validated (100.0 %) on 2 cases.

## Models against exact closed-form values

200 of 200 values validated (100.0 %) on 3 cases.

| case | model | quantities | values validated | % | documented |
|---|---|---|---|---|---|
| model_a | pk1.iv_bolus | 2 / 2 | 60 / 60 | 100.0 | 0 |
| model_b | pk2.iv_bolus | 2 / 2 | 100 / 100 | 100.0 | 0 |
| model_c | pk2.oral_1 | 2 / 2 | 40 / 40 | 100.0 | 0 |

### model_a
<!-- floor model_a conc 1 1 0 -->

### model_b
<!-- floor model_b conc 2 2 0 -->

### model_c
<!-- floor model_c conc 2 2 0 -->
";

    const REGISTRY_OK: &str = "\
pub const MODELS: &[Model] = &[
    Model { id: \"pk1.iv_bolus\" },
    Model { id: \"pk2.iv_bolus\" },
    Model { id: \"pk2.oral_1\" },
];
";

    /// The README fixture of the coverage tests: one model row per family, with the counts given.
    fn readme_with_rows(one: (usize, usize), two: (usize, usize)) -> String {
        let header = "| What | Checked against | Tolerance | Cases and values | Not validated |";
        let rule = "|---|---|---|---|---|";
        format!(
            "See `docs/conformance.md` for the table.\n\n{header}\n{rule}\n\
             | Model curves: six one-compartment models | closed forms | 1e-12 | {} case, {} values | none |\n\
             | Model curves: six two-compartment models | closed forms | 1e-12 | {} cases, {} values | none |\n",
            one.0, one.1, two.0, two.1
        )
    }

    #[test]
    fn a_consistent_conformance_file_has_no_finding() {
        let findings = conformance_total(&tree(&[(CONFORMANCE, CONFORMANCE_OK)]));
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn a_section_sum_that_differs_from_the_overall_line_fails() {
        let text = CONFORMANCE_OK.replace("**Overall: 300", "**Overall: 301");
        let findings = conformance_total(&tree(&[(CONFORMANCE, &text)]));
        assert!(
            findings.iter().all(|f| f.kind == Kind::Fail) && !findings.is_empty(),
            "{findings:?}"
        );
    }

    #[test]
    fn a_missing_floor_or_an_orphan_floor_fails() {
        let missing = CONFORMANCE_OK.replace("<!-- floor model_b conc 2 2 0 -->", "");
        let findings = conformance_floors(&tree(&[(CONFORMANCE, &missing)]));
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].message.contains("model_b"));

        let orphan = CONFORMANCE_OK.replace("### model_c\n", "### model_z\n");
        let findings = conformance_floors(&tree(&[(CONFORMANCE, &orphan)]));
        assert_eq!(findings.len(), 2, "{findings:?}");
    }

    #[test]
    fn a_model_without_a_case_or_a_case_without_a_model_fails() {
        let files = [(CONFORMANCE, CONFORMANCE_OK), (REGISTRY, REGISTRY_OK)];
        assert!(model_ids(&tree(&files)).is_empty());

        let registry = REGISTRY_OK.replace("    Model { id: \"pk2.oral_1\" },\n", "");
        let findings = model_ids(&tree(&[
            (CONFORMANCE, CONFORMANCE_OK),
            (REGISTRY, registry.as_str()),
        ]));
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].message.contains("pk2.oral_1"));
    }

    #[test]
    fn a_commented_out_id_is_not_a_model() {
        let registry = format!("{REGISTRY_OK}// Model {{ id: \"pk2.oral_0\" }},\n");
        assert!(!registry_ids(&registry).contains_key("pk2.oral_0"));
    }

    #[test]
    fn the_input_to_refuse_row_is_not_a_model_id() {
        let text = CONFORMANCE_OK.replace(
            "| model_a | pk1.iv_bolus | 2 / 2 | 60 / 60 | 100.0 | 0 |",
            "| model_a | pk1.iv_bolus | 2 / 2 | 60 / 60 | 100.0 | 0 |\n\
             | model_err | pk2.* (inputs to refuse) | 6 / 6 | 80 / 80 | 100.0 | 0 |",
        );
        assert!(
            text.contains("inputs to refuse"),
            "the fixture lost its row"
        );
        let findings = model_ids(&tree(&[
            (CONFORMANCE, text.as_str()),
            (REGISTRY, REGISTRY_OK),
        ]));
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn the_coverage_counts_only_the_models_section() {
        let text = format!(
            "{CONFORMANCE_OK}\n## Weighted least-squares fits\n\n\
             | case | model | weighting | subjects | groups | values validated | % | documented |\n\
             |---|---|---|---|---|---|---|---|\n\
             | fit_a | pk1.iv_bolus | uniform | 1 | 2 / 2 | 50 / 50 | 100.0 | 0 |\n"
        );
        let per_model = coverage(&text);
        assert_eq!(per_model.get("pk1"), Some(&(1, 60)), "{per_model:?}");
    }

    #[test]
    fn a_version_mismatch_fails_and_a_match_does_not() {
        let cargo = "[workspace.package]\nversion = \"0.1.0\"\n";
        let cite = "version: 0.1.0\n";
        assert!(versions(&tree(&[("Cargo.toml", cargo), ("CITATION.cff", cite)])).is_empty());
        let cite = "cff-version: 1.2.0\ntitle: x\nversion: 0.2.0\n";
        let findings = versions(&tree(&[("Cargo.toml", cargo), ("CITATION.cff", cite)]));
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].message.contains("0.2.0"));
        assert_eq!(
            findings[0].line, 3,
            "the line of the version in CITATION.cff"
        );
    }

    #[test]
    fn a_version_line_that_the_manifest_does_not_have_is_not_a_finding() {
        let cargo = "[package]\nversion.workspace = true\n";
        let cite = "version: 0.1.0\n";
        let findings = versions(&tree(&[("Cargo.toml", cargo), ("CITATION.cff", cite)]));
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn a_spec_absent_from_the_index_fails() {
        let files = [
            ("specs/README.md", "Files: `nca.md`.\n"),
            ("specs/nca.md", "x\n"),
            ("specs/new.md", "x\n"),
        ];
        let findings = specs_index(&tree(&files));
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].message.contains("new.md"));
    }

    #[test]
    fn both_model_rows_of_the_coverage_table_are_read() {
        let readme = readme_with_rows((21, 886), (172, 16906));
        assert_eq!(
            readme_claims(&readme),
            Some(vec![
                Claim {
                    family: Family::One,
                    cases: 21,
                    values: 886,
                    line: 5,
                },
                Claim {
                    family: Family::Two,
                    cases: 172,
                    values: 16906,
                    line: 6,
                },
            ]),
            "fixture:\n{readme}"
        );
    }

    #[test]
    fn a_readme_coverage_table_is_checked_against_the_conformance_file() {
        let readme = readme_with_rows((1, 60), (2, 140));
        let files = vec![
            (CONFORMANCE, CONFORMANCE_OK),
            ("README.md", readme.as_str()),
        ];
        let findings = claims_checked(&tree(&files));
        assert!(
            findings.is_empty(),
            "an agreement is not a finding, or --strict could never pass: {findings:?}"
        );

        let wrong = readme.replace("1 case, 60 values", "1 case, 61 values");
        let files = vec![(CONFORMANCE, CONFORMANCE_OK), ("README.md", wrong.as_str())];
        let findings = claims_checked(&tree(&files));
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].kind, Kind::Fail);
        assert_eq!(findings[0].line, 5, "the line of the one-compartment row");
    }

    #[test]
    fn a_readme_without_the_reference_is_reported_as_unchecked() {
        let findings = claims_checked(&tree(&[
            ("README.md", "# Caladrius\n"),
            (CONFORMANCE, CONFORMANCE_OK),
        ]));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, Kind::Note);
    }

    #[test]
    fn the_spec_status_table_reports_the_counts_the_readme_states() {
        let readme = "\
| Specification | confirmed by oracle | documented, untested | assumed | observed |
|---|---|---|---|---|
| `specs/nca.md` | 1 | 0 | 1 | 0 |
";
        let nca = "- Status: `confirmed by oracle`\n- Status: `assumed`\n";
        let findings = specs_counts(&tree(&[("README.md", readme), ("specs/nca.md", nca)]));
        assert!(findings.is_empty(), "{findings:?}");

        let nca = "- Status: `confirmed by oracle`\n- Status: `confirmed by oracle`\n";
        let findings = specs_counts(&tree(&[("README.md", readme), ("specs/nca.md", nca)]));
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].message.contains("nca.md"));
    }

    #[test]
    fn a_missing_conformance_file_fails_the_total() {
        let findings = conformance_total(&tree(&[]));
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].kind, Kind::Fail);
        assert!(findings[0].message.contains("missing"));
    }

    #[test]
    fn a_conformance_file_without_an_overall_line_fails() {
        let text =
            CONFORMANCE_OK.replace("**Overall: 300 of 300 values validated (100.0 %).**", "");
        let findings = conformance_total(&tree(&[(CONFORMANCE, text.as_str())]));
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].kind, Kind::Fail);
        assert!(findings[0].message.contains("Overall"));
    }

    #[test]
    fn an_overall_line_whose_two_halves_differ_fails_at_its_line() {
        let text = CONFORMANCE_OK.replace("**Overall: 300 of 300", "**Overall: 300 of 299");
        let findings = conformance_total(&tree(&[(CONFORMANCE, text.as_str())]));
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].message.contains("two halves differ"));
        assert_eq!(
            findings[0].line, 3,
            "the overall line is the third of the fixture"
        );
    }

    #[test]
    fn the_floor_findings_name_the_line_of_the_claim() {
        let missing = CONFORMANCE_OK.replace("<!-- floor model_b conc 2 2 0 -->", "");
        let findings = conformance_floors(&tree(&[(CONFORMANCE, missing.as_str())]));
        let heading = missing
            .lines()
            .position(|line| line == "### model_b")
            .map(|index| index + 1);
        assert_eq!(Some(findings[0].line), heading, "{findings:?}");
    }

    #[test]
    fn an_empty_registry_fails_the_model_ids_once() {
        let findings = model_ids(&tree(&[
            (CONFORMANCE, CONFORMANCE_OK),
            (REGISTRY, "pub const MODELS: &[Model] = &[];\n"),
        ]));
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].kind, Kind::Fail);
        assert!(findings[0].message.contains("no `pk1.`"));
    }

    #[test]
    fn a_missing_registry_fails_the_model_ids() {
        let findings = model_ids(&tree(&[(CONFORMANCE, CONFORMANCE_OK)]));
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].message.contains("registry file is missing"));
    }

    #[test]
    fn a_pk2_model_without_a_pk2_case_fails_at_its_registry_line() {
        let conformance = CONFORMANCE_OK
            .replace(
                "| model_b | pk2.iv_bolus | 2 / 2 | 100 / 100 | 100.0 | 0 |\n",
                "",
            )
            .replace(
                "| model_c | pk2.oral_1 | 2 / 2 | 40 / 40 | 100.0 | 0 |\n",
                "",
            );
        let findings = model_ids(&tree(&[
            (CONFORMANCE, conformance.as_str()),
            (REGISTRY, REGISTRY_OK),
        ]));
        assert_eq!(findings.len(), 2, "{findings:?}");
        assert!(findings.iter().all(|f| f.file == REGISTRY && f.line > 0));
        assert!(findings.iter().all(|f| f.message.contains("pk2.")));
    }

    #[test]
    fn a_readme_claiming_pk2_when_the_conformance_file_has_no_pk2_case_fails() {
        let conformance = CONFORMANCE_OK
            .replace(
                "| model_b | pk2.iv_bolus | 2 / 2 | 100 / 100 | 100.0 | 0 |\n",
                "",
            )
            .replace(
                "| model_c | pk2.oral_1 | 2 / 2 | 40 / 40 | 100.0 | 0 |\n",
                "",
            );
        let readme = readme_with_rows((1, 60), (2, 140));
        let findings = claims_checked(&tree(&[
            (CONFORMANCE, conformance.as_str()),
            ("README.md", readme.as_str()),
        ]));
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].kind, Kind::Fail);
        assert!(findings[0].message.contains("no pk2 case"));
        assert_eq!(findings[0].line, 6, "the line of the two-compartment row");
    }

    #[test]
    fn a_readme_naming_the_file_but_stating_no_counts_is_a_note() {
        let readme = "See `docs/conformance.md`.\n\n| What | Checked |\n|---|---|\n";
        let findings = claims_checked(&tree(&[
            (CONFORMANCE, CONFORMANCE_OK),
            ("README.md", readme),
        ]));
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].kind, Kind::Note);
        assert!(findings[0].message.contains("no model case/value counts"));
    }

    #[test]
    fn claims_are_not_checked_without_a_conformance_file() {
        let readme = readme_with_rows((1, 60), (2, 140));
        assert!(claims_checked(&tree(&[("README.md", readme.as_str())])).is_empty());
    }

    #[test]
    fn a_readme_with_no_two_compartment_row_fails_when_the_registry_has_pk2() {
        let readme = "See `docs/conformance.md`.\n\n\
                      | Model curves: six one-compartment models | closed forms | 1e-12 | 1 case, 60 values | none |\n";
        let files = [("README.md", readme), (REGISTRY, REGISTRY_OK)];
        let findings = two_compartment_readme(&tree(&files));
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].kind, Kind::Fail);
        assert_eq!(findings[0].rule, "review-pk2-readme");
        assert!(findings[0].message.contains("no two-compartment row"));
    }

    #[test]
    fn a_readme_that_still_says_two_compartments_are_not_implemented_fails() {
        let mut readme = readme_with_rows((1, 60), (2, 140));
        readme.push_str(
            "| Other | x | y | z | user-written models, two-compartment models (not implemented) |\n",
        );
        let files = [("README.md", readme.as_str()), (REGISTRY, REGISTRY_OK)];
        let findings = two_compartment_readme(&tree(&files));
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].message.contains("not implemented"));
        assert_eq!(findings[0].line, 7, "the line of the stale clause");
    }

    #[test]
    fn a_readme_with_the_two_compartment_row_and_no_stale_clause_passes() {
        // The not-implemented clause of user-written models sits next to a two-compartment fit
        // in the same cell: separate clauses, so no finding.
        let mut readme = readme_with_rows((1, 60), (2, 140));
        readme.push_str(
            "| Other | x | y | z | user-written models (not implemented); the two-compartment fit (no reference) |\n",
        );
        let files = [("README.md", readme.as_str()), (REGISTRY, REGISTRY_OK)];
        let findings = two_compartment_readme(&tree(&files));
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn the_pk2_readme_rule_is_silent_while_the_registry_has_no_pk2_model() {
        let registry = "Model { id: \"pk1.iv_bolus\" },\n";
        let readme = "See `docs/conformance.md`. Two-compartment models are not implemented.\n";
        let files = [("README.md", readme), (REGISTRY, registry)];
        assert!(two_compartment_readme(&tree(&files)).is_empty());
        assert!(two_compartment_readme(&tree(&[("README.md", readme)])).is_empty());
    }

    #[test]
    fn an_unreadable_version_fails_instead_of_passing() {
        let cargo = "[workspace.package]\nversion = \"0.1.0\"\n";
        let findings = versions(&tree(&[
            ("Cargo.toml", cargo),
            ("CITATION.cff", "title: x\n"),
        ]));
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].kind, Kind::Fail);
        assert!(findings[0].message.contains("unreadable"));

        let findings = versions(&tree(&[
            ("Cargo.toml", "[workspace]\nmembers = []\n"),
            ("CITATION.cff", "version: 0.1.0\n"),
        ]));
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].file, "Cargo.toml");
    }

    #[test]
    fn a_nested_or_foreign_version_is_not_the_version() {
        // An indented `version:` belongs to a nested entry; a `version` outside the package
        // section (a dependency) is not the manifest version.
        let cite = "references:\n  - version: 9.9.9\nversion: 0.1.0\n";
        assert_eq!(
            citation_version(cite),
            Declared::Version("0.1.0".to_owned(), 3)
        );
        let cargo = "[dependencies]\nx = { version = \"1\" }\nversion = \"9.9.9\"\n\
                     [workspace.package]\nversion = \"0.1.0\" # tagged\n";
        assert_eq!(
            manifest_version(cargo),
            Declared::Version("0.1.0".to_owned(), 5)
        );
    }

    #[test]
    fn the_specs_counts_note_names_the_line_of_the_readme_row() {
        let readme = "intro\n\n\
                      | `specs/nca.md` | 5 | 0 | 0 | 0 |\n";
        let nca = "- Status: `confirmed by oracle`\n";
        let findings = specs_counts(&tree(&[("README.md", readme), ("specs/nca.md", nca)]));
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].line, 3);
    }

    #[test]
    fn a_truncated_table_row_is_skipped_without_a_panic() {
        let readme = "See `docs/conformance.md`.\n|||||\n| `specs/nca.md` | 1 |\n| a | b | c |\n";
        assert_eq!(readme_claims(readme), Some(Vec::new()));
        assert!(specs_counts(&tree(&[("README.md", readme)])).is_empty());
        assert!(
            coverage("## Models against exact closed-form values\n|||||\n| a | pk1.x |\n")
                .is_empty()
        );
    }

    #[test]
    fn the_drift_note_names_the_tag_and_the_files_it_counts() {
        let changed = "README.md\ndocs/conformance.md\ncrates/x/src/lib.rs\noracle/scripts/a.R\n\
                       oracle/data/big.csv\noracle/README.md\nREADME.mdx\n";
        let findings = drift_findings(Some("v0.1.0"), Some(changed));
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].kind, Kind::Note);
        let message = &findings[0].message;
        assert!(message.starts_with("4 file(s) under docs/, specs/, oracle/scripts/, oracle/README.md or README.md changed since v0.1.0"), "{message}");
        assert!(
            message.contains("README.md, docs/conformance.md, oracle/README.md"),
            "{message}"
        );
    }

    #[test]
    fn the_drift_note_says_when_nothing_changed() {
        let findings = drift_findings(Some("v0.1.0"), Some("crates/x/src/lib.rs\n"));
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].message.starts_with("nothing under "));
        assert!(findings[0].message.contains("since v0.1.0"));
    }

    #[test]
    fn the_drift_note_is_a_note_when_git_has_no_tag_or_no_diff() {
        for (tag, changed, expect) in [
            (None, None, "no tag found"),
            (Some("v0.1.0"), None, "failed"),
        ] {
            let findings = drift_findings(tag, changed);
            assert_eq!(findings.len(), 1, "{findings:?}");
            assert_eq!(findings[0].kind, Kind::Note);
            assert!(findings[0].message.contains(expect), "{findings:?}");
        }
    }

    #[test]
    fn a_consistent_tree_has_no_finding_but_the_drift_note() {
        let readme = readme_with_rows((1, 60), (2, 140));
        let files = [
            (CONFORMANCE, CONFORMANCE_OK),
            (REGISTRY, REGISTRY_OK),
            ("README.md", readme.as_str()),
            ("Cargo.toml", "[workspace.package]\nversion = \"0.1.0\"\n"),
            ("CITATION.cff", "version: 0.1.0\n"),
        ];
        let tree = tree(&files);
        let mut findings = Vec::new();
        findings.extend(conformance_total(&tree));
        findings.extend(conformance_floors(&tree));
        findings.extend(model_ids(&tree));
        findings.extend(versions(&tree));
        findings.extend(specs_index(&tree));
        findings.extend(claims_checked(&tree));
        findings.extend(two_compartment_readme(&tree));
        findings.extend(specs_counts(&tree));
        assert!(
            findings.is_empty(),
            "--strict must be able to pass: {findings:?}"
        );
    }

    #[test]
    fn the_number_helpers_read_the_first_and_second_number() {
        assert_eq!(first_number("21 cases, 886 values"), Some(21));
        assert_eq!(second_number("21 cases, 886 values"), Some(886));
        assert_eq!(second_number("no numbers"), None);
    }
}
