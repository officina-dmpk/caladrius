//! Rule `deny-header`: every crate root starts with the deny lints of golden rule 6.
//!
//! The crate roots are `src/lib.rs` and `src/main.rs` of each workspace member. The attribute must
//! list at least the five lints below (more is allowed, fewer is not).

use super::{Finding, Tree};

const REQUIRED: [&str; 5] = [
    "clippy::unwrap_used",
    "clippy::expect_used",
    "clippy::panic",
    "clippy::todo",
    "clippy::unreachable",
];

pub fn check(tree: &Tree) -> Vec<Finding> {
    let mut findings = Vec::new();
    for dir in &tree.members {
        for root in ["src/lib.rs", "src/main.rs"] {
            let path = format!("{dir}/{root}");
            if let Some(text) = tree.files.get(&path) {
                findings.extend(check_root(&path, text));
            }
        }
    }
    findings
}

fn check_root(path: &str, text: &str) -> Option<Finding> {
    let mut best: Option<(usize, Vec<&str>)> = None;
    for (index, line) in text.lines().enumerate() {
        if !line.trim_start().starts_with("#![deny(") {
            continue;
        }
        let missing = missing_lints(text, index);
        let better = best
            .as_ref()
            .is_none_or(|(_, previous)| missing.len() < previous.len());
        if better {
            best = Some((index + 1, missing));
        }
    }
    match best {
        None => Some(Finding::new(
            path,
            1,
            "deny-header",
            "the crate root has no `#![deny(clippy::unwrap_used, ...)]` header (AGENTS.md golden rule 6)",
            "",
        )),
        Some((_, missing)) if missing.is_empty() => None,
        Some((line, missing)) => Some(Finding::new(
            path,
            line,
            "deny-header",
            format!("the deny header does not list {}", missing.join(", ")),
            "",
        )),
    }
}

/// The required lints that the attribute starting at `first_line` (0-based) does not name.
fn missing_lints(text: &str, first_line: usize) -> Vec<&'static str> {
    let mut attribute = String::new();
    for line in text.lines().skip(first_line) {
        // Comments inside the attribute do not count.
        let code = line.split("//").next().unwrap_or_default();
        attribute.push_str(code);
        attribute.push(' ');
        if code.contains(")]") {
            break;
        }
    }
    let listed: Vec<&str> = attribute
        .split(|c: char| c == ',' || c == '(' || c == ')' || c.is_whitespace())
        .collect();
    REQUIRED
        .iter()
        .copied()
        .filter(|lint| !listed.contains(lint))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::fixture;
    use super::*;

    const GOOD: &str = "#![deny(\n    clippy::unwrap_used,\n    clippy::expect_used,\n    clippy::panic,\n    clippy::todo,\n    clippy::unreachable\n)]\n//! docs\n";

    #[test]
    fn a_complete_header_is_accepted_in_lib_and_main() {
        let tree = fixture(
            &[("crates/a/src/lib.rs", GOOD), ("apps/b/src/main.rs", GOOD)],
            &["crates/a", "apps/b"],
        );
        assert!(check(&tree).is_empty());
    }

    #[test]
    fn a_single_line_header_and_extra_lints_are_accepted() {
        let one_line = "#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::todo, clippy::unreachable, unsafe_code)]\n";
        let tree = fixture(&[("crates/a/src/lib.rs", one_line)], &["crates/a"]);
        assert!(check(&tree).is_empty());
    }

    #[test]
    fn a_missing_header_is_reported_on_line_one() {
        let tree = fixture(
            &[("crates/a/src/lib.rs", "//! no header\nfn f() {}\n")],
            &["crates/a"],
        );
        let findings = check(&tree);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].line, 1);
        assert_eq!(findings[0].rule, "deny-header");
    }

    #[test]
    fn an_incomplete_header_names_the_missing_lints() {
        let partial = "//! docs\n#![deny(clippy::unwrap_used, clippy::panic)]\n";
        let tree = fixture(&[("apps/b/src/main.rs", partial)], &["apps/b"]);
        let findings = check(&tree);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].line, 2);
        assert!(findings[0].message.contains("clippy::expect_used"));
        assert!(findings[0].message.contains("clippy::todo"));
        assert!(findings[0].message.contains("clippy::unreachable"));
        assert!(!findings[0].message.contains("clippy::panic"));
    }

    #[test]
    fn a_commented_out_header_does_not_count() {
        let commented = "// #![deny(clippy::unwrap_used)]\n";
        let tree = fixture(&[("crates/a/src/lib.rs", commented)], &["crates/a"]);
        assert_eq!(check(&tree).len(), 1);
    }

    #[test]
    fn files_that_are_not_crate_roots_or_not_members_are_ignored() {
        let tree = fixture(
            &[
                ("crates/a/src/other.rs", "fn f() {}\n"),
                ("crates/z/src/lib.rs", "fn f() {}\n"),
            ],
            &["crates/a"],
        );
        assert!(check(&tree).is_empty());
    }
}
