//! Sudo mode runs what the model calls, as a coding agent's bypass mode does:
//! reads, writes, services, packages, files. One kind of call still stops the
//! turn and asks the owner, with the exact command on the card: one that
//! deletes data, which has no undo.
//!
//! - SQL asks when it names `DROP`, `TRUNCATE` or `DELETE`: in the SQL tools,
//!   and anywhere in a shell line that runs psql (on the line, a pipe or a
//!   heredoc, so the whole line is read).
//! - A shell line asks when one of its commands deletes files outside /tmp,
//!   wipes or repartitions a disk, drops a database or role, or runs a
//!   `virtues` verb that erases the box (`reset`, `uninstall`, `deprovision`,
//!   `restore`). `bash -c`, `sudo` and `xargs` are looked through.
//!
//! This catches the plain spellings of a delete, and is not a sandbox: a
//! script can remove a file without saying `rm`. The rest rests on the prompt
//! (`SUDO_MODE_PROMPT`) and on Stop. When unsure the check asks, which costs
//! the owner one click.
//!
//! A grant is for one exact command in one chat, so allowing
//! `DELETE … WHERE id = 1` allows nothing else.

use sha2::{Digest, Sha256};

use super::executor::ToolResult;

/// Whether SQL names a statement that deletes: `DROP`, `TRUNCATE`, `DELETE`.
/// Matched as words, case-insensitively, so `deleted_at` is not one and a
/// mention inside a string literal is (a click). `ON DELETE` in a foreign key
/// is not.
pub fn sql_destroys(sql: &str) -> bool {
    let lower = sql.to_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|w| !w.is_empty())
        .collect();
    words.iter().enumerate().any(|(i, w)| match *w {
        "drop" | "truncate" => true,
        "delete" => i == 0 || words[i - 1] != "on",
        _ => false,
    })
}

/// Whether a shell command line deletes data. See the module comment.
pub fn shell_destroys(line: &str) -> bool {
    let commands = commands(line);
    let runs_sql = commands.iter().any(|c| {
        let (program, args) = program(c);
        program == "psql" || (program == "virtues" && matches!(first_positional(args), Some("query" | "write")))
    });
    (runs_sql && sql_destroys(line)) || commands.iter().any(|c| command_destroys(c))
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
        "note": "Not run. This deletes data, so the owner has to allow it. \
                 Stop here. Once they allow it, run exactly the same command again.",
    }))
}

fn command_destroys(words: &[String]) -> bool {
    let (program, args) = program(words);
    let has = |flags: &[&str]| args.iter().any(|a| flags.contains(&a.as_str()));
    match program {
        // Files, unless every one named is scratch. `xargs rm` names none.
        "rm" | "shred" | "unlink" | "truncate" => {
            let paths = paths(args);
            paths.is_empty() || !paths.iter().all(|p| is_scratch(p))
        }
        "dd" => args.iter().any(|a| a.strip_prefix("of=").is_some_and(|p| !is_scratch(p))),
        "find" => {
            let starts: Vec<&String> = args.iter().take_while(|a| !a.starts_with('-')).collect();
            (has(&["-delete"]) && !(!starts.is_empty() && starts.iter().all(|p| is_scratch(p))))
                || args.iter().enumerate().any(|(i, a)| {
                    matches!(a.as_str(), "-exec" | "-execdir" | "-ok" | "-okdir") && {
                        let inner: Vec<String> =
                            args[i + 1..].iter().take_while(|w| *w != ";" && *w != "+").cloned().collect();
                        command_destroys(&inner)
                    }
                })
        }
        // Disks and partitions. Listing them is a read.
        "wipefs" | "fdisk" | "sfdisk" | "cfdisk" | "parted" | "sgdisk" | "gdisk" | "blkdiscard" | "mkswap"
        | "mke2fs" => !has(&["-l", "--list"]),
        p if p.starts_with("mkfs") => true,
        "dropdb" | "dropuser" | "pg_dropcluster" | "pg_resetwal" | "pg_resetxlog" => true,
        "virtues" => matches!(first_positional(args), Some("reset" | "uninstall" | "deprovision" | "restore")),
        "xargs" => command_destroys(skip_flags(args, &["-n", "-I", "-P", "-d", "-L", "-s", "-E", "-a"])),
        "bash" | "sh" | "zsh" | "dash" | "su" => {
            args.iter().position(|a| a == "-c" || (a.starts_with('-') && !a.starts_with("--") && a.ends_with('c')))
                .and_then(|i| args.get(i + 1))
                .is_some_and(|script| shell_destroys(script))
        }
        _ => false,
    }
}

