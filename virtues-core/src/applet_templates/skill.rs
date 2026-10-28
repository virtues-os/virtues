//! Applet folders written as `SKILL.md`, in the Agent Skills format.
//!
//! The frontmatter holds the spec's keys (`name`, `description`, `license`,
//! `compatibility`, `allowed-tools`, `metadata`); the Virtues wiring lives
//! under `metadata`, where every value is a string. The body is the prompt.
//! [`parse`] reads one into the same [`Template`] a `manifest.toml` produces,
//! so reconcile, the runner and the scheduler treat both alike. The format
//! and every key are described in agents/plan/applet-shape.md.
//!
//! Every problem is returned as a sentence, and the loader refuses the folder
//! when there is any.
//!
//! Parts of the format that have nothing behind them yet are refused rather
//! than accepted and ignored: data triggers (`new <table>`) and SQL in `run`.
//!
//! A `net(host)` entry asks for a host. It is kept in `config.net_hosts`,
//! and a run reaches it only when the owner has also allowed it on the
//! applet's page (`app_applets.net_hosts`), so nothing written in the file
//! opens a way out on its own.

use std::path::Path;

use yaml_rust2::{Yaml, YamlLoader};

use super::{SourceRef, Template};

/// The spec's top-level keys. Nothing else may appear beside them.
const SPEC_KEYS: &[&str] = &[
    "name",
    "description",
    "license",
    "compatibility",
    "allowed-tools",
    "metadata",
];

/// The Virtues keys under `metadata`.
const METADATA_KEYS: &[&str] = &[
    "invoke", "trigger", "run", "source", "limit", "model", "enabled", "icon", "color",
];

const COLORS: &[&str] = &["gray", "red", "orange", "yellow", "green", "blue", "purple", "pink"];

/// Spellings the displayed name keeps instead of lowercasing.
const BRAND_SPELLINGS: &[(&str, &str)] = &[
    ("ai", "AI"),
    ("github", "GitHub"),
    ("gmail", "Gmail"),
    ("imessage", "iMessage"),
    ("ios", "iOS"),
    ("macos", "macOS"),
    ("sql", "SQL"),
];

/// Tools every prompt applet has whatever `allowed-tools` says: thinking and
/// its own notes are not permissions.
pub(crate) const ALWAYS_ALLOWED_TOOLS: &[&str] = &["think", "update_applet_memory"];

/// The `allowed-tools` entry that mounts the whole lake read-only for the
/// applet's code (`applet_runner::sandboxed`).
const LAKE_GRANT: &str = "lake";

/// The largest per-run spend `limit` may set.
const MAX_DOLLARS_PER_RUN: f64 = 10.0;

