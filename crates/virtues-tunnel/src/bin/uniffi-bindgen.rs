//! Bindings generator entrypoint (uniffi library mode). Invoked by
//! build-xcframework.sh after the staticlib is built:
//!
//! ```sh
//! cargo run --bin uniffi-bindgen -- generate \
//!   --library target/aarch64-apple-ios/release/libvirtues_tunnel.a \
//!   --language swift --out-dir generated/
//! ```
fn main() {
    uniffi::uniffi_bindgen_main()
}