/// The program a command runs and its arguments, past assignments and the
/// wrappers that run another program (`sudo -u x`, `env`, `nice`, `timeout 5`).
fn program(words: &[String]) -> (&str, &[String]) {
    let mut rest = words;
    loop {
        let Some(first) = rest.first() else { return ("", rest) };
        if is_assignment(first) {
            rest = &rest[1..];
            continue;
        }
        match first.rsplit('/').next().unwrap_or(first) {
            "sudo" => rest = skip_flags(&rest[1..], &["-u", "-g", "-C", "-h", "-p", "-U", "-D", "-R"]),
            "env" => rest = skip_flags(&rest[1..], &["-u", "-C", "-S"]),
            "nice" | "ionice" => rest = skip_flags(&rest[1..], &["-n", "-c", "-p"]),
            "time" | "command" | "nohup" | "exec" => rest = &rest[1..],
            "timeout" => {
                rest = skip_flags(&rest[1..], &["-s", "-k"]);
                // The duration.
                rest = rest.get(1..).unwrap_or(&[]);
            }
            program => return (program, &rest[1..]),
        }
    }
}

/// The arguments that are not flags; everything after `--` is one.
fn paths(args: &[String]) -> Vec<&String> {
    let mut out = Vec::new();
    let mut flags_done = false;
    for a in args {
        if !flags_done && a == "--" {
            flags_done = true;
        } else if flags_done || !a.starts_with('-') {
            out.push(a);
        }
    }
    out
}

fn first_positional(args: &[String]) -> Option<&str> {
    args.iter().find(|a| !a.starts_with('-')).map(String::as_str)
}

/// Scratch space: deleting there loses nothing anyone keeps.
fn is_scratch(path: &str) -> bool {
    (path.starts_with("/tmp/") || path.starts_with("/var/tmp/")) && !path.contains("..")
}