/// Read a `SKILL.md` into a template.
///
/// `folder` is the applet's folder on disk and `dir` its path relative to
/// the applet root, which the id comes from. `shipped` is whether the folder
/// is in the shipped root, which only the box can write.
pub(super) fn parse(text: &str, folder: &Path, dir: &str, shipped: bool) -> Result<Template, Vec<String>> {
    let mut errors = Vec::new();
    let (front, body) = match split_frontmatter(text) {
        Ok(parts) => parts,
        Err(e) => return Err(vec![e]),
    };
    let doc = match YamlLoader::load_from_str(front) {
        Ok(mut docs) if docs.len() == 1 => docs.remove(0),
        Ok(_) => return Err(vec!["The frontmatter must be one YAML document.".into()]),
        Err(e) => return Err(vec![format!("The frontmatter is not valid YAML: {e}.")]),
    };
    let Yaml::Hash(top) = doc else {
        return Err(vec!["The frontmatter must be a map of keys to values.".into()]);
    };

    for key in top.keys() {
        match key.as_str() {
            Some(k) if SPEC_KEYS.contains(&k) => {}
            Some(k) => errors.push(unknown_key(k, "a top-level key", SPEC_KEYS)),
            None => errors.push("Every frontmatter key must be a string.".into()),
        }
    }
    let get = |key: &str| top.get(&Yaml::String(key.into()));
    let string = |key: &str, errors: &mut Vec<String>| -> Option<String> {
        match get(key)? {
            Yaml::String(s) => Some(s.clone()),
            _ => {
                errors.push(format!("`{key}` must be a string."));
                None
            }
        }
    };

    // Spec keys.
    let folder_name = dir.rsplit('/').next().unwrap_or(dir);
    let name = string("name", &mut errors);
    match &name {
        None if get("name").is_none() => errors.push("`name` is required.".into()),
        None => {}
        Some(n) => {
            if let Err(e) = check_name(n) {
                errors.push(e);
            } else if n != folder_name {
                errors.push(format!(
                    "`name` is \"{n}\" but the folder is \"{folder_name}\". They must be the same."
                ));
            }
        }
    }
    let description = string("description", &mut errors);
    match description.as_deref().map(str::trim) {
        None if get("description").is_none() => errors.push("`description` is required.".into()),
        Some("") => errors.push("`description` is empty. Say what the applet does and when to use it.".into()),
        Some(d) if d.chars().count() > 1024 => {
            errors.push("`description` is longer than 1024 characters.".into())
        }
        _ => {}
    }
    if let Some(c) = string("compatibility", &mut errors) {
        if c.chars().count() > 500 {
            errors.push("`compatibility` is longer than 500 characters.".into());
        }
    }
    let _ = string("license", &mut errors);

    let agent = Some(body.trim()).filter(|b| !b.is_empty()).map(str::to_string);
    let has_prompt = agent.is_some();

    let (allowed_tools, net_hosts) = match string("allowed-tools", &mut errors) {
        Some(s) => {
            let (tools, hosts) = check_allowed_tools(&s, &mut errors);
            (Some(tools), hosts)
        }
        None => (None, Vec::new()),
    };

    // Virtues keys.
    let metadata = match get("metadata") {
        None => Default::default(),
        Some(Yaml::Hash(h)) => {
            let mut out = std::collections::BTreeMap::new();
            for (k, v) in h {
                let Some(k) = k.as_str() else {
                    errors.push("Every `metadata` key must be a string.".into());
                    continue;
                };
                if !METADATA_KEYS.contains(&k) {
                    errors.push(unknown_key(k, "a `metadata` key", METADATA_KEYS));
                    continue;
                }
                match scalar(v) {
                    Some(s) => {
                        out.insert(k.to_string(), s);
                    }
                    None => errors.push(format!("`metadata.{k}` must be a string.")),
                }
            }
            out
        }
        Some(_) => {
            errors.push("`metadata` must be a map of keys to strings.".into());
            Default::default()
        }
    };
    let meta = |key: &str| metadata.get(key).map(String::as_str);

    let invoke = match meta("invoke") {
        Some(v) => parse_invoke(v, &mut errors),
        None if has_prompt => vec![Invoke::Chat, Invoke::Assistant],
        None => Vec::new(),
    };
    if !invoke.is_empty() && !has_prompt {
        errors.push("`invoke` lets someone start it, but there is no prompt below the frontmatter. Add one, or set `invoke: none`.".into());
    }

    let triggers = meta("trigger").map(|v| parse_triggers(v, &mut errors)).unwrap_or_default();
    let command = meta("run").and_then(|v| parse_run(v, folder, shipped, &mut errors));
    let source = meta("source").map(str::trim).map(str::to_string);
    if let Some(s) = &source {
        if s.is_empty() || !s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_') {
            errors.push(format!("`source` \"{s}\" is not a source id, such as `plaid`."));
        }
    }

    let starts_alone = triggers.schedule.is_some() || triggers.webhook;
    if starts_alone && !has_prompt && command.is_none() {
        errors.push("`trigger` starts it, but it has no prompt and no `run`, so a run would do nothing.".into());
    }
    if triggers.webhook && source.is_none() {
        errors.push("A `webhook` trigger needs a `source`, which says whose posts it accepts.".into());
    }

    let mut limits = toml::value::Table::new();
    if let Some(v) = meta("limit") {
        parse_limit(v, &mut limits, &mut errors);
    }
    let model = match meta("model").map(str::trim) {
        None | Some("standard") => "standard",
        Some("deep") => "deep",
        Some(other) => {
            errors.push(format!("`model` is \"{other}\". Use `standard` or `deep`."));
            "standard"
        }
    };
    let enabled = match meta("enabled").map(str::trim) {
        None | Some("true") => true,
        Some("false") => false,
        Some(other) => {
            errors.push(format!("`enabled` is \"{other}\". Use `true` or `false`."));
            true
        }
    };
    if let Some(icon) = meta("icon") {
        let ok = icon
            .strip_prefix("ri:")
            .is_some_and(|i| !i.is_empty() && i.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'));
        if !ok {
            errors.push(format!("`icon` is \"{icon}\". Use a Remix icon id such as `ri:calendar-line`."));
        }
    }
    if let Some(color) = meta("color") {
        if !COLORS.contains(&color.trim()) {
            errors.push(format!("`color` is \"{color}\". Use one of: {}.", COLORS.join(", ")));
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }
    let name = name.expect("checked above");

    // The run's triggers. Run now, and the thread for a prompt, are always
    // there; a webhook-only applet starts only from its posts.
    let mut trigger_list: Vec<String> = Vec::new();
    if (has_prompt || command.is_some()) && !(triggers.webhook && triggers.schedule.is_none()) {
        trigger_list.push("manual".into());
    }
    if has_prompt {
        trigger_list.push("message".into());
    }
    if invoke.contains(&Invoke::Assistant) {
        trigger_list.push("tool".into());
    }
    if triggers.schedule.is_some() {
        trigger_list.push("cron".into());
    }
    if triggers.webhook {
        trigger_list.push("webhook".into());
    }

    let mut config = toml::value::Table::new();
    if !limits.is_empty() {
        config.insert("limits".into(), toml::Value::Table(limits));
    }
    if has_prompt {
        config.insert("model_slot".into(), toml::Value::String(model.into()));
    }
    if let Some(tools) = allowed_tools {
        config.insert(
            "allowed_tools".into(),
            toml::Value::Array(tools.into_iter().map(toml::Value::String).collect()),
        );
    }
    if !net_hosts.is_empty() {
        config.insert(
            "net_hosts".into(),
            toml::Value::Array(net_hosts.into_iter().map(toml::Value::String).collect()),
        );
    }

    Ok(Template {
        id_prefix: Some(id_for_dir(dir)),
        name: display_name(&name),
        description: description.map(|d| d.trim().to_string()),
        owner: owner_for(dir, shipped).into(),
        triggers: trigger_list,
        schedule: triggers.schedule,
        default_enabled: enabled,
        condition: None,
        until: triggers.until,
        agent,
        per_credential: source.is_some(),
        source: source.map(|id| SourceRef { id }),
        command,
        config: (!config.is_empty()).then_some(toml::Value::Table(config)),
        writes: Vec::new(),
        needs_vault_key: false,
        secrets: Vec::new(),
        forked_from: None,
        dir: dir.to_string(),
        unknown: Default::default(),
    })
}

/// The id a folder gets. The spec's names use hyphens and ids, schemas and
/// the applet slug use underscores, so a hyphen becomes an underscore.
fn id_for_dir(dir: &str) -> String {
    format!("applet_{}", dir.replace('/', "__").replace('-', "_"))
}

/// Who the row belongs to, from where the folder is. A manifest's own
/// `owner` is not used: anyone who can write the folder could set it.
fn owner_for(dir: &str, shipped: bool) -> &'static str {
    if shipped {
        "system"
    } else if dir.starts_with("user/") {
        "ai"
    } else {
        "user"
    }
}

