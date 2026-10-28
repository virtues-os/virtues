//! Skills: a prompt the chat reaches for, the way it reaches for a tool.
//!
//! A built-in "mode" is four things bolted together: a prompt block baked
//! into the cached prefix, a tool allowlist in `tools/mod.rs`, a step ceiling
//! in `chat.rs`, and a picker in the composer. Changing one behavior means
//! editing three files, and the prompt block sits in the prefix, so switching
//! modes mid-chat rewrites the cache.
//!
//! A skill is one file. Frontmatter names it, says in one line when it
//! applies, and lists the tools and ceilings it needs; the body is the
//! prompt. It is loaded on demand — appended to the per-turn tail of the
//! system prompt, never the cached prefix — so the prefix is the same
//! whatever the chat is doing. The step ceiling and the budget follow from
//! the file. The tool allowlist follows from the file. The picker still
//! sends the skill's name as `agent_mode` for now; that is the one seam the
//! old shape still owns.
//!
//! Compiled in with `include_str!` for the same reason tools are Rust
//! constants: the shipped set needs no installer step and cannot
//! be missing on a box. Authored skills, the person's own, are files in the
//! same format under the box's state root; `virtues-core`'s `api::skills`
//! reads and writes them with [`parse`] and [`render`].
//!
//! An applet is the same thing run unattended: a prompt, a tool list, a
//! budget, and a trigger. This format is written so that field can be
//! added rather than a second format grown.

/// A skill as its file declares it.
#[derive(Debug, Clone)]
pub struct Skill {
    /// The name the composer sends as `agent_mode` and the file's `name:`.
    pub name: String,
    /// One line on when this applies — what a router (person or model)
    /// reads to choose it. Kept to a line because every skill's line rides
    /// in the prefix once model routing exists.
    pub description: String,
    /// The prompt, verbatim. Appended to the turn's tail inside
    /// `<skill name="…">`.
    pub body: String,
    /// The tools the turn may call; each must be a registered tool id.
    pub tools: Vec<String>,
    /// The step ceiling for a turn running this skill.
    pub max_steps: u32,
    /// The cost ceiling, in dollars as the gateway reports them.
    pub max_cost_usd: f64,
    /// The wall-clock ceiling, in minutes.
    pub max_minutes: u64,
    /// The model tier the turn runs on: `model: standard` (the default when
    /// the key is absent) or `model: deep`. A skill picks its tier the way it
    /// picks its tools — by declaring it, not by a router guessing.
    pub slot: crate::models::ModelSlot,
    /// The icon the composer's `/` menu shows, an `ri:` id. Optional.
    pub icon: Option<String>,
}

/// The shipped skill files, as `(name, include_str!("../../../skills/<name>/SKILL.md"))`.
const SHIPPED: &[(&str, &str)] = &[
    ("meeting-prep", include_str!("../../../skills/meeting-prep/SKILL.md")),
    ("morning-brief", include_str!("../../../skills/morning-brief/SKILL.md")),
    ("new-skill", include_str!("../../../skills/new-skill/SKILL.md")),
    ("trip-planner", include_str!("../../../skills/trip-planner/SKILL.md")),
    ("weekly-review", include_str!("../../../skills/weekly-review/SKILL.md")),
];

/// Every shipped skill. Parsed on each call, like `default_tools()`; the
/// sources are compiled in, so a parse failure is a build-time mistake
/// caught by the test below, and at runtime a bad file is skipped and
/// logged rather than taking the chat down with it.
pub fn default_skills() -> Vec<Skill> {
    SHIPPED
        .iter()
        .filter_map(|(name, src)| match parse(src) {
            Ok(skill) => Some(skill),
            Err(e) => {
                eprintln!("skill {name} is malformed and was not loaded: {e}");
                None
            }
        })
        .collect()
}

/// The skill a chat is running, by the name the request carried — or none,
/// which is ordinary chat.
pub fn skill_named(name: &str) -> Option<Skill> {
    default_skills().into_iter().find(|s| s.name == name)
}

