//! Builds the QNN serving daemon (`csrc/qnn_server.cpp`) into this crate's
//! binary, linking it behind the thin Rust `main` in `src/main.rs`.
//!
//! The daemon compiles against the Qualcomm QAIRT SDK headers, which are
//! Confidential/Proprietary and therefore NOT vendored into the repo. We locate
//! a SDK install via `QNN_SDK_ROOT` at build time (the same way `models/`
//! compiles context binaries). The SDK's runtime libs (`libQnnHtp.so`,
//! `libQnnSystem.so`) are `dlopen`'d by the daemon at runtime, so the build only
//! needs the headers.
//!
//! When `QNN_SDK_ROOT` is unset or invalid — every machine without the QAIRT
//! SDK, i.e. all normal dev boxes and CI legs that don't target the Dragon — we
//! compile `csrc/stub.cpp` instead. That keeps `cargo build` green across the
//! whole workspace; the resulting binary just prints a build-config error if
//! run. Real daemons are produced only where the SDK is present (the Dragon
//! itself, or a release leg that sets `QNN_SDK_ROOT`).

use std::path::Path;

fn main() {
    println!("cargo:rerun-if-env-changed=QNN_SDK_ROOT");
    println!("cargo:rerun-if-changed=csrc/qnn_server.cpp");
    println!("cargo:rerun-if-changed=csrc/stub.cpp");

    let mut build = cc::Build::new();
    build.cpp(true).std("c++17").flag_if_supported("-O2");

    match std::env::var("QNN_SDK_ROOT").ok().filter(|s| !s.trim().is_empty()) {
        Some(sdk) if Path::new(&sdk).join("include/QNN/QnnInterface.h").exists() => {
            // Real build. The daemon mixes `QNN/…` and `System/…` include
            // styles, so both roots are needed (see the header closure).
            let inc = Path::new(&sdk).join("include");
            let inc_qnn = inc.join("QNN");
            build
                .file("csrc/qnn_server.cpp")
                .include(&inc)
                .include(&inc_qnn);
            // The daemon dlopen's the QNN libs, so no link-time QNN lib — just libdl.
            println!("cargo:rustc-link-lib=dylib=dl");
            println!("cargo:warning=virtues-qnnd: building REAL QNN daemon against QNN_SDK_ROOT={sdk}");
        }
        Some(sdk) => {
            println!(
                "cargo:warning=virtues-qnnd: QNN_SDK_ROOT={sdk} has no include/QNN/QnnInterface.h — building STUB daemon"
            );
            build.file("csrc/stub.cpp");
        }
        None => {
            println!(
                "cargo:warning=virtues-qnnd: QNN_SDK_ROOT unset — building STUB daemon (no NPU). Set it to a QAIRT 2.42 install to build the real one."
            );
            build.file("csrc/stub.cpp");
        }
    }

    build.compile("qnnd");
}
