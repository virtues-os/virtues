// COMMANDS drives Tauri's ACL codegen: a command missing here is refused at
// runtime no matter what the permission TOMLs say. The provision_* trio was
// added to commands.rs without being added here (2026-08-10) and would have
// been silently blocked — keep this list in lockstep with
// `tauri::generate_handler!` in lib.rs.
const COMMANDS: &[&str] = &[
  "pair",
  "reach_status",
  "forget",
  "discover",
  "provision_open",
  "provision_networks",
  "provision_join",
  "wifi_join",
  "wifi_forget",
  "outbox_stats",
  "drain_now",
  "radio_stats",
];

fn main() {
  tauri_plugin::Builder::new(COMMANDS).ios_path("ios").build();
}
