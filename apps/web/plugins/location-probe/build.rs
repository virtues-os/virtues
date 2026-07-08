const COMMANDS: &[&str] = &["start_probe", "read_rows"];

fn main() {
  tauri_plugin::Builder::new(COMMANDS)
    .android_path("android")
    .ios_path("ios")
    .build();
}
