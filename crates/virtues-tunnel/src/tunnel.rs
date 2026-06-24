//! Public [`Tunnel`] handle, the [`TunnelStream`] duplex byte stream, and the
//! single-threaded event loop that ties WireGuard ([`WgTunnel`]) to the smoltcp
//! netstack ([`VirtualDevice`]).
//!
//! Threading model: one background thread owns all protocol state and runs a
//! poll loop. The app talks to it over channels — [`Tunnel::dial`] sends a
//! command and gets back a [`TunnelStream`] whose `Read`/`Write` impls move
//! bytes via per-stream channels. Nothing here is async, so the FFI surface
//! needs no runtime.

use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::net::IpAddr;
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::collections::HashSet;
use std::sync::mpsc::{SyncSender as MpscSyncSender, TrySendError};
use std::time::{Duration, Instant as StdInstant};

use smoltcp::iface::{Config, Interface, SocketHandle, SocketSet};
use smoltcp::socket::tcp;
use smoltcp::time::Instant;
use smoltcp::wire::{HardwareAddress, IpCidr};

use crate::netstack::VirtualDevice;
use crate::wg::WgTunnel;
use crate::{PairingBundle, TunnelError};

/// How long to wait for a dialed TCP socket to reach Established before failing.
///
/// Must stay above `candidate_count × CANDIDATE_TIMEOUT` so a dial issued at
/// connect time survives a full cycle through every candidate endpoint (≤3
/// candidates × 2.5s ≈ 7.5s, comfortably under 15s).
const DIAL_TIMEOUT: Duration = Duration::from_secs(15);
/// How long to wait for a handshake on the current candidate endpoint before
/// cycling to the next one. The box ranks candidates best-first, so the common
/// case (first candidate works) never waits.
const CANDIDATE_TIMEOUT: Duration = Duration::from_millis(2500);
/// Per-stream socket buffer (64 KiB each direction).
const SOCK_BUF: usize = 64 * 1024;
/// WG timer cadence (keepalive, handshake retransmit, expiry check).
const WG_TICK: Duration = Duration::from_millis(250);
/// Max time the loop blocks on UDP when streams are active — bounds the latency
/// between an app write and the loop noticing it. Tear-down-between-bursts (the
/// iOS usage) means there's no idle spin to worry about. A `mio::Waker` would
/// drive this to zero, but for periodic background uploads this is ample.
const ACTIVE_POLL_CAP: Duration = Duration::from_millis(5);
/// Block longer when there are no active streams (just driving WG timers).
const IDLE_POLL: Duration = Duration::from_millis(250);
/// Bound on the loop→app read channel: SOCK_BUF-sized chunks, so ~1 MiB of
/// in-flight buffering before TCP backpressure kicks in (we stop draining the
/// socket, its window closes, the box stops sending).
const READ_CHANNEL_DEPTH: usize = 16;
/// Idle read deadline for a request/response [`TunnelStream`]. If no chunk
/// arrives within this window the read returns `TimedOut` instead of blocking
/// forever. This is an *idle* timeout — it resets on every chunk — so it never
/// penalizes a slow-but-progressing transfer; it only fires when the WG path
/// has silently gone dead mid-exchange (handshake completed at dial time, then
/// the session stopped delivering and the loop never drops the read sender).
/// Without it a dead path wedges the caller indefinitely. Matches the direct
/// `URLSession` resource timeout on the Swift side. Only applies to the
/// bundled `TunnelStream` (HTTP request/response); the split half used by the
/// desktop full-duplex proxy keeps blocking semantics since it legitimately
/// idles between frames.
const READ_IDLE_TIMEOUT: Duration = Duration::from_secs(30);

/// Coarse connection state, surfaced to the app's Settings UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TunnelStatus {
    Connecting,
    Connected,
    Failed(String),
    Closed,
}

enum Command {
    Dial {
        ip: IpAddr,
        port: u16,
        reply: SyncSender<Result<TunnelStream, TunnelError>>,
    },
    Close(SocketHandle),
    Shutdown,
}

/// A live, paired tunnel. Dropping it shuts the background loop down.
///
/// `cmd_tx` is wrapped in a `Mutex` only so the whole handle is `Sync` (an
/// `mpsc::Sender` is `Send` but not `Sync`) and can be shared across the FFI as
/// an `Arc`. The lock is held only to enqueue a command, never across the
/// blocking wait for a reply.
pub struct Tunnel {
    cmd_tx: Mutex<Sender<Command>>,
    status: Arc<Mutex<TunnelStatus>>,
    join: Option<JoinHandle<()>>,
}

