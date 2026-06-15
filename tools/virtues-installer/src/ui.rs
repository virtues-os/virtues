//! Output helpers shared across modules.
//!
//! Thin layer over `cliclack` and `console`. The point is consistency: one
//! place for the section-header format, the success/warn/error iconography,
//! the indent rules. Modules that need a custom prompt go through cliclack
//! directly; everything else uses these.

use console::style;
use std::process;

use crate::brand;

/// Print a top-level section header (e.g. "∴ Installing").
pub fn section(title: &str) {
    println!();
    println!("  {}  {}", style(brand::mark()).bold().green(), style(title).bold());
}

/// A successful step under the current section.
pub fn ok(msg: &str) {
    println!("  {}  {}", style("✓").green(), msg);
}

/// A skipped/no-op step ("already configured").
pub fn skip(msg: &str) {
    println!("  {}  {}", style("·").dim(), style(msg).dim());
}

/// A non-fatal warning.
pub fn warn(msg: &str) {
    println!("  {}  {}", style("⚠").yellow(), style(msg).yellow());
}

/// Fatal error — print + exit with non-zero code. Never returns.
pub fn die(msg: &str) -> ! {
    eprintln!();
    eprintln!("  {}  {}", style("✖").red().bold(), style(msg).red());
    eprintln!();
    process::exit(1);
}

/// Network topology callout — more weight than a plain `warn` because this is
/// an architectural fact the user may need to plan around, not a transient
/// issue. Red marker, extra vertical space, and bullet context so it stands
/// out from the rest of the pre-flight checklist.
pub fn network_critical(headline: &str, bullets: &[&str], note: &str) {
    println!();
    println!("  {}  {}", style("⚠").bold().red(), style(headline).bold().red());
    for bullet in bullets {
        println!("       {}", style(bullet).red());
    }
    if !note.is_empty() {
        println!("       {}", style(note).dim());
    }
    println!();
}

