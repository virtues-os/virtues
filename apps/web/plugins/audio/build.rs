// COMMANDS is the single source of truth for this plugin's command surface:
// lockstep verifies generate_handler! against it, generates default.toml from
// it, and prunes stale permission files (see plugins/lockstep). `set_notify`
// was missing from this list for six weeks while registered everywhere else —
// the ACL refused it at runtime and the notify toggle silently died.
const COMMANDS: &[&str] = &["enable", "disable", "resume", "status", "set_notify"];

fn main() {
  virtues_plugin_lockstep::enforce(COMMANDS);
  tauri_plugin::Builder::new(COMMANDS).ios_path("ios").build();
}