impl Tunnel {
    /// Bring up the tunnel from a pairing bundle and the device's base64 private
    /// key. Returns immediately; the handshake proceeds in the background. Use
    /// [`status`](Self::status) to observe progress, or just [`dial`](Self::dial)
    /// (which blocks until connected or times out).
    pub fn connect(bundle: &PairingBundle, private_key_b64: &str) -> Result<Self, TunnelError> {
        let client_ip = parse_addr(&bundle.wg.client_address, "client_address")?;
        let server_ip = parse_addr(&bundle.wg.server_address, "server_address")?;

        let wg = WgTunnel::new(&bundle.wg, private_key_b64)?;
        let udp = wg.udp_clone().map_err(TunnelError::from)?;
        // Initial timeout; the event loop resets it each iteration via poll_timeout.
        udp.set_read_timeout(Some(IDLE_POLL)).map_err(TunnelError::from)?;

        let status = Arc::new(Mutex::new(TunnelStatus::Connecting));
        let (cmd_tx, cmd_rx) = mpsc::channel::<Command>();
        let loop_status = status.clone();
        let loop_cmd_tx = cmd_tx.clone();

        let join = std::thread::Builder::new()
            .name("virtues-tunnel".into())
            .spawn(move || {
                EventLoop::new(wg, udp, client_ip, server_ip, loop_status, loop_cmd_tx)
                    .run(cmd_rx);
            })
            .map_err(TunnelError::from)?;

        Ok(Self {
            cmd_tx: Mutex::new(cmd_tx),
            status,
            join: Some(join),
        })
    }

    /// Enqueue a command, returning `NotConnected` if the loop has stopped.
    fn send_cmd(&self, cmd: Command) -> Result<(), TunnelError> {
        self.cmd_tx
            .lock()
            .expect("cmd mutex")
            .send(cmd)
            .map_err(|_| TunnelError::NotConnected)
    }

    /// Open a TCP connection to `(ip, port)` *inside the tunnel* (i.e. the box's
    /// ULA + http_port). Blocks until connected or [`DIAL_TIMEOUT`] elapses.
    pub fn dial(&self, ip: IpAddr, port: u16) -> Result<TunnelStream, TunnelError> {
        // Depth 1 so the loop's reply.send() never blocks the event loop even if
        // the dialer has already timed out and stopped receiving.
        let (reply, reply_rx) = mpsc::sync_channel(1);
        self.send_cmd(Command::Dial { ip, port, reply })?;
        reply_rx
            .recv_timeout(DIAL_TIMEOUT + Duration::from_secs(1))
            .map_err(|_| TunnelError::Dial {
                addr: format!("{ip}:{port}"),
                reason: "event loop did not respond".into(),
            })?
    }

    /// Current connection status.
    pub fn status(&self) -> TunnelStatus {
        self.status.lock().expect("status mutex").clone()
    }
}

