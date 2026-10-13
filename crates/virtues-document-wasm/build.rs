//! The module's memory is capped, so a conversion that would grow past it
//! traps instead of taking a phone's memory with it; the editor then pastes
//! the text as it is and starts a fresh instance (`converter.ts`).

/// 256 MiB, in bytes: a whole number of 64 KiB pages.
const MAX_MEMORY: u64 = 256 << 20;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("wasm32") {
        println!("cargo:rustc-link-arg-cdylib=--max-memory={MAX_MEMORY}");
    }
}
