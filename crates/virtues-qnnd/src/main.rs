//! The QNN serving daemon: C++ NPU engine + llama-server-compatible HTTP.
//!
//! One process, two layers. A background thread runs the C++ daemon
//! (`csrc/qnn_server.cpp`, `qnnd_main`) exactly as before — its binary TCP loop
//! on loopback is now an internal implementation detail. The main task serves
//! the box-facing HTTP contract (`/health`, `/v1/models`, `/v1/embeddings`,
//! `/v1/rerank`) on the same ports llama-server would occupy, so virtues-core
//! talks to a Dragon box and a DIY box through one identical path — the
//! installer just points `VIRTUES_EMBED_URL`/`VIRTUES_RERANK_URL` here.
//!
//! Self-TCP costs ~µs against the ~4 ms NPU execute; keeping the C++ untouched
//! is worth far more than removing it. (Collapsing the internal loop into an
//! in-process call is a later, optional cleanup.)
//!
//! ```text
//! virtues-qnnd <embed.bin> <rerank.bin> [--burst] [--port 7788]
//!              [--models-dir /var/lib/virtues/models/qnn]
//!              [--embed-http 18181] [--rerank-http 18182] [--no-http]
//! ```
//! `--no-http` preserves the legacy TCP-only behavior (dev tools, e2e_demo.py).

use std::ffi::CString;
use std::os::raw::{c_char, c_int};
use std::path::PathBuf;
use std::sync::Arc;

mod engine;
mod http;

extern "C" {
    fn qnnd_main(argc: c_int, argv: *mut *mut c_char) -> c_int;
}

/// Run the C++ daemon with the given argv; never returns normally in serving
/// mode, so a return means startup failure (or the stub build) — propagate it
/// as the whole process's exit. Keeps the CStrings alive for the whole call —
/// `argv` holds borrowed ptrs.
fn run_cpp_daemon(args: Vec<String>) -> ! {
    let c_args: Vec<CString> = args
        .into_iter()
        .map(|a| CString::new(a).unwrap_or_else(|_| CString::new("").unwrap()))
        .collect();
    let mut argv: Vec<*mut c_char> = c_args.iter().map(|a| a.as_ptr() as *mut c_char).collect();
    argv.push(std::ptr::null_mut()); // argv[argc] == NULL, as C expects
    let code = unsafe { qnnd_main(c_args.len() as c_int, argv.as_mut_ptr()) };
    eprintln!("virtues-qnnd: engine loop exited with code {code}");
    std::process::exit(code);
}

struct Args {
    /// argv for the C++ daemon: program name + context binaries + --burst/--port.
    cpp: Vec<String>,
    tcp_port: u16,
    models_dir: PathBuf,
    embed_http: u16,
    rerank_http: u16,
    no_http: bool,
}

/// Hand-rolled parse: the flag set is tiny and stable, and the C++ side must
/// receive only the flags it knows (`--burst`, `--port`) — unknown flags there
/// are a startup error.
fn parse_args() -> Args {
    let mut all = std::env::args();
    let prog = all.next().unwrap_or_else(|| "virtues-qnnd".into());
    let mut cpp = vec![prog];
    let mut tcp_port: u16 = 7788;
    let mut models_dir = std::env::var("VIRTUES_QNND_MODELS_DIR")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/var/lib/virtues/models/qnn"));
    let mut embed_http: u16 = 18181;
    let mut rerank_http: u16 = 18182;
    let mut no_http = false;

    fn next_u16(it: &mut std::env::Args, flag: &str) -> u16 {
        match it.next().and_then(|v| v.parse().ok()) {
            Some(p) => p,
            None => die(&format!("{flag} needs a port number")),
        }
    }

    while let Some(arg) = all.next() {
        match arg.as_str() {
            "--burst" => cpp.push(arg),
            "--port" => {
                tcp_port = next_u16(&mut all, "--port");
                cpp.push("--port".into());
                cpp.push(tcp_port.to_string());
            }
            "--models-dir" => {
                models_dir = match all.next() {
                    Some(p) => PathBuf::from(p),
                    None => die("--models-dir needs a path"),
                };
            }
            "--embed-http" => embed_http = next_u16(&mut all, "--embed-http"),
            "--rerank-http" => rerank_http = next_u16(&mut all, "--rerank-http"),
            "--no-http" => no_http = true,
            _ => cpp.push(arg), // positional: context binaries
        }
    }
    // The C++ defaults to 7788 when --port is absent; pin our view of it either way.
    if !cpp.iter().any(|a| a == "--port") {
        cpp.push("--port".into());
        cpp.push(tcp_port.to_string());
    }
    Args { cpp, tcp_port, models_dir, embed_http, rerank_http, no_http }
}

fn die(msg: &str) -> ! {
    eprintln!("virtues-qnnd: {msg}");
    std::process::exit(2);
}

fn main() {
    let args = parse_args();

    if args.no_http {
        // Legacy shape: the C++ daemon owns the process.
        run_cpp_daemon(args.cpp);
    }

    // Engine on a background thread; if it dies, run_cpp_daemon exits the
    // whole process — systemd's Restart=on-failure brings both layers back
    // together, never an HTTP frontend serving a dead engine.
    let cpp_args = args.cpp.clone();
    std::thread::spawn(move || run_cpp_daemon(cpp_args));

    let rt = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(rt) => rt,
        Err(e) => die(&format!("tokio runtime: {e}")),
    };

    rt.block_on(async move {
        // Wait for the engine loop to accept before exposing /health — context
        // binaries load in seconds; 60s is generous headroom.
        let tcp_addr = format!("127.0.0.1:{}", args.tcp_port);
        let mut up = false;
        for _ in 0..120 {
            if tokio::net::TcpStream::connect(&tcp_addr).await.is_ok() {
                up = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        if !up {
            die(&format!("engine loop never came up on {tcp_addr}"));
        }

        let client = match engine::QnnClient::new(tcp_addr, &args.models_dir) {
            Ok(c) => Arc::new(c),
            Err(e) => die(&format!("{e:#}")),
        };

        // Two listeners, one router — the box's default URLs are per-role
        // (:18181 embed, :18182 rerank, matching the llama-server pair), and
        // serving the full contract on both keeps either URL fully functional.
        async fn serve(port: u16, client: Arc<engine::QnnClient>) {
            let addr = format!("127.0.0.1:{port}");
            let listener = match tokio::net::TcpListener::bind(&addr).await {
                Ok(l) => l,
                Err(e) => die(&format!("bind {addr}: {e}")),
            };
            eprintln!("virtues-qnnd: HTTP contract on {addr}");
            if let Err(e) = axum::serve(listener, http::router(client)).await {
                die(&format!("serve {addr}: {e}"));
            }
        }

        tokio::join!(
            serve(args.embed_http, client.clone()),
            serve(args.rerank_http, client)
        );
    });
}