impl Drop for Tunnel {
    fn drop(&mut self) {
        let _ = self.send_cmd(Command::Shutdown);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn parse_addr(s: &str, field: &str) -> Result<IpAddr, TunnelError> {
    // Tolerate a trailing prefix length, e.g. "fd00:5654::2/128".
    let base = s.split('/').next().unwrap_or(s);
    base.parse()
        .map_err(|e| TunnelError::BadBundle(format!("{field} '{s}': {e}")))
}

// ─────────────────────────────────────────────────────────────────────────────
// Event loop
// ─────────────────────────────────────────────────────────────────────────────

struct Conn {
    /// loop → app (decapsulated payload). Bounded for backpressure: when it's
    /// full the loop stops draining the socket, closing the TCP window.
    read_tx: MpscSyncSender<Vec<u8>>,
    /// app → loop (bytes to send).
    write_rx: Receiver<Vec<u8>>,
    /// Bytes accepted from `write_rx` but not yet fully handed to the socket.
    pending: Vec<u8>,
    /// Held until the socket reaches Established, then fired with the stream.
    connect_reply: Option<SyncSender<Result<TunnelStream, TunnelError>>>,
    established: bool,
    opened: StdInstant,
    /// The stream handed to the app, parked until connected.
    stream: Option<TunnelStream>,
    /// Local TCP port this conn holds (freed on teardown so it can be reused).
    local_port: u16,
}

struct EventLoop {
    wg: WgTunnel,
    udp: std::net::UdpSocket,
    iface: Interface,
    device: VirtualDevice,
    sockets: SocketSet<'static>,
    conns: HashMap<SocketHandle, Conn>,
    status: Arc<Mutex<TunnelStatus>>,
    cmd_tx: Sender<Command>,
    next_local_port: u16,
    /// Local ports currently bound by a live conn — alloc skips these so a wrap
    /// can't reissue a port still in use / lingering in TIME_WAIT.
    used_ports: HashSet<u16>,
    /// When the current candidate's handshake window expires and we cycle to the
    /// next endpoint. Reset on every advance and on a full rehandshake.
    candidate_deadline: StdInstant,
    /// True once a session established — stop cycling and lock onto the winning
    /// endpoint. Only `is_expired` (the session later died) clears it.
    locked: bool,
}

impl EventLoop {
    fn new(
        wg: WgTunnel,
        udp: std::net::UdpSocket,
        client_ip: IpAddr,
        server_ip: IpAddr,
        status: Arc<Mutex<TunnelStatus>>,
        cmd_tx: Sender<Command>,
    ) -> Self {
        let mut device = VirtualDevice::new();
        let config = Config::new(HardwareAddress::Ip);
        let mut iface = Interface::new(config, &mut device, Instant::now());
        // Give the interface the client ULA. /64 so the box's ULA is on-link
        // (no gateway needed on this point-to-point link).
        iface.update_ip_addrs(|addrs| {
            let _ = addrs.push(IpCidr::new(client_ip.into(), 64));
        });
        // Default route via the box so its address is reachable even if it ever
        // sits outside the client's /64 (belt-and-suspenders on top of on-link).
        match server_ip {
            IpAddr::V6(v6) => {
                let _ = iface.routes_mut().add_default_ipv6_route(v6);
            }
            IpAddr::V4(v4) => {
                let _ = iface.routes_mut().add_default_ipv4_route(v4);
            }
        }

        Self {
            wg,
            udp,
            iface,
            device,
            sockets: SocketSet::new(Vec::new()),
            conns: HashMap::new(),
            status,
            cmd_tx,
            next_local_port: 49152,
            used_ports: HashSet::new(),
            candidate_deadline: StdInstant::now() + CANDIDATE_TIMEOUT,
            locked: false,
        }
    }

    /// Adopt a fresh read clone after a cross-family socket rebind, re-arming its
    /// read timeout (per-fd, so a new clone starts with none). The old clone is
    /// dropped; the underlying kernel socket died with the `WgTunnel`'s replaced
    /// handle.
    fn adopt_socket(&mut self, new_udp: std::net::UdpSocket) {
        let _ = new_udp.set_read_timeout(Some(self.poll_timeout()));
        self.udp = new_udp;
    }

    fn set_status(&self, s: TunnelStatus) {
        let mut g = self.status.lock().expect("status mutex");
        if *g != s {
            *g = s;
        }
    }

    fn run(mut self, cmd_rx: Receiver<Command>) {
        self.wg.initiate();
        let mut last_tick = StdInstant::now();
        self.candidate_deadline = StdInstant::now() + CANDIDATE_TIMEOUT;
        let mut buf = vec![0u8; 2048];

        loop {
            // 1) Block for inbound WG datagrams, up to a state-dependent timeout
            //    (short when streams are active so app writes are picked up
            //    promptly; longer when idle). Then opportunistically drain the
            //    rest of the UDP backlog non-blocking.
            let _ = self.udp.set_read_timeout(Some(self.poll_timeout()));
            match self.udp.recv(&mut buf) {
                Ok(n) if n > 0 => self.ingest_datagram(&buf[..n]),
                _ => {}
            }
            let _ = self.udp.set_read_timeout(Some(Duration::from_millis(1)));
            while let Ok(n2) = self.udp.recv(&mut buf) {
                if n2 == 0 {
                    break;
                }
                self.ingest_datagram(&buf[..n2]);
            }

            // 2) Handle commands.
            let mut shutdown = false;
            while let Ok(cmd) = cmd_rx.try_recv() {
                match cmd {
                    Command::Dial { ip, port, reply } => self.handle_dial(ip, port, reply),
                    Command::Close(h) => self.handle_close(h),
                    Command::Shutdown => {
                        shutdown = true;
                        break;
                    }
                }
            }
            if shutdown {
                break;
            }

            // 3) Move app writes into sockets, run the stack, flush what it emits.
            self.pump_writes();
            let _ = self.iface.poll(Instant::now(), &mut self.device, &mut self.sockets);
            self.flush_outbound();

            // 4) Service sockets (connected replies, reads, cleanup).
            self.service_sockets();

            // 5) Drive WG timers periodically: re-handshake if boringtun gave up,
            //    tick keepalive/retransmit, and reflect liveness into status.
            if last_tick.elapsed() >= WG_TICK {
                if self.wg.is_established() {
                    // A session landed on the current candidate — lock onto it
                    // and stop cycling. (Checked first so a handshake that
                    // completes mid-window is never cycled away.)
                    self.locked = true;
                    self.set_status(TunnelStatus::Connected);
                } else if self.wg.is_expired() {
                    // boringtun gave up on this path (or a locked session died).
                    // Restart cycling from the preferred candidate.
                    match self.wg.rehandshake() {
                        Ok(Some(new_udp)) => self.adopt_socket(new_udp),
                        Ok(None) => {}
                        Err(e) => tracing::debug!("rehandshake rebind failed: {e:?}"),
                    }
                    self.locked = false;
                    self.candidate_deadline = StdInstant::now() + CANDIDATE_TIMEOUT;
                    self.set_status(TunnelStatus::Connecting);
                } else if !self.locked
                    && self.wg.candidate_count() > 1
                    && StdInstant::now() >= self.candidate_deadline
                {
                    // Handshake on this candidate hasn't landed in time — try the
                    // next endpoint.
                    match self.wg.advance_candidate() {
                        Ok(Some(new_udp)) => self.adopt_socket(new_udp),
                        Ok(None) => {}
                        Err(e) => tracing::debug!("advance_candidate rebind failed: {e:?}"),
                    }
                    self.candidate_deadline = StdInstant::now() + CANDIDATE_TIMEOUT;
                    self.set_status(TunnelStatus::Connecting);
                } else {
                    self.set_status(TunnelStatus::Connecting);
                }
                self.wg.tick();
                self.flush_outbound();
                self.expire_pending_dials();
                last_tick = StdInstant::now();
            }
        }

        self.set_status(TunnelStatus::Closed);
    }

    /// Decapsulate one WG datagram and queue any inbound IP packets for smoltcp.
    fn ingest_datagram(&mut self, datagram: &[u8]) {
        let mut ips = Vec::new();
        self.wg.process_datagram(datagram, &mut ips);
        for p in ips {
            self.device.inbound.push_back(p);
        }
    }

    /// Encapsulate every IP packet smoltcp produced and send it to the box.
    fn flush_outbound(&mut self) {
        while let Some(pkt) = self.device.outbound.pop_front() {
            self.wg.send_ip(&pkt);
        }
    }

    /// How long to block on the UDP socket this iteration: short while streams
    /// are active (so app writes and TCP retransmits are serviced promptly),
    /// longer when idle (just driving WG keepalive/timers).
    fn poll_timeout(&self) -> Duration {
        if self.conns.is_empty() {
            IDLE_POLL
        } else {
            ACTIVE_POLL_CAP
        }
    }

    fn handle_dial(
        &mut self,
        ip: IpAddr,
        port: u16,
        reply: SyncSender<Result<TunnelStream, TunnelError>>,
    ) {
        let mut socket = tcp::Socket::new(
            tcp::SocketBuffer::new(vec![0u8; SOCK_BUF]),
            tcp::SocketBuffer::new(vec![0u8; SOCK_BUF]),
        );
        let local_port = self.alloc_port();
        let cx = self.iface.context();
        if let Err(e) = socket.connect(cx, (ip, port), local_port) {
            self.used_ports.remove(&local_port);
            let _ = reply.send(Err(TunnelError::Dial {
                addr: format!("{ip}:{port}"),
                reason: format!("{e}"),
            }));
            return;
        }
        let handle = self.sockets.add(socket);

        let (write_tx, write_rx) = mpsc::channel::<Vec<u8>>();
        let (read_tx, read_rx) = mpsc::sync_channel::<Vec<u8>>(READ_CHANNEL_DEPTH);
        let stream = TunnelStream {
            handle,
            write_tx,
            read_rx,
            read_carry: Vec::new(),
            read_idle_timeout: READ_IDLE_TIMEOUT,
            cmd_tx: self.cmd_tx.clone(),
            closed: false,
        };

        self.conns.insert(
            handle,
            Conn {
                read_tx,
                write_rx,
                pending: Vec::new(),
                connect_reply: Some(reply),
                established: false,
                opened: StdInstant::now(),
                stream: Some(stream),
                local_port,
            },
        );
    }

    /// Remove a conn: free its port, close + drop the socket, and drop the
    /// channels (dropping `read_tx` is what signals EOF to the app's reader).
    /// Idempotent — a second call for the same handle is a no-op.
    fn teardown_conn(&mut self, handle: SocketHandle) {
        if let Some(conn) = self.conns.remove(&handle) {
            self.used_ports.remove(&conn.local_port);
            self.sockets.get_mut::<tcp::Socket>(handle).close();
            self.sockets.remove(handle);
        }
    }

    fn handle_close(&mut self, handle: SocketHandle) {
        self.teardown_conn(handle);
    }

    fn pump_writes(&mut self) {
        for (handle, conn) in self.conns.iter_mut() {
            // Pull all queued app writes into the pending buffer.
            while let Ok(chunk) = conn.write_rx.try_recv() {
                conn.pending.extend_from_slice(&chunk);
            }
            if conn.pending.is_empty() {
                continue;
            }
            let sock = self.sockets.get_mut::<tcp::Socket>(*handle);
            if sock.can_send() {
                match sock.send_slice(&conn.pending) {
                    Ok(sent) if sent > 0 => {
                        conn.pending.drain(..sent);
                    }
                    _ => {}
                }
            }
        }
    }

    fn service_sockets(&mut self) {
        let mut to_close = Vec::new();

        for (handle, conn) in self.conns.iter_mut() {
            let sock = self.sockets.get_mut::<tcp::Socket>(*handle);

            // Connection just came up → hand the stream back to the dialer.
            if !conn.established && sock.may_send() {
                conn.established = true;
                if let (Some(reply), Some(stream)) =
                    (conn.connect_reply.take(), conn.stream.take())
                {
                    let _ = reply.send(Ok(stream));
                }
            }

            // Drain the receive buffer to the app, with backpressure: if the
            // app's read channel is full, stop draining so the socket's RX
            // buffer fills and the TCP window closes (the box stops sending)
            // rather than buffering unboundedly in the channel.
            let mut backpressured = false;
            while sock.can_recv() {
                let mut chunk = Vec::new();
                let r = sock.recv(|data| {
                    chunk.extend_from_slice(data);
                    (data.len(), ())
                });
                if r.is_err() || chunk.is_empty() {
                    break;
                }
                match conn.read_tx.try_send(chunk) {
                    Ok(()) => {}
                    Err(TrySendError::Full(_)) => {
                        backpressured = true;
                        break;
                    }
                    Err(TrySendError::Disconnected(_)) => {
                        // App dropped the stream → tear down.
                        to_close.push(*handle);
                        break;
                    }
                }
            }

            // EOF / teardown. Only when not backpressured (else we'd drop data
            // still sitting in the socket buffer):
            //   * peer closed its send side and we've drained it (`!may_recv`)
            //     and our writes are flushed → drop the conn, which drops
            //     `read_tx` and surfaces EOF to the reader; or
            //   * the socket is fully dead.
            if !backpressured
                && ((conn.established && !sock.may_recv() && conn.pending.is_empty())
                    || !sock.is_active())
            {
                to_close.push(*handle);
            }
        }

        for h in to_close {
            self.teardown_conn(h);
        }
    }

    fn expire_pending_dials(&mut self) {
        let mut expired = Vec::new();
        for (handle, conn) in self.conns.iter_mut() {
            if !conn.established && conn.opened.elapsed() > DIAL_TIMEOUT {
                if let Some(reply) = conn.connect_reply.take() {
                    let _ = reply.send(Err(TunnelError::HandshakeTimeout));
                }
                expired.push(*handle);
            }
        }
        // Note: we don't latch `Failed` here — status is driven by WG liveness
        // in the run loop. A single dial timeout (box up, port closed/slow)
        // shouldn't mark the whole tunnel failed; the dialer already got the
        // error via its reply channel.
        for h in expired {
            self.teardown_conn(h);
        }
    }

    /// Allocate a local TCP port not currently in use, scanning forward and
    /// wrapping. Records it in `used_ports`; freed on teardown.
    fn alloc_port(&mut self) -> u16 {
        for _ in 0..(60000 - 49152) {
            let p = self.next_local_port;
            self.next_local_port = if p >= 60000 { 49152 } else { p + 1 };
            if self.used_ports.insert(p) {
                return p;
            }
        }
        // All ports somehow taken (>10k concurrent streams) — reuse current.
        self.next_local_port
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Stream
// ─────────────────────────────────────────────────────────────────────────────

/// A duplex byte stream over one TCP connection inside the tunnel. Implements
/// [`Read`] + [`Write`] so the app (or its FFI shim) can speak plain HTTP.
pub struct TunnelStream {
    handle: SocketHandle,
    write_tx: Sender<Vec<u8>>,
    read_rx: Receiver<Vec<u8>>,
    /// Leftover bytes from a recv that didn't fit the caller's buffer.
    read_carry: Vec<u8>,
    /// Idle deadline applied to each blocking recv (see [`READ_IDLE_TIMEOUT`]).
    read_idle_timeout: Duration,
    cmd_tx: Sender<Command>,
    closed: bool,
}

impl Read for TunnelStream {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if self.read_carry.is_empty() {
            match self.read_rx.recv_timeout(self.read_idle_timeout) {
                Ok(bytes) => self.read_carry = bytes,
                // No data for the whole idle window → the path is dead. Surface
                // a timeout so the caller unwinds (dropping the stream, which
                // lets the tunnel tear down) instead of blocking forever.
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "tunnel read idle timeout",
                    ));
                }
                // Sender dropped → clean EOF.
                Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(0),
            }
        }
        let n = out.len().min(self.read_carry.len());
        out[..n].copy_from_slice(&self.read_carry[..n]);
        self.read_carry.drain(..n);
        Ok(n)
    }
}

