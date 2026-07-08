//! Thin Rust entry point for the QNN serving daemon.
//!
//! The daemon proper is C++ (`csrc/qnn_server.cpp`, exposed as `qnnd_main`),
//! compiled and linked by `build.rs`. This `main` just forwards the process
//! argv into it and propagates the exit code. See the crate README for the
//! protocol and the model-artifact contract.

use std::ffi::CString;
use std::os::raw::{c_char, c_int};

extern "C" {
    fn qnnd_main(argc: c_int, argv: *mut *mut c_char) -> c_int;
}

fn main() {
    // Keep the CStrings alive for the whole call — `argv` holds borrowed ptrs.
    let args: Vec<CString> = std::env::args()
        .map(|a| CString::new(a).unwrap_or_else(|_| CString::new("").unwrap()))
        .collect();
    let mut argv: Vec<*mut c_char> = args.iter().map(|a| a.as_ptr() as *mut c_char).collect();
    argv.push(std::ptr::null_mut()); // argv[argc] == NULL, as C expects

    let code = unsafe { qnnd_main(args.len() as c_int, argv.as_mut_ptr()) };
    std::process::exit(code);
}