/// A tool id as it is named now. A skill saved before the applet tools became
/// one `applet` tool names them separately, and without this it would lose
/// applet access without a word.
fn current_tool_id(id: &str) -> &str {
    match id {
        "setup_applet" | "list_applets" | "get_applet" | "edit_applet" | "delete_applet" | "run_applet" => "applet",
        other => other,
    }
}

/// Parse `---` frontmatter of `key: value` lines, then the body. No YAML
/// library: the fields are flat scalars and a comma list, and a format this
/// small is better read by ten lines that say exactly what they accept.
pub fn parse(src: &str) -> Result<Skill, String> {
    let rest = src
        .strip_prefix("---\n")
        .ok_or("frontmatter must open with --- on the first line")?;
    let (front, body) = rest
        .split_once("\n---\n")
        .ok_or("frontmatter must close with --- on its own line")?;

    let mut name = None;
    let mut description = None;
    let mut tools: Vec<String> = Vec::new();
    let mut max_steps = None;
    let mut max_cost_usd = None;
    let mut max_minutes = None;
    let mut slot = crate::models::ModelSlot::Standard;
    let mut icon = None;
    for line in front.lines() {
        let (key, value) = line
            .split_once(':')
            .ok_or_else(|| format!("frontmatter line is not key: value — {line:?}"))?;
        let value = value.trim();
        match key.trim() {
            "name" => name = Some(value.to_string()),
            "description" => description = Some(value.to_string()),
            "tools" => {
                for t in value.split(',').map(|t| current_tool_id(t.trim())).filter(|t| !t.is_empty()) {
                    if !tools.iter().any(|known| known == t) {
                        tools.push(t.to_string());
                    }
                }
            }
            "max_steps" => max_steps = Some(value.parse().map_err(|e| format!("max_steps: {e}"))?),
            "max_cost_usd" => {
                max_cost_usd = Some(value.parse().map_err(|e| format!("max_cost_usd: {e}"))?)
            }
            "max_minutes" => {
                max_minutes = Some(value.parse().map_err(|e| format!("max_minutes: {e}"))?)
            }
            "model" => {
                use crate::models::ModelSlot;
                slot = match ModelSlot::from_name(value) {
                    Some(s @ (ModelSlot::Standard | ModelSlot::Deep)) => s,
                    _ => return Err(format!("model: {value:?} is not standard or deep")),
                }
            }
            "icon" => {
                let ok = value.starts_with("ri:")
                    && value.len() <= 64
                    && value[3..].chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
                if !ok {
                    return Err(format!("icon: {value:?} is not an ri: icon id"));
                }
                icon = Some(value.to_string());
            }
            other => return Err(format!("unknown frontmatter key {other:?}")),
        }
    }

    let body = body.trim();
    if body.is_empty() {
        return Err("the body is empty".into());
    }
    Ok(Skill {
        name: name.ok_or("name is required")?,
        description: description.ok_or("description is required")?,
        body: body.to_string(),
        tools,
        max_steps: max_steps.ok_or("max_steps is required")?,
        max_cost_usd: max_cost_usd.ok_or("max_cost_usd is required")?,
        max_minutes: max_minutes.ok_or("max_minutes is required")?,
        slot,
        icon,
    })
}

