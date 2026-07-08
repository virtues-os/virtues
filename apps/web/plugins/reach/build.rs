const COMMANDS: &[&str] = &["pair", "reach_status", "forget", "discover", "outbox_stats"];

fn main() {
  tauri_plugin::Builder::new(COMMANDS).build();
}
