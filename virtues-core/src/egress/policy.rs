//! What the egress proxy lets through: which host names, and which
//! addresses those names may resolve to.
//!
//! The rules are in agents/plan/applet-shape.md under Network. A host must be
//! listed by name, an IP literal is refused, a host anyone can write to or a
//! model provider's API is refused even when listed, and a name that
//! resolves to the box, the LAN, loopback, link-local or a cloud metadata
//! address is refused after resolution, so DNS cannot point a listed name
//! back inside.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// Hosts refused even when an applet lists them, matched as the host itself
/// or any name under it. Two kinds:
///
/// - Hosts anyone can write to and read back: code hosts, paste and request
///   bins, forms, shared drives, cloud storage and API front doors, chat
///   webhooks. Data sent there reaches whoever made the page.
/// - Model and inference APIs, which accept uploads under the caller's own
///   key, so an attacker's key turns them into a place to send data.
const REFUSED_HOSTS: &[&str] = &[
    // Code and package hosts.
    "github.com",
    "githubusercontent.com",
    "gitlab.com",
    "bitbucket.org",
    "npmjs.com",
    "npmjs.org",
    "pypi.org",
    "huggingface.co",
    // Pastes, request bins and tunnels.
    "pastebin.com",
    "hastebin.com",
    "paste.ee",
    "webhook.site",
    "requestbin.com",
    "pipedream.net",
    "ngrok.io",
    "ngrok-free.app",
    "trycloudflare.com",
    // Forms, documents and shared drives.
    "forms.gle",
    "docs.google.com",
    "drive.google.com",
    "dropbox.com",
    "dropboxusercontent.com",
    "notion.site",
    // Cloud storage and API front doors anyone can create.
    "amazonaws.com",
    "storage.googleapis.com",
    "blob.core.windows.net",
    "azurewebsites.net",
    "r2.dev",
    "workers.dev",
    "vercel.app",
    "netlify.app",
    "firebaseio.com",
    "appspot.com",
    // Chat webhooks.
    "hooks.slack.com",
    "discord.com",
    "discordapp.com",
    "api.telegram.org",
    // Model and inference APIs.
    "anthropic.com",
    "claude.ai",
    "openai.com",
    "x.ai",
    "generativelanguage.googleapis.com",
    "aiplatform.googleapis.com",
    "openrouter.ai",
    "mistral.ai",
    "cohere.ai",
    "cohere.com",
    "groq.com",
    "together.xyz",
    "together.ai",
    "deepseek.com",
    "perplexity.ai",
    "replicate.com",
    "fireworks.ai",
];

/// A host name, lowercased and without a trailing dot, when it is one a
/// proxy may be asked for. Refuses IP literals, malformed names and the
/// refused list; says nothing about whether an applet listed it.
pub fn check_host(host: &str) -> Result<String, String> {
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    if bare.parse::<IpAddr>().is_ok() {
        return Err(format!("{host} is an IP address. List a host name instead."));
    }
    let valid = !host.is_empty()
        && host.len() <= 253
        && host.contains('.')
        && host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        });
    if !valid {
        return Err(format!("{host} is not a host name, such as api.weather.gov."));
    }
    if let Some(refused) = REFUSED_HOSTS
        .iter()
        .find(|r| host == **r || host.ends_with(&format!(".{r}")))
    {
        return Err(format!(
            "{host} is refused even when listed: data sent to {refused} can be read by whoever \
             controls the account it is sent to."
        ));
    }
    Ok(host)
}

/// Whether a resolved address is outside the box and its network. `own`
/// is the box's own interface addresses.
pub fn check_address(ip: IpAddr, own: &[IpAddr]) -> Result<(), String> {
    if own.contains(&ip) {
        return Err(format!("{ip} is this box."));
    }
    match ip {
        IpAddr::V4(v4) => check_v4(v4),
        IpAddr::V6(v6) => check_v6(v6, own),
    }
}

