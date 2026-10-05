//! Sudo mode's one question: does this call only read, or does it change
//! something? Reads run. A change stops the turn and asks the owner, with the
//! exact command on the card, and runs only once they allow that command.
//!
//! The split is enforced where it can be and guessed where it cannot:
//!
//! - SQL (`sql_query` / `sql_write` in sudo) runs inside a `READ ONLY`
//!   transaction until allowed, so Postgres itself says what is a write.
//! - `psql -c` from the shell runs with `default_transaction_read_only` set
//!   through `PGOPTIONS`, and a read-only refusal in its output turns into the
//!   same question. Under `sudo` the environment is reset and that setting is
//!   lost, so `sudo … psql` counts as a write; so does SQL psql reads from
//!   somewhere the line does not show (a file, a pipe).
//! - Read-only mode still lets a few statements act: `COPY … TO` writes a
//!   file or runs a program, some functions signal backends or reload config,
//!   and a statement can switch read-only off. Those ask by name
//!   (`sql_acts_anyway`).
//! - Any other shell command is a read only when every command in the line
//!   is a program known to read, with no output redirected to a file, no
//!   command substitution and no heredoc. Everything else asks. A read that
//!   asks costs the owner a click; a write that does not ask is the failure
//!   this exists for.
//!
//! A grant is for one exact command in one chat, so allowing
//! `UPDATE … WHERE id = 1` allows nothing else.

use sha2::{Digest, Sha256};

use super::executor::ToolResult;

/// The `PGOPTIONS` a shell command runs with until its write is allowed.
pub const READ_ONLY_PGOPTIONS: &str = "-c default_transaction_read_only=on";

/// What Postgres says, in `psql`'s output or a driver error, when a read-only
/// transaction refuses a write (SQLSTATE 25006), or when a statement that
/// changes things cannot run in a transaction at all (`VACUUM`,
/// `CREATE DATABASE`, `ALTER SYSTEM`).
pub fn is_read_only_refusal(text: &str) -> bool {
    text.contains("in a read-only transaction") || text.contains("cannot run inside a transaction block")
}

/// Whether SQL names something a read-only transaction does not stop: `COPY`
/// (to a file or a program), functions that signal backends, reload config,
/// take session locks or touch files, and a switch of read-only itself.
/// Matched as words, case-insensitively; a mention inside a string literal
/// asks too, which costs a click.
pub fn sql_acts_anyway(sql: &str) -> bool {
    const WORDS: &[&str] = &["copy", "dblink"];
    const PREFIXES: &[&str] = &[
        "pg_terminate", "pg_cancel", "pg_signal", "pg_reload", "pg_rotate", "pg_promote",
        "pg_switch", "pg_create_", "pg_drop_", "pg_log_", "pg_advisory", "pg_file_", "pg_replication",
        "lo_", "dblink",
    ];
    let lower = sql.to_lowercase();
    let ident = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let starts_word = |at: usize| lower[..at].chars().next_back().is_none_or(|c| !ident(c));
    let found = |needle: &str, whole: bool| {
        lower.match_indices(needle).any(|(at, _)| {
            starts_word(at) && (!whole || lower[at + needle.len()..].chars().next().is_none_or(|c| !ident(c)))
        })
    };
    let words: Vec<&str> = lower.split(|c: char| !ident(c)).filter(|w| !w.is_empty()).collect();
    WORDS.iter().any(|w| found(w, true))
        || PREFIXES.iter().any(|p| found(p, false))
        // `transaction_read_only`, `default_transaction_read_only`, `READ WRITE`.
        || lower.contains("read_only")
        || words.windows(2).any(|w| w == ["read", "write"])
}

/// The permission id for one exact command in one kind of tool.
pub fn grant_id(kind: &str, command: &str) -> String {
    let digest = Sha256::digest(format!("{kind}\0{}", command.trim()).as_bytes());
    format!("sudo:{kind}:{}", &hex::encode(digest)[..24])
}

