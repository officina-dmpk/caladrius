//! Board hygiene (`AGENTS.md` section 9).
//!
//! - `board-state`: every `board/tasks/T-*.md` has a `- **State:** <state>` line in its header and the
//!   state is one of the allowed ones (the five of the contract, plus `split` for an umbrella card).
//! - `board-index`: every card has a row in `board/INDEX.md` with the same state, and every row has a card.
//! - `board-log`: every non-empty line of `board/LOG.md` that is not a heading starts with `- YYYY-MM-DD `.

use std::collections::BTreeMap;

use super::{Finding, Tree};

/// The five states of `AGENTS.md` section 9, plus `split` (an umbrella card whose work moved to its sub-cards).
const STATES: [&str; 6] = [
    "todo",
    "in progress",
    "to review",
    "done",
    "blocked",
    "split",
];

const INDEX: &str = "board/INDEX.md";
const LOG: &str = "board/LOG.md";

pub fn check(tree: &Tree) -> Vec<Finding> {
    let mut findings = Vec::new();
    let index = parse_index(
        tree.files
            .get(INDEX)
            .map(String::as_str)
            .unwrap_or_default(),
    );
    let mut cards: BTreeMap<String, (usize, String)> = BTreeMap::new();
    for (path, text) in &tree.files {
        let Some(id) = card_id(path) else { continue };
        match card_state(text) {
            None => findings.push(Finding::new(
                path,
                1,
                "board-state",
                "no `- **State:** <state>` line in the card header",
                "",
            )),
            Some((line, state)) if !STATES.contains(&state.as_str()) => {
                findings.push(Finding::new(
                    path,
                    line,
                    "board-state",
                    format!(
                        "`{state}` is not a valid state (one of: {})",
                        STATES.join(", ")
                    ),
                    &state,
                ));
            }
            Some((line, state)) => {
                cards.insert(id.to_owned(), (line, state));
            }
        }
    }
    for (id, (line, state)) in &cards {
        let path = format!("board/tasks/{id}.md");
        match index.get(id) {
            None => findings.push(Finding::new(
                &path,
                *line,
                "board-index",
                format!("{id} is not listed in {INDEX}"),
                "",
            )),
            Some((_, indexed, _)) if indexed != state => findings.push(Finding::new(
                &path,
                *line,
                "board-index",
                format!("state `{state}` differs from `{indexed}` in {INDEX}"),
                "",
            )),
            Some(_) => {}
        }
    }
    for (id, (line, _, text)) in &index {
        let has_card = tree.files.contains_key(&format!("board/tasks/{id}.md"));
        if !has_card {
            findings.push(Finding::new(
                INDEX,
                *line,
                "board-index",
                format!("{id} has no card in board/tasks/"),
                text,
            ));
        }
    }
    findings.extend(check_log(
        tree.files.get(LOG).map(String::as_str).unwrap_or_default(),
    ));
    findings
}

/// `board/tasks/T-042.md` gives `T-042`.
fn card_id(path: &str) -> Option<&str> {
    let name = path.strip_prefix("board/tasks/")?.strip_suffix(".md")?;
    let rest = name.strip_prefix("T-")?;
    let digits = rest.trim_end_matches(|c: char| c.is_ascii_lowercase());
    (!digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())).then_some(name)
}

/// The first `- **State:** ...` line of the card (1-based line, state without trailing dot).
fn card_state(text: &str) -> Option<(usize, String)> {
    text.lines().enumerate().find_map(|(index, line)| {
        let value = line.trim_start().strip_prefix("- **State:**")?;
        Some((index + 1, value.trim().trim_end_matches('.').to_owned()))
    })
}

/// A row of the index table: line number, state, text of the line.
type IndexRow = (usize, String, String);

/// The rows of the index table, by id.
fn parse_index(text: &str) -> BTreeMap<String, IndexRow> {
    let mut rows = BTreeMap::new();
    for (index, line) in text.lines().enumerate() {
        let Some(row) = line.trim().strip_prefix('|') else {
            continue;
        };
        let cells: Vec<&str> = row.split('|').map(str::trim).collect();
        let (Some(id), Some(state)) = (cells.first(), cells.get(3)) else {
            continue;
        };
        if card_id(&format!("board/tasks/{id}.md")).is_some() {
            rows.insert(
                (*id).to_owned(),
                (index + 1, (*state).to_owned(), line.to_owned()),
            );
        }
    }
    rows
}

fn check_log(text: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let starts_with_date = trimmed.strip_prefix("- ").is_some_and(starts_with_date);
        if !starts_with_date {
            findings.push(Finding::new(
                LOG,
                index + 1,
                "board-log",
                "a log entry must start with `- YYYY-MM-DD `",
                line,
            ));
        }
    }
    findings
}

