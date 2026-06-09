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

/// Themed in-progress message. Used sparingly between concrete progress to
/// give the install a sense of intentionality. Tone is reflective, not
/// whimsical — Claude-Code-"thinking"-style copy, not chakra/woo.
///
/// Locked list (sync with install flow):
///   - "Forging your box's identity…"     CA + WG keypair generation
///   - "Provisioning your data layer…"    Postgres role + DB + pgvector
///   - "Establishing trust…"              systemd unit + service start
///   - "Linking inference…"               Ollama + embedding model pull
///   - "Sealing your sovereignty…"        final identity finalization
///   - "Preparing your hardware…"         driver/arch detection
pub fn thinking(msg: &str) {
    println!();
    println!("  {}  {}", style("⋯").dim(), style(msg).italic().dim());
}
