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
fn main() {
    tauri_build::build()
}