/// The result that stops the turn and puts an Allow card in the chat.
/// `awaiting_owner` is what the agent loop pauses on (`agent::turn::needs_user`).
pub fn ask(kind: &str, command: &str) -> ToolResult {
    let shown: String = command.trim().chars().take(2000).collect();
    ToolResult::success(serde_json::json!({
        "permission_needed": true,
        "awaiting_owner": true,
        "entity_id": grant_id(kind, command),
        "entity_type": "command",
        "entity_title": shown,
        "message": match kind {
            "sql" => "Run this statement on the database?",
            _ => "Run this command on the server?",
        },
        "note": "Not run. This changes something, so the owner has to allow it. \
                 Stop here. Once they allow it, run exactly the same command again.",
    }))
}

/// Whether a shell command line only reads. See the module comment.
pub fn shell_is_read(command: &str) -> bool {
    match segments(command) {
        Some(segs) => !segs.is_empty() && segs.iter().all(|s| segment_is_read(s)),
        None => false,
    }
}

/// Split a command line into simple commands, or `None` for anything the
/// split cannot see through: command substitution, heredocs, subshells, and
/// output redirected anywhere but /dev/null.
fn segments(line: &str) -> Option<Vec<Vec<String>>> {
    let chars: Vec<char> = line.chars().collect();
    let mut segs: Vec<Vec<String>> = Vec::new();
    let mut words: Vec<String> = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut i = 0;

    fn end_word(word: &mut String, in_word: &mut bool, words: &mut Vec<String>) {
        if *in_word {
            words.push(std::mem::take(word));
            *in_word = false;
        }
    }
    fn end_seg(words: &mut Vec<String>, segs: &mut Vec<Vec<String>>) {
        if !words.is_empty() {
            segs.push(std::mem::take(words));
        }
    }

    while i < chars.len() {
        let c = chars[i];
        match c {
            '\'' => {
                in_word = true;
                i += 1;
                while i < chars.len() && chars[i] != '\'' {
                    word.push(chars[i]);
                    i += 1;
                }
                if i >= chars.len() {
                    return None;
                }
            }
            '"' => {
                in_word = true;
                i += 1;
                while i < chars.len() && chars[i] != '"' {
                    match chars[i] {
                        '`' => return None,
                        '$' if chars.get(i + 1) == Some(&'(') => return None,
                        // Inside double quotes a backslash escapes only these;
                        // before anything else it stays, as in `"\copy"`.
                        '\\' if matches!(chars.get(i + 1), Some('$' | '`' | '"' | '\\' | '\n')) => {
                            i += 1;
                            word.push(chars[i]);
                        }
                        other => word.push(other),
                    }
                    i += 1;
                }
                if i >= chars.len() {
                    return None;
                }
            }
            '\\' => {
                in_word = true;
                i += 1;
                if let Some(&next) = chars.get(i) {
                    if next != '\n' {
                        word.push(next);
                    }
                }
            }
            '`' | '(' | ')' | '{' | '}' => return None,
            '$' if chars.get(i + 1) == Some(&'(') => return None,
            ' ' | '\t' => end_word(&mut word, &mut in_word, &mut words),
            '\n' | ';' | '|' | '&' => {
                // `&>file` is a redirect, not a separator.
                if c == '&' && chars.get(i + 1) == Some(&'>') {
                    end_word(&mut word, &mut in_word, &mut words);
                    i += 1;
                    continue;
                }
                end_word(&mut word, &mut in_word, &mut words);
                end_seg(&mut words, &mut segs);
            }
            '<' => {
                if chars.get(i + 1) == Some(&'<') {
                    return None; // heredoc or herestring
                }
                // Kept as a word: psql reading SQL from a file must show.
                end_word(&mut word, &mut in_word, &mut words);
                words.push("<".into());
            }
            '>' => {
                // A bare fd number before it (`2>`) is part of the redirect.
                if in_word && word.chars().all(|d| d.is_ascii_digit()) {
                    word.clear();
                    in_word = false;
                }
                end_word(&mut word, &mut in_word, &mut words);
                i += 1;
                if chars.get(i) == Some(&'>') {
                    i += 1;
                }
                // `>&2`, `2>&1`: duplicating a descriptor writes no file.
                if chars.get(i) == Some(&'&') {
                    i += 1;
                    while chars.get(i).is_some_and(|d| d.is_ascii_digit() || *d == '-') {
                        i += 1;
                    }
                    continue;
                }
                while chars.get(i).is_some_and(|d| *d == ' ' || *d == '\t') {
                    i += 1;
                }
                let mut target = String::new();
                while let Some(&d) = chars.get(i) {
                    if d.is_whitespace() || matches!(d, ';' | '|' | '&' | '<' | '>') {
                        break;
                    }
                    target.push(d);
                    i += 1;
                }
                if target.trim_matches(|q| q == '"' || q == '\'') != "/dev/null" {
                    return None;
                }
                continue;
            }
            other => {
                in_word = true;
                word.push(other);
            }
        }
        i += 1;
    }
    end_word(&mut word, &mut in_word, &mut words);
    end_seg(&mut words, &mut segs);
    Some(segs)
}

