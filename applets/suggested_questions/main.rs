//! suggested_questions: the hourly run behind the question an empty new chat
//! offers in its composer.
//!
//! The work lives in `virtues-core`'s `api::suggestions::refresh`, which
//! skips quiet hours and an unchanged record on its own. A manual run skips
//! neither check, so it always writes. This binary owns only the stdin and
//! stdout contract.

use anyhow::Result;
use virtues_helpers::{output_with_records, read_input};

#[tokio::main]
async fn main() -> Result<()> {
    virtues_applets::init_tracing();

    let input = read_input()?;
    let pool = virtues_helpers::connect_from_env("virtues-action-suggested_questions").await?;

    // A run someone started by hand writes now; a scheduled one waits for
    // waking hours and for something to have changed.
    let force = input.trigger.as_deref() == Some("manual");
    let run = virtues::api::suggestions::refresh(&pool, force).await?;
    let result = match run.skipped {
        Some(why) => format!("Skipped: {why}."),
        None => format!("Wrote {} suggestions, {} offered.", run.candidates, run.offered),
    };
    output_with_records(&result, &input.config, run.candidates as i64)
}
