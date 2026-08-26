// COMMANDS drives Tauri's ACL codegen: a command missing here is refused at
// runtime no matter what the permission TOMLs say. That bit three times —
// the provision_* trio (2026-08-10), improv_pair_code (2026-08-19),
// audio.set_notify (2026-08-26) — so lockstep now enforces it at build time:
// generate_handler! in lib.rs is diffed against this list, default.toml is
// generated from it, and stale permission files are pruned (see
// plugins/lockstep).
const COMMANDS: &[&str] = &[
  "pair",
  "reach_status",
  "forget",
  "discover",
  "provision_open",
  "provision_networks",
  "provision_join",
  "improv_discover",
  "improv_claim",
  "improv_grant",
  "improv_wifi_scan",
  "improv_provision",
  "improv_pair",
  "improv_disconnect",
  "outbox_stats",
  "drain_now",
  "radio_stats",
];

fn main() {
  virtues_plugin_lockstep::enforce(COMMANDS);
  tauri_plugin::Builder::new(COMMANDS).ios_path("ios").build();
}