/// Programs that read and nothing else, whatever their arguments — given that
/// redirects were already refused.
const READERS: &[&str] = &[
    "ls", "cat", "head", "tail", "grep", "egrep", "fgrep", "zgrep", "stat", "wc",
    "du", "df", "free", "uptime", "uname", "whoami", "id", "groups",
    "ps", "pgrep", "pidof", "lsof", "netstat", "echo", "printf", "true", "false",
    "test", "[", "which", "type", "realpath", "readlink", "dirname", "basename",
    "cut", "tr", "column", "nl", "jq", "strings", "hexdump", "xxd", "od", "md5sum", "sha1sum",
    "sha256sum", "b2sum", "diff", "cmp", "printenv", "lsblk", "blkid", "findmnt",
    "lscpu", "lsusb", "lspci", "nproc", "getent", "zcat", "xzcat", "bzcat", "cd", "pwd", "sleep",
];

fn segment_is_read(words: &[String]) -> bool {
    let mut rest = words;
    let mut under_sudo = false;
    // Assignments and wrappers ahead of the program.
    loop {
        let Some(first) = rest.first() else { return true };
        if is_assignment(first) {
            // Would lift psql's read-only hold.
            if first.starts_with("PGOPTIONS=") {
                return false;
            }
            rest = &rest[1..];
            continue;
        }
        match first.as_str() {
            "sudo" => {
                under_sudo = true;
                rest = skip_flags(&rest[1..], &["-u", "-g", "-C", "-h", "-p", "-U", "-D", "-R"]);
            }
            "env" => rest = skip_flags(&rest[1..], &["-u", "-C", "-S"]),
            "nice" | "ionice" => rest = skip_flags(&rest[1..], &["-n", "-c", "-p"]),
            "time" | "command" | "nohup" => rest = &rest[1..],
            "timeout" => {
                rest = skip_flags(&rest[1..], &["-s", "-k"]);
                // The duration.
                rest = rest.get(1..).unwrap_or(&[]);
            }
            _ => break,
        }
    }
    let Some(program) = rest.first() else { return true };
    let program = program.rsplit('/').next().unwrap_or(program);
    let args = &rest[1..];
    let has = |flags: &[&str]| args.iter().any(|a| flags.iter().any(|f| a == f || a.starts_with(&format!("{f}="))));
    let first_arg = args.iter().find(|a| !a.starts_with('-')).map(String::as_str);
    let positional = args.iter().filter(|a| !a.starts_with('-') && *a != "<").count();
    // A short-flag cluster holding one of `letters` (`-sSo` holds `o`).
    let short = |letters: &str| {
        args.iter().any(|a| a.starts_with('-') && !a.starts_with("--") && a.chars().skip(1).any(|c| letters.contains(c)))
    };

    if READERS.contains(&program) {
        return true;
    }
    match program {
        // Not `-i` (or a cluster holding it, `-ni`), and a script of the
        // shapes that only print; `w` writes a file and `e` runs a command.
        // `-e`/`-f` scripts and anything unusual ask.
        "sed" => {
            !short("ief")
                && !has(&["--in-place", "--expression", "--file"])
                && args.iter().find(|a| !a.starts_with('-')).is_some_and(|script| sed_script_is_read(script))
        }
        "sort" => !short("o") && !has(&["--output"]),
        // `uniq in out` writes `out`.
        "uniq" => positional < 2,
        "tree" => !short("o"),
        "rg" => !args.iter().any(|a| a.starts_with("--pre")),
        "file" => !short("C") && !has(&["--compile"]),
        "date" => !short("s") && !has(&["--set"]),
        "hostname" => positional == 0 && !short("bF") && !has(&["--file", "--boot"]),
        "ss" => !short("K") && !has(&["--kill"]),
        "hostnamectl" | "timedatectl" => matches!(first_arg, None | Some("status" | "show")),
        "awk" | "gawk" | "mawk" => !args.iter().any(|a| a.contains("system") || a.contains('>') || a.contains('|') || a == "-i"),
        "find" => !has(&["-delete", "-exec", "-execdir", "-ok", "-okdir", "-fprint", "-fprint0", "-fprintf", "-fls"]),
        "systemctl" => matches!(
            first_arg,
            Some("status" | "show" | "cat" | "list-units" | "list-unit-files" | "list-timers"
                | "list-sockets" | "list-dependencies" | "is-active" | "is-enabled"
                | "is-failed" | "is-system-running" | "get-default")
        ),
        "journalctl" => !args.iter().any(|a| {
            ["--vacuum", "--rotate", "--flush", "--sync", "--relinquish", "--setup-keys"]
                .iter()
                .any(|f| a.starts_with(f))
        }),
        "dmesg" => !has(&["-c", "-C", "--clear", "--read-clear"]),
        "ip" => !args.iter().any(|a| {
            matches!(a.as_str(), "set" | "add" | "del" | "delete" | "flush" | "change" | "replace" | "append" | "exec")
        }),
        // Read-only through PGOPTIONS, which sudo's env reset would drop.
        // psql's own escapes that write files or run commands still ask.
        // SQL has to be on the line (`-c`), not read from a file or a pipe,
        // and name nothing read-only mode lets through.
        "psql" => {
            let on_the_line = short("cl") || has(&["--command", "--list"]);
            !under_sudo
                && on_the_line
                && !short("foL")
                && !has(&["--file", "--output", "--log-file"])
                && !args.iter().any(|a| {
                    a == "<"
                        || sql_acts_anyway(a)
                        || ["\\!", "\\copy", "\\o", "\\w", "\\g ", "\\gx ", "\\gexec", "\\i", "\\lo_", "\\setenv"]
                            .iter()
                            .any(|e| a.contains(e))
                })
        }
        "git" => {
            matches!(
                first_arg,
                Some("status" | "log" | "diff" | "show" | "rev-parse" | "ls-files" | "describe" | "blame" | "grep")
            ) && !short("O")
                && !has(&["--output", "--open-files-in-pager", "--ext-diff"])
        }
        "curl" => {
            !short("XdFToOK")
                && !args.iter().any(|a| {
                    ["--request", "--data", "--form", "--upload-file", "--output", "--remote-name", "--config", "--json"]
                        .iter()
                        .any(|f| a.starts_with(f))
                })
        }
        "virtues" => matches!(first_arg, Some("status" | "help" | "doctor"))
            || has(&["--version", "--help", "-h"]),
        "podman" | "docker" => matches!(first_arg, Some("ps" | "images" | "logs" | "inspect" | "stats" | "version" | "info")),
        "xargs" => {
            let inner = skip_flags(args, &["-n", "-I", "-P", "-d", "-L", "-s", "-E", "-a"]);
            !inner.is_empty() && segment_is_read(inner)
        }
        _ => false,
    }
}

