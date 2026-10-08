#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code: helpers panic on purpose

//! The documented-difference marker (task T-014): every identifier used in an oracle options file
//! has an entry in `specs/differences.md`, and a marked value is skipped by the comparison while a
//! value next to it is not.

use caladrius_testkit::oracle::oracle_dir;
use caladrius_testkit::{Table, Tolerance, compare_tables_documented, list_cases, load_case};

fn differences_text() -> String {
    let path = oracle_dir().join("..").join("specs").join("differences.md");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

#[test]
fn every_identifier_used_by_a_case_is_written_up() {
    let text = differences_text();
    let mut used = 0;
    for name in list_cases().unwrap() {
        let case = load_case(&name).unwrap();
        for d in &case.options.documented_differences {
            used += 1;
            let heading = format!("\n## {} ", d.id);
            assert!(
                text.contains(&heading),
                "{name}: {} has no entry (a `## {} ...` heading) in specs/differences.md",
                d.id,
                d.id
            );
        }
    }
    assert!(used >= 1, "no case uses a documented difference any more");
}

#[test]
fn the_negative_case_documents_d01_for_subject_2_only() {
    let case = load_case("edge_negative_linear").unwrap();
    let d = &case.options.documented_differences;
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].id, "D-01");
    assert_eq!(d[0].subjects.as_deref(), Some(&["2".to_string()][..]));
    assert_eq!(d[0].parameters.len(), 16);
    assert!(d[0].covers("2", "tlast") && !d[0].covers("1", "tlast"));
    assert!(!d[0].covers("2", "cmax"));
}

/// A table that equals the expected one except for a marked value is accepted and reports it; the
/// same change on an unmarked value is a failure.
#[test]
fn a_marked_value_is_skipped_and_an_unmarked_one_is_not() {
    let case = load_case("edge_negative_linear").unwrap();
    let change = |subject: &str, name: &str| {
        let mut actual = Table::new();
        for (s, n, v) in case.expected.iter() {
            let v = if s == subject && n == name {
                v.map(|x| x + 1.0)
            } else {
                v
            };
            actual.insert(s, n, v);
        }
        compare_tables_documented(
            &case.expected,
            &actual,
            Tolerance::NCA_VS_PKNCA,
            &case.options.documented_differences,
        )
    };
    let marked = change("2", "tlast");
    assert!(marked.is_ok(), "{marked}");
    assert_eq!(marked.documented.len(), 16);
    assert_eq!(marked.compared, case.expected.len() - 16);
    let unmarked = change("1", "tlast");
    assert!(!unmarked.is_ok(), "{unmarked}");
    assert_eq!(unmarked.mismatches.len(), 1);
    let other = change("2", "cmax");
    assert!(!other.is_ok(), "{other}");
}
