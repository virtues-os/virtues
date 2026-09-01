// COMMANDS is the single source of truth — lockstep verifies generate_handler!
// against it and generates default.toml from it (see plugins/lockstep).
const COMMANDS: &[&str] = &["enable", "resume", "status", "collect"];

fn main() {
  virtues_plugin_lockstep::enforce(COMMANDS);
  tauri_plugin::Builder::new(COMMANDS).ios_path("ios").build();
}
