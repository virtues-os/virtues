//! Skills the composer's `/` menu offers: the shipped ones compiled into the
//! registry, and the person's own, saved by the `save_skill` tool.
//!
//! An authored skill is a `SKILL.md` in the registry's format under
//! `<applet state root>/skills/<name>/`. It lives in the applet state root so
//! that backup, restore and deprovisioning, which already carry that folder,
//! carry skills too. The applet catalog scans the same root and skips this
//! folder, because nothing in it has a `manifest.toml`.
//!
//! An authored file is checked twice: when it is saved and each time it is
//! read, so a file edited by hand cannot reach past the checks. It may name
//! only tools that ordinary chat offers, so it can never reach the sudo-only
//! `sudo` tool, and its ceilings are capped at deep research's.

use serde::Serialize;
use virtues_registry::models::ModelSlot;
use virtues_registry::skills::{self as registry, Skill};

use crate::error::{Error, Result};

/// Names a skill may not take: the built-in modes the same wire field carries.
const RESERVED_NAMES: &[&str] = &[
    "chat",
    "sudo",
    "deep_research",
    "interview",
    "local",
    crate::api::getting_started::AGENT_MODE,
];

/// The ceilings an authored skill may declare, at most. Deep research's.
const MAX_STEPS: u32 = 50;
const MAX_COST_USD: f64 = 10.0;
const MAX_MINUTES: u64 = 25;
const MAX_BODY_CHARS: usize = 8_000;
const MAX_DESCRIPTION_CHARS: usize = 160;

/// A skill as the `/` menu lists it.
#[derive(Debug, Clone, Serialize)]
pub struct SkillSummary {
    pub name: String,
    pub description: String,
    pub icon: Option<String>,
    /// `shipped` or `authored`.
    pub source: &'static str,
}

#[derive(Debug, Serialize)]
pub struct SkillList {
    pub skills: Vec<SkillSummary>,
    /// The skill that writes skills, which the menu pins below the list as
    /// its "create" row instead of listing.
    pub create: Option<SkillSummary>,
}

/// The shipped skill that helps the person write one of their own.
pub const CREATE_SKILL: &str = "new-skill";

/// Where authored skills live.
pub fn authored_root() -> std::path::PathBuf {
    crate::applet_templates::state_root().join("skills")
}

/// Every skill and every applet `/name` starts, shipped first, each group by
/// name.
pub fn list() -> SkillList {
    let summary = |s: Skill, source| SkillSummary {
        name: s.name,
        description: s.description,
        icon: s.icon,
        source,
    };
    let mut shipped = registry::default_skills();
    shipped.sort_by(|a, b| a.name.cmp(&b.name));
    let (create, shipped): (Vec<_>, Vec<_>) = shipped.into_iter().partition(|s| s.name == CREATE_SKILL);
    let applets = crate::applet_templates::chat_applets();
    let (shipped_applets, own_applets): (Vec<_>, Vec<_>) = applets.into_iter().partition(|a| a.shipped);
    let mut skills: Vec<_> = shipped_applets.into_iter().map(|a| summary(skill_from_applet(a), "shipped")).collect();
    skills.extend(shipped.into_iter().map(|s| summary(s, "shipped")));
    skills.sort_by(|a, b| a.name.cmp(&b.name));
    skills.extend(own_applets.into_iter().map(|a| summary(skill_from_applet(a), "authored")));
    skills.extend(authored().into_iter().map(|s| summary(s, "authored")));
    SkillList { skills, create: create.into_iter().next().map(|s| summary(s, "shipped")) }
}

/// What a turn asked for by name: an applet `/name` starts, then a shipped
/// skill, then the person's own.
pub fn skill_named(name: &str) -> Option<Skill> {
    crate::applet_templates::chat_applet_named(name)
        .map(skill_from_applet)
        .or_else(|| registry::skill_named(name))
        .or_else(|| authored().into_iter().find(|s| s.name == name))
}

/// The step ceiling for a turn an applet starts. The format has no key for
/// it (agents/plan/applet-shape.md keeps steps internal); thirty is what the
/// longest shipped skills had.
const APPLET_MAX_STEPS: u32 = 30;

