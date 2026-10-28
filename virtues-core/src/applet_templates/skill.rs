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
//! than accepted and ignored: data triggers (`new <table>`), and `new_rows`
//! in a `run` SQL, which only a data trigger fills.
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
    "invoke", "trigger", "run", "source", "limit", "model", "enabled", "icon", "color", "writes", "secrets",
    "forked-from",
];

/// Shipped applets that re-encrypt stored secrets and so are given the
/// vault key (`VIRTUES_ENCRYPTION_KEY`). A kernel list rather than a key in
/// the file, so no folder can ask for it.
const VAULT_KEY_APPLETS: &[&str] = &["credential-refresh"];

/// Shipped applets that are the owner's to change once installed, as a
/// starting point: a prompt edit of theirs is kept, and a shipped fix reaches
/// them only while they have not edited it (reconcile's `user` branch).
const SHIPPED_STARTERS: &[&str] = &["daily-readings", "dot-cloud", "morning-examen"];

/// The most a `view.html` may weigh.
const VIEW_MAX_BYTES: u64 = 48 * 1024;

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

/// Tools every unattended run has whatever `allowed-tools` says: thinking and
/// its own notes are not permissions. A turn `/name` starts has only what is
/// listed, `think` included.
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
            let (tools, hosts) = check_allowed_tools(&s, shipped, &mut errors);
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
    if invoke.contains(&Invoke::Chat) && !has_prompt {
        errors.push("`invoke: chat` starts it as a chat turn, but there is no prompt below the frontmatter. Add one, or leave `chat` out.".into());
    }

    let triggers = meta("trigger").map(|v| parse_triggers(v, &mut errors)).unwrap_or_default();
    let (command, run_sql) = match meta("run").and_then(|v| parse_run(v, folder, shipped, &mut errors)) {
        Some(Run::Command(argv)) => (Some(argv), None),
        Some(Run::Sql(sql)) => (None, Some(sql)),
        None => (None, None),
    };
    let source = meta("source").map(str::trim).map(str::to_string);
    if let Some(s) = &source {
        if s.is_empty() || !s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_') {
            errors.push(format!("`source` \"{s}\" is not a source id, such as `plaid`."));
        }
    }

    if invoke.contains(&Invoke::Assistant) && !has_prompt && command.is_none() && run_sql.is_none() {
        errors.push("`invoke: assistant` lets the assistant start it, but it has no prompt and no `run`, so a run would do nothing.".into());
    }
    let starts_alone = triggers.schedule.is_some() || triggers.webhook;
    if starts_alone && !has_prompt && command.is_none() && run_sql.is_none() {
        errors.push("`trigger` starts it, but it has no prompt and no `run`, so a run would do nothing.".into());
    }
    if triggers.webhook && source.is_none() {
        errors.push("A `webhook` trigger needs a `source`, which says whose posts it accepts.".into());
    }

    let mut limits = toml::value::Table::new();
    if let Some(v) = meta("limit") {
        parse_limit(v, &mut limits, &mut errors);
    }
    // `lite` is the background model unattended applets ran on before this
    // format, kept so a converted applet does not change what it costs.
    let model = match meta("model").map(str::trim) {
        None | Some("standard") => "standard",
        Some("deep") => "deep",
        Some("lite") => "lite",
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
    // The data streams its code writes, by registry name, for the streams
    // page. Kept until each run's record of what it wrote replaces it.
    let writes: Vec<String> = meta("writes").map(list).unwrap_or_default();
    if !writes.is_empty() {
        let known: Vec<&str> = virtues_registry::registered_ontologies().iter().map(|o| o.name).collect();
        for w in &writes {
            if !known.contains(&w.as_str()) {
                errors.push(unknown_key(w, "a data stream", &known));
            }
        }
    }
    // The owner's keys its code needs, set in Settings under Keys.
    let secrets: Vec<String> = meta("secrets").map(list).unwrap_or_default();
    for s in &secrets {
        if !super::valid_secret_name(s) {
            errors.push(format!(
                "`secrets` has \"{s}\". A key name is capitals, digits and underscores, starting with a \
                 capital, and not one the box sets itself."
            ));
        }
    }
    if let Ok(meta) = folder.join("view.html").metadata() {
        if meta.len() > VIEW_MAX_BYTES {
            errors.push(format!("view.html is {} KB. The limit is 48 KB.", meta.len() / 1024));
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
    if (has_prompt || command.is_some() || run_sql.is_some()) && !(triggers.webhook && triggers.schedule.is_none()) {
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
        run_sql,
        config: (!config.is_empty()).then_some(toml::Value::Table(config)),
        writes,
        needs_vault_key: shipped && VAULT_KEY_APPLETS.contains(&name.as_str()),
        secrets,
        // Where a fork came from, `virtues@<version>`, for the source pane.
        forked_from: meta("forked-from").map(|f| f.trim().to_string()),
        invoke_chat: invoke.contains(&Invoke::Chat),
        icon: meta("icon").map(|i| i.trim().to_string()),
        dir: dir.to_string(),
        unknown: Default::default(),
    })
}

/// What a writer puts in a `SKILL.md`: the fields setup and the skills move
/// set. [`render`] turns it into the file and [`parse`] reads it back.
#[derive(Debug)]
pub(crate) struct SkillMd {
    pub name: String,
    pub description: String,
    pub allowed_tools: Option<Vec<String>>,
    /// Empty writes no `invoke`, which defaults by whether there is a prompt.
    pub invoke: Vec<String>,
    /// `trigger`'s value: `cron 0 0 7 * * *, webhook`.
    pub trigger: Option<String>,
    /// `run`'s value: a binary, `scripts/…`, or `sql …`.
    pub run: Option<String>,
    pub source: Option<String>,
    /// `limit`'s value, as it reads in the file: `$3 per run, 10 minutes`.
    pub limit: Option<String>,
    pub deep: bool,
    /// `model: lite`, which converted unattended applets keep.
    pub lite: bool,
    /// `enabled: false` when false.
    pub enabled: bool,
    pub writes: Vec<String>,
    pub secrets: Vec<String>,
    pub forked_from: Option<String>,
    pub icon: Option<String>,
    /// The prompt.
    pub body: String,
}

impl Default for SkillMd {
    fn default() -> Self {
        Self {
            name: String::new(),
            description: String::new(),
            allowed_tools: None,
            invoke: Vec::new(),
            trigger: None,
            run: None,
            source: None,
            limit: None,
            deep: false,
            lite: false,
            enabled: true,
            writes: Vec::new(),
            secrets: Vec::new(),
            forked_from: None,
            icon: None,
            body: String::new(),
        }
    }
}

/// The file a [`SkillMd`] is. The frontmatter goes through a YAML emitter, so
/// a description with a colon or a quote in it is quoted rather than broken.
pub(crate) fn render(md: &SkillMd) -> Result<String, String> {
    use yaml_rust2::{yaml::Hash, YamlEmitter};
    let s = |v: &str| Yaml::String(v.to_string());
    let mut front = Hash::new();
    front.insert(s("name"), s(&md.name));
    front.insert(s("description"), s(md.description.trim()));
    if let Some(tools) = &md.allowed_tools {
        front.insert(s("allowed-tools"), s(&tools.join(" ")));
    }
    let mut meta = Hash::new();
    if !md.invoke.is_empty() {
        meta.insert(s("invoke"), s(&md.invoke.join(", ")));
    }
    for (key, value) in [("trigger", &md.trigger), ("run", &md.run), ("source", &md.source), ("limit", &md.limit)] {
        if let Some(v) = value {
            meta.insert(s(key), s(v));
        }
    }
    if md.deep {
        meta.insert(s("model"), s("deep"));
    } else if md.lite {
        meta.insert(s("model"), s("lite"));
    }
    if !md.enabled {
        meta.insert(s("enabled"), s("false"));
    }
    if !md.writes.is_empty() {
        meta.insert(s("writes"), s(&md.writes.join(", ")));
    }
    if !md.secrets.is_empty() {
        meta.insert(s("secrets"), s(&md.secrets.join(", ")));
    }
    if let Some(f) = &md.forked_from {
        meta.insert(s("forked-from"), s(f));
    }
    if let Some(icon) = &md.icon {
        meta.insert(s("icon"), s(icon));
    }
    if !meta.is_empty() {
        front.insert(s("metadata"), Yaml::Hash(meta));
    }
    let mut yaml = String::new();
    YamlEmitter::new(&mut yaml)
        .dump(&Yaml::Hash(front))
        .map_err(|e| format!("could not write the frontmatter: {e}"))?;
    Ok(format!("{yaml}\n---\n{}\n", md.body.trim()))
}

/// `limit`'s value for a `limits` object as the runner reads it
/// (`applet_runner::limits`), or `None` when it sets nothing `limit` says.
pub(crate) fn limit_from_config(limits: &serde_json::Map<String, serde_json::Value>) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(n) = limits.get("max_llm_cost").and_then(|v| v.as_f64()) {
        parts.push(format!("${n} per run"));
    }
    if let Some(n) = limits.get("max_llm_cost_per_day").and_then(|v| v.as_f64()) {
        parts.push(format!("${n} per day"));
    }
    if let Some(n) = limits.get("max_runs_per_day").or_else(|| limits.get("max_runs")).and_then(|v| v.as_u64()) {
        parts.push(format!("{n} runs per day"));
    }
    if let Some(n) = limits.get("max_runs_per_hour").and_then(|v| v.as_u64()) {
        parts.push(format!("{n} runs per hour"));
    }
    if let Some(n) = limits.get("timeout_s").and_then(|v| v.as_u64()) {
        parts.push(format!("{n} seconds"));
    }
    (!parts.is_empty()).then(|| parts.join(", "))
}

