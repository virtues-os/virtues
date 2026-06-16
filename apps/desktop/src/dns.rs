//! Minimal UDP DNS server for the `.virtues` TLD.
//!
//! Answers `*.virtues` AAAA queries with the WireGuard server's ULA address
//! so that `http://servername.virtues:8000` routes through the tunnel.
//! All other queries receive NXDOMAIN.
//!
//! Bound to `127.0.0.1:5354` (unprivileged port). macOS
//! `/etc/resolver/virtues` delegates `.virtues` lookups here:
//!
//! ```text
//! nameserver 127.0.0.1
//! port 5354
//! ```
//!
//! That file is written by `virtues-client daemon` (runs as root) on startup.

use std::net::{IpAddr, Ipv6Addr, SocketAddr};

use anyhow::{Context, Result};
use tokio::net::UdpSocket;

/// Port the DNS server listens on. Non-privileged; `/etc/resolver/virtues`
/// points mDNSResponder here.
const DNS_PORT: u16 = 5354;

const QTYPE_A: u16 = 1;
const QTYPE_AAAA: u16 = 28;
const QTYPE_ANY: u16 = 255;

/// Run the `.virtues` DNS server forever. Returns on socket bind error only.
///
/// `server_ip` is the WireGuard internal address from the paired bundle
/// (`bundle.internal_ip`). All `*.virtues` AAAA queries resolve to it.
pub async fn run_dns_server(server_ip: IpAddr) -> Result<()> {
    let server_v6: Ipv6Addr = match server_ip {
        IpAddr::V6(v) => v,
        IpAddr::V4(v) => v.to_ipv6_mapped(),
    };

    let bind_addr: SocketAddr = format!("127.0.0.1:{DNS_PORT}").parse().unwrap();
    let sock = UdpSocket::bind(bind_addr)
        .await
        .with_context(|| format!("bind DNS socket on {bind_addr}"))?;

    tracing::info!(addr = %bind_addr, server_ip = %server_ip, "virtues DNS server listening");
    eprintln!("DNS server listening on {bind_addr} → *.virtues = {server_ip}");

    let mut buf = [0u8; 512];
    loop {
        let (n, src) = match sock.recv_from(&mut buf).await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("DNS recv error: {e}");
                continue;
            }
        };
        let packet = &buf[..n];
        if let Some(resp) = handle_query(packet, server_v6) {
            if let Err(e) = sock.send_to(&resp, src).await {
                tracing::warn!("DNS send error to {src}: {e}");
            }
        }
    }
}

fn handle_query(packet: &[u8], server_ip: Ipv6Addr) -> Option<Vec<u8>> {
    if packet.len() < 12 {
        return None;
    }

    let id = u16::from_be_bytes([packet[0], packet[1]]);
    let qr = (packet[2] & 0x80) != 0;
    if qr {
        return None; // ignore responses
    }

    let qdcount = u16::from_be_bytes([packet[4], packet[5]]) as usize;
    if qdcount != 1 {
        return None;
    }

    let (qname, qname_end) = parse_qname(packet, 12)?;
    if qname_end + 4 > packet.len() {
        return None;
    }
    let qtype = u16::from_be_bytes([packet[qname_end], packet[qname_end + 1]]);
    let qclass = u16::from_be_bytes([packet[qname_end + 2], packet[qname_end + 3]]);

    // Question section bytes to echo back in the response.
    let question_bytes = &packet[12..qname_end + 4];

    let is_virtues = qname.to_ascii_lowercase().ends_with(".virtues")
        || qname.to_ascii_lowercase() == "virtues";

    if !is_virtues || qclass != 1 {
        // NXDOMAIN — not a .virtues query or not IN class
        return Some(build_response(id, question_bytes, None));
    }

    match qtype {
        QTYPE_AAAA | QTYPE_ANY => {
            // Positive AAAA response
            Some(build_response(id, question_bytes, Some(server_ip)))
        }
        QTYPE_A => {
            // No IPv4 record, but domain exists: NOERROR with 0 answers
            Some(build_noerror_empty(id, question_bytes))
        }
        _ => {
            // NXDOMAIN for other types
            Some(build_response(id, question_bytes, None))
        }
    }
}

/// Parse a DNS QNAME (length-prefixed labels, null-terminated) at `pos`.
/// Returns `(dotted_name, byte_after_terminal_zero)` or None on malformed.
fn parse_qname(buf: &[u8], pos: usize) -> Option<(String, usize)> {
    let mut name = String::new();
    let mut i = pos;
    loop {
        if i >= buf.len() {
            return None;
        }
        let len = buf[i] as usize;
        i += 1;
        if len == 0 {
            break;
        }
        // Compression pointer — not expected in queries, reject.
        if len & 0xC0 == 0xC0 {
            return None;
        }
        if i + len > buf.len() {
            return None;
        }
        if !name.is_empty() {
            name.push('.');
        }
        name.push_str(&String::from_utf8_lossy(&buf[i..i + len]));
        i += len;
    }
    Some((name, i))
}

