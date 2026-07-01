//! Host binary uniffi invokes to generate the foreign-language bindings from the
//! built library (`cargo run -p virtues-iroh-ffi --bin uniffi-bindgen -- ...`).
fn main() {
    uniffi::uniffi_bindgen_main()
}
