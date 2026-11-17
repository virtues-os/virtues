//! Applets `/name` starts in chat: `SKILL.md` folders whose `invoke`
//! includes `chat` (`applet_templates::chat_applets`).
//!
//! `/name` runs the applet as a turn of the chat it was typed in, not as a
//! headless run: its body rides in the turn's tail as the prompt, and its
//! tools, ceilings and model shape the turn ([`AppletTurn`]). The composer
//! sends the name as `agent_mode`; `ChatMode::from_wire` resolves it here.
//!
//! The person's own skills, which `save_skill` wrote to
//! `<state root>/skills/<name>/SKILL.md` before skills were applets, move once
//! into `<state root>/user/<name>/` as applets ([`move_saved_skills`]).

use serde::Serialize;
use virtues_registry::models::ModelSlot;

/// The shape of a turn an applet runs in: what the chat needs from it.
#[derive(Debug, Clone)]
pub struct AppletTurn {
    /// What follows the `/`, and what the composer sends as `agent_mode`.
    pub name: String,
    pub description: String,
    /// The prompt, appended to the turn's tail inside `<applet name="…">`.
    pub body: String,
    /// The tools the turn may call.
    pub tools: Vec<String>,
    pub max_steps: u32,
    /// In dollars, as the gateway reports them.
    pub max_cost_usd: f64,
    pub max_minutes: u64,
    pub slot: ModelSlot,
    /// An icon id for the `/` menu.
    pub icon: Option<String>,
    /// It ships with the box. Only a shipped applet can make a sudo turn
    /// (`ChatMode::is_sudo`).
    pub shipped: bool,
}

/// An applet as the composer's `/` menu lists it. The wire shape is the one
/// the menu read when these were skills, so an older client still reads it.
#[derive(Debug, Clone, Serialize)]
pub struct MenuEntry {
    pub name: String,
    pub description: String,
    pub icon: Option<String>,
    /// `shipped` or `authored`.
    pub source: &'static str,
}

#[derive(Debug, Serialize)]
pub struct Menu {
    /// Under the name the menu has always read.
    pub skills: Vec<MenuEntry>,
    /// The applet that writes applets, which the menu pins below the list as
    /// its "create" row instead of listing.
    pub create: Option<MenuEntry>,
}

/// The shipped applet that helps the person write one of their own.
pub const CREATE_APPLET: &str = "new-applet";

/// The shipped applet that runs commands on the server as its admin.
pub const ADMIN_APPLET: &str = "admin";

/// The shipped applet that investigates and writes a cited report.
pub const DEEP_RESEARCH_APPLET: &str = "deep-research";

/// The step ceiling for a turn an applet starts. The format has no key for
/// it (agents/plan/applet-shape.md keeps steps internal), so the applet's
/// `limit` in dollars and minutes is what bounds a turn; sixty is what admin
/// work needed as a mode.
const MAX_STEPS: u32 = 60;

/// Every applet `/name` starts, shipped first, each group by name.
pub fn menu() -> Menu {
    let entry = |a: crate::applet_templates::ChatApplet| MenuEntry {
        source: if a.shipped { "shipped" } else { "authored" },
        name: a.name,
        description: a.description,
        icon: a.icon,
    };
    let (create, rest): (Vec<_>, Vec<_>) = crate::applet_templates::chat_applets()
        .into_iter()
        .partition(|a| a.shipped && a.name == CREATE_APPLET);
    Menu {
        skills: rest.into_iter().map(entry).collect(),
        create: create.into_iter().next().map(entry),
    }
}

/// The turn `/name` runs, or none.
pub fn turn_named(name: &str) -> Option<AppletTurn> {
    crate::applet_templates::chat_applet_named(name).map(turn_from_applet)
}

/// A turn's shape from an applet: its body as the prompt, its tools, and its
/// `limit` and `model`. Without `allowed-tools` it gets what ordinary chat
/// offers, less what leaves the box. Without a `limit` it gets $3 and ten
/// minutes, the shipped skills' usual ceilings.
fn turn_from_applet(a: crate::applet_templates::ChatApplet) -> AppletTurn {
    let tools = a.tools.unwrap_or_else(|| {
        chat_tool_ids()
            .into_iter()
            .filter(|t| t != crate::browse::TOOL && t != crate::tools::send_imessage::TOOL)
            .collect()
    });
    AppletTurn {
        name: a.name,
        description: a.description,
        body: a.body,
        tools,
        max_steps: MAX_STEPS,
        max_cost_usd: a.max_cost_usd.unwrap_or(3.0),
        max_minutes: a.max_minutes.unwrap_or(10),
        slot: if a.deep { ModelSlot::Deep } else { ModelSlot::Standard },
        icon: a.icon,
        shipped: a.shipped,
    }
}

