//! Rule `forbidden-name`: the names the project never uses (`AGENTS.md`, first paragraph).

use super::{Finding, Tree};

/// Matched case-insensitively. The few legitimate mentions are in `xtask/lint_allow.toml`.
const NAMES: [&str; 3] = ["Phoenix", "WinNonlin", "Certara"];

pub fn check(tree: &Tree) -> Vec<Finding> {
    let mut findings = Vec::new();
    for (path, text) in &tree.files {
        for (index, line) in text.lines().enumerate() {
            let lower = line.to_ascii_lowercase();
            for name in NAMES {
                if lower.contains(&name.to_ascii_lowercase()) {
                    findings.push(Finding::new(
                        path,
                        index + 1,
                        "forbidden-name",
                        format!("`{name}` must not appear in versioned files"),
                        line,
                    ));
                }
            }
        }
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::super::fixture;
    use super::*;

    #[test]
    fn each_forbidden_name_is_found_whatever_its_case() {
        let tree = fixture(
            &[(
                "docs/a.md",
                "fine\nsee PHOENIX here\nwinnonlin\nCertara and Phoenix\n",
            )],
            &[],
        );
        let findings = check(&tree);
        let lines: Vec<usize> = findings.iter().map(|f| f.line).collect();
        assert_eq!(lines, [2, 3, 4, 4]);
        assert!(findings.iter().all(|f| f.rule == "forbidden-name"));
        assert!(findings.iter().all(|f| f.file == "docs/a.md"));
    }

    #[test]
    fn clean_files_give_no_finding() {
        let tree = fixture(
            &[("a.md", "Caladrius\nPKNCA\n"), ("b.rs", "fn main() {}\n")],
            &[],
        );
        assert!(check(&tree).is_empty());
    }

    #[test]
    fn an_allow_list_entry_accepts_the_notice_line_only() {
        let tree = fixture(
            &[("README.md", "trademarks of Certara.\nuse Certara code\n")],
            &[],
        );
        let allow = "[[allow]]\nrule = \"forbidden-name\"\nfile = \"README.md\"\ncontains = \"trademarks of\"\nreason = \"notice\"\n";
        let findings = super::super::check(&tree, allow).unwrap_or_default();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].line, 2);
    }
}
