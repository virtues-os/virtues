//! `virtues sandbox-bridge -- <program> <args…>`: the proxy's door inside a
//! sandboxed unit.
//!
//! Clients find a proxy through `HTTPS_PROXY`, which takes a host and port,
//! not a Unix socket. So inside the unit this listens on loopback at
//! [`BRIDGE_PORT`], forwards every connection to the egress socket at
//! [`EGRESS_SOCKET`], runs the program, and exits with its status. The
//! unit's network is private, so loopback here is the unit's own and the
//! socket is the only thing the bridge can reach.
//!
//! It runs before any of the CLI's startup (`main.rs`), which reads the
//! box's environment file and may re-run as the service user; inside the
//! sandbox neither exists.

use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};

use super::{BRIDGE_PORT, EGRESS_SOCKET};

/// Run the bridge with the arguments after `sandbox-bridge`. Returns the
/// process exit code.
pub fn main(args: &[String]) -> i32 {
    let argv = match args.split_first() {
        Some((dashes, rest)) if dashes == "--" && !rest.is_empty() => rest,
        _ => {
            eprintln!("usage: virtues sandbox-bridge -- <program> [args…]");
            return 2;
        }
    };
    let listener = match TcpListener::bind(("127.0.0.1", BRIDGE_PORT)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("sandbox-bridge: could not listen on 127.0.0.1:{BRIDGE_PORT}: {e}");
            return 1;
        }
    };
    let socket = PathBuf::from(EGRESS_SOCKET);
    std::thread::spawn(move || serve(listener, &socket));
    match std::process::Command::new(&argv[0]).args(&argv[1..]).status() {
        Ok(status) => status.code().unwrap_or(1),
        Err(e) => {
            eprintln!("sandbox-bridge: could not run {}: {e}", argv[0]);
            127
        }
    }
}

/// Forward every connection on `listener` to `socket`, each on its own
/// threads, until the listener fails.
pub fn serve(listener: TcpListener, socket: &Path) {
    for conn in listener.incoming() {
        let Ok(tcp) = conn else { continue };
        let socket = socket.to_path_buf();
        std::thread::spawn(move || {
            if let Ok(unix) = UnixStream::connect(&socket) {
                pump(tcp, unix);
            }
        });
    }
}

/// Copy both ways until each side has finished sending.
fn pump(tcp: TcpStream, unix: UnixStream) {
    let (Ok(tcp_back), Ok(unix_back)) = (tcp.try_clone(), unix.try_clone()) else {
        return;
    };
    let up = std::thread::spawn(move || {
        copy(tcp, &unix_back);
        let _ = unix_back.shutdown(Shutdown::Write);
    });
    copy(unix, &tcp_back);
    let _ = tcp_back.shutdown(Shutdown::Write);
    let _ = up.join();
}

fn copy(mut from: impl Read, mut to: impl Write) {
    let mut buf = [0u8; 16 * 1024];
    loop {
        match from.read(&mut buf) {
            Ok(0) | Err(_) => return,
            Ok(n) => {
                if to.write_all(&buf[..n]).is_err() {
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;

    /// A connection to the bridge's port reaches the socket and both
    /// directions carry bytes, including the close.
    #[test]
    fn a_loopback_connection_reaches_the_socket() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("egress.sock");
        let unix = UnixListener::bind(&path).unwrap();
        std::thread::spawn(move || {
            let (mut s, _) = unix.accept().unwrap();
            let mut got = Vec::new();
            s.read_to_end(&mut got).unwrap();
            s.write_all(&[b"echo:".as_slice(), &got].concat()).unwrap();
        });

        let tcp = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = tcp.local_addr().unwrap().port();
        let socket = path.clone();
        std::thread::spawn(move || serve(tcp, &socket));

        let mut c = TcpStream::connect(("127.0.0.1", port)).unwrap();
        c.write_all(b"CONNECT api.example.com:443 HTTP/1.1\r\n\r\n").unwrap();
        c.shutdown(Shutdown::Write).unwrap();
        let mut out = String::new();
        c.read_to_string(&mut out).unwrap();
        assert_eq!(out, "echo:CONNECT api.example.com:443 HTTP/1.1\r\n\r\n");
    }

    #[test]
    fn it_needs_a_program_after_the_dashes() {
        assert_eq!(main(&[]), 2);
        assert_eq!(main(&["--".to_string()]), 2);
    }
}
