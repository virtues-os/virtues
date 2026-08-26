// COMMANDS is the single source of truth — lockstep verifies generate_handler!
// against it and generates default.toml from it (see plugins/lockstep).
// `resume_probe` sat here without a default-set grant until 2026-08-26;
// generation from this list is what closes that gap for good.
const COMMANDS: &[&str] = &["start_probe", "resume_probe", "read_rows"];

fn main() {
  virtues_plugin_lockstep::enforce(COMMANDS);
  tauri_plugin::Builder::new(COMMANDS)
    .android_path("android")
    .ios_path("ios")
    .build();
}