fn check_v4(ip: Ipv4Addr) -> Result<(), String> {
    let [a, b, ..] = ip.octets();
    let why = if ip.is_loopback() {
        "loopback"
    } else if ip.is_private() {
        "a private network address"
    } else if ip.is_link_local() {
        // 169.254.169.254 is the cloud metadata endpoint.
        "link-local, which includes cloud metadata"
    } else if ip.is_unspecified() || a == 0 {
        "unspecified"
    } else if ip.is_broadcast() || ip.is_multicast() {
        "broadcast or multicast"
    } else if a == 100 && (64..128).contains(&b) {
        "carrier-grade NAT, which is a private network"
    } else if a == 192 && b == 0 && ip.octets()[2] == 0 {
        "reserved for protocol use"
    } else if a == 198 && (18..20).contains(&b) {
        "reserved for benchmarking"
    } else if a >= 240 {
        "reserved"
    } else {
        return Ok(());
    };
    Err(format!("{ip} is {why}."))
}

fn check_v6(ip: Ipv6Addr, own: &[IpAddr]) -> Result<(), String> {
    let s = ip.segments();
    // Forms that carry an IPv4 address: mapped (::ffff:a.b.c.d), NAT64
    // (64:ff9b::/96) and 6to4 (2002::/16). The embedded address decides.
    let embedded = if let Some(v4) = ip.to_ipv4_mapped() {
        Some(v4)
    } else if s[0] == 0x64 && s[1] == 0xff9b && s[2..6] == [0, 0, 0, 0] {
        Some(Ipv4Addr::new((s[6] >> 8) as u8, s[6] as u8, (s[7] >> 8) as u8, s[7] as u8))
    } else if s[0] == 0x2002 {
        Some(Ipv4Addr::new((s[1] >> 8) as u8, s[1] as u8, (s[2] >> 8) as u8, s[2] as u8))
    } else {
        None
    };
    if let Some(v4) = embedded {
        return check_address(IpAddr::V4(v4), own);
    }
    let why = if ip.is_loopback() {
        "loopback"
    } else if ip.is_unspecified() {
        "unspecified"
    } else if ip.is_multicast() {
        "multicast"
    } else if (s[0] & 0xfe00) == 0xfc00 {
        "a private network address"
    } else if (s[0] & 0xffc0) == 0xfe80 {
        "link-local"
    } else {
        return Ok(());
    };
    Err(format!("{ip} is {why}."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_host_is_a_name_and_not_on_the_refused_list() {
        assert_eq!(check_host("API.Weather.gov.").unwrap(), "api.weather.gov");
        for (host, expect) in [
            ("93.184.216.34", "is an IP address"),
            ("[2606:4700::1111]", "is an IP address"),
            ("localhost", "is not a host name"),
            ("bad_host.example.com", "is not a host name"),
            ("-x.example.com", "is not a host name"),
            ("gist.github.com", "refused even when listed"),
            ("api.anthropic.com", "refused even when listed"),
            ("my-bucket.s3.amazonaws.com", "refused even when listed"),
            ("hooks.slack.com", "refused even when listed"),
        ] {
            let e = check_host(host).unwrap_err();
            assert!(e.contains(expect), "{host}: {e}");
        }
        // A suffix match is on a label boundary.
        assert!(check_host("notgithub.com").is_ok());
    }

    #[test]
    fn an_address_inside_the_box_or_its_network_is_refused() {
        let own: Vec<IpAddr> = vec!["203.0.113.7".parse().unwrap()];
        for ip in [
            "127.0.0.1",
            "10.1.2.3",
            "172.20.0.1",
            "192.168.1.10",
            "169.254.169.254",
            "100.100.100.200",
            "0.0.0.0",
            "203.0.113.7",
            "::1",
            "fd00::1",
            "fe80::1",
            "::ffff:127.0.0.1",
            "::ffff:192.168.1.10",
            "64:ff9b::a9fe:a9fe",
            "2002:c0a8:0101::1",
        ] {
            let ip: IpAddr = ip.parse().unwrap();
            assert!(check_address(ip, &own).is_err(), "{ip} must be refused");
        }
        for ip in ["93.184.216.34", "2606:4700:4700::1111", "::ffff:93.184.216.34"] {
            let ip: IpAddr = ip.parse().unwrap();
            assert!(check_address(ip, &own).is_ok(), "{ip} must be allowed");
        }
    }
}
