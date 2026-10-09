//! The allow-list: `[[allow]]` entries of `xtask/lint_allow.toml`, each with a rule, a file and a reason.

use toml::{Table, Value};

use super::Finding;
use crate::error::{Result, XtaskError};

/// One accepted exception.
#[derive(Debug, PartialEq, Eq)]
pub struct Entry {
    pub rule: String,
    pub file: String,
    /// When present, only the lines containing this text are accepted; otherwise the whole file.
    pub contains: Option<String>,
    pub reason: String,
}

/// Reads the entries. A missing field or an empty reason is an error: an exception without a reason is not one.
pub fn parse(text: &str) -> Result<Vec<Entry>> {
    let table: Table = text
        .parse()
        .map_err(|source| XtaskError::new(format!("cannot parse the lint allow-list: {source}")))?;
    let Some(list) = table.get("allow") else {
        return Ok(Vec::new());
    };
    let list = list.as_array().ok_or_else(|| {
        XtaskError::new("the lint allow-list: `allow` must be an array of tables ([[allow]])")
    })?;
    let mut entries = Vec::with_capacity(list.len());
    for (index, item) in list.iter().enumerate() {
        let number = index + 1;
        let item = item.as_table().ok_or_else(|| {
            XtaskError::new(format!(
                "the lint allow-list: entry {number} is not a table"
            ))
        })?;
        let field = |name: &str| -> Result<String> {
            item.get(name)
                .and_then(Value::as_str)
                .map(str::to_owned)
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    XtaskError::new(format!(
                        "the lint allow-list: entry {number} needs a non-empty string `{name}`"
                    ))
                })
        };
        entries.push(Entry {
            rule: field("rule")?,
            file: field("file")?,
            contains: item
                .get("contains")
                .and_then(Value::as_str)
                .map(str::to_owned),
            reason: field("reason")?,
        });
    }
    Ok(entries)
}

impl Entry {
    fn accepts(&self, finding: &Finding) -> bool {
        self.rule == finding.rule
            && self.file == finding.file
            && self
                .contains
                .as_ref()
                .is_none_or(|needle| finding.text.contains(needle.as_str()))
    }

    fn label(&self) -> String {
        match &self.contains {
            Some(needle) => format!("{} in {} containing `{needle}`", self.rule, self.file),
            None => format!("{} in {}", self.rule, self.file),
        }
    }
}

/// Drops the accepted findings and adds one `allow-unused` finding per entry that accepted nothing.
pub fn apply(entries: &[Entry], findings: Vec<Finding>, allow_file: &str) -> Vec<Finding> {
    let mut used = vec![false; entries.len()];
    let mut kept = Vec::new();
    for finding in findings {
        let mut accepted = false;
        for (index, entry) in entries.iter().enumerate() {
            if entry.accepts(&finding) {
                if let Some(flag) = used.get_mut(index) {
                    *flag = true;
                }
                accepted = true;
            }
        }
        if !accepted {
            kept.push(finding);
        }
    }
    for (entry, was_used) in entries.iter().zip(used) {
        if !was_used {
            kept.push(Finding::new(
                allow_file,
                1,
                "allow-unused",
                format!(
                    "the exception for {} matches nothing: remove it",
                    entry.label()
                ),
                "",
            ));
        }
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIST: &str = r#"
[[allow]]
rule = "r1"
file = "a.md"
contains = "kept"
reason = "because"

[[allow]]
rule = "r2"
file = "b.md"
reason = "whole file"
"#;

    fn finding(file: &str, rule: &'static str, text: &str) -> Finding {
        Finding::new(file, 3, rule, "m", text)
    }

    #[test]
    fn entries_are_read_with_their_reason() {
        let entries = parse(LIST).unwrap_or_default();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].contains.as_deref(), Some("kept"));
        assert_eq!(entries[1].contains, None);
        assert_eq!(entries[1].reason, "whole file");
    }

    #[test]
    fn an_entry_without_a_reason_is_refused() {
        let text = "[[allow]]\nrule = \"r\"\nfile = \"f\"\n";
        assert!(parse(text).is_err());
        let blank = "[[allow]]\nrule = \"r\"\nfile = \"f\"\nreason = \"  \"\n";
        assert!(parse(blank).is_err());
        assert!(parse("allow = 3").is_err());
    }

    #[test]
    fn no_entry_is_fine() {
        assert_eq!(parse("").unwrap_or_default(), Vec::new());
    }

    #[test]
    fn a_matching_finding_is_accepted_and_the_others_stay() {
        let entries = parse(LIST).unwrap_or_default();
        let findings = vec![
            finding("a.md", "r1", "this line is kept"),
            finding("a.md", "r1", "another line"),
            finding("b.md", "r2", "anything"),
            finding("a.md", "r2", "wrong file for r2"),
        ];
        let left = apply(&entries, findings, "allow.toml");
        let texts: Vec<&str> = left.iter().map(|f| f.text.as_str()).collect();
        assert_eq!(texts, ["another line", "wrong file for r2"]);
    }

    #[test]
    fn an_entry_that_matches_nothing_is_a_finding() {
        let entries = parse(LIST).unwrap_or_default();
        let left = apply(&entries, Vec::new(), "allow.toml");
        assert_eq!(left.len(), 2);
        assert!(left.iter().all(|f| f.rule == "allow-unused"));
        assert!(left.iter().all(|f| f.file == "allow.toml"));
    }
}