/// A chat turn's shape from an applet `/name` starts: its body as the
/// prompt, its tools, and its `limit` and `model`. Without `allowed-tools`
/// it gets what ordinary chat offers, less what leaves the box. Without a
/// `limit` it gets $3 and ten minutes, the shipped skills' usual ceilings.
fn skill_from_applet(a: crate::applet_templates::ChatApplet) -> Skill {
    let tools = a.tools.unwrap_or_else(|| {
        chat_tool_ids()
            .into_iter()
            .filter(|t| t != crate::browse::TOOL && t != crate::tools::send_imessage::TOOL)
            .collect()
    });
    Skill {
        name: a.name,
        description: a.description,
        body: a.body,
        tools,
        max_steps: APPLET_MAX_STEPS,
        max_cost_usd: a.max_cost_usd.unwrap_or(3.0),
        max_minutes: a.max_minutes.unwrap_or(10),
        slot: if a.deep { ModelSlot::Deep } else { ModelSlot::Standard },
        icon: a.icon,
    }
}

/// The person's own skills, by name. A file that fails a check is skipped
/// and logged, so one bad file cannot hide the rest.
pub fn authored() -> Vec<Skill> {
    let root = authored_root();
    let Ok(entries) = std::fs::read_dir(&root) else {
        // No folder yet: nobody has saved a skill on this box.
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path().join("SKILL.md");
        let Ok(src) = std::fs::read_to_string(&path) else {
            continue;
        };
        let checked = registry::parse(&src).map_err(Error::InvalidInput).and_then(check);
        match checked {
            Ok(skill) if entry.file_name().to_str() == Some(skill.name.as_str()) => out.push(skill),
            Ok(skill) => tracing::warn!(
                path = %path.display(),
                name = %skill.name,
                "authored skill's name does not match its folder; skipped"
            ),
            Err(e) => tracing::warn!(path = %path.display(), error = %e, "authored skill skipped"),
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// What `save_skill` was asked to write.
pub struct NewSkill {
    pub name: String,
    pub description: String,
    pub icon: Option<String>,
    pub tools: Vec<String>,
    pub body: String,
    pub slot: ModelSlot,
}

/// Write one of the person's skills, replacing their own of the same name.
/// Returns the skill as saved and whether it replaced one.
pub fn save(new: NewSkill) -> Result<(Skill, bool)> {
    // Ceilings follow from the tier, as chat's and deep research's do, so the
    // model is not asked to pick dollar figures.
    let (max_steps, max_cost_usd, max_minutes) = match new.slot {
        ModelSlot::Deep => (MAX_STEPS, MAX_COST_USD, MAX_MINUTES),
        _ => (20, 2.5, 8),
    };
    let skill = check(Skill {
        name: new.name.trim().to_string(),
        description: new.description.trim().to_string(),
        body: new.body.trim().to_string(),
        tools: new.tools.iter().map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect(),
        max_steps,
        max_cost_usd,
        max_minutes,
        slot: new.slot,
        icon: new.icon.map(|i| i.trim().to_string()).filter(|i| !i.is_empty()),
    })?;
    let text = registry::render(&skill);
    // What is written must read back as the same skill, or a later turn would
    // run something other than what was saved.
    registry::parse(&text).map_err(|e| Error::Other(format!("saved skill does not read back: {e}")))?;

    let dir = authored_root().join(&skill.name);
    let path = dir.join("SKILL.md");
    let replaced = path.is_file();
    std::fs::create_dir_all(&dir)
        .map_err(|e| Error::Other(format!("create {}: {e}", dir.display())))?;
    // Written whole, then moved into place, so a reader never sees half a file.
    let tmp = dir.join(".SKILL.md.tmp");
    std::fs::write(&tmp, text).map_err(|e| Error::Other(format!("write {}: {e}", tmp.display())))?;
    std::fs::rename(&tmp, &path).map_err(|e| Error::Other(format!("move into {}: {e}", path.display())))?;
    Ok((skill, replaced))
}

/// The checks every authored skill passes, on save and on read.
fn check(skill: Skill) -> Result<Skill> {
    let bad = |why: String| Err(Error::InvalidInput(why));
    let name = skill.name.as_str();
    let well_formed = (2..=40).contains(&name.len())
        && name.starts_with(|c: char| c.is_ascii_lowercase())
        && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !well_formed {
        return bad(format!("name {name:?} must be 2 to 40 lowercase letters, digits or dashes"));
    }
    if RESERVED_NAMES.contains(&name) {
        return bad(format!("{name:?} is a built-in mode's name"));
    }
    if registry::skill_named(name).is_some() {
        return bad(format!("{name:?} is a built-in skill's name"));
    }
    if crate::applet_templates::chat_applet_named(name).is_some() {
        return bad(format!("{name:?} is an applet's name"));
    }
    if skill.description.is_empty() || skill.description.chars().count() > MAX_DESCRIPTION_CHARS {
        return bad(format!("description must be 1 to {MAX_DESCRIPTION_CHARS} characters"));
    }
    if skill.body.is_empty() || skill.body.chars().count() > MAX_BODY_CHARS {
        return bad(format!("body must be 1 to {MAX_BODY_CHARS} characters"));
    }
    if skill.tools.is_empty() {
        return bad("name at least one tool".into());
    }
    let offered = chat_tool_ids();
    if let Some(t) = skill.tools.iter().find(|t| !offered.contains(t)) {
        return bad(format!("tool {t:?} is not one ordinary chat offers"));
    }
    if skill.max_steps > MAX_STEPS || skill.max_cost_usd > MAX_COST_USD || skill.max_minutes > MAX_MINUTES {
        return bad(format!(
            "ceilings may be at most {MAX_STEPS} steps, ${MAX_COST_USD} and {MAX_MINUTES} minutes"
        ));
    }
    if !matches!(skill.slot, ModelSlot::Standard | ModelSlot::Deep) {
        return bad("model must be standard or deep".into());
    }
    Ok(skill)
}

/// The tools ordinary chat offers: never the sudo-only `sudo` tool, the system
/// tools, or the Mac app's. `think` is in the registry's list for skills too.
pub(crate) fn chat_tool_ids() -> Vec<String> {
    crate::tools::get_tool_definitions_for_llm()
        .iter()
        .filter_map(|t| t["function"]["name"].as_str().map(str::to_string))
        .chain(["think".to_string()])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new(name: &str, tools: &[&str]) -> NewSkill {
        NewSkill {
            name: name.into(),
            description: "d".into(),
            icon: None,
            tools: tools.iter().map(|t| t.to_string()).collect(),
            body: "<x>body</x>".into(),
            slot: ModelSlot::Standard,
        }
    }

    fn err(new: NewSkill) -> String {
        match save(new) {
            Err(Error::InvalidInput(why)) => why,
            other => panic!("expected InvalidInput, got {:?}", other.map(|(s, _)| s.name)),
        }
    }

    /// An authored skill cannot reach `sudo`, take a mode's or a shipped
    /// skill's name, or carry a name that is not a folder name.
    #[test]
    fn checks_refuse_what_authored_skills_may_not_do() {
        assert!(err(new("admin", &["sudo"])).contains("ordinary chat"));
        assert!(err(new("sudo", &["sql_query"])).contains("built-in mode"));
        assert!(err(new("new-skill", &["sql_query"])).contains("built-in skill"));
        assert!(err(new("weekly-review", &["sql_query"])).contains("an applet's name"));
        assert!(err(new("../escape", &["sql_query"])).contains("lowercase"));
        assert!(err(new("Weekly", &["sql_query"])).contains("lowercase"));
        assert!(err(new("nothing", &[])).contains("at least one tool"));
    }

    /// The create row is the shipped skill that can save, and it is not
    /// listed twice.
    #[test]
    fn the_create_skill_is_pinned_not_listed() {
        let list = list();
        assert_eq!(list.create.as_ref().map(|s| s.name.as_str()), Some(CREATE_SKILL));
        assert!(list.skills.iter().all(|s| s.name != CREATE_SKILL));
        let create = registry::skill_named(CREATE_SKILL).expect("shipped");
        assert_eq!(create.tools, ["save_skill"]);
    }

    /// Ceilings stay inside the caps, and the tier is standard or deep.
    #[test]
    fn ceilings_and_tier_are_capped() {
        let base = || Skill {
            name: "xx".into(),
            description: "d".into(),
            body: "b".into(),
            tools: vec!["sql_query".into()],
            max_steps: 1,
            max_cost_usd: 1.0,
            max_minutes: 1,
            slot: ModelSlot::Standard,
            icon: None,
        };
        assert!(check(base()).is_ok());
        assert!(check(Skill { max_steps: MAX_STEPS + 1, ..base() }).is_err());
        assert!(check(Skill { max_cost_usd: MAX_COST_USD + 1.0, ..base() }).is_err());
        assert!(check(Skill { slot: ModelSlot::Image, ..base() }).is_err());
    }
}