/// The tools ordinary chat offers: never the sudo-only `sudo` tool, the system
/// tools, or the Mac app's. `think` too, which chat sends separately.
pub(crate) fn chat_tool_ids() -> Vec<String> {
    crate::tools::get_tool_definitions_for_llm()
        .iter()
        .filter_map(|t| t["function"]["name"].as_str().map(str::to_string))
        .chain(["think".to_string()])
        .collect()
}

/// Where `save_skill` kept the person's skills.
fn saved_skills_root() -> std::path::PathBuf {
    crate::applet_templates::state_root().join(SAVED_SKILLS_DIR)
}

/// The state root's folder of skills saved before skills were applets. The
/// applet catalog skips it (`applet_templates::scan_root`) until it is gone.
pub(crate) const SAVED_SKILLS_DIR: &str = "skills";

/// Move each skill the person saved into `<state root>/user/<name>/SKILL.md`
/// as an applet `/name` starts, and remove the old folder once its applet is
/// written and loads. A skill that cannot move (its name is taken, its file
/// does not read) stays where it is and is logged. Returns how many moved.
///
/// Run at boot before the catalog loads. Idempotent: a box with nothing left
/// to move does nothing.
// TODO(2026-10-27): delete with SAVED_SKILLS_DIR once every box has booted a
// release carrying this.
pub fn move_saved_skills() -> usize {
    let root = saved_skills_root();
    let Ok(entries) = std::fs::read_dir(&root) else {
        // No folder: nothing was ever saved, or everything has moved.
        return 0;
    };
    let mut moved = 0;
    for entry in entries.flatten() {
        let Some(name) = entry.file_name().to_str().map(str::to_string) else { continue };
        let old = entry.path();
        match move_one(&old, &name) {
            Ok(()) => {
                if let Err(e) = std::fs::remove_dir_all(&old) {
                    tracing::warn!(skill = %name, error = %e, "moved a saved skill but could not remove its old folder");
                }
                tracing::info!(skill = %name, "moved a saved skill into the applets");
                moved += 1;
            }
            Err(why) => tracing::warn!(skill = %name, why = %why, "a saved skill was not moved into the applets"),
        }
    }
    let _ = std::fs::remove_dir(&root); // only succeeds when it is empty
    moved
}

fn move_one(old: &std::path::Path, name: &str) -> Result<(), String> {
    let text = std::fs::read_to_string(old.join("SKILL.md")).map_err(|e| format!("could not read it: {e}"))?;
    let saved = parse_saved(&text)?;
    let rel = format!("user/{name}");
    let dir = crate::applet_templates::state_root().join(&rel);
    if dir.exists() {
        return Err(format!("{} already exists", dir.display()));
    }
    let md = crate::applet_templates::SkillMd {
        name: name.to_string(),
        description: saved.description,
        allowed_tools: Some(saved.tools),
        invoke: vec!["chat".into()],
        limit: Some(format!("${} per run, {} minutes", saved.max_cost_usd, saved.max_minutes)),
        deep: saved.deep,
        icon: saved.icon,
        body: saved.body,
        ..Default::default()
    };
    let text = crate::applet_templates::render_skill_md(&md)?;
    crate::applet_templates::check_skill_md(&text, &dir, &rel).map_err(|problems| problems.join(" "))?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
    std::fs::write(dir.join("SKILL.md"), text).map_err(|e| format!("could not write {}: {e}", dir.display()))
}

/// A skill as `save_skill` wrote it: `key: value` frontmatter, then the body.
#[derive(Debug, PartialEq)]
struct SavedSkill {
    description: String,
    tools: Vec<String>,
    max_cost_usd: f64,
    max_minutes: u64,
    deep: bool,
    icon: Option<String>,
    body: String,
}