/// `weekly-review` is shown as "Weekly review".
fn display_name(name: &str) -> String {
    let words: Vec<String> = name
        .split('-')
        .enumerate()
        .map(|(i, w)| match BRAND_SPELLINGS.iter().find(|(k, _)| *k == w) {
            // A brand keeps its spelling even first: "iOS battery check".
            Some((_, brand)) => brand.to_string(),
            None if i == 0 => {
                let mut chars = w.chars();
                chars.next().map_or_else(String::new, |c| c.to_ascii_uppercase().to_string() + chars.as_str())
            }
            None => w.to_string(),
        })
        .collect();
    words.join(" ")
}

/// The frontmatter between the opening and closing `---` lines, and the body.
fn split_frontmatter(text: &str) -> Result<(&str, &str), String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let missing = || "SKILL.md must start with frontmatter between two `---` lines.".to_string();
    let rest = text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))
        .ok_or_else(missing)?;
    let mut offset = 0;
    for line in rest.split_inclusive('\n') {
        if line.trim_end_matches(['\r', '\n']) == "---" {
            return Ok((&rest[..offset], &rest[offset + line.len()..]));
        }
        offset += line.len();
    }
    Err(missing())
}

/// A YAML scalar as the string the spec says it is. An unquoted `true` or
/// `5` is still the string "true" or "5".
fn scalar(v: &Yaml) -> Option<String> {
    match v {
        Yaml::String(s) | Yaml::Real(s) => Some(s.clone()),
        Yaml::Integer(i) => Some(i.to_string()),
        Yaml::Boolean(b) => Some(b.to_string()),
        _ => None,
    }
}

fn unknown_key(key: &str, what: &str, known: &[&str]) -> String {
    let closest = known
        .iter()
        .map(|k| (crate::tools::applet_setup::levenshtein(key, k), *k))
        .min()
        .filter(|(d, _)| *d <= 3);
    match closest {
        Some((_, k)) => format!("`{key}` is not {what}. Did you mean `{k}`?"),
        None => format!("`{key}` is not {what}. Use one of: {}.", known.join(", ")),
    }
}

/// The spec's rule for `name`: 1–64 lowercase letters, digits and single
/// hyphens, not starting or ending with a hyphen.
fn check_name(name: &str) -> Result<(), String> {
    let ok = !name.is_empty()
        && name.len() <= 64
        && name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !name.starts_with('-')
        && !name.ends_with('-')
        && !name.contains("--");
    if ok {
        Ok(())
    } else {
        Err(format!(
            "`name` \"{name}\" must be 1 to 64 lowercase letters, digits and single hyphens, \
             not starting or ending with a hyphen."
        ))
    }
}