impl Write for TunnelStream {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.write_tx
            .send(data.to_vec())
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "tunnel closed"))?;
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl TunnelStream {
    /// Split into independent read + write halves so an async bridge (e.g. the
    /// desktop proxy's loopback forwarder) can run reads and writes on separate
    /// threads concurrently — required for full-duplex traffic like WebSockets.
    ///
    /// CONTRACT: the **write half owns the connection's lifetime**. Dropping it
    /// sends exactly one `Close`, which causes the read half to return EOF. The
    /// read half has no `Drop` of its own. This is intentional and is how the
    /// forwarder tears a connection down (the browser→box copy drops the write
    /// half on EOF to end the box→browser copy). The consequence a caller must
    /// respect: **do not keep reading after dropping the write half** — the
    /// split is for concurrent use within ONE connection's lifetime, not for
    /// giving the two halves independent lifetimes.
    pub fn into_split(self) -> (TunnelReadHalf, TunnelWriteHalf) {
        // Move fields out without firing the bundled `Drop` (which would send a
        // premature `Close`). SocketHandle is `Copy`; the rest are read once.
        let me = std::mem::ManuallyDrop::new(self);
        // SAFETY: each field is read exactly once and the original is forgotten.
        let write_tx = unsafe { std::ptr::read(&me.write_tx) };
        let read_rx = unsafe { std::ptr::read(&me.read_rx) };
        let read_carry = unsafe { std::ptr::read(&me.read_carry) };
        let cmd_tx = unsafe { std::ptr::read(&me.cmd_tx) };
        let handle = me.handle;
        (
            TunnelReadHalf { read_rx, read_carry },
            TunnelWriteHalf { write_tx, cmd_tx, handle, closed: false },
        )
    }
}

