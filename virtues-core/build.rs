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

    // Get build timestamp in ISO 8601 format
    let built_at = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    println!("cargo:rustc-env=BUILD_TIME={}", built_at);
}
