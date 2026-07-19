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

/// The serif Virtues wordmark — the same "Georgia11" figlet the CLI opens with,
/// replicated here so `curl … | sh` and the CLI share one visual identity.
/// Plain ASCII art: it pipes and logs cleanly with no ANSI to garble.
const WORDMARK: &str = r#"
              ,,
`7MMF'   `7MF'db             mm
  `MA     ,V                 MM
   VM:   ,V `7MM  `7Mb,od8 mmMMmm `7MM  `7MM  .gP"Ya  ,pP"Ybd
    MM.  M'   MM    MM' "'   MM     MM    MM ,M'   Yb 8I   `"
    `MM A'    MM    MM       MM     MM    MM 8M"""""" `YMMMa.
     :MM;     MM    MM       MM     MM    MM YM.    , L.   I8
      VF    .JMML..JMML.     `Mbmo  `Mbod"YML.`Mbmmd' M9mmmP'
"#;

const MISSION: &str = "   This is technology that helps you be the person you ought to become.";

/// The brand badges — claims, not instructions, so they live here in the
/// installer (discovery mode) rather than on the pair screen (task mode).
const BADGES: &str = "   ◆ Open Source    ◆ 100% Yours    ◆ $0 Venture Funding    ◆ Public Benefit Co";

/// The single brand header, printed once at the top of the installer.
pub fn print_header() {
    if !is_tty() {
        // Plain-text fallback for logs — no ANSI.
        println!("{WORDMARK}");
        println!("{MISSION}");
        println!();
        println!("{BADGES}");
        println!();
        return;
    }

    println!("{WORDMARK}");
    println!("   {}", style("This is technology that helps you be the person you ought to become.").dim());
    println!();
    println!("   {}", style("◆ Open Source    ◆ 100% Yours    ◆ $0 Venture Funding    ◆ Public Benefit Co").bold());
    println!();
}

/// Single-character brand mark for inline use ("∴ Installing…").
pub fn mark() -> &'static str {
    "∴"
}