/// The tool ids an applet asked for, checked against what an applet's run
/// can be given, and the hosts its `net(host)` entries ask for.
fn check_allowed_tools(list: &str, errors: &mut Vec<String>) -> (Vec<String>, Vec<String>) {
    let available = crate::tools::applet_run_tool_ids();
    let known: Vec<String> = virtues_registry::tools::default_tools().into_iter().map(|t| t.id).collect();
    let mut out = Vec::new();
    let mut hosts: Vec<String> = Vec::new();
    for entry in list.split_whitespace() {
        if let Some(rest) = entry.strip_prefix("net(") {
            let Some(host) = rest.strip_suffix(')') else {
                errors.push(format!("`{entry}` must be written as `net(host)`, such as `net(api.weather.gov)`."));
                continue;
            };
            match crate::egress::policy::check_host(host) {
                Ok(h) if !hosts.contains(&h) => hosts.push(h),
                Ok(_) => {}
                Err(why) => errors.push(format!("`{entry}`: {why}")),
            }
        } else if entry == LAKE_GRANT {
            // Not a tool: the whole lake, read-only, for the applet's code.
            if !out.iter().any(|t| t == entry) {
                out.push(entry.to_string());
            }
        } else if ALWAYS_ALLOWED_TOOLS.contains(&entry) {
            // Always given; listing it changes nothing.
        } else if available.contains(&entry) {
            if !out.iter().any(|t| t == entry) {
                out.push(entry.to_string());
            }
        } else if known.iter().any(|k| k == entry) {
            errors.push(format!("`{entry}` is not available to an applet's run."));
        } else {
            let names: Vec<&str> = available.to_vec();
            errors.push(unknown_key(entry, "a tool an applet can use", &names));
        }
    }
    (out, hosts)
}

#[derive(Debug, PartialEq)]
enum Invoke {
    Chat,
    Assistant,
}

fn parse_invoke(value: &str, errors: &mut Vec<String>) -> Vec<Invoke> {
    if value.trim() == "none" {
        return Vec::new();
    }
    let mut out = Vec::new();
    for part in value.split(',').map(str::trim) {
        match part {
            "chat" => out.push(Invoke::Chat),
            "assistant" => out.push(Invoke::Assistant),
            other => errors.push(format!(
                "`invoke` has \"{other}\". Use `chat`, `assistant`, both separated by a comma, or `none`."
            )),
        }
    }
    out
}

#[derive(Debug, Default)]
struct Triggers {
    /// A six-field cron in the box's timezone.
    schedule: Option<String>,
    /// Archive after a successful run: `once`, or a date check in SQL.
    until: Option<String>,
    webhook: bool,
}

fn parse_triggers(value: &str, errors: &mut Vec<String>) -> Triggers {
    let mut out = Triggers::default();
    for part in value.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        if part == "webhook" {
            out.webhook = true;
            continue;
        }
        let words: Vec<&str> = part.split_whitespace().collect();
        if words.first() == Some(&"new") || words.contains(&"before") || words.last() == Some(&"ends") {
            errors.push(format!(
                "\"{part}\": an applet cannot start on new data yet. Use a schedule, such as `every day at 09:00`."
            ));
            continue;
        }
        match parse_time_trigger(&words) {
            Ok((cron, until)) => {
                if out.schedule.is_some() {
                    errors.push("`trigger` has more than one schedule. An applet has one.".into());
                    continue;
                }
                out.schedule = Some(cron);
                out.until = until;
            }
            Err(e) => errors.push(format!("\"{part}\": {e}")),
        }
    }
    out
}

const TIME_HELP: &str = "Use a schedule such as `every day at 07:00`, `every monday at 09:00`, \
                         `every 15 minutes`, `on 2026-11-02 at 09:00`, or `cron` and six fields.";

/// One time trigger as a six-field cron, and the `until` it implies.
fn parse_time_trigger(words: &[&str]) -> Result<(String, Option<String>), String> {
    // `cron` takes the rest verbatim.
    if words.first() == Some(&"cron") {
        let expr = words[1..].join(" ");
        if words.len() != 7 || crate::scheduler::slots::parse(&expr).is_none() {
            return Err(format!("`cron {expr}` is not a six-field cron (seconds first)."));
        }
        return Ok((expr, None));
    }

    if words.first() == Some(&"on") {
        // on YYYY-MM-DD at HH:MM
        let [_, date, "at", time] = words else {
            return Err(TIME_HELP.into());
        };
        let date = parse_date(date)?;
        let (h, m) = parse_clock(time)?;
        use chrono::Datelike;
        return Ok((format!("0 {m} {h} {} {} *", date.day(), date.month()), Some("once".into())));
    }

    if words.first() != Some(&"every") {
        return Err(TIME_HELP.into());
    }
    let (schedule, until) = match words.iter().position(|w| *w == "until") {
        Some(i) => {
            let [date] = &words[i + 1..] else {
                return Err("`until` takes one date, such as `until 2026-12-01`.".into());
            };
            let date = parse_date(date)?;
            (&words[1..i], Some(format!("current_date >= date '{date}'")))
        }
        None => (&words[1..], None),
    };

    let cron = match schedule {
        ["minute"] => "0 * * * * *".to_string(),
        ["hour"] => "0 0 * * * *".to_string(),
        [n, "minutes"] => format!("0 */{} * * * *", parse_step(n, 59)?),
        [n, "hours"] => format!("0 0 */{} * * *", parse_step(n, 23)?),
        ["day", "at", t] => {
            let (h, m) = parse_clock(t)?;
            format!("0 {m} {h} * * *")
        }
        ["weekday", "at", t] => {
            let (h, m) = parse_clock(t)?;
            format!("0 {m} {h} * * 1-5")
        }
        ["month", "on", "day", d, "at", t] => {
            let day: u32 = d
                .parse()
                .ok()
                .filter(|d| (1..=31).contains(d))
                .ok_or_else(|| format!("\"{d}\" is not a day of the month."))?;
            let (h, m) = parse_clock(t)?;
            format!("0 {m} {h} {day} * *")
        }
        [.., "at", t] => {
            // every monday and thursday at 09:00
            let days = &schedule[..schedule.len() - 2];
            let mut numbers = Vec::new();
            for (i, w) in days.iter().enumerate() {
                if i % 2 == 1 {
                    if *w != "and" {
                        return Err(TIME_HELP.into());
                    }
                    continue;
                }
                numbers.push(weekday_number(w).ok_or_else(|| TIME_HELP.to_string())?);
            }
            if numbers.is_empty() || days.len() % 2 == 0 {
                return Err(TIME_HELP.into());
            }
            let (h, m) = parse_clock(t)?;
            let list: Vec<String> = numbers.iter().map(u32::to_string).collect();
            format!("0 {m} {h} * * {}", list.join(","))
        }
        _ => return Err(TIME_HELP.into()),
    };
    Ok((cron, until))
}