/// Write a skill back out as a file [`parse`] reads. Single-line fields have
/// their newlines folded to spaces, because a frontmatter line ends at one.
pub fn render(skill: &Skill) -> String {
    let line = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out = String::from("---\n");
    out.push_str(&format!("name: {}\n", line(&skill.name)));
    out.push_str(&format!("description: {}\n", line(&skill.description)));
    if let Some(icon) = &skill.icon {
        out.push_str(&format!("icon: {}\n", line(icon)));
    }
    out.push_str(&format!("tools: {}\n", skill.tools.join(", ")));
    out.push_str(&format!("max_steps: {}\n", skill.max_steps));
    out.push_str(&format!("max_cost_usd: {}\n", skill.max_cost_usd));
    out.push_str(&format!("max_minutes: {}\n", skill.max_minutes));
    if skill.slot != crate::models::ModelSlot::Standard {
        out.push_str(&format!("model: {}\n", skill.slot.as_str()));
    }
    out.push_str("---\n");
    out.push_str(skill.body.trim());
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::default_tools;

    /// The shipped files parse, and every tool a skill names is a tool the
    /// registry has — a typo here would silently hand a skill no tools at
    /// all, and the model would be told to use tools it cannot call.
    #[test]
    fn every_shipped_skill_parses_and_names_real_tools() {
        let skills = default_skills();
        assert_eq!(skills.len(), SHIPPED.len(), "a shipped skill failed to parse");
        let known: Vec<String> = default_tools().into_iter().map(|t| t.id).collect();
        for s in &skills {
            assert!(!s.description.is_empty(), "{} has no description", s.name);
            assert!(!s.tools.is_empty(), "{} names no tools", s.name);
            for t in &s.tools {
                assert!(known.contains(t), "{} names unknown tool {t}", s.name);
            }
            assert!(s.max_steps > 0 && s.max_cost_usd > 0.0 && s.max_minutes > 0, "{} has a zero ceiling", s.name);
            // The body is a prompt, not a mode block: `<mode>` was the old
            // shape's tag and the loop no longer reads it.
            assert!(!s.body.contains("<mode>"), "{} body still carries a <mode> tag", s.name);
        }
    }

    #[test]
    fn a_malformed_file_says_what_is_wrong() {
        assert!(parse("no frontmatter").unwrap_err().contains("open with ---"));
        assert!(parse("---\nname: x\n---\n").unwrap_err().contains("empty"));
        assert!(parse("---\nname: x\nbogus: 1\n---\nbody").unwrap_err().contains("bogus"));
        assert!(parse("---\nname: x\n---\nbody").unwrap_err().contains("description"));
        assert!(parse("---\nname: x\nmodel: image\n---\nbody").unwrap_err().contains("standard or deep"));
        assert!(parse("---\nname: x\nicon: <svg>\n---\nbody").unwrap_err().contains("ri: icon"));
    }

    /// A skill saved when the applet tools were separate keeps applet access.
    #[test]
    fn old_applet_tool_ids_read_as_the_applet_tool() {
        let s = parse("---\nname: x\ndescription: d\ntools: sql_query, list_applets, run_applet\nmax_steps: 1\nmax_cost_usd: 1\nmax_minutes: 1\n---\nbody").unwrap();
        assert_eq!(s.tools, ["sql_query", "applet"]);
    }

    /// What `render` writes, `parse` reads back unchanged, so an authored
    /// skill survives being saved.
    #[test]
    fn render_round_trips() {
        for s in default_skills() {
            let back = parse(&render(&s)).expect("rendered skill parses");
            assert_eq!(
                (&back.name, &back.description, &back.icon, &back.tools, &back.body),
                (&s.name, &s.description, &s.icon, &s.tools, &s.body)
            );
            assert_eq!((back.max_steps, back.max_minutes, back.slot), (s.max_steps, s.max_minutes, s.slot));
            assert!((back.max_cost_usd - s.max_cost_usd).abs() < 1e-9);
        }
        let deep = parse("---\nname: d\ndescription: d\nmax_steps: 1\nmax_cost_usd: 1\nmax_minutes: 1\nmodel: deep\n---\nbody").unwrap();
        assert_eq!(parse(&render(&deep)).unwrap().slot, crate::models::ModelSlot::Deep);
    }

    #[test]
    fn a_skill_carries_its_ceilings_and_tier() {
        let s = parse("---\nname: x\ndescription: d\ntools: think, sql_query\nmax_steps: 40\nmax_cost_usd: 5\nmax_minutes: 15\nmodel: deep\n---\n<x>body</x>")
            .expect("parses");
        assert_eq!(s.tools, ["think", "sql_query"]);
        assert_eq!((s.max_steps, s.max_minutes), (40, 15));
        assert_eq!(s.slot, crate::models::ModelSlot::Deep);
        let plain = parse("---\nname: y\ndescription: d\nmax_steps: 1\nmax_cost_usd: 1\nmax_minutes: 1\n---\nbody").unwrap();
        assert_eq!(plain.slot, crate::models::ModelSlot::Standard, "no model key is Standard");
        assert!(skill_named("chat").is_none(), "ordinary chat is not a skill");
    }
}
