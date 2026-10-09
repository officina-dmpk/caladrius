//! Rules about `private/` (`AGENTS.md` sections 3 and 5): nothing from it is versioned, and no file of it is named.
//!
//! - `private-tracked`: `git ls-files` lists a path under `private/` other than its `README.md`.
//! - `private-reference`: a versioned file names a data file (`.csv`, `.rtf`, `.xls`, `.xlsx`) under
//!   `private/`. Folders and patterns (`private/exports/<td>/`, `private/*.csv`) are fine; a file name
//!   from a private folder is not, because the name itself can say what the data are.

use super::{Finding, Tree};

const DATA_EXTENSIONS: [&str; 4] = [".csv", ".rtf", ".xls", ".xlsx"];

pub fn check(tree: &Tree) -> Vec<Finding> {
    let mut findings = Vec::new();
    for path in &tree.tracked {
        if path.starts_with("private/") && path != "private/README.md" {
            findings.push(Finding::new(
                path,
                1,
                "private-tracked",
                "a file under private/ is tracked by git (only private/README.md may be)",
                path,
            ));
        }
    }
    for (path, text) in &tree.files {
        if path.starts_with("private/") {
            continue;
        }
        for (index, line) in text.lines().enumerate() {
            for named in named_data_files(line) {
                findings.push(Finding::new(
                    path,
                    index + 1,
                    "private-reference",
                    format!("names the data file `{named}` of private/"),
                    line,
                ));
            }
        }
    }
    findings
}

/// The paths of the line that start with `private/` and end with a data file name.
fn named_data_files(line: &str) -> Vec<String> {
    let mut found = Vec::new();
    for (start, _) in line.match_indices("private/") {
        // `deprivate/` is not the folder.
        let before = line.get(..start).and_then(|head| head.chars().next_back());
        if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
            continue;
        }
        let rest = line.get(start..).unwrap_or_default();
        let end = rest.find(|c: char| !is_path_char(c)).unwrap_or(rest.len());
        let path = rest.get(..end).unwrap_or_default().trim_end_matches('.');
        if is_named_data_file(path) {
            found.push(path.to_owned());
        }
    }
    found
}

fn is_path_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '.' | '/' | '-' | '*' | '<' | '>' | '{' | '}')
}

/// A path whose last segment is `<stem>.<data extension>` with a real stem (not `*`, `<td>` or `{name}`).
fn is_named_data_file(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let Some(extension) = DATA_EXTENSIONS.iter().find(|ext| lower.ends_with(*ext)) else {
        return false;
    };
    let name = lower.rsplit('/').next().unwrap_or_default();
    let stem = name.strip_suffix(extension).unwrap_or(name);
    let mut real = false;
    let mut depth = 0_u32;
    for c in stem.chars() {
        match c {
            '<' | '{' => depth += 1,
            '>' | '}' => depth = depth.saturating_sub(1),
            c if depth == 0 && c.is_alphanumeric() => real = true,
            _ => {}
        }
    }
    real
}

#[cfg(test)]
mod tests {
    use super::super::fixture;
    use super::*;

    #[test]
    fn a_tracked_file_under_private_is_a_finding_but_its_readme_is_not() {
        let mut tree = fixture(&[], &[]);
        tree.tracked = vec![
            "private/README.md".to_owned(),
            "private/exports/x.txt".to_owned(),
            "docs/private/a.md".to_owned(),
        ];
        let findings = check(&tree);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].file, "private/exports/x.txt");
        assert_eq!(findings[0].rule, "private-tracked");
    }

    #[test]
    fn a_data_file_named_under_private_is_a_finding() {
        let tree = fixture(
            &[(
                "board/tasks/T-1.md",
                "ok\nreads private/coursework/td1_a.csv for the test\nexport private/exports/td1/Final.RTF.\nsheet private/x.xlsx\n",
            )],
            &[],
        );
        let findings = check(&tree);
        let lines: Vec<usize> = findings.iter().map(|f| f.line).collect();
        assert_eq!(lines, [2, 3, 4]);
        assert!(
            findings[1]
                .message
                .contains("private/exports/td1/Final.RTF")
        );
        assert!(findings.iter().all(|f| f.rule == "private-reference"));
    }

    #[test]
    fn folders_patterns_and_placeholders_are_not_names() {
        let tree = fixture(
            &[(
                "AGENTS.md",
                "private/exports/<td>/ and private/coursework/ and private/*.csv\nprivate/exports/{id}.csv and private/README.md\nthe deprivate/a.csv\nprivate/exports/td1/manifest.json\n",
            )],
            &[],
        );
        assert!(check(&tree).is_empty());
    }

    #[test]
    fn files_inside_private_are_not_scanned() {
        let tree = fixture(&[("private/README.md", "see private/a.csv\n")], &[]);
        assert!(check(&tree).is_empty());
    }
}