/// A sed script made only of printing commands: `1,40p`, `/re/p`, `$d`, `q`,
/// and `s/a/b/` with flags that do not write or run anything.
fn sed_script_is_read(script: &str) -> bool {
    // One address: a line number, `$`, or `/regex/`. Returns what follows.
    fn address(s: &str) -> Option<&str> {
        if let Some(rest) = s.strip_prefix('/') {
            let end = delimited_end(rest, '/')?;
            return Some(&rest[end + 1..]);
        }
        let len = s.find(|c: char| !(c.is_ascii_digit() || c == '$')).unwrap_or(s.len());
        Some(&s[len..])
    }
    // Index of the first unescaped `delim`.
    fn delimited_end(s: &str, delim: char) -> Option<usize> {
        let mut escaped = false;
        for (i, c) in s.char_indices() {
            match c {
                _ if escaped => escaped = false,
                '\\' => escaped = true,
                c if c == delim => return Some(i),
                _ => {}
            }
        }
        None
    }
    script.split([';', '\n']).map(str::trim).filter(|c| !c.is_empty()).all(|cmd| {
        let Some(mut rest) = address(cmd) else { return false };
        if let Some(r) = rest.strip_prefix(',') {
            let Some(r) = address(r) else { return false };
            rest = r;
        }
        let rest = rest.trim_start().trim_start_matches('!').trim_start();
        let mut chars = rest.chars();
        match chars.next() {
            Some('p' | 'd' | 'q' | 'Q' | 'n' | '=' | 'l') => chars.as_str().trim().is_empty(),
            Some('s') => {
                let Some(delim) = chars.next() else { return false };
                let body = chars.as_str();
                let Some(a) = delimited_end(body, delim) else { return false };
                let Some(b) = delimited_end(&body[a + delim.len_utf8()..], delim) else { return false };
                let flags = &body[a + delim.len_utf8() + b + delim.len_utf8()..];
                flags.chars().all(|c| "gpiIm0123456789".contains(c))
            }
            _ => false,
        }
    })
}

