//! Offline threshold sweep for the transcription VAD gate.
//!
//! Usage: cargo run -p virtues-applets --release --example vad_sweep -- <dir>...
//! Each <dir> holds m4a files of one labeled class (e.g. fp/ = passed the old
//! gate but Gemini found no speech; speech/ soft/ = must keep passing). Prints
//! per-file speech statistics and a pass-count grid over gate parameters.

#[path = "../transcription_resolution/vad.rs"]
mod vad;

use std::path::Path;

struct Clip {
    class: String,
    name: String,
    probs: Vec<f32>,
    dur: f32,
}

fn main() -> anyhow::Result<()> {
    let dirs: Vec<String> = std::env::args().skip(1).collect();
    if dirs.is_empty() {
        anyhow::bail!("usage: vad_sweep <dir-of-m4a>...");
    }
    let v = vad::Vad::new()?;
    let mut clips: Vec<Clip> = Vec::new();

    for dir in &dirs {
        let class = Path::new(dir)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let mut entries: Vec<_> = std::fs::read_dir(dir)?
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|x| x == "m4a"))
            .collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let bytes = std::fs::read(e.path())?;
            match v.speech_probs(&bytes)? {
                Some((probs, dur)) => clips.push(Clip {
                    class: class.clone(),
                    name: e.file_name().to_string_lossy().to_string(),
                    probs,
                    dur,
                }),
                None => eprintln!("{}: too short, skipped", e.path().display()),
            }
        }
    }

    // Per-clip detail at a few frame thresholds: total speech secs + longest run.
    println!(
        "{:<6} {:<14} {:>6}  {}",
        "class",
        "clip",
        "dur",
        "p=0.5: tot/max-run | p=0.7 | p=0.9"
    );
    for c in &clips {
        let fs = c.dur / c.probs.len().max(1) as f32;
        let stat = |p: f32| {
            let (mut tot, mut run, mut maxr) = (0usize, 0usize, 0usize);
            for &x in &c.probs {
                if x >= p {
                    tot += 1;
                    run += 1;
                    maxr = maxr.max(run);
                } else {
                    run = 0;
                }
            }
            format!("{:6.1}/{:5.2}", tot as f32 * fs, maxr as f32 * fs)
        };
        println!(
            "{:<6} {:<14} {:>5.0}s  {} | {} | {}",
            c.class,
            &c.name[..12.min(c.name.len())],
            c.dur,
            stat(0.5),
            stat(0.7),
            stat(0.9)
        );
    }

    // Grid: pass counts per class for each parameter combo.
    let classes: Vec<String> = {
        let mut cs: Vec<String> = clips.iter().map(|c| c.class.clone()).collect();
        cs.dedup();
        cs
    };
    println!("\n{:<28} {}", "gate (p/min_run/min_total)", classes.join(" / "));
    for &p in &[0.6f32, 0.65, 0.7, 0.75] {
        for &min_run in &[0.0f32, 0.1, 0.2] {
            for &min_total in &[0.2f32, 0.25, 0.3, 0.5] {
                let counts: Vec<String> = classes
                    .iter()
                    .map(|cl| {
                        let (pass, all) = clips.iter().filter(|c| &c.class == cl).fold(
                            (0, 0),
                            |(p_, a), c| {
                                let ok = vad::gate(&c.probs, c.dur, p, min_run, min_total);
                                (p_ + ok as usize, a + 1)
                            },
                        );
                        format!("{pass}/{all}")
                    })
                    .collect();
                println!(
                    "p={p:.1} run≥{min_run:.1}s tot≥{min_total:.1}s      {}",
                    counts.join("   ")
                );
            }
        }
    }
    Ok(())
}