/// A `SKILL.md` with an edit applied: the fields the PATCH path sends
/// (`scheduler::applets::update_applet`), written into the frontmatter and
/// the body, so reconcile reads the edit back instead of reverting it.
///
/// - `agent` is the body; null empties it.
/// - `schedule` replaces `trigger`'s time entry with `cron <expr>`; null
///   removes it. A `webhook` entry stays.
/// - `enabled` is `metadata.enabled`.
/// - `triggers`: `tool` is `invoke`'s `assistant`; `manual` and `message`
///   need no key; `cron` follows `schedule`; `webhook` is kept as it is.
/// - `config`: `limits` and `model_slot` become `limit` and `model`. Its
///   other keys live in the row only.
/// - `memory` lives in the row only.
///
/// `name` is the folder's and cannot change here; `condition` is now SQL in
/// `run`. Both are refused with a sentence.
pub(crate) fn apply_patch(text: &str, patch: &serde_json::Map<String, serde_json::Value>) -> Result<String, String> {
    use yaml_rust2::YamlEmitter;
    let (front, body) = split_frontmatter(text)?;
    let doc = YamlLoader::load_from_str(front)
        .map_err(|e| format!("its frontmatter is not valid YAML: {e}"))?
        .into_iter()
        .next()
        .ok_or("its frontmatter is empty")?;
    let Yaml::Hash(mut top) = doc else { return Err("its frontmatter is not a map".into()) };
    let key = |k: &str| Yaml::String(k.to_string());
    let mut meta = match top.remove(&key("metadata")) {
        Some(Yaml::Hash(h)) => h,
        _ => Default::default(),
    };
    let text_of = |h: &yaml_rust2::yaml::Hash, k: &str| h.get(&key(k)).and_then(scalar).unwrap_or_default();
    let mut body = body.trim().to_string();

    for (field, value) in patch {
        match field.as_str() {
            "name" => return Err("An applet written as SKILL.md is named by its folder, so its name cannot change here.".into()),
            "condition" if !value.is_null() => {
                return Err("A condition is now SQL in `run`: set `run: sql select 1 where (…)` in its SKILL.md.".into())
            }
            "condition" | "memory" => {}
            "agent" => body = value.as_str().unwrap_or_default().trim().to_string(),
            "schedule" => {
                let mut kept: Vec<String> =
                    trigger_entries(&text_of(&meta, "trigger")).into_iter().filter(|t| t == "webhook").collect();
                if let Some(cron) = value.as_str().filter(|c| !c.trim().is_empty()) {
                    kept.insert(0, format!("cron {}", cron.trim()));
                }
                if kept.is_empty() {
                    meta.remove(&key("trigger"));
                } else {
                    meta.insert(key("trigger"), Yaml::String(kept.join(", ")));
                }
            }
            "enabled" => {
                if let Some(b) = value.as_bool() {
                    meta.insert(key("enabled"), Yaml::String(b.to_string()));
                }
            }
            "triggers" => {
                let wanted: Vec<&str> = value.as_array().map(|a| a.iter().filter_map(|t| t.as_str()).collect()).unwrap_or_default();
                let mut invoke: Vec<String> = list(&text_of(&meta, "invoke")).into_iter().filter(|i| i == "chat").collect();
                if wanted.contains(&"tool") {
                    invoke.push("assistant".into());
                }
                meta.insert(key("invoke"), Yaml::String(if invoke.is_empty() { "none".into() } else { invoke.join(", ") }));
            }
            "config" => {
                if let Some(limits) = value.get("limits").and_then(|l| l.as_object()) {
                    match limit_from_config(limits) {
                        Some(l) => meta.insert(key("limit"), Yaml::String(l)),
                        None => meta.remove(&key("limit")),
                    };
                }
                match value.get("model_slot").and_then(|m| m.as_str()) {
                    Some("deep") => {
                        meta.insert(key("model"), Yaml::String("deep".into()));
                    }
                    Some(_) => {
                        meta.remove(&key("model"));
                    }
                    None => {}
                }
            }
            other => return Err(format!("`{other}` cannot be edited in a SKILL.md.")),
        }
    }
    if !meta.is_empty() {
        top.insert(key("metadata"), Yaml::Hash(meta));
    }
    let mut yaml = String::new();
    YamlEmitter::new(&mut yaml)
        .dump(&Yaml::Hash(top))
        .map_err(|e| format!("could not write the frontmatter: {e}"))?;
    Ok(format!("{yaml}\n---\n{body}\n"))
}

