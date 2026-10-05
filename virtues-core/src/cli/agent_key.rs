//! `virtues agent-key` and `virtues agent-exec`: how an outside agent (Claude
//! Code, Codex) reaches a box over SSH without getting a shell.
//!
//! The key belongs to its own user, `virtues-agent`, never to the owner's
//! login or to `virtues`. The service user has passwordless sudo, so a key
//! there is a key to root; a key on the owner's login is a shell. The agent
//! user has no database role, no sudo and no groups, and every line in its
//! `authorized_keys` is `restrict,command="virtues agent-exec --key <name>"`:
//! whatever the agent asks to run, sshd runs `agent-exec`, which reads the
//! request from `SSH_ORIGINAL_COMMAND`, allows only the data verbs, and sends
//! each to the server's console door over loopback (`server/api/console.rs`).
//! The server runs it through the executor and its restricted roles, so the
//! roles are the boundary the agent meets. The plan is
//! agents/plan/cli-data-verbs-plan.md.

use std::path::{Path, PathBuf};

use base64::Engine;
use sha2::{Digest, Sha256};
use tokio::process::Command;

use super::types::{AgentKeyCmd, AppletCmd, Cli, Commands};
use super::ui;

const AGENT_USER: &str = "virtues-agent";
const AGENT_HOME: &str = "/var/lib/virtues-agent";
/// The stable path `virtues upgrade` swaps in place; never a release slot.
const BINARY: &str = "/usr/local/bin/virtues";
/// Marks the lines this command owns, so `ls`/`rm` never touch a key someone
/// added by hand.
const COMMENT_PREFIX: &str = "virtues-agent:";

/// The verbs an agent key may run. Lifecycle verbs (`upgrade`, `reset`,
/// `backup`) are the owner's, at the box.
const AGENT_VERBS: &[&str] = &["query", "search", "schema", "write", "applet", "page"];

const KEY_TYPES: &[&str] = &[
    "ssh-ed25519",
    "sk-ssh-ed25519@openssh.com",
    "ecdsa-sha2-nistp256",
    "ecdsa-sha2-nistp384",
    "ecdsa-sha2-nistp521",
    "sk-ecdsa-sha2-nistp256@openssh.com",
    "ssh-rsa",
];

/// One public key, as `ssh-keygen` writes it.
#[derive(Debug, PartialEq)]
struct PublicKey {
    kind: String,
    blob: String,
    bytes: Vec<u8>,
}

impl PublicKey {
    fn parse(text: &str) -> Result<Self, String> {
        let mut parts = text.split_whitespace();
        let (Some(kind), Some(blob)) = (parts.next(), parts.next()) else {
            return Err("that is not a public key: expected `<type> <base64> [comment]`".into());
        };
        if !KEY_TYPES.contains(&kind) {
            return Err(format!("{kind} is not an SSH public key type; pass the .pub file, not the private key"));
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(blob)
            .map_err(|_| "the key's base64 does not decode".to_string())?;
        if bytes.len() < 4 + kind.len() || &bytes[4..4 + kind.len()] != kind.as_bytes() {
            return Err(format!("the key body does not say {kind}"));
        }
        Ok(Self { kind: kind.to_string(), blob: blob.to_string(), bytes })
    }

    /// `SHA256:…`, the form `ssh-keygen -l` prints.
    fn fingerprint(&self) -> String {
        let digest = Sha256::digest(&self.bytes);
        format!("SHA256:{}", base64::engine::general_purpose::STANDARD_NO_PAD.encode(digest))
    }
}

fn valid_name(name: &str) -> bool {
    (1..=32).contains(&name.len())
        && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

/// The `authorized_keys` line for one agent key. `restrict` turns off every
/// forwarding and the terminal; `command` replaces whatever the client asked
/// to run.
fn key_line(name: &str, key: &PublicKey) -> String {
    format!(
        "restrict,command=\"{BINARY} agent-exec --key {name}\" {} {} {COMMENT_PREFIX}{name}",
        key.kind, key.blob
    )
}

/// The agent keys in an `authorized_keys` file: (name, key) for each line
/// this command wrote.
fn agent_keys(file: &str) -> Vec<(String, PublicKey)> {
    file.lines()
        .filter_map(|line| {
            let name = line.rsplit_once(' ')?.1.strip_prefix(COMMENT_PREFIX)?.to_string();
            // Options, then the key. The forced command has spaces in it, so
            // the key starts at the first word that is a key type.
            let words: Vec<&str> = line.split_whitespace().collect();
            let at = words.iter().position(|w| KEY_TYPES.contains(w))?;
            let key = PublicKey::parse(&words[at..].join(" ")).ok()?;
            Some((name, key))
        })
        .collect()
}

/// `file` with `name`'s line added. Refuses a name or a key already present.
fn with_key(file: &str, name: &str, key: &PublicKey) -> Result<String, String> {
    for (existing, k) in agent_keys(file) {
        if existing == name {
            return Err(format!("there is already an agent key called {name}; remove it first"));
        }
        if k.blob == key.blob {
            return Err(format!("that key is already added, as {existing}"));
        }
    }
    let mut out = file.to_string();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(&key_line(name, key));
    out.push('\n');
    Ok(out)
}

/// `file` without `name`'s line, and whether there was one.
fn without_key(file: &str, name: &str) -> (String, bool) {
    let marker = format!("{COMMENT_PREFIX}{name}");
    let mut found = false;
    let kept: Vec<&str> = file
        .lines()
        .filter(|line| {
            let mine = line.rsplit_once(' ').is_some_and(|(_, c)| c == marker);
            found |= mine;
            !mine
        })
        .collect();
    let mut out = kept.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    (out, found)
}

fn authorized_keys() -> PathBuf {
    Path::new(AGENT_HOME).join(".ssh").join("authorized_keys")
}

async fn is_root() -> bool {
    Command::new("id")
        .arg("-u")
        .output()
        .await
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "0")
        .unwrap_or(false)
}

async fn run(cmd: &str, args: &[&str]) -> Result<(), String> {
    let out = Command::new(cmd).args(args).output().await.map_err(|e| format!("{cmd}: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!("{cmd} {}: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim()))
    }
}

