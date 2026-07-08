const COMMANDS: &[&str] = &["pair", "reach_status", "forget", "discover"];

fn main() {
  tauri_plugin::Builder::new(COMMANDS).build();
}