/// `YYYY-MM-DD` followed by a space.
fn starts_with_date(text: &str) -> bool {
    let bytes = text.as_bytes();
    let digits = |from: usize, to: usize| -> Option<u32> {
        let part = bytes.get(from..to)?;
        if !part.iter().all(u8::is_ascii_digit) {
            return None;
        }
        std::str::from_utf8(part).ok()?.parse().ok()
    };
    let (Some(year), Some(month), Some(day)) = (digits(0, 4), digits(5, 7), digits(8, 10)) else {
        return false;
    };
    year >= 2000
        && (1..=12).contains(&month)
        && (1..=31).contains(&day)
        && bytes.get(4) == Some(&b'-')
        && bytes.get(7) == Some(&b'-')
        && bytes.get(10) == Some(&b' ')
}

#[cfg(test)]
mod tests {
    use super::super::fixture;
    use super::*;

    const INDEX_OK: &str = "# Board index\n\n| id | title | role | state | depends on |\n|---|---|---|---|---|\n| T-001 | a | interface | done | |\n| T-002a | b | engine | in progress | T-001 |\n";
    const CARD_DONE: &str = "# T-001: a\n\n- **Role:** interface\n- **State:** done\n\n## Thread\n\n- 2026-10-09 x: State: todo.\n";
    const CARD_PROGRESS: &str = "# T-002a\n\n- **State:** in progress\n";
    const LOG_OK: &str = "# Log\n\n- 2026-10-08 T-001 (interface): done.\n";

    fn tree(files: &[(&str, &str)]) -> Tree {
        fixture(files, &[])
    }

    fn good() -> Vec<(&'static str, &'static str)> {
        vec![
            (INDEX, INDEX_OK),
            (LOG, LOG_OK),
            ("board/tasks/T-001.md", CARD_DONE),
            ("board/tasks/T-002a.md", CARD_PROGRESS),
        ]
    }

    fn rules(findings: &[Finding]) -> Vec<(&str, &'static str)> {
        findings.iter().map(|f| (f.file.as_str(), f.rule)).collect()
    }

    #[test]
    fn a_consistent_board_gives_no_finding() {
        assert!(check(&tree(&good())).is_empty());
    }

    #[test]
    fn a_card_without_a_state_or_with_an_unknown_state_is_a_finding() {
        let mut files = good();
        files[2] = ("board/tasks/T-001.md", "# T-001\n\nno state here\n");
        files[3] = ("board/tasks/T-002a.md", "# x\n- **State:** doing\n");
        let findings = check(&tree(&files));
        assert_eq!(
            rules(&findings),
            [
                ("board/tasks/T-001.md", "board-state"),
                ("board/tasks/T-002a.md", "board-state")
            ]
        );
        assert_eq!(findings[1].line, 2);
    }

    #[test]
    fn the_state_in_a_thread_note_is_not_the_card_state() {
        // CARD_DONE has a "State: todo." note below the header; the header says done.
        assert!(check(&tree(&good())).is_empty());
    }

    #[test]
    fn the_five_states_and_split_are_valid() {
        for state in [
            "todo",
            "in progress",
            "to review",
            "done",
            "blocked",
            "split",
        ] {
            let card = format!("- **State:** {state}\n");
            let index = format!("| T-001 | a | r | {state} | |\n");
            let files = [
                (INDEX, index.as_str()),
                (LOG, LOG_OK),
                ("board/tasks/T-001.md", card.as_str()),
            ];
            assert!(check(&tree(&files)).is_empty(), "{state}");
        }
    }

    #[test]
    fn a_state_that_differs_from_the_index_is_a_finding() {
        let mut files = good();
        files[0] = (
            INDEX,
            "| T-001 | a | interface | todo | |\n| T-002a | b | e | in progress | |\n",
        );
        let findings = check(&tree(&files));
        assert_eq!(rules(&findings), [("board/tasks/T-001.md", "board-index")]);
        assert_eq!(findings[0].line, 4);
        assert!(findings[0].message.contains("`todo`"));
    }

    #[test]
    fn a_card_missing_from_the_index_and_a_row_without_card_are_findings() {
        let mut files = good();
        files[0] = (
            INDEX,
            "| T-001 | a | interface | done | |\n| T-009 | z | e | done | |\n",
        );
        let findings = check(&tree(&files));
        assert_eq!(
            rules(&findings),
            [
                ("board/tasks/T-002a.md", "board-index"),
                (INDEX, "board-index")
            ]
        );
        assert_eq!(findings[1].line, 2);
    }

    #[test]
    fn log_lines_must_start_with_a_date() {
        let log = "# Log\n\n- 2026-10-08 T-001 ok\n- T-002 no date\nstray text\n- 2026-13-01 bad month\n- 2026-10-0 short\n";
        let mut files = good();
        files[1] = (LOG, log);
        let findings = check(&tree(&files));
        let lines: Vec<usize> = findings.iter().map(|f| f.line).collect();
        assert_eq!(lines, [4, 5, 6, 7]);
        assert!(findings.iter().all(|f| f.rule == "board-log"));
    }

    #[test]
    fn other_files_of_the_board_directory_are_not_cards() {
        let mut files = good();
        files.push(("board/tasks/notes.md", "no state\n"));
        files.push(("board/messages/2026-10-09-a-b-c.md", "no state\n"));
        assert!(check(&tree(&files)).is_empty());
    }
}
