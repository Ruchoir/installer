//! What a run did, or would do: per table, the rows read, written and found already there; what
//! was left behind, counted by reason; and what the operator should know.

use std::collections::BTreeMap;
use std::fmt;

use serde::Serialize;

/// One table of one target.
#[derive(Debug, Default, Clone, Serialize, PartialEq, Eq)]
pub struct Line {
    /// Rows the source gave for it.
    pub read: u64,
    /// Rows written by this run.
    pub written: u64,
    /// Rows already there (an earlier run), left as they were.
    pub kept: u64,
}

#[derive(Debug, Default, Serialize)]
pub struct Report {
    pub dry_run: bool,
    /// `target.table` to its line.
    pub tables: BTreeMap<String, Line>,
    /// Why something was not carried over, to how many rows or objects.
    pub dropped: BTreeMap<String, u64>,
    /// Objects of the store: copied, already there, missing from the copy.
    pub objects: Line,
    pub objects_missing: Vec<String>,
    /// What the operator should check or tell people.
    pub notes: Vec<String>,
}

impl Report {
    pub fn line(&mut self, name: &str) -> &mut Line {
        self.tables.entry(name.to_owned()).or_default()
    }

    /// `count` rows left behind for `reason` (nothing recorded for none).
    pub fn drop(&mut self, reason: &str, count: u64) {
        if count > 0 {
            *self.dropped.entry(reason.to_owned()).or_default() += count;
        }
    }

    pub fn note(&mut self, note: impl Into<String>) {
        self.notes.push(note.into());
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.dry_run {
            writeln!(f, "Dry run: nothing was written.")?;
        }
        writeln!(
            f,
            "{:<44} {:>9} {:>9} {:>9}",
            "table", "read", "written", "kept"
        )?;
        for (name, line) in &self.tables {
            writeln!(
                f,
                "{name:<44} {:>9} {:>9} {:>9}",
                line.read, line.written, line.kept
            )?;
        }
        writeln!(
            f,
            "{:<44} {:>9} {:>9} {:>9}",
            "objects", self.objects.read, self.objects.written, self.objects.kept
        )?;
        if !self.objects_missing.is_empty() {
            writeln!(
                f,
                "\n{} object(s) missing from the copy of the store, first ones:",
                self.objects_missing.len()
            )?;
            for key in self.objects_missing.iter().take(20) {
                writeln!(f, "  {key}")?;
            }
        }
        if !self.dropped.is_empty() {
            writeln!(f, "\nLeft behind:")?;
            for (reason, count) in &self.dropped {
                writeln!(f, "  {count:>7}  {reason}")?;
            }
        }
        if !self.notes.is_empty() {
            writeln!(f, "\nTo know:")?;
            for note in &self.notes {
                writeln!(f, "  - {note}")?;
            }
        }
        Ok(())
    }
}