fn is_assignment(word: &str) -> bool {
    match word.split_once('=') {
        Some((name, _)) => {
            !name.is_empty()
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                && !name.starts_with(|c: char| c.is_ascii_digit())
        }
        None => false,
    }
}

/// Drop leading flags (and the value of those in `with_value`), and a `--`.
fn skip_flags<'a>(mut words: &'a [String], with_value: &[&str]) -> &'a [String] {
    while let Some(w) = words.first() {
        if w == "--" {
            return &words[1..];
        }
        if !w.starts_with('-') || w == "-" {
            break;
        }
        let takes_value = with_value.contains(&w.as_str());
        words = &words[1..];
        if takes_value && !words.is_empty() {
            words = &words[1..];
        }
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_the_onboarding_run_did_reads_where_it_should() {
        // The exploration from the run that prompted this gate.
        for cmd in [
            r#"psql "$DATABASE_URL" -c "\dt""#,
            r#"psql virtues -c "SELECT id, onboarding_status FROM app_user_profile""#,
            r#"rg -l -i "onboarding" /usr/local/share/virtues /opt 2>/dev/null | head -40"#,
            r#"which rg grep find; grep -r -l -i "onboarding" /usr/local/share/virtues/current"#,
            r#"ls /usr/local/share/virtues/current | head -40; echo "==== SRC? ====""#,
            r#"strings /usr/local/share/virtues/current/virtues | grep -i -E "onboarding|getting_started""#,
            "journalctl -u virtues --no-pager -n 200 -p warning",
            "sort -n /tmp/x | uniq -c",
            "date +%s",
            "systemctl status virtues --no-pager",
            "df -h && free -m 2>&1",
            "curl -s http://localhost:8000/health",
            "find /var/lib/virtues -name '*.log' -mtime -1",
            "sed -n '1,40p' /etc/virtues/env",
            "sed -n '/error/,/^$/p' /var/log/x",
            "sed 's/secret=.*/secret=***/g' /etc/virtues/virtues.env",
            "tail -n 50 /tmp/job.log >&2",
        ] {
            assert!(shell_is_read(cmd), "should read: {cmd}");
        }
    }

    #[test]
    fn changes_and_anything_opaque_ask() {
        for cmd in [
            // Under sudo the read-only PGOPTIONS is reset away.
            r#"sudo -u postgres psql virtues -c "\dt""#,
            "python3 - << 'PY'\nprint(1)\nPY",
            "cat <<EOF > /etc/x\nhi\nEOF",
            "echo hi > /etc/motd",
            "echo hi >> ~/.bashrc",
            "ls $(rm -rf /tmp/x)",
            "ls `whoami`",
            "rm -rf /tmp/x",
            "systemctl restart virtues",
            "sed -i 's/a/b/' /etc/virtues/env",
            "find /tmp -name x -delete",
            "find / -exec rm {} ;",
            "journalctl --vacuum-time=1d",
            "curl -X POST localhost:8000/api/setup/skip-onboarding -d '{}'",
            "ls && rm x",
            "cat x | tee y",
            "psql \"$DATABASE_URL\" -c '\\copy t to /tmp/t.csv'",
            r#"psql "$DATABASE_URL" -c "\copy t to /tmp/t.csv""#,
            "git checkout main",
            // Read-only mode lets these act.
            r#"psql "$DATABASE_URL" -c "COPY (SELECT 1) TO PROGRAM 'rm -rf /tmp/x'""#,
            r#"psql "$DATABASE_URL" -c "SELECT pg_terminate_backend(123)""#,
            r#"psql "$DATABASE_URL" -c "SET transaction_read_only = off; UPDATE t SET a = 1""#,
            r#"psql "$DATABASE_URL" -c "BEGIN READ WRITE; UPDATE t SET a = 1""#,
            // SQL from somewhere the line does not show.
            r#"psql "$DATABASE_URL" -f /tmp/x.sql"#,
            r#"psql "$DATABASE_URL" < /tmp/x.sql"#,
            r#"echo "UPDATE t SET a = 1" | psql "$DATABASE_URL""#,
            r#"psql "$DATABASE_URL" -c "\i /tmp/x.sql""#,
            r#"PGOPTIONS= psql "$DATABASE_URL" -c "UPDATE t SET a = 1""#,
            r#"env PGOPTIONS=-cx psql "$DATABASE_URL" -c "SELECT 1""#,
            // Readers with a writing flag.
            "sort -o /etc/hosts /tmp/x",
            "uniq /tmp/a /etc/hosts",
            "date -s '2020-01-01'",
            "hostname evil",
            "ss -K dst 10.0.0.1",
            "rg --pre /tmp/run.sh x",
            "sed -n 's/a/b/w /etc/x' /tmp/y",
            "sed '1e rm -rf /tmp/x' /tmp/y",
            "sed -e 's/a/b/' -e '1w /etc/x' /tmp/y",
            "sed 's/a/b/e' /tmp/y",
            "curl -sSo /tmp/x http://example.com",
            "timedatectl set-timezone UTC",
            "apt-get install -y jq",
            "(cd /tmp && ls)",
            "bash -c 'ls'",
            "virtues upgrade --pre",
            "xargs rm",
            "",
            "echo 'unterminated",
        ] {
            assert!(!shell_is_read(cmd), "should ask: {cmd}");
        }
    }

    #[test]
    fn quoting_hides_operators_that_are_not_operators() {
        assert!(shell_is_read(r#"psql "$DATABASE_URL" -c "SELECT 1 WHERE 2 > 1""#));
        assert!(shell_is_read(r#"grep -E 'a|b; c > d' /etc/hosts"#));
        assert!(!shell_is_read(r#"echo "$(id)""#));
    }

    #[test]
    fn sql_that_acts_despite_read_only_is_named() {
        for sql in [
            "COPY t TO '/tmp/t.csv'",
            "copy (select 1) to program 'id'",
            "SELECT pg_terminate_backend(pid) FROM pg_stat_activity",
            "SELECT pg_reload_conf()",
            "SELECT pg_advisory_lock(1)",
            "SELECT lo_export(1, '/tmp/x')",
            "SET transaction_read_only = off",
            "SET default_transaction_read_only TO off",
            "BEGIN READ\nWRITE",
        ] {
            assert!(sql_acts_anyway(sql), "{sql}");
        }
        for sql in [
            "SELECT onboarding_status FROM app_user_profile",
            "SELECT copyright, hello_world FROM t",
            "SELECT * FROM pg_stat_activity",
            "\\dt",
        ] {
            assert!(!sql_acts_anyway(sql), "{sql}");
        }
    }

    #[test]
    fn a_grant_is_for_one_exact_command() {
        assert_eq!(grant_id("shell", "rm x"), grant_id("shell", "  rm x\n"));
        assert_ne!(grant_id("shell", "rm x"), grant_id("shell", "rm y"));
        assert_ne!(grant_id("shell", "rm x"), grant_id("sql", "rm x"));
    }

    #[test]
    fn the_ask_pauses_the_turn_and_names_the_command() {
        let r = ask("shell", "rm x");
        assert_eq!(r.data["awaiting_owner"], true);
        assert_eq!(r.data["permission_needed"], true);
        assert_eq!(r.data["entity_type"], "command");
        assert_eq!(r.data["entity_title"], "rm x");
        assert_eq!(r.data["entity_id"], grant_id("shell", "rm x"));
    }
}
