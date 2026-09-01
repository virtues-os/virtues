// macOS + Bluetooth: setup CANNOT be exercised by `tauri dev`.
//
// `tauri dev` runs a bare binary, and TCC reads usage descriptions from an app
// BUNDLE's Info.plist. Embedding the plist in the executable's
// `__TEXT,__info_plist` section — the documented trick for non-bundled Mach-O
// — was tried on 2026-08-11 and does NOT satisfy TCC here: the process still
// died with `namespace: TCC, "attempted to access privacy-sensitive data
// without a usage description"`, naming the very key that was embedded.
//
// So the dev loop for anything touching CoreBluetooth is:
//
//     pnpm tauri build --debug && open src-tauri/target/debug/bundle/macos/Virtues.app
//
// which carries the real Info.plist (see ../Info.plist) and prompts for
// Bluetooth the first time. Everything else in the app dev-loops normally.
//
// This comment is the deliverable: the failure mode is a silent SIGABRT with
// no dialog and nothing in stdout, and the next person to hit it should find
// this instead of spending the afternoon re-deriving it.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

/// Every `#[tauri::command]` this shell registers, across BOTH entry points
/// (`main.rs` for desktop, `lib.rs` for mobile). The single source of truth
/// for the app's own ACL — the sibling of each plugin's `build.rs` COMMANDS.
///
/// WHY THIS EXISTS. A paired desktop window loads the box-served SPA at
/// `http://localhost:<loopback>`, which Tauri classifies as a REMOTE origin
/// (`is_local_url`: local means the tauri:// protocol, `frontendDist`, or a
/// registered custom scheme — a loopback HTTP URL is none of those). And for a
/// remote origin, `Webview::on_message` refuses any command whose ACL does not
/// resolve:
///
///     if (plugin_command.is_some() || has_app_acl_manifest || !is_local)
///        && invoke.acl.is_none() { reject("Command {} not allowed by ACL") }
///
/// The app previously declared NO app manifest, so its own commands had no
/// permissions to resolve, and every one of them was refused from the box UI —
/// "Command get_collector_status not allowed by ACL" on Settings → This Mac,
/// with the collector running perfectly the whole time. `remote.urls` in the
/// capability (added 2026-06-15 for exactly this symptom) makes the capability
/// APPLY to that origin, but grants nothing on its own: the permission still
/// has to exist. Fourth occurrence of the ACL-drift class, and the first one
/// outside the plugins.
///
/// CARE, both directions:
///   • declaring an app manifest turns `has_app_acl_manifest` on, which makes
///     LOCAL windows enforce the ACL too — so the airlock (`connect.html`) and
///     the whole mobile shell need their commands permitted as well, or adding
///     this list would break the very flows it is meant to fix. Hence the
///     capability check below covers every entry point.
///   • Tauri slugifies to `allow-$command` in kebab-case, so
///     `get_collector_status` is granted by `allow-get-collector-status`.
const APP_COMMANDS: &[&str] = &[
    // Shell identity + UI delivery (both platforms).
    "command_surface_version",
    "shell_identity_cmd",
    "bundle_boot_ok",
    "set_appearance",
    "ota_check_now",
    // App updater (desktop).
    "update_state_cmd",
    "apply_update_cmd",
    "check_app_update_cmd",
    // Pairing + box reachability (desktop).
    "get_client_status",
    "discover_servers",
    "pair_with_code",
    "install_helpers",
    "uninstall_helpers",
    "forget_pairing",
    "recheck_box",
    "diagnose_box",
    "restart_app",
    // The Mac collector (desktop).
    "get_collector_status",
    "install_collector",
    "uninstall_collector",
    "pause_collector",
    "resume_collector",
    "stop_collector",
    "open_full_disk_access",
    "open_accessibility_settings",
    // Window chrome (desktop).
    "set_summon_shortcut",
];

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let desktop = registered_commands(root, "src/main.rs");
    let mobile = registered_commands(root, "src/lib.rs");

    // 1. APP_COMMANDS must match what is actually registered, both ways. A
    //    registered-but-unlisted command is the runtime refusal this whole
    //    module exists to prevent; a listed-but-unregistered one grants a
    //    permission for a command that errors on dispatch.
    let registered: BTreeSet<&str> = desktop.union(&mobile).copied().collect();
    let listed: BTreeSet<&str> = APP_COMMANDS.iter().copied().collect();
    for cmd in registered.difference(&listed) {
        panic!(
            "[app-acl] `{cmd}` is in generate_handler! but not in APP_COMMANDS \
             (build.rs) — it would be refused at runtime from the box UI"
        );
    }
    for cmd in listed.difference(&registered) {
        panic!("[app-acl] `{cmd}` is in APP_COMMANDS but registered nowhere");
    }

    // 2. Every registered command needs `allow-<kebab>` in the capability that
    //    covers its platform, or the ACL cannot resolve it.
    check_capability(root, "capabilities/default.json", &desktop);
    check_capability(root, "capabilities/mobile.json", &mobile);

    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(APP_COMMANDS)),
    )
    .expect("failed to run tauri build");
}

/// The union of every `generate_handler![...]` block in one entry point.
fn registered_commands(root: &Path, rel: &str) -> BTreeSet<&'static str> {
    let path = root.join(rel);
    println!("cargo:rerun-if-changed={}", path.display());
    let src = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("[app-acl] cannot read {}: {e}", path.display()));
    let mut found = BTreeSet::new();
    let mut rest = src.as_str();
    while let Some(at) = rest.find("generate_handler![") {
        rest = &rest[at + "generate_handler![".len()..];
        let close = rest
            .find(']')
            .unwrap_or_else(|| panic!("[app-acl] unterminated generate_handler! in {rel}"));
        for raw in rest[..close].split(',') {
            let name = raw.trim();
            if name.is_empty() || name.starts_with("//") {
                continue;
            }
            // Borrow from APP_COMMANDS so the set can outlive `src`; an
            // unknown name is reported by the caller's difference check.
            if let Some(known) = APP_COMMANDS.iter().find(|c| **c == name) {
                found.insert(*known);
            } else {
                panic!(
                    "[app-acl] `{name}` is in generate_handler! ({rel}) but not in \
                     APP_COMMANDS (build.rs) — it would be refused at runtime"
                );
            }
        }
        rest = &rest[close..];
    }
    found
}

/// Fail the build naming the exact line to add, rather than shipping a binary
/// that refuses its own commands on a real machine.
fn check_capability(root: &Path, rel: &str, commands: &BTreeSet<&str>) {
    let path = root.join(rel);
    println!("cargo:rerun-if-changed={}", path.display());
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("[app-acl] cannot read {}: {e}", path.display()));
    for cmd in commands {
        let perm = format!("allow-{}", cmd.replace('_', "-"));
        assert!(
            text.contains(&format!("\"{perm}\"")),
            "[app-acl] {rel} is missing \"{perm}\" — `{cmd}` would be refused by \
             the ACL. Add it to the capability's permissions array."
        );
    }
}