/// Split a command line into simple commands, each a list of words. Lenient:
/// it never refuses a line, it just finds the commands in it. Command
/// substitution and subshells start a new command; a redirect's target is
/// dropped; a heredoc's body lines come out as commands of their own.
fn commands(line: &str) -> Vec<Vec<String>> {
    let chars: Vec<char> = line.chars().collect();
    let mut cmds: Vec<Vec<String>> = Vec::new();
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
    fn end_cmd(words: &mut Vec<String>, cmds: &mut Vec<Vec<String>>) {
        if !words.is_empty() {
            cmds.push(std::mem::take(words));
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
            }
            '"' => {
                in_word = true;
                i += 1;
                while i < chars.len() && chars[i] != '"' {
                    // Inside double quotes a backslash escapes only these;
                    // before anything else it stays, as in `"\copy"`.
                    if chars[i] == '\\' && matches!(chars.get(i + 1), Some('$' | '`' | '"' | '\\' | '\n')) {
                        i += 1;
                    }
                    word.push(chars[i]);
                    i += 1;
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
            '$' if chars.get(i + 1) == Some(&'(') => {
                end_word(&mut word, &mut in_word, &mut words);
                end_cmd(&mut words, &mut cmds);
                i += 1;
            }
            '\n' | ';' | '|' | '&' | '`' | '(' | ')' => {
                // `&>file` is a redirect, not a separator.
                if c == '&' && chars.get(i + 1) == Some(&'>') {
                    i += 1;
                    continue;
                }
                end_word(&mut word, &mut in_word, &mut words);
                end_cmd(&mut words, &mut cmds);
            }
            ' ' | '\t' => end_word(&mut word, &mut in_word, &mut words),
            '<' | '>' => {
                // A bare fd number before it (`2>`) is part of the redirect.
                if in_word && word.chars().all(|d| d.is_ascii_digit()) {
                    word.clear();
                    in_word = false;
                }
                end_word(&mut word, &mut in_word, &mut words);
                while chars.get(i).is_some_and(|d| matches!(d, '<' | '>' | '-')) {
                    i += 1;
                }
                // `>&2`, `2>&1`: duplicating a descriptor names no file.
                if chars.get(i) == Some(&'&') {
                    while chars.get(i).is_some_and(|d| *d == '&' || *d == '-' || d.is_ascii_digit()) {
                        i += 1;
                    }
                    continue;
                }
                while chars.get(i).is_some_and(|d| *d == ' ' || *d == '\t') {
                    i += 1;
                }
                // The target (or a heredoc's delimiter), quotes and all.
                while chars.get(i).is_some_and(|d| !d.is_whitespace() && !matches!(d, ';' | '|' | '&' | '<' | '>')) {
                    i += 1;
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
    end_cmd(&mut words, &mut cmds);
    cmds
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
    fn looking_and_changing_run() {
        for cmd in [
            // The diagnostic that asked for an Allow before this.
            r#"psql "$DATABASE_URL" -c "\d box_secrets" -c "SELECT id, created_at, updated_at, left(name, 40) FROM box_secrets;" 2>&1 | head -80; echo "==== secrets dir ===="; sudo ls -la /var/lib/virtues/secrets 2>&1 | head -40; echo "==== channel ===="; sudo ls -la /var/lib/virtues/channel 2>&1; cat /var/lib/virtues/channel 2>/dev/null | head -20; virtues channel 2>&1 | head -30"#,
            r#"strings /usr/local/share/virtues/current/virtues | grep -i -E "onboarding|getting_started""#,
            "journalctl -u virtues --no-pager -n 200 -p warning | grep -i delete",
            "systemctl restart virtues",
            "sudo apt-get install -y jq",
            "echo 'VIRTUES_X=1' | sudo tee -a /etc/virtues/env",
            "sed -i 's/a/b/' /etc/virtues/env",
            "python3 - << 'PY'\nprint(1)\nPY",
            r#"psql "$DATABASE_URL" -c "UPDATE app_user_profile SET onboarding_status = 'onboarding'""#,
            r#"psql "$DATABASE_URL" -c "CREATE TABLE t (id int REFERENCES u ON DELETE CASCADE)""#,
            r#"psql "$DATABASE_URL" -c "SELECT deleted_at FROM app_pages""#,
            "rm -rf /tmp/virtues-debug /var/tmp/x.log",
            "find /tmp/scratch -name '*.json' -delete",
            "dd if=/dev/zero of=/tmp/blob bs=1M count=10",
            "sudo fdisk -l",
            "virtues upgrade --pre",
            "virtues status",
            "curl -X POST localhost:8000/api/setup/skip-onboarding -d '{}'",
            "git checkout main",
        ] {
            assert!(!shell_destroys(cmd), "should run: {cmd}");
        }
    }

    #[test]
    fn deleting_data_asks() {
        for cmd in [
            "rm -rf /var/lib/virtues/lake",
            "rm notes.txt",
            "sudo rm /var/lib/virtues/secrets/github",
            "rm -rf /tmp/../var/lib/virtues",
            "rm -rf /tmp",
            "ls /var/lib/virtues/old | xargs rm -rf",
            "find /var/lib/virtues -name '*.bak' -delete",
            "find /var/lib/virtues -name '*.bak' -exec rm {} ;",
            "truncate -s 0 /var/log/virtues.log",
            "shred -u /etc/virtues/env",
            "sudo dd if=/dev/zero of=/dev/nvme0n1 bs=1M",
            "sudo mkfs.ext4 /dev/sda1",
            "sudo wipefs -a /dev/sda",
            "sudo -u postgres dropdb virtues",
            "sudo virtues reset --yes",
            "virtues uninstall",
            "sudo virtues restore /media/backup/x",
            "sudo bash -c 'rm -rf /var/lib/virtues/applets'",
            "ls && rm -rf ~/x",
            "echo $(rm -rf /home/virtues/x)",
            r#"psql "$DATABASE_URL" -c "DELETE FROM app_chats WHERE id = 1""#,
            r#"sudo -u postgres psql virtues -c "DROP TABLE data_location_point""#,
            r#"psql "$DATABASE_URL" -c "truncate wiki_pages""#,
            "psql \"$DATABASE_URL\" <<'SQL'\nDELETE FROM app_chats;\nSQL",
            r#"echo "DROP SCHEMA applet_x CASCADE" | psql "$DATABASE_URL""#,
            r#"virtues write "DELETE FROM data_x""#,
        ] {
            assert!(shell_destroys(cmd), "should ask: {cmd}");
        }
    }

    #[test]
    fn sql_that_deletes_is_named() {
        for sql in [
            "DELETE FROM app_chats WHERE id = 1",
            "drop table t",
            "ALTER TABLE t DROP COLUMN x",
            "TRUNCATE wiki_pages",
            "WITH gone AS (DELETE FROM t RETURNING id) SELECT count(*) FROM gone",
        ] {
            assert!(sql_destroys(sql), "{sql}");
        }
        for sql in [
            "SELECT onboarding_status FROM app_user_profile",
            "UPDATE app_user_profile SET getting_started_dismissed = '{}'",
            "INSERT INTO t VALUES (1)",
            "SELECT deleted_at, drop_count FROM t",
            "CREATE TABLE t (id int REFERENCES u ON DELETE CASCADE)",
            "VACUUM ANALYZE t",
        ] {
            assert!(!sql_destroys(sql), "{sql}");
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
