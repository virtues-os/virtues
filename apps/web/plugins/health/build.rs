const COMMANDS: &[&str] = &["enable", "resume", "status", "collect"];

fn main() {
  tauri_plugin::Builder::new(COMMANDS).ios_path("ios").build();
}