/// The agent user, its home and its `.ssh`, created when missing. A shell is
/// required: sshd runs the forced command through the user's shell, so
/// `nologin` would refuse every call.
async fn ensure_agent_user() -> Result<(), String> {
    let exists = Command::new("id").arg("-u").arg(AGENT_USER).output().await.map_err(|e| e.to_string())?;
    if !exists.status.success() {
        run(
            "useradd",
            &["--system", "--create-home", "--home-dir", AGENT_HOME, "--shell", "/bin/sh", AGENT_USER],
        )
        .await?;
        ui::ok(&format!("created the {AGENT_USER} user (no sudo, no groups, no database role)"));
    }
    let ssh = Path::new(AGENT_HOME).join(".ssh");
    std::fs::create_dir_all(&ssh).map_err(|e| format!("{}: {e}", ssh.display()))?;
    run("chmod", &["700", &ssh.to_string_lossy()]).await?;
    run("chown", &[&format!("{AGENT_USER}:{AGENT_USER}"), &ssh.to_string_lossy()]).await?;
    Ok(())
}

/// Replace `authorized_keys` whole: written beside it, then renamed, so sshd
/// never reads half a file.
async fn write_authorized_keys(content: &str) -> Result<(), String> {
    let path = authorized_keys();
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, content).map_err(|e| format!("{}: {e}", tmp.display()))?;
    run("chmod", &["600", &tmp.to_string_lossy()]).await?;
    run("chown", &[&format!("{AGENT_USER}:{AGENT_USER}"), &tmp.to_string_lossy()]).await?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("{}: {e}", path.display()))
}

fn read_authorized_keys() -> Result<String, String> {
    match std::fs::read_to_string(authorized_keys()) {
        Ok(s) => Ok(s),
        // No file yet is no keys yet.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(format!("{}: {e}", authorized_keys().display())),
    }
}

pub async fn manage(cmd: AgentKeyCmd) -> Result<(), String> {
    if !is_root().await {
        return Err("agent keys are managed as root: run it with sudo".into());
    }
    match cmd {
        AgentKeyCmd::Add { key, name } => {
            if !valid_name(&name) {
                return Err("a key name is 1-32 characters of a-z, 0-9, - and _".into());
            }
            let text = if key == "-" {
                std::io::read_to_string(std::io::stdin()).map_err(|e| format!("reading stdin: {e}"))?
            } else {
                std::fs::read_to_string(&key).map_err(|e| format!("{key}: {e}"))?
            };
            let key = PublicKey::parse(text.trim())?;
            ensure_agent_user().await?;
            let file = with_key(&read_authorized_keys()?, &name, &key)?;
            write_authorized_keys(&file).await?;
            ui::ok(&format!("added {name} ({})", key.fingerprint()));
            println!();
            println!("  Connect with the matching private key:");
            println!("    ssh -i <private key> {AGENT_USER}@<this box> virtues schema");
            println!("  It can run: {}. Nothing else, and no shell.", AGENT_VERBS.join(", "));
            Ok(())
        }
        AgentKeyCmd::Ls => {
            let keys = agent_keys(&read_authorized_keys()?);
            if keys.is_empty() {
                ui::skip("no agent keys");
            }
            for (name, key) in keys {
                println!("{name}\t{}\t{}", key.kind, key.fingerprint());
            }
            Ok(())
        }
        AgentKeyCmd::Rm { name } => {
            let (file, found) = without_key(&read_authorized_keys()?, &name);
            if !found {
                return Err(format!("no agent key called {name}"));
            }
            write_authorized_keys(&file).await?;
            ui::ok(&format!("removed {name}; its next connection is refused"));
            Ok(())
        }
    }
}

/// `virtues agent-exec`: run what the agent asked for, if it is a data verb.
pub async fn exec(key: String) -> Result<(), String> {
    let request = std::env::var("SSH_ORIGINAL_COMMAND")
        .map_err(|_| "agent-exec is an agent key's forced command; sshd runs it with SSH_ORIGINAL_COMMAND set")?;
    match parse_request(&request)? {
        Request::Help(text) => {
            println!("{text}");
            Ok(())
        }
        Request::Run(command) => super::data::run(&super::data::Verbs::remote(key), command).await,
    }
}