/// A `SKILL.md` with `metadata.<key>` set to `value`, everything else as it
/// was. A fork stamps where it came from this way.
pub(crate) fn with_metadata(text: &str, key: &str, value: &str) -> Result<String, String> {
    use yaml_rust2::YamlEmitter;
    let (front, body) = split_frontmatter(text)?;
    let doc = YamlLoader::load_from_str(front)
        .map_err(|e| format!("its frontmatter is not valid YAML: {e}"))?
        .into_iter()
        .next()
        .ok_or("its frontmatter is empty")?;
    let Yaml::Hash(mut top) = doc else { return Err("its frontmatter is not a map".into()) };
    let k = |k: &str| Yaml::String(k.to_string());
    let mut meta = match top.remove(&k("metadata")) {
        Some(Yaml::Hash(h)) => h,
        _ => Default::default(),
    };
    meta.insert(k(key), Yaml::String(value.to_string()));
    top.insert(k("metadata"), Yaml::Hash(meta));
    let mut yaml = String::new();
    YamlEmitter::new(&mut yaml)
        .dump(&Yaml::Hash(top))
        .map_err(|e| format!("could not write the frontmatter: {e}"))?;
    Ok(format!("{yaml}\n---\n{}\n", body.trim()))
}

/// A comma-separated `metadata` value as its entries.
fn list(value: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for v in value.split(',').map(str::trim).filter(|v| !v.is_empty()) {
        if !out.iter().any(|o| o == v) {
            out.push(v.to_string());
        }
    }
    out
}

