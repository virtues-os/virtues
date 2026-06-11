//! Brand-mark + visual identity helpers.
//!
//! The `∴` (therefore) glyph is the Virtues logo. We use it as:
//!   - The top-of-script ASCII header
//!   - The prefix on every section header throughout the install
//!   - The closer on the install-complete handoff
//!
//! All output is TTY-gated — non-interactive runs (CI, systemd journal
//! capture) get plain text without ANSI sequences.

use console::{style, Term};

pub fn is_tty() -> bool {
    Term::stdout().is_term()
}

/// The single brand header, printed once at the top of the installer.
/// Three lines, low ink, scannable at any terminal width.
pub fn print_header() {
    if !is_tty() {
        // Plain-text fallback for logs.
        println!();
        println!("   --------- ∴ ---------");
        println!("        V I R T U E S");
        println!("   your data. your hardware.");
        println!("   ---------------------");
        println!();
        return;
    }

    println!();
    println!("   {}", style("─────────∴─────────").bold());
    println!("   {}", style("     V I R T U E S").bold());
    println!("   {}", style("your data. your hardware.").dim());
    println!("   {}", style("───────────────────").bold());
    println!();
}

/// Single-character brand mark for inline use ("∴ Installing…").
pub fn mark() -> &'static str {
    "∴"
}
