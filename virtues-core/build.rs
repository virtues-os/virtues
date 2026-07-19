use chrono::Utc;

fn main() {
    // Recompile if migrations change
    println!("cargo:rerun-if-changed=migrations");

    // Recompile if git HEAD changes
    println!("cargo:rerun-if-changed=.git/HEAD");

    // Get git commit SHA
    let output = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output();

    let commit = match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        _ => std::env::var("GIT_COMMIT").unwrap_or_else(|_| "unknown".to_string()),
    };

    println!("cargo:rustc-env=GIT_COMMIT={}", commit);

    // Full release tag for `--version`, e.g. "v0.1.0-staging.43" (or with an
    // offset/`-dirty` for local builds between tags). CI sets VIRTUES_BUILD_VERSION
    // to the exact release tag because its checkout is shallow and has no tags;
    // local builds derive it from `git describe`. Empty when neither is available.
    println!("cargo:rerun-if-env-changed=VIRTUES_BUILD_VERSION");
    let describe = std::env::var("VIRTUES_BUILD_VERSION")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| {
            std::process::Command::new("git")
                .args(["describe", "--tags", "--always", "--dirty", "--match", "v*"])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_default();
    println!("cargo:rustc-env=GIT_DESCRIBE={}", describe);

    // Get build timestamp in ISO 8601 format
    let built_at = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    println!("cargo:rustc-env=BUILD_TIME={}", built_at);
}