/// The id a folder gets. The spec's names use hyphens and ids, schemas and
/// the applet slug use underscores, so a hyphen becomes an underscore.
fn id_for_dir(dir: &str) -> String {
    format!("applet_{}", dir.replace('/', "__").replace('-', "_"))
}

/// Who the row belongs to, from where the folder is. A manifest's own
/// `owner` is not used: anyone who can write the folder could set it.
fn owner_for(dir: &str, shipped: bool) -> &'static str {
    if shipped && SHIPPED_STARTERS.contains(&dir) {
        "user"
    } else if shipped {
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

/// Tools no applet may list: they leave the box (`browse`, `send_imessage`)
/// or run as root (`sudo`).
const NEVER_LISTED: &[&str] = &["browse", "send_imessage", "sudo"];

/// Tools only a shipped applet may list: `sudo`, for `admin`, and
/// `dispatch_subagents`, the worker fan-out `deep-research` runs on, which no
/// applet could reach before these were applets. The shipped root is written
/// only by the installer, and a folder elsewhere with the same name is read
/// as unshipped, so it is refused.
const SHIPPED_ONLY: &[&str] = &["sudo", "dispatch_subagents"];

/// The tool ids an applet may list: what ordinary chat offers, for a turn
/// `/name` starts, and what an unattended run can be given. An unattended
/// run is offered only the tools on both its list and the run allowlist
/// (`tools::get_tools_for_applet`).
fn listable_tools() -> Vec<String> {
    let mut out: Vec<String> = crate::api::chat_applets::chat_tool_ids();
    out.extend(crate::tools::applet_run_tool_ids().iter().map(|t| t.to_string()));
    out.retain(|t| !NEVER_LISTED.contains(&t.as_str()));
    out.sort();
    out.dedup();
    out
}

/// The tool ids an applet asked for, checked against [`listable_tools`],
/// and the hosts its `net(host)` entries ask for.
fn check_allowed_tools(list: &str, shipped: bool, errors: &mut Vec<String>) -> (Vec<String>, Vec<String>) {
    let mut available = listable_tools();
    if shipped {
        available.extend(SHIPPED_ONLY.iter().map(|t| t.to_string()));
    }
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
        } else if entry == "think" {
            // A run always has it; listed, a turn `/name` starts gets it too.
            if !out.iter().any(|t| t == entry) {
                out.push(entry.to_string());
            }
        } else if ALWAYS_ALLOWED_TOOLS.contains(&entry) {
            // Always given; listing it changes nothing.
        } else if available.iter().any(|t| t == entry) {
            if !out.iter().any(|t| t == entry) {
                out.push(entry.to_string());
            }
        } else if known.iter().any(|k| k == entry) {
            errors.push(format!("`{entry}` is not available to an applet."));
        } else {
            let names: Vec<&str> = available.iter().map(String::as_str).collect();
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

/// `trigger`'s entries. A comma separates two entries, except inside a
/// `cron` entry, whose fields may hold lists (`0 0 10,22 * * 1,3`): a piece
/// joins the `cron` entry before it while that entry lacks its six fields,
/// or when the piece is a bare continuation of its last field. Every other
/// entry has a space in it, or is `webhook`.
fn trigger_entries(value: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for piece in value.split(',') {
        if let Some(last) = out.last_mut() {
            let cron = last.starts_with("cron ");
            let bare = !piece.trim().is_empty() && !piece.trim().contains(char::is_whitespace) && piece.trim() != "webhook";
            if cron && (last.split_whitespace().count() < 7 || bare) {
                last.push(',');
                last.push_str(piece.trim());
                continue;
            }
        }
        let piece = piece.trim();
        if !piece.is_empty() {
            out.push(piece.to_string());
        }
    }
    out
}

fn parse_triggers(value: &str, errors: &mut Vec<String>) -> Triggers {
    let mut out = Triggers::default();
    for part in trigger_entries(value).iter().map(String::as_str) {
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

/// What `run` names: code to spawn, or SQL to run before the prompt.
enum Run {
    Command(Vec<String>),
    Sql(String),
}

/// SQL `run` may hold: one statement, and no `new_rows`, which only a data
/// trigger fills. Whether it prepares is for Postgres, at save.
fn check_run_sql(sql: &str, errors: &mut Vec<String>) -> Option<String> {
    let sql = sql.trim().trim_end_matches(';').trim();
    if sql.is_empty() {
        errors.push("`run` has no SQL after `sql`.".into());
        return None;
    }
    if sql.contains(';') {
        errors.push("`run` SQL is one statement.".into());
        return None;
    }
    if sql.to_ascii_lowercase().contains("new_rows") {
        errors.push("`run` SQL reads `new_rows`, which only a data trigger fills, and an applet cannot start on new data yet.".into());
        return None;
    }
    Some(sql.to_string())
}

/// `run` as the argv the runner spawns, or the SQL it runs.
fn parse_run(value: &str, folder: &Path, shipped: bool, errors: &mut Vec<String>) -> Option<Run> {
    let value = value.trim();
    if let Some(sql) = value.strip_prefix("sql ") {
        return check_run_sql(sql, errors).map(Run::Sql);
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
        if path.extension().and_then(|e| e.to_str()) == Some("sql") {
            return match std::fs::read_to_string(&path) {
                Ok(sql) => check_run_sql(&sql, errors).map(Run::Sql),
                Err(e) => {
                    errors.push(format!("`run` names {value}, which could not be read: {e}"));
                    None
                }
            };
        }
        let path = path.to_string_lossy().into_owned();
        return Some(Run::Command(match Path::new(value).extension().and_then(|e| e.to_str()) {
            Some("py") => vec!["python3".into(), path],
            Some("sh") => vec!["sh".into(), path],
            _ => vec![path],
        }));
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
    Some(Run::Command(vec![value.to_string()]))
}

/// `limit`: `$N per run`, `$N per day` and a duration, comma-separated.
fn parse_limit(value: &str, limits: &mut toml::value::Table, errors: &mut Vec<String>) {
    let help = "Use `$N per run`, `$N per day`, `N runs per day`, `N runs per hour` or a duration such as \
                `10 minutes`, separated by commas.";
    for part in value.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let words: Vec<&str> = part.split_whitespace().collect();
        let (key, number) = match words.as_slice() {
            [amount, "per", "run"] if amount.starts_with('$') => ("max_llm_cost", dollars(amount)),
            [amount, "per", "day"] if amount.starts_with('$') => ("max_llm_cost_per_day", dollars(amount)),
            [n, "runs" | "run", "per", "day"] => ("max_runs_per_day", n.parse::<f64>().ok().filter(|n| *n >= 1.0)),
            [n, "runs" | "run", "per", "hour"] => ("max_runs_per_hour", n.parse::<f64>().ok().filter(|n| *n >= 1.0)),
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
        let value = if matches!(key, "timeout_s" | "max_runs_per_day" | "max_runs_per_hour") {
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
        assert_eq!(tools, ["sql_query", "create_page", "think"], "a listed think reaches a chat turn");
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
            ("cron 0 0 10,22 * * 1,3", "0 0 10,22 * * 1,3", None),
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
            ("sql select 1; drop table x", "one statement"),
            ("sql select * from new_rows", "only a data trigger fills"),
            ("scripts/check.sql", "which is not in the folder"),
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
        let e = refused(&format!("{BASE}\nallowed-tools: sudo"), "Do it.");
        assert!(e.contains("`sudo` is not available to an applet"), "{e}");
        let f = folder();
        let shipped = parse(&skill(&format!("{BASE}\nallowed-tools: sudo"), "Do it."), f.path(), "weekly-review", true)
            .expect("a shipped applet may list sudo");
        assert_eq!(shipped.config.unwrap()["allowed_tools"][0].as_str(), Some("sudo"));
        assert!(ok(&format!("{BASE}\nallowed-tools: get_project_item"), "Do it.").config.is_some(), "a chat tool is listable");
        assert!(ok(&format!("{BASE}\nallowed-tools: shell"), "Do it.").config.is_some(), "the sandboxed shell is");
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

    /// What a writer renders, the reader takes back unchanged, quotes and
    /// colons in the description included.
    #[test]
    fn a_rendered_skill_md_reads_back() {
        let md = SkillMd {
            name: "weekly-review".into(),
            description: "Review: the week's \"main\" events #1".into(),
            allowed_tools: Some(vec!["sql_query".into(), "create_page".into()]),
            invoke: vec!["chat".into(), "assistant".into()],
            limit: Some("$3 per run, 10 minutes".into()),
            deep: true,
            icon: Some("ri:calendar-line".into()),
            body: "Write the review.".into(),
            ..Default::default()
        };
        let text = render(&md).unwrap();
        let f = folder();
        let t = parse(&text, f.path(), "weekly-review", false).unwrap_or_else(|e| panic!("{e:?}\n{text}"));
        assert_eq!(t.description.as_deref(), Some("Review: the week's \"main\" events #1"));
        assert_eq!(t.agent.as_deref(), Some("Write the review."));
        assert!(t.invoke_chat);
        assert_eq!(t.triggers, ["manual", "message", "tool"]);
        assert_eq!(t.icon.as_deref(), Some("ri:calendar-line"));
        let cfg = t.config.unwrap();
        assert_eq!(cfg["model_slot"].as_str(), Some("deep"));
        assert_eq!(cfg["limits"]["max_llm_cost"].as_float(), Some(3.0));
    }

    #[test]
    fn writes_and_secrets_name_real_streams_and_valid_keys() {
        let t = ok(
            &format!("{BASE}\nmetadata:\n  writes: health_sleep, health_steps\n  secrets: OPENAI_API_KEY, GITHUB_TOKEN"),
            "Do it.",
        );
        assert_eq!(t.writes, ["health_sleep", "health_steps"]);
        assert_eq!(t.secrets, ["OPENAI_API_KEY", "GITHUB_TOKEN"]);
        let e = refused(&format!("{BASE}\nmetadata:\n  writes: health_slep"), "Do it.");
        assert!(e.contains("Did you mean `health_sleep`?"), "{e}");
        let e = refused(&format!("{BASE}\nmetadata:\n  secrets: DATABASE_URL"), "Do it.");
        assert!(e.contains("`secrets` has \"DATABASE_URL\""), "{e}");
    }

    #[test]
    fn only_a_listed_shipped_applet_gets_the_vault_key_and_a_view_has_a_size_limit() {
        let f = folder();
        let text = skill("name: credential-refresh\ndescription: Refreshes tokens.\nmetadata:\n  run: credential_refresh\n  trigger: every hour", "");
        assert!(parse(&text, f.path(), "credential-refresh", true).unwrap().needs_vault_key);
        let text = skill("name: weekly-review\ndescription: x", "Do it.");
        assert!(!parse(&text, f.path(), "weekly-review", true).unwrap().needs_vault_key);
        std::fs::write(f.path().join("view.html"), vec![b'x'; 49 * 1024]).unwrap();
        let e = parse(&text, f.path(), "weekly-review", false).unwrap_err().join(" ");
        assert!(e.contains("The limit is 48 KB"), "{e}");
    }

    /// SQL in `run`, inline or as a file under `scripts/`, runs before the
    /// prompt; a schedule with only SQL behind it is a run.
    #[test]
    fn run_takes_sql_inline_or_from_a_file() {
        let t = ok(
            &format!("{BASE}\nmetadata:\n  run: sql select 1 where not exists (select 1 from data_health_workout)\n  trigger: every day at 09:00"),
            "Say it has been a while.",
        );
        assert_eq!(t.run_sql.as_deref(), Some("select 1 where not exists (select 1 from data_health_workout)"));
        assert!(t.command.is_none());
        let f = folder();
        std::fs::create_dir_all(f.path().join("scripts")).unwrap();
        std::fs::write(f.path().join("scripts/high.sql"), "select bpm from data_health_heart_rate where bpm > 100;\n").unwrap();
        let text = skill("name: high\ndescription: High readings.\nmetadata:\n  run: scripts/high.sql\n  trigger: every hour", "");
        let t = parse(&text, f.path(), "high", false).unwrap();
        assert_eq!(t.run_sql.as_deref(), Some("select bpm from data_health_heart_rate where bpm > 100"));
        assert!(t.triggers.contains(&"cron".to_string()));
    }

    /// An edit lands in the file: a retimed applet keeps its schedule
    /// through a reconcile because reconcile reads it from here.
    #[test]
    fn an_edit_is_written_into_the_skill_md() {
        let f = folder();
        let text = skill(
            &format!("{BASE}\nallowed-tools: sql_query\nmetadata:\n  trigger: every day at 07:00, webhook\n  source: example_source\n  invoke: chat"),
            "Old prompt.",
        );
        let patch = serde_json::json!({
            "schedule": "0 30 9 * * *",
            "agent": "New prompt.",
            "enabled": false,
            "triggers": ["cron", "manual", "tool"],
            "config": { "limits": { "max_llm_cost": 2, "max_runs_per_day": 4 }, "model_slot": "deep", "chat_id": "chat_a" },
            "memory": "row only",
        });
        let edited = apply_patch(&text, patch.as_object().unwrap()).unwrap();
        let t = parse(&edited, f.path(), "weekly-review", false).unwrap_or_else(|e| panic!("{e:?}\n{edited}"));
        assert_eq!(t.schedule.as_deref(), Some("0 30 9 * * *"));
        assert_eq!(t.agent.as_deref(), Some("New prompt."));
        assert!(!t.default_enabled);
        assert!(t.triggers.contains(&"webhook".to_string()), "a webhook entry stays");
        assert!(t.triggers.contains(&"tool".to_string()), "tool became invoke: assistant");
        assert!(t.invoke_chat, "chat stays");
        let cfg = t.config.unwrap();
        assert_eq!(cfg["model_slot"].as_str(), Some("deep"));
        assert_eq!(cfg["limits"]["max_runs_per_day"].as_integer(), Some(4));
        assert!(cfg.get("chat_id").is_none(), "row-only config stays out of the file");

        let cleared = apply_patch(&edited, serde_json::json!({ "schedule": null }).as_object().unwrap()).unwrap();
        assert!(parse(&cleared, f.path(), "weekly-review", false).unwrap().schedule.is_none());
        assert!(apply_patch(&text, serde_json::json!({ "name": "x" }).as_object().unwrap()).is_err());
        assert!(apply_patch(&text, serde_json::json!({ "condition": "true" }).as_object().unwrap()).is_err());
    }

    /// A fork stamps where it came from; stamping again replaces it.
    #[test]
    fn a_fork_stamp_is_read_back_and_replaced() {
        let text = skill(BASE, "Do it.");
        let once = with_metadata(&text, "forked-from", "virtues@v0.3.0").unwrap();
        let twice = with_metadata(&once, "forked-from", "virtues@v0.4.0").unwrap();
        assert_eq!(twice.matches("forked-from").count(), 1, "replaced, not appended");
        assert_eq!(ok(twice.split_once("---\n").unwrap().1.split_once("\n---\n").unwrap().0, "Do it.").forked_from.as_deref(), Some("virtues@v0.4.0"));
    }

    #[test]
    fn the_displayed_name_keeps_brand_spellings() {
        assert_eq!(display_name("weekly-review"), "Weekly review");
        assert_eq!(display_name("ios-battery-check"), "iOS battery check");
        assert_eq!(display_name("sync-github-stars"), "Sync GitHub stars");
    }
}
