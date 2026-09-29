//! Putting `virtues-qnnd`'s QAIRT runtime libs on the box.
//!
//! The daemon is built against QAIRT headers in CI but opens `libQnnHtp.so`
//! and friends at runtime, and nothing else puts them there. The fetch itself,
//! the pinned digests, and why we fetch from Qualcomm rather than re-host live
//! in the `virtues-qairt` crate, which core shares for the local model's
//! runtime. This module is the installer's progress and messages around it.

use anyhow::{Context, Result};
use std::path::PathBuf;
use std::time::Duration;
use virtues_qairt::{link_cdsprpc, Dirs, QNND};

use crate::config::InstallConfig;
use crate::ui;

/// Ensure qnnd's runtime libs are on the box, fetching them if they are not.
/// Returns `(host_dir, dsp_dir)` for the unit's two library paths.
pub async fn ensure_libs(cfg: &InstallConfig) -> Result<(PathBuf, PathBuf)> {
    let dirs = Dirs::under(&cfg.qnn_managed_lib_dir());

    if QNND.is_installed(&dirs) {
        ui::skip("QAIRT runtime libs already present");
    } else {
        ui::info(&format!(
            "Fetching QAIRT {} runtime libs from Qualcomm (~6 MB of a 1.4 GB SDK)",
            QNND.version
        ));
        let d = dirs.clone();
        // Blocking: each seek is an HTTP range request. Confined to one
        // blocking task rather than sprinkling block_on through a Read impl.
        tokio::task::spawn_blocking(move || {
            let spinner = indicatif::ProgressBar::new_spinner();
            spinner.set_style(
                indicatif::ProgressStyle::with_template("  {spinner:.dim} {msg}")
                    .unwrap()
                    .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
            );
            spinner.enable_steady_tick(Duration::from_millis(80));
            spinner.set_message("reading QAIRT SDK index");
            let r = QNND.fetch_blocking(&d, |f| spinner.set_message(format!("fetching {f}")));
            spinner.finish_and_clear();
            r
        })
        .await
        .context("QAIRT extraction task panicked")??;
        ui::ok(&format!("QAIRT runtime libs installed ({})", dirs.host.display()));
    }

    // The symlink is not part of the digest set, so "already present" must
    // still ensure it. A box with verified libs and no symlink is exactly the
    // reinstall case, and skipping it would leave qnnd failing with 14001.
    if let Err(e) = link_cdsprpc(&dirs.host) {
        ui::warn(&format!("{e:#}; the NPU will not serve without it"));
    }
    Ok((dirs.host, dirs.dsp))
}