/// Read the old skill format. Tool ids from before a rename read as today's;
/// a tool that no longer exists is dropped rather than failing the move.
fn parse_saved(src: &str) -> Result<SavedSkill, String> {
    let rest = src.strip_prefix("---\n").ok_or("no frontmatter")?;
    let (front, body) = rest.split_once("\n---\n").ok_or("the frontmatter does not close")?;
    let mut saved = SavedSkill {
        description: String::new(),
        tools: Vec::new(),
        max_cost_usd: 3.0,
        max_minutes: 10,
        deep: false,
        icon: None,
        body: body.trim().to_string(),
    };
    let listable = chat_tool_ids();
    for line in front.lines() {
        let Some((key, value)) = line.split_once(':') else { continue };
        let value = value.trim();
        match key.trim() {
            "description" => saved.description = value.to_string(),
            "tools" => {
                for t in value.split(',').map(str::trim).filter(|t| !t.is_empty()) {
                    let t = match t {
                        "setup_applet" | "list_applets" | "get_applet" | "edit_applet" | "delete_applet"
                        | "run_applet" => "applet",
                        "code_interpreter" => "shell",
                        other => other,
                    };
                    if listable.iter().any(|l| l == t) && !saved.tools.iter().any(|known| known == t) {
                        saved.tools.push(t.to_string());
                    }
                }
            }
            "max_cost_usd" => saved.max_cost_usd = value.parse().map_err(|e| format!("max_cost_usd: {e}"))?,
            "max_minutes" => saved.max_minutes = value.parse().map_err(|e| format!("max_minutes: {e}"))?,
            "model" => saved.deep = value == "deep",
            "icon" => saved.icon = Some(value.to_string()),
            _ => {}
        }
    }
    if saved.body.is_empty() {
        return Err("the body is empty".into());
    }
    Ok(saved)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The menu lists the shipped applets `/name` starts and pins the one
    /// that writes applets below them.
    #[test]
    fn the_menu_lists_chat_applets_and_pins_new_applet() {
        let menu = menu();
        let names: Vec<&str> = menu.skills.iter().map(|s| s.name.as_str()).collect();
        for name in ["meeting-prep", "morning-brief", "trip-planner", "weekly-review"] {
            assert!(names.contains(&name), "{name} missing from {names:?}");
        }
        assert_eq!(menu.create.as_ref().map(|c| c.name.as_str()), Some(CREATE_APPLET));
        assert!(!names.contains(&CREATE_APPLET));
        let create = turn_named(CREATE_APPLET).expect("new-applet resolves");
        assert_eq!(create.tools, ["applet"]);
    }

    /// A skill `save_skill` wrote reads in the old format, with renamed tools
    /// mapped and vanished ones dropped.
    #[test]
    fn a_saved_skill_reads_in_the_old_format() {
        let src = "---\nname: garden-log\ndescription: What grew this week\nicon: ri:plant-line\n\
                   tools: sql_query, code_interpreter, list_applets, save_skill\nmax_steps: 20\n\
                   max_cost_usd: 2.5\nmax_minutes: 8\nmodel: deep\n---\nSay what grew.\n";
        let saved = parse_saved(src).unwrap();
        assert_eq!(saved.tools, ["sql_query", "shell", "applet"]);
        assert_eq!((saved.max_cost_usd, saved.max_minutes, saved.deep), (2.5, 8, true));
        assert_eq!(saved.body, "Say what grew.");
        assert!(parse_saved("no frontmatter").is_err());
    }

    /// The moved applet is a `SKILL.md` `/name` starts, with the skill's
    /// prompt, tools and ceilings.
    #[test]
    fn a_saved_skill_becomes_an_applet_that_loads() {
        let src = "---\nname: garden-log\ndescription: What grew: this week\ntools: sql_query\n\
                   max_steps: 20\nmax_cost_usd: 2.5\nmax_minutes: 8\n---\nSay what grew.\n";
        let saved = parse_saved(src).unwrap();
        let md = crate::applet_templates::SkillMd {
            name: "garden-log".into(),
            description: saved.description,
            allowed_tools: Some(saved.tools),
            invoke: vec!["chat".into()],
            limit: Some(format!("${} per run, {} minutes", saved.max_cost_usd, saved.max_minutes)),
            deep: saved.deep,
            icon: saved.icon,
            body: saved.body,
            ..Default::default()
        };
        let text = crate::applet_templates::render_skill_md(&md).unwrap();
        let dir = tempfile::tempdir().unwrap();
        crate::applet_templates::check_skill_md(&text, dir.path(), "user/garden-log")
            .unwrap_or_else(|e| panic!("{e:?}\n{text}"));
    }
}