/// Build a DNS response with one AAAA answer (Some) or NXDOMAIN (None).
fn build_response(id: u16, question: &[u8], answer_ip: Option<Ipv6Addr>) -> Vec<u8> {
    let ancount: u16 = if answer_ip.is_some() { 1 } else { 0 };
    let rcode: u16 = if answer_ip.is_none() { 3 } else { 0 }; // 3 = NXDOMAIN

    // Flags: QR=1, AA=1, RD=1, RA=0, RCODE
    // 0x8000 QR, 0x0400 AA, 0x0100 RD
    let flags: u16 = 0x8000 | 0x0400 | 0x0100 | rcode;

    let mut resp = Vec::with_capacity(12 + question.len() + 28);
    resp.extend_from_slice(&id.to_be_bytes());
    resp.extend_from_slice(&flags.to_be_bytes());
    resp.extend_from_slice(&1u16.to_be_bytes()); // QDCOUNT
    resp.extend_from_slice(&ancount.to_be_bytes());
    resp.extend_from_slice(&0u16.to_be_bytes()); // NSCOUNT
    resp.extend_from_slice(&0u16.to_be_bytes()); // ARCOUNT
    resp.extend_from_slice(question);

    if let Some(ip) = answer_ip {
        // NAME: compression pointer → offset 12 (start of question section)
        resp.extend_from_slice(&[0xC0, 0x0C]);
        resp.extend_from_slice(&QTYPE_AAAA.to_be_bytes()); // TYPE AAAA
        resp.extend_from_slice(&1u16.to_be_bytes()); // CLASS IN
        resp.extend_from_slice(&60u32.to_be_bytes()); // TTL 60s
        resp.extend_from_slice(&16u16.to_be_bytes()); // RDLENGTH
        resp.extend_from_slice(&ip.octets()); // RDATA
    }

    resp
}

/// Build a NOERROR response with zero answers (for A queries on a AAAA-only name).
fn build_noerror_empty(id: u16, question: &[u8]) -> Vec<u8> {
    let flags: u16 = 0x8000 | 0x0400 | 0x0100; // QR=1, AA=1, RD=1, RCODE=0
    let mut resp = Vec::with_capacity(12 + question.len());
    resp.extend_from_slice(&id.to_be_bytes());
    resp.extend_from_slice(&flags.to_be_bytes());
    resp.extend_from_slice(&1u16.to_be_bytes()); // QDCOUNT
    resp.extend_from_slice(&0u16.to_be_bytes()); // ANCOUNT
    resp.extend_from_slice(&0u16.to_be_bytes()); // NSCOUNT
    resp.extend_from_slice(&0u16.to_be_bytes()); // ARCOUNT
    resp.extend_from_slice(question);
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_query(qname: &str, qtype: u16) -> Vec<u8> {
        let mut pkt = vec![
            0x12, 0x34, // ID = 0x1234
            0x01, 0x00, // FLAGS: standard query, RD=1
            0x00, 0x01, // QDCOUNT=1
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // ANCOUNT/NSCOUNT/ARCOUNT
        ];
        // Encode QNAME
        for label in qname.split('.') {
            let b = label.as_bytes();
            pkt.push(b.len() as u8);
            pkt.extend_from_slice(b);
        }
        pkt.push(0); // terminal zero
        pkt.extend_from_slice(&qtype.to_be_bytes());
        pkt.extend_from_slice(&1u16.to_be_bytes()); // QCLASS IN
        pkt
    }

    const SERVER: Ipv6Addr = Ipv6Addr::new(0xfd00, 0x5654, 0, 0, 0, 0, 0, 1);

    #[test]
    fn aaaa_query_for_virtues_returns_answer() {
        let q = make_query("myserver.virtues", QTYPE_AAAA);
        let resp = handle_query(&q, SERVER).unwrap();
        // ANCOUNT should be 1
        let ancount = u16::from_be_bytes([resp[6], resp[7]]);
        assert_eq!(ancount, 1);
        // Last 16 bytes should be the server IP
        assert_eq!(&resp[resp.len() - 16..], &SERVER.octets());
    }

    #[test]
    fn non_virtues_query_returns_nxdomain() {
        let q = make_query("example.com", QTYPE_AAAA);
        let resp = handle_query(&q, SERVER).unwrap();
        let flags = u16::from_be_bytes([resp[2], resp[3]]);
        assert_eq!(flags & 0x000F, 3, "expected RCODE=3 (NXDOMAIN)");
        let ancount = u16::from_be_bytes([resp[6], resp[7]]);
        assert_eq!(ancount, 0);
    }

    #[test]
    fn a_query_for_virtues_returns_noerror_no_answers() {
        let q = make_query("myserver.virtues", QTYPE_A);
        let resp = handle_query(&q, SERVER).unwrap();
        let flags = u16::from_be_bytes([resp[2], resp[3]]);
        assert_eq!(flags & 0x000F, 0, "expected RCODE=0 (NOERROR)");
        let ancount = u16::from_be_bytes([resp[6], resp[7]]);
        assert_eq!(ancount, 0);
    }

    #[test]
    fn any_query_for_virtues_returns_aaaa() {
        let q = make_query("anything.virtues", QTYPE_ANY);
        let resp = handle_query(&q, SERVER).unwrap();
        let ancount = u16::from_be_bytes([resp[6], resp[7]]);
        assert_eq!(ancount, 1);
    }

    #[test]
    fn response_echoes_query_id() {
        let q = make_query("x.virtues", QTYPE_AAAA);
        let resp = handle_query(&q, SERVER).unwrap();
        assert_eq!(&resp[0..2], &[0x12, 0x34]);
    }

    #[test]
    fn ignores_response_packets() {
        let mut q = make_query("x.virtues", QTYPE_AAAA);
        q[2] |= 0x80; // set QR bit → this is a response
        assert!(handle_query(&q, SERVER).is_none());
    }
}
