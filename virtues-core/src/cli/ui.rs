//! Output vocabulary for the virtues CLI.
//!
//! Kept deliberately in lockstep with the installer's `ui.rs` (by eyeball,
//! not a shared crate — 60 lines of println wrappers don't earn a version
//! seam). The doctrine is the same one the web System page follows: the
//! machine examined — a ship's log, not htop. Concretely:
//!
//!   · one glyph set: `∴` opens a section, `✓ · ⚠ ✖` are the only statuses
//!   · dot-leader ledgers for key…value rows
//!   · colour only when it carries meaning (green=fine, yellow=pressure,
//!     red=fault, dim=absent, cyan=the one thing to copy or run)
//!   · everything degrades to clean plain text when piped

use console::style;

/// Whether stdout is a terminal. Colour and middle-ellipsis are TTY-only so
/// piped/captured output stays complete and grep-able.
pub fn tty() -> bool {
    console::Term::stdout().is_term()
}

/// Top-level section header: `∴ Title`.
pub fn section(title: &str) {
    println!();
    println!("  {}  {}", style("∴").green().bold(), style(title).bold());
}

/// Sub-grouping inside a section (e.g. Doctor's "Inference" / "Reach").
pub fn subsection(title: &str) {
    println!();
    println!("    {}", style(title).bold());
}

/// A successful step.
pub fn ok(msg: &str) {
    println!("  {}  {}", style("✓").green(), msg);
}

/// A skipped/no-op step ("already gone", "kept").
pub fn skip(msg: &str) {
    println!("  {}  {}", style("·").dim(), style(msg).dim());
}

/// A non-fatal warning.
pub fn warn(msg: &str) {
    println!("  {}  {}", style("⚠").yellow(), style(msg).yellow());
}

/// An in-progress step announcement, printed before a long operation.
pub fn step(msg: &str) {
    println!("  {}  {}", style("→").dim(), msg);
}

/// Column the dot leaders fill to in default `kv` rows.
const KV_COL: usize = 12;

/// One ledger row: `key ……… value`, dot-leadered to a shared column so a block
/// of rows reads as a ledger. See `kv_at` for ledgers with longer keys.
pub fn kv(key: &str, value: &str) {
    kv_at(KV_COL, key, value);
}

/// `kv` with an explicit leader column, for ledgers whose keys outgrow the
/// default. Keys at or past the column still get one leader glyph so the
/// key/value seam never disappears.
pub fn kv_at(col: usize, key: &str, value: &str) {
    let klen = key.chars().count();
    let dots = if klen + 1 >= col { 1 } else { col - klen };
    println!("      {} {} {}", key, style("…".repeat(dots)).dim(), value);
}

/// Middle-ellipsize a long opaque value (node id, hash) for TTY display —
/// head and tail are how humans compare keys. Callers should pass the full
/// string when piped (gate on `tty()`), so logs keep the whole value.
pub fn ellipsize_middle(s: &str, max: usize) -> String {
    let n = s.chars().count();
    if n <= max || max < 5 {
        return s.to_string();
    }
    let keep = max - 1;
    let head: String = s.chars().take(keep - keep / 2).collect();
    let tail: String = s.chars().skip(n - keep / 2).collect();
    format!("{head}…{tail}")
}

/// Compact relative timestamp for ledgers ("2h ago"); absolute date past a
/// week (or for timestamps in the future, which only clock skew produces).
pub fn rel_time(t: chrono::DateTime<chrono::Utc>) -> String {
    let secs = (chrono::Utc::now() - t).num_seconds();
    if secs < 0 {
        return t.format("%Y-%m-%d").to_string();
    }
    match secs {
        0..=59 => "just now".to_string(),
        60..=3_599 => format!("{}m ago", secs / 60),
        3_600..=86_399 => format!("{}h ago", secs / 3_600),
        86_400..=604_799 => format!("{}d ago", secs / 86_400),
        _ => t.format("%Y-%m-%d").to_string(),
    }
}

/// How much a finding matters to the verdict: warnings inform, errors fail
/// the command's exit code (so `virtues doctor && …` composes in scripts).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Warn,
    Error,
}

struct Issue {
    severity: Severity,
    what: String,
    /// The remedy is always a runnable command or a concrete next step —
    /// a diagnosis without one is a readout, not a doctor.
    remedy: Option<String>,
}

/// Accumulates findings during a report, then renders one verdict block.
#[derive(Default)]
pub struct Issues(Vec<Issue>);

impl Issues {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn warn(&mut self, what: impl Into<String>, remedy: Option<&str>) {
        self.0.push(Issue {
            severity: Severity::Warn,
            what: what.into(),
            remedy: remedy.map(str::to_string),
        });
    }

    pub fn error(&mut self, what: impl Into<String>, remedy: Option<&str>) {
        self.0.push(Issue {
            severity: Severity::Error,
            what: what.into(),
            remedy: remedy.map(str::to_string),
        });
    }

    pub fn has_errors(&self) -> bool {
        self.0.iter().any(|i| i.severity == Severity::Error)
    }

    /// Print the verdict block and return the process exit code: 1 when any
    /// error-severity issue exists, 0 otherwise (warnings don't fail scripts).
    pub fn verdict(&self) -> i32 {
        println!();
        if self.0.is_empty() {
            println!("  {}  {}", style("✓").green().bold(), style("healthy").green());
            println!();
            return 0;
        }
        let head = if self.has_errors() {
            style("✖").red().bold()
        } else {
            style("⚠").yellow().bold()
        };
        let noun = if self.0.len() == 1 { "issue" } else { "issues" };
        println!("  {head}  {} {noun}", self.0.len());
        for i in &self.0 {
            let mark = match i.severity {
                Severity::Error => style("✖").red(),
                Severity::Warn => style("⚠").yellow(),
            };
            println!("     {mark}  {}", i.what);
            if let Some(r) = &i.remedy {
                println!("        {} {}", style("→").dim(), style(r).cyan());
            }
        }
        println!();
        if self.has_errors() {
            1
        } else {
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ellipsize_keeps_short_and_trims_long() {
        assert_eq!(ellipsize_middle("abc", 20), "abc");
        let id = "d1211990832d5cce27b3f79d170e225414d29065654eb272c46bdcee7baae7c8";
        let out = ellipsize_middle(id, 20);
        assert_eq!(out.chars().count(), 20);
        assert!(out.starts_with("d1211990"));
        assert!(out.ends_with("baae7c8"));
        assert!(out.contains('…'));
    }

    #[test]
    fn rel_time_buckets() {
        let now = chrono::Utc::now();
        assert_eq!(rel_time(now), "just now");
        assert_eq!(rel_time(now - chrono::Duration::minutes(5)), "5m ago");
        assert_eq!(rel_time(now - chrono::Duration::hours(3)), "3h ago");
        assert_eq!(rel_time(now - chrono::Duration::days(2)), "2d ago");
        // Past a week → absolute date, so a stale ledger reads as a date, not "412d ago".
        assert!(rel_time(now - chrono::Duration::days(30)).starts_with(|c: char| c.is_ascii_digit()));
    }

    #[test]
    fn verdict_exit_codes() {
        assert_eq!(Issues::new().verdict(), 0);
        let mut warns = Issues::new();
        warns.warn("something soft", None);
        assert_eq!(warns.verdict(), 0);
        let mut errs = Issues::new();
        errs.error("something hard", Some("run `virtues pair`"));
        assert_eq!(errs.verdict(), 1);
    }
}
