const COMMANDS: &[&str] = &["pair", "reach_status", "forget", "discover", "outbox_stats", "drain_now"];

fn main() {
  tauri_plugin::Builder::new(COMMANDS).build();
}