fn weekday_number(w: &str) -> Option<u32> {
    Some(match w {
        "sunday" => 0,
        "monday" => 1,
        "tuesday" => 2,
        "wednesday" => 3,
        "thursday" => 4,
        "friday" => 5,
        "saturday" => 6,
        _ => return None,
    })
}

fn parse_clock(t: &str) -> Result<(u32, u32), String> {
    let bad = || format!("\"{t}\" is not a time. Write it as 24-hour HH:MM, such as 07:00.");
    let (h, m) = t.split_once(':').ok_or_else(bad)?;
    if h.len() != 2 || m.len() != 2 {
        return Err(bad());
    }
    let h: u32 = h.parse().map_err(|_| bad())?;
    let m: u32 = m.parse().map_err(|_| bad())?;
    if h > 23 || m > 59 {
        return Err(bad());
    }
    Ok((h, m))
}

fn parse_date(d: &str) -> Result<chrono::NaiveDate, String> {
    chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d")
        .map_err(|_| format!("\"{d}\" is not a date. Write it as YYYY-MM-DD."))
}

fn parse_step(n: &str, max: u32) -> Result<u32, String> {
    n.parse::<u32>()
        .ok()
        .filter(|n| (2..=max).contains(n))
        .ok_or_else(|| format!("\"{n}\" must be a whole number from 2 to {max}."))
}

/// `run` as the argv the runner spawns.
fn parse_run(value: &str, folder: &Path, shipped: bool, errors: &mut Vec<String>) -> Option<Vec<String>> {
    let value = value.trim();
    if value.starts_with("sql ") || value.ends_with(".sql") {
        errors.push("SQL in `run` is not supported yet. Use a script under `scripts/`.".into());
        return None;
    }
    if value.contains('/') {
        let Some(rel) = value.strip_prefix("scripts/") else {
            errors.push(format!("`run` \"{value}\" must be a file under `scripts/`."));
            return None;
        };
        if rel.is_empty() || rel.split('/').any(|p| p.is_empty() || p == "." || p == "..") {
            errors.push(format!("`run` \"{value}\" must be a file under `scripts/`."));
            return None;
        }
        let path = folder.join(value);
        if !path.is_file() {
            errors.push(format!("`run` names {value}, which is not in the folder."));
            return None;
        }
        let path = path.to_string_lossy().into_owned();
        return Some(match Path::new(value).extension().and_then(|e| e.to_str()) {
            Some("py") => vec!["python3".into(), path],
            Some("sh") => vec!["sh".into(), path],
            _ => vec![path],
        });
    }
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_') {
        errors.push(format!(
            "`run` \"{value}\" must be a script under `scripts/`, or the name of a binary that ships with the box."
        ));
        return None;
    }
    if !shipped {
        errors.push(format!(
            "`run` \"{value}\" names a binary. Only an applet that ships with the box can do that; put the code under `scripts/`."
        ));
        return None;
    }
    Some(vec![value.to_string()])
}

/// `limit`: `$N per run`, `$N per day` and a duration, comma-separated.
fn parse_limit(value: &str, limits: &mut toml::value::Table, errors: &mut Vec<String>) {
    let help = "Use `$N per run`, `$N per day` or a duration such as `10 minutes`, separated by commas.";
    for part in value.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let words: Vec<&str> = part.split_whitespace().collect();
        let (key, number) = match words.as_slice() {
            [amount, "per", "run"] if amount.starts_with('$') => ("max_llm_cost", dollars(amount)),
            [amount, "per", "day"] if amount.starts_with('$') => ("max_llm_cost_per_day", dollars(amount)),
            [n, unit] => match seconds_per(unit) {
                Some(per) => ("timeout_s", n.parse::<f64>().ok().filter(|n| *n > 0.0).map(|n| n * per)),
                None => {
                    errors.push(format!("\"{part}\" in `limit`. {help}"));
                    continue;
                }
            },
            _ => {
                errors.push(format!("\"{part}\" in `limit`. {help}"));
                continue;
            }
        };
        let Some(number) = number else {
            errors.push(format!("\"{part}\" in `limit` needs a positive number. {help}"));
            continue;
        };
        if key == "max_llm_cost" && number > MAX_DOLLARS_PER_RUN {
            errors.push(format!("`limit` allows at most ${MAX_DOLLARS_PER_RUN} per run."));
            continue;
        }
        if limits.contains_key(key) {
            errors.push(format!("\"{part}\" repeats a limit `limit` already sets."));
            continue;
        }
        let value = if key == "timeout_s" {
            toml::Value::Integer(number.ceil() as i64)
        } else {
            toml::Value::Float(number)
        };
        limits.insert(key.into(), value);
    }
}

