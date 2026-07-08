const COMMANDS: &[&str] = &["start_probe", "resume_probe", "read_rows"];

fn main() {
  tauri_plugin::Builder::new(COMMANDS)
    .android_path("android")
    .ios_path("ios")
    .build();
}