enum Request {
    Run(Commands),
    /// `--help` anywhere: clap's text, printed rather than treated as an error.
    Help(String),
}

/// The forced command. `request` is `SSH_ORIGINAL_COMMAND`: what the agent
/// asked sshd to run, which is only ever parsed here, never executed.
fn parse_request(request: &str) -> Result<Request, String> {
    let mut words = shlex::split(request).ok_or("the command has an unclosed quote")?;
    if words.first().map(String::as_str) == Some("virtues") {
        words.remove(0);
    }
    let Some(verb) = words.first() else {
        return Err(format!("no command given. An agent key can run: {}", AGENT_VERBS.join(", ")));
    };
    if !AGENT_VERBS.contains(&verb.as_str()) {
        return Err(format!(
            "`{verb}` is not available to an agent key. It can run: {}",
            AGENT_VERBS.join(", ")
        ));
    }
    let cli = match <Cli as clap::Parser>::try_parse_from(std::iter::once("virtues".to_string()).chain(words)) {
        Ok(cli) => cli,
        Err(e) if matches!(
            e.kind(),
            clap::error::ErrorKind::DisplayHelp
                | clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
        ) =>
        {
            return Ok(Request::Help(e.to_string()))
        }
        Err(e) => return Err(e.to_string()),
    };
    let command = cli.command.ok_or("no command given")?;
    // A folder path names a directory on the box, which is not where the
    // agent's folder is.
    if let Commands::Applet { cmd: AppletCmd::Check { path, .. } | AppletCmd::Put { path, .. } } = &command {
        if path != "-" {
            return Err("send the applet as JSON on stdin: `applet check -` or `applet put -`. A folder path would name a directory on the box, not on your machine".into());
        }
    }
    Ok(Request::Run(command))
}

#[cfg(test)]
mod tests {
    use super::*;

    // A real ed25519 public key, generated for this test and used nowhere else.
    const ED25519: &str =
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIGAFOignENdLKjLvgV/CKOH9kBdVXF/FrPpS9DuCsv/w test@example.com";

    #[test]
    fn a_public_key_parses_and_a_private_one_does_not() {
        let key = PublicKey::parse(ED25519).unwrap();
        assert_eq!(key.kind, "ssh-ed25519");
        assert!(key.fingerprint().starts_with("SHA256:"));
        assert!(PublicKey::parse("-----BEGIN OPENSSH PRIVATE KEY-----").is_err());
        assert!(PublicKey::parse("ssh-ed25519 notbase64!").is_err());
        // The body names its own type; a relabelled key is refused.
        let relabelled = ED25519.replacen("ssh-ed25519", "ssh-rsa", 1);
        assert!(PublicKey::parse(&relabelled).is_err());
    }

    #[test]
    fn the_line_restricts_and_forces_the_command() {
        let line = key_line("claude-code", &PublicKey::parse(ED25519).unwrap());
        assert!(line.starts_with("restrict,command=\"/usr/local/bin/virtues agent-exec --key claude-code\" ssh-ed25519 "));
        assert!(line.ends_with(" virtues-agent:claude-code"));
    }

    #[test]
    fn add_ls_rm_round_trip_and_leave_other_lines_alone() {
        let key = PublicKey::parse(ED25519).unwrap();
        let by_hand = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIByhand someone@example.com\n";
        let file = with_key(by_hand, "claude-code", &key).unwrap();
        assert_eq!(agent_keys(&file).len(), 1);
        assert!(with_key(&file, "claude-code", &key).is_err(), "duplicate name");
        assert!(with_key(&file, "other", &key).is_err(), "duplicate key");
        let (file, found) = without_key(&file, "claude-code");
        assert!(found);
        assert_eq!(file, by_hand);
        assert!(!without_key(&file, "claude-code").1);
    }

    #[test]
    fn names_are_plain() {
        assert!(valid_name("claude-code"));
        assert!(!valid_name(""));
        assert!(!valid_name("a b"));
        assert!(!valid_name("x\" y"));
    }

    #[test]
    fn requests_are_data_verbs_only() {
        let run = |r: &str| match parse_request(r) {
            Ok(Request::Run(c)) => Some(c),
            _ => None,
        };
        assert!(matches!(run("virtues query 'select 1'"), Some(Commands::Query { .. })));
        assert!(matches!(run("schema"), Some(Commands::Schema { .. })));
        assert!(matches!(run("applet put -"), Some(Commands::Applet { .. })));
        assert!(matches!(parse_request("query --help"), Ok(Request::Help(_))));
        for refused in ["upgrade", "reset --yes", "sh -c id", "virtues agent-key ls", "", "query 'unclosed"] {
            assert!(parse_request(refused).is_err(), "{refused}");
        }
        assert!(parse_request("applet put /tmp/x").is_err(), "a box path");
    }
}