impl Drop for TunnelStream {
    fn drop(&mut self) {
        if !self.closed {
            self.closed = true;
            let _ = self.cmd_tx.send(Command::Close(self.handle));
        }
    }
}

/// Read half of a split [`TunnelStream`]. See [`TunnelStream::into_split`].
pub struct TunnelReadHalf {
    read_rx: Receiver<Vec<u8>>,
    read_carry: Vec<u8>,
}

impl Read for TunnelReadHalf {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if self.read_carry.is_empty() {
            match self.read_rx.recv() {
                Ok(bytes) => self.read_carry = bytes,
                Err(_) => return Ok(0), // sender dropped → clean EOF
            }
        }
        let n = out.len().min(self.read_carry.len());
        out[..n].copy_from_slice(&self.read_carry[..n]);
        self.read_carry.drain(..n);
        Ok(n)
    }
}

/// Write half of a split [`TunnelStream`]. Owns the socket close-on-drop.
pub struct TunnelWriteHalf {
    write_tx: Sender<Vec<u8>>,
    cmd_tx: Sender<Command>,
    handle: SocketHandle,
    closed: bool,
}

impl Write for TunnelWriteHalf {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.write_tx
            .send(data.to_vec())
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "tunnel closed"))?;
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for TunnelWriteHalf {
    fn drop(&mut self) {
        if !self.closed {
            self.closed = true;
            let _ = self.cmd_tx.send(Command::Close(self.handle));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate_keypair;
    use base64::Engine;
    use std::net::UdpSocket;
    use virtues_protocol::{PairingBundle, WgParams};

    fn b64_32(b: u8) -> String {
        base64::engine::general_purpose::STANDARD.encode([b; 32])
    }

    /// Bring the tunnel up against a fake box that never answers, then drop it.
    /// Exercises the real event-loop thread: spawn, handshake initiation, status,
    /// and clean shutdown (Drop must join without hanging or panicking).
    #[test]
    fn connect_status_and_clean_shutdown() {
        let fake_box = UdpSocket::bind("[::1]:0").unwrap();
        let endpoint = fake_box.local_addr().unwrap();
        let client = generate_keypair();
        let server = generate_keypair();

        let bundle = PairingBundle {
            bearer: "test".into(),
            wg: WgParams {
                server_public_key: server.public_key_b64,
                server_endpoint: endpoint.to_string(),
                server_endpoints: Vec::new(),
                preshared_key: b64_32(7),
                client_address: "fd00:5654::2".into(),
                server_address: "fd00:5654::1".into(),
                allowed_ips: vec!["fd00:5654::1/128".into()],
                client_private_key: None,
            },
            internal_host: "virtues.internal".into(),
            internal_ip: "fd00:5654::1".into(),
            http_port: 8000,
        };

        let tunnel = Tunnel::connect(&bundle, &client.private_key_b64).expect("connect");
        // No real peer → never establishes → stays Connecting.
        assert_eq!(tunnel.status(), TunnelStatus::Connecting);
        std::thread::sleep(Duration::from_millis(60));
        // Drop joins the background loop; must return promptly without panic.
        drop(tunnel);
    }

    /// The tunnel must cycle off a dead first candidate and try the next one.
    /// Candidate #1 is a black-hole socket (bound, never read → never answers);
    /// candidate #2 is a recorder. Neither completes a real Noise handshake, so
    /// after `CANDIDATE_TIMEOUT` the loop advances to #2 and sends the handshake
    /// initiation there — which the recorder observes, proving the cycle worked.
    #[test]
    fn cycles_to_second_candidate_when_first_is_dead() {
        // Hold both sockets for the test's lifetime so their ports stay reserved.
        let dead = UdpSocket::bind("[::1]:0").unwrap();
        let recorder = UdpSocket::bind("[::1]:0").unwrap();
        let dead_ep = dead.local_addr().unwrap();
        let rec_ep = recorder.local_addr().unwrap();

        let client = generate_keypair();
        let server = generate_keypair();
        let bundle = PairingBundle {
            bearer: "test".into(),
            wg: WgParams {
                server_public_key: server.public_key_b64,
                // Primary is the dead one; the live recorder is the 2nd candidate.
                server_endpoint: dead_ep.to_string(),
                server_endpoints: vec![dead_ep.to_string(), rec_ep.to_string()],
                preshared_key: b64_32(7),
                client_address: "fd00:5654::2".into(),
                server_address: "fd00:5654::1".into(),
                allowed_ips: vec!["fd00:5654::1/128".into()],
                client_private_key: None,
            },
            internal_host: "virtues.internal".into(),
            internal_ip: "fd00:5654::1".into(),
            http_port: 8000,
        };

        let tunnel = Tunnel::connect(&bundle, &client.private_key_b64).expect("connect");

        // Within ~CANDIDATE_TIMEOUT + slack the loop should have cycled to the
        // recorder and sent it the handshake initiation.
        recorder
            .set_read_timeout(Some(CANDIDATE_TIMEOUT + Duration::from_secs(2)))
            .unwrap();
        let mut buf = [0u8; 2048];
        let got = recorder.recv(&mut buf);
        drop(tunnel);
        assert!(
            matches!(got, Ok(n) if n > 0),
            "recorder (2nd candidate) never received a datagram — loop did not cycle: {got:?}"
        );
    }
}