fn dollars(amount: &str) -> Option<f64> {
    amount[1..].parse::<f64>().ok().filter(|n| n.is_finite() && *n > 0.0)
}

fn seconds_per(unit: &str) -> Option<f64> {
    match unit {
        "second" | "seconds" => Some(1.0),
        "minute" | "minutes" => Some(60.0),
        "hour" | "hours" => Some(3600.0),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    fn skill(front: &str, body: &str) -> String {
        format!("---\n{front}\n---\n{body}")
    }

    fn ok(front: &str, body: &str) -> Template {
        let f = folder();
        parse(&skill(front, body), f.path(), "weekly-review", false)
            .unwrap_or_else(|e| panic!("expected it to load: {e:?}"))
    }

    /// The problems a folder is refused for, joined, so a test can look for
    /// the sentence it expects.
    fn refused(front: &str, body: &str) -> String {
        let f = folder();
        match parse(&skill(front, body), f.path(), "weekly-review", false) {
            Ok(_) => panic!("expected it to be refused:\n{front}"),
            Err(e) => e.join("\n"),
        }
    }

    const BASE: &str = "name: weekly-review\ndescription: Looks back over your week.";

    #[test]
    fn a_minimal_skill_is_a_prompt_applet_you_and_the_assistant_can_start() {
        let t = ok(BASE, "Write a short review of the week.\n");
        assert_eq!(t.id_prefix.as_deref(), Some("applet_weekly_review"));
        assert_eq!(t.name, "Weekly review");
        assert_eq!(t.description.as_deref(), Some("Looks back over your week."));
        assert_eq!(t.agent.as_deref(), Some("Write a short review of the week."));
        assert_eq!(t.owner, "user");
        assert_eq!(t.triggers, ["manual", "message", "tool"]);
        assert!(t.schedule.is_none());
        assert!(t.default_enabled);
        let cfg = t.config.unwrap();
        assert_eq!(cfg["model_slot"].as_str(), Some("standard"));
    }

    #[test]
    fn the_wiring_under_metadata_maps_onto_the_row() {
        let t = ok(
            &format!(
                "{BASE}\nallowed-tools: sql_query create_page think\nmetadata:\n  \
                 trigger: every sunday at 18:00\n  limit: $1 per run, $3 per day, 10 minutes\n  \
                 model: deep\n  enabled: false\n  invoke: chat\n  color: blue\n  icon: ri:calendar-line"
            ),
            "Review the week.",
        );
        assert_eq!(t.schedule.as_deref(), Some("0 0 18 * * 0"));
        assert_eq!(t.triggers, ["manual", "message", "cron"]);
        assert!(!t.default_enabled);
        let cfg = t.config.unwrap();
        assert_eq!(cfg["model_slot"].as_str(), Some("deep"));
        assert_eq!(cfg["limits"]["max_llm_cost"].as_float(), Some(1.0));
        assert_eq!(cfg["limits"]["max_llm_cost_per_day"].as_float(), Some(3.0));
        assert_eq!(cfg["limits"]["timeout_s"].as_integer(), Some(600));
        let tools: Vec<&str> = cfg["allowed_tools"].as_array().unwrap().iter().filter_map(|v| v.as_str()).collect();
        assert_eq!(tools, ["sql_query", "create_page"], "think is always given and not listed");
    }

    #[test]
    fn time_triggers_become_cron_in_the_box_timezone() {
        let cases = [
            ("every day at 07:00", "0 0 7 * * *", None),
            ("every weekday at 08:30", "0 30 8 * * 1-5", None),
            ("every monday at 09:00", "0 0 9 * * 1", None),
            ("every monday and thursday at 09:00", "0 0 9 * * 1,4", None),
            ("every month on day 1 at 09:00", "0 0 9 1 * *", None),
            ("every hour", "0 0 * * * *", None),
            ("every 15 minutes", "0 */15 * * * *", None),
            ("every 2 hours", "0 0 */2 * * *", None),
            ("cron 0 0 7 * * *", "0 0 7 * * *", None),
            ("on 2026-11-02 at 09:00", "0 0 9 2 11 *", Some("once")),
            (
                "every day at 07:00 until 2026-12-01",
                "0 0 7 * * *",
                Some("current_date >= date '2026-12-01'"),
            ),
        ];
        for (trigger, cron, until) in cases {
            let t = ok(&format!("{BASE}\nmetadata:\n  trigger: {trigger}"), "Do it.");
            assert_eq!(t.schedule.as_deref(), Some(cron), "{trigger}");
            assert_eq!(t.until.as_deref(), until, "{trigger}");
            assert!(crate::scheduler::slots::parse(cron).is_some(), "{cron} must parse");
        }
    }

    #[test]
    fn a_script_runs_from_the_folder_and_a_webhook_needs_a_source() {
        let f = folder();
        std::fs::create_dir_all(f.path().join("scripts")).unwrap();
        std::fs::write(f.path().join("scripts/fetch.py"), "print('{}')").unwrap();
        let text = skill(
            "name: tides\ndescription: Fetches tide times.\nmetadata:\n  run: scripts/fetch.py\n  trigger: webhook\n  source: example_source",
            "",
        );
        let t = parse(&text, f.path(), "tides", false).unwrap();
        let cmd = t.command.unwrap();
        assert_eq!(cmd[0], "python3");
        assert!(cmd[1].ends_with("scripts/fetch.py") && Path::new(&cmd[1]).is_absolute());
        assert_eq!(t.triggers, ["webhook"], "a webhook-only applet has no Run now");
        assert!(t.per_credential);
        assert!(t.agent.is_none());
    }

    #[test]
    fn where_the_folder_is_decides_the_owner_and_a_shipped_one_may_name_a_binary() {
        let f = folder();
        let text = skill("name: plaid-sync\ndescription: Syncs.\nmetadata:\n  run: plaid_transactions_sync\n  trigger: every hour", "");
        let t = parse(&text, f.path(), "plaid-sync", true).unwrap();
        assert_eq!(t.owner, "system");
        assert_eq!(t.command.unwrap(), ["plaid_transactions_sync"]);

        let text = skill("name: mine\ndescription: Mine.", "Hello.");
        assert_eq!(parse(&text, f.path(), "user/mine", false).unwrap().owner, "ai");
        assert_eq!(
            parse(&text, f.path(), "user/mine", false).unwrap().id_prefix.as_deref(),
            Some("applet_user__mine")
        );
    }

    #[test]
    fn metadata_values_are_strings_but_an_unquoted_scalar_counts_as_one() {
        let t = ok(&format!("{BASE}\nmetadata:\n  enabled: false"), "Do it.");
        assert!(!t.default_enabled);
        let e = refused(&format!("{BASE}\nmetadata:\n  trigger:\n    - every hour"), "Do it.");
        assert!(e.contains("`metadata.trigger` must be a string"), "{e}");
    }

    // Each check refuses.

    #[test]
    fn frontmatter_must_be_there_and_be_yaml() {
        let f = folder();
        let e = parse("# Weekly review\n", f.path(), "weekly-review", false).unwrap_err();
        assert!(e[0].contains("must start with frontmatter"), "{e:?}");
        let e = parse("---\nname: [\n---\n", f.path(), "weekly-review", false).unwrap_err();
        assert!(e[0].contains("not valid YAML"), "{e:?}");
    }

    #[test]
    fn name_follows_the_spec_and_matches_the_folder() {
        for bad in ["Weekly-Review", "-weekly", "weekly-", "weekly--review", "weekly_review"] {
            let e = refused(&format!("name: {bad}\ndescription: x"), "Do it.");
            assert!(e.contains("must be 1 to 64 lowercase"), "{bad}: {e}");
        }
        let e = refused(&format!("name: {}\ndescription: x", "a".repeat(65)), "Do it.");
        assert!(e.contains("must be 1 to 64"), "{e}");
        let e = refused("name: monthly-review\ndescription: x", "Do it.");
        assert!(e.contains("the folder is \"weekly-review\""), "{e}");
        let e = refused("description: x", "Do it.");
        assert!(e.contains("`name` is required"), "{e}");
    }

    #[test]
    fn description_is_required_and_at_most_1024_characters() {
        let e = refused("name: weekly-review", "Do it.");
        assert!(e.contains("`description` is required"), "{e}");
        let e = refused("name: weekly-review\ndescription: \"  \"", "Do it.");
        assert!(e.contains("`description` is empty"), "{e}");
        let e = refused(&format!("name: weekly-review\ndescription: {}", "x".repeat(1025)), "Do it.");
        assert!(e.contains("longer than 1024"), "{e}");
        let e = refused(&format!("{BASE}\ncompatibility: {}", "x".repeat(501)), "Do it.");
        assert!(e.contains("`compatibility` is longer than 500"), "{e}");
    }

    #[test]
    fn an_unknown_key_names_the_closest_one() {
        let e = refused(&format!("{BASE}\nmetadata:\n  triger: every hour"), "Do it.");
        assert!(e.contains("Did you mean `trigger`?"), "{e}");
        let e = refused(&format!("{BASE}\nallowed_tools: sql_query"), "Do it.");
        assert!(e.contains("Did you mean `allowed-tools`?"), "{e}");
        let e = refused(&format!("{BASE}\nuser-invocable: false"), "Do it.");
        assert!(e.contains("`user-invocable` is not a top-level key"), "{e}");
    }

    #[test]
    fn triggers_that_cannot_fire_are_refused() {
        for (trigger, expect) in [
            ("every day at 7am", "is not a time"),
            ("every day at 24:00", "is not a time"),
            ("every fortnight", "Use a schedule such as"),
            ("every 1 minutes", "from 2 to 59"),
            ("every month on day 32 at 09:00", "is not a day of the month"),
            ("cron 0 0 7 * *", "is not a six-field cron"),
            ("on 2026-13-01 at 09:00", "is not a date"),
            ("every hour, every day at 07:00", "more than one schedule"),
            ("new data_health_heart_rate", "cannot start on new data yet"),
            ("30 minutes before data_calendar_event", "cannot start on new data yet"),
        ] {
            let e = refused(&format!("{BASE}\nmetadata:\n  trigger: {trigger}"), "Do it.");
            assert!(e.contains(expect), "{trigger}: {e}");
        }
    }

    #[test]
    fn a_trigger_or_invoke_needs_something_to_run() {
        let e = refused(&format!("{BASE}\nmetadata:\n  trigger: every hour\n  invoke: none"), "");
        assert!(e.contains("no prompt and no `run`"), "{e}");
        let e = refused(&format!("{BASE}\nmetadata:\n  invoke: chat"), "");
        assert!(e.contains("there is no prompt"), "{e}");
        let e = refused(&format!("{BASE}\nmetadata:\n  trigger: webhook"), "Do it.");
        assert!(e.contains("needs a `source`"), "{e}");
        let e = refused(&format!("{BASE}\nmetadata:\n  invoke: everyone"), "Do it.");
        assert!(e.contains("`invoke` has \"everyone\""), "{e}");
    }

    #[test]
    fn run_is_a_script_in_the_folder_or_a_shipped_binary() {
        for (run, expect) in [
            ("sql select 1", "SQL in `run` is not supported yet"),
            ("scripts/check.sql", "SQL in `run` is not supported yet"),
            ("../escape.py", "must be a file under `scripts/`"),
            ("scripts/../../etc/passwd", "must be a file under `scripts/`"),
            ("scripts/missing.py", "which is not in the folder"),
            ("plaid_transactions_sync", "Only an applet that ships with the box"),
            ("rm -rf", "must be a script under `scripts/`"),
        ] {
            let e = refused(&format!("{BASE}\nmetadata:\n  run: {run}"), "");
            assert!(e.contains(expect), "{run}: {e}");
        }
    }

    #[test]
    fn allowed_tools_lists_tools_an_applet_run_can_have() {
        let t = ok(&format!("{BASE}\nallowed-tools: sql_query net(API.Weather.gov) net(api.weather.gov)"), "Do it.");
        let cfg = t.config.unwrap();
        assert_eq!(cfg["net_hosts"].as_array().unwrap().len(), 1, "one host, normalized: {cfg:?}");
        assert_eq!(cfg["net_hosts"][0].as_str(), Some("api.weather.gov"));
        for (entry, expect) in [
            ("net(93.184.216.34)", "is an IP address"),
            ("net(gist.github.com)", "refused even when listed"),
            ("net(api.weather.gov", "must be written as `net(host)`"),
        ] {
            let e = refused(&format!("{BASE}\nallowed-tools: {entry}"), "Do it.");
            assert!(e.contains(expect), "{entry}: {e}");
        }
        let e = refused(&format!("{BASE}\nallowed-tools: shell"), "Do it.");
        assert!(e.contains("`shell` is not available to an applet's run"), "{e}");
        let e = refused(&format!("{BASE}\nallowed-tools: sql_qeury"), "Do it.");
        assert!(e.contains("Did you mean `sql_query`?"), "{e}");
        let t = ok(&format!("{BASE}\nallowed-tools: sql_query lake"), "Do it.");
        let tools: Vec<String> = t.config.unwrap()["allowed_tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        assert_eq!(tools, ["sql_query", "lake"], "lake is a grant, kept beside the tools");
    }

    #[test]
    fn limit_model_enabled_icon_and_color_take_only_their_values() {
        for (key, value, expect) in [
            ("limit", "$11 per run", "at most $10 per run"),
            ("limit", "$1 per week", "in `limit`"),
            ("limit", "$0 per run", "needs a positive number"),
            ("limit", "$1 per run, $2 per run", "repeats a limit"),
            ("model", "smart", "Use `standard` or `deep`"),
            ("enabled", "yes", "Use `true` or `false`"),
            ("icon", "calendar", "Remix icon id"),
            ("color", "\"#ff0000\"", "Use one of: gray"),
        ] {
            let e = refused(&format!("{BASE}\nmetadata:\n  {key}: {value}"), "Do it.");
            assert!(e.contains(expect), "{key}: {value}: {e}");
        }
    }

    #[test]
    fn the_displayed_name_keeps_brand_spellings() {
        assert_eq!(display_name("weekly-review"), "Weekly review");
        assert_eq!(display_name("ios-battery-check"), "iOS battery check");
        assert_eq!(display_name("sync-github-stars"), "Sync GitHub stars");
    }
}
