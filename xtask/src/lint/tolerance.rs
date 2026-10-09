//! Rule `tolerance-literal`: tolerances are defined once, in `caladrius-testkit` (`AGENTS.md` section 5).
//!
//! Heuristic, on the integration tests `crates/<crate>/tests/**/*.rs` of every crate but the test kit:
//! a line that holds a negative-exponent literal (`1e-6`, `2.5e-12`) and sits in an assertion (the
//! line contains `assert`, or compares an `.abs()` with `<`, `<=`, `>` or `>=`) is a finding.
//! Comments and string literals are ignored. The tests that need their own bound (property tests,
//! derived comparisons) are listed in `xtask/lint_allow.toml` with the reason.

use super::{Finding, Tree};

pub fn check(tree: &Tree) -> Vec<Finding> {
    let mut findings = Vec::new();
    for (path, text) in &tree.files {
        if !is_numerical_test(path) {
            continue;
        }
        for (index, line) in text.lines().enumerate() {
            let code = strip_comment_and_strings(line);
            if has_small_literal(&code) && is_assertion(&code) {
                findings.push(Finding::new(
                    path,
                    index + 1,
                    "tolerance-literal",
                    "numeric tolerance in an assertion: use a constant of caladrius-testkit (Tolerance) or list the test in lint_allow.toml",
                    line,
                ));
            }
        }
    }
    findings
}

/// `crates/<name>/tests/.../*.rs`, except the test kit's own.
fn is_numerical_test(path: &str) -> bool {
    let mut parts = path.split('/');
    let (Some("crates"), Some(name), Some("tests")) = (parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    name != "caladrius-testkit" && path.ends_with(".rs")
}

fn is_assertion(code: &str) -> bool {
    if code.contains("assert") {
        return true;
    }
    code.find(".abs()").is_some_and(|at| {
        let after = code.get(at..).unwrap_or_default();
        after.contains('<') || after.contains('>')
    })
}

/// The line without its `//` comment and with the content of string literals removed.
fn strip_comment_and_strings(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut in_string = false;
    let mut escaped = false;
    let mut previous = '\0';
    for c in line.chars() {
        if in_string {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            previous = c;
            continue;
        }
        if c == '"' {
            in_string = true;
        } else if c == '/' && previous == '/' {
            out.pop();
            break;
        } else {
            out.push(c);
        }
        previous = c;
    }
    out
}

/// True if the code holds a literal such as `1e-6` or `2.5E-12`.
fn has_small_literal(code: &str) -> bool {
    let chars: Vec<char> = code.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let starts_number = chars.get(i).is_some_and(char::is_ascii_digit)
            && (i == 0
                || chars
                    .get(i - 1)
                    .is_some_and(|p| !p.is_alphanumeric() && *p != '_'));
        if !starts_number {
            i += 1;
            continue;
        }
        let mut j = i;
        while chars
            .get(j)
            .is_some_and(|c| c.is_ascii_digit() || *c == '_')
        {
            j += 1;
        }
        if chars.get(j) == Some(&'.') && chars.get(j + 1).is_some_and(char::is_ascii_digit) {
            j += 1;
            while chars
                .get(j)
                .is_some_and(|c| c.is_ascii_digit() || *c == '_')
            {
                j += 1;
            }
        }
        let exponent = matches!(chars.get(j), Some('e' | 'E'))
            && chars.get(j + 1) == Some(&'-')
            && chars.get(j + 2).is_some_and(char::is_ascii_digit);
        if exponent {
            return true;
        }
        i = j.max(i + 1);
    }
    false
}

#[cfg(test)]
mod tests {
    use super::super::fixture;
    use super::*;

    fn lines_found(path: &str, text: &str) -> Vec<usize> {
        check(&fixture(&[(path, text)], &[]))
            .iter()
            .map(|f| f.line)
            .collect()
    }

    #[test]
    fn a_literal_tolerance_in_an_assertion_is_a_finding() {
        let text = "fn t() {\n    assert!((a - b).abs() < 1e-6);\n    assert_eq!(x, 1);\n    let ok = (a - b).abs() <= 2.5e-12 * b;\n    assert!(c < 1E-9, \"msg\");\n}\n";
        assert_eq!(
            lines_found("crates/caladrius-nca/tests/a.rs", text),
            [2, 4, 5]
        );
    }

    #[test]
    fn the_test_kit_and_other_directories_are_exempt() {
        let text = "assert!(a < 1e-6);\n";
        assert!(lines_found("crates/caladrius-testkit/tests/a.rs", text).is_empty());
        assert!(lines_found("crates/caladrius-testkit/src/lib.rs", text).is_empty());
        assert!(lines_found("crates/caladrius-nca/src/lib.rs", text).is_empty());
        assert!(lines_found("apps/x/tests/a.rs", text).is_empty());
        assert_eq!(
            lines_found("crates/caladrius-fit/tests/common/mod.rs", text),
            [1]
        );
    }

    #[test]
    fn literals_outside_assertions_comments_and_strings_are_fine() {
        let text = "let h = 1e-6 * value;\ninput.options.convergence = 1e-10;\n// assert!(a < 1e-6)\nassert!(a, \"within 1e-6 relative\");\nlet table = [(1e-3, 1e-2)];\nassert!(a < 1e6);\nassert!(a < x1e-6);\n";
        assert!(lines_found("crates/caladrius-fit/tests/a.rs", text).is_empty());
    }

    #[test]
    fn a_comparison_without_abs_or_assert_is_not_an_assertion() {
        let text = "let small = value < 1e-6;\n";
        assert!(lines_found("crates/caladrius-nca/tests/a.rs", text).is_empty());
    }

    #[test]
    fn an_abs_comparison_outside_assert_macros_counts() {
        let text = "fn close(a: f64, b: f64) -> bool {\n    (a - b).abs() <= 1e-12 * b.abs()\n}\n";
        assert_eq!(
            lines_found("crates/caladrius-nca/tests/units.rs", text),
            [2]
        );
    }

    #[test]
    fn the_literal_scanner_reads_forms_of_scientific_notation() {
        assert!(has_small_literal("x < 1e-6"));
        assert!(has_small_literal("x < 1.5e-3_f64"));
        assert!(has_small_literal("[-1e-4]"));
        assert!(!has_small_literal("x < 1e6"));
        assert!(!has_small_literal("x < 1e+6"));
        assert!(!has_small_literal("let e = 3; x - e"));
        assert!(!has_small_literal("var1e-6"));
    }
}
