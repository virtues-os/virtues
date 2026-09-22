//! Skills: a prompt the chat reaches for, the way it reaches for a tool.
//!
//! Council and Deep Research were "modes" — and a mode here was four things
//! bolted together: a prompt block baked into the cached prefix, a tool
//! allowlist in `tools/mod.rs`, a step ceiling in `chat.rs`, and a picker in
//! the composer. Three of the four had to be edited in three files to change
//! one behavior, and the prompt block sat in the prefix, so switching modes
//! mid-chat rewrote the cache.
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
//! Compiled in with `include_str!` for the same reason tools and personas
//! are Rust constants: the shipped set needs no installer step and cannot
//! be missing on a box. Authored skills — the person's own — would live in
//! the state root beside authored applets, and are not built.
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
}

const COUNCIL: &str = include_str!("../../../skills/council/SKILL.md");

/// Every shipped skill. Parsed on each call, like `default_tools()`; the
/// sources are compiled in, so a parse failure is a build-time mistake
/// caught by the test below, and at runtime a bad file is skipped and
/// logged rather than taking the chat down with it.
pub fn default_skills() -> Vec<Skill> {
    [("council", COUNCIL)]
        .into_iter()
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

/// Parse `---` frontmatter of `key: value` lines, then the body. No YAML
/// library: the fields are flat scalars and a comma list, and a format this
/// small is better read by ten lines that say exactly what they accept.
fn parse(src: &str) -> Result<Skill, String> {
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
    for line in front.lines() {
        let (key, value) = line
            .split_once(':')
            .ok_or_else(|| format!("frontmatter line is not key: value — {line:?}"))?;
        let value = value.trim();
        match key.trim() {
            "name" => name = Some(value.to_string()),
            "description" => description = Some(value.to_string()),
            "tools" => {
                tools = value
                    .split(',')
                    .map(|t| t.trim().to_string())
                    .filter(|t| !t.is_empty())
                    .collect()
            }
            "max_steps" => max_steps = Some(value.parse().map_err(|e| format!("max_steps: {e}"))?),
            "max_cost_usd" => {
                max_cost_usd = Some(value.parse().map_err(|e| format!("max_cost_usd: {e}"))?)
            }
            "max_minutes" => {
                max_minutes = Some(value.parse().map_err(|e| format!("max_minutes: {e}"))?)
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
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::default_tools;

    /// The shipped files parse, and every tool a skill names is a tool the
    /// registry has — a typo here would silently hand a skill no tools at
    /// all, and the model would be told to convene voices it cannot call.
    #[test]
    fn every_shipped_skill_parses_and_names_real_tools() {
        let skills = default_skills();
        assert!(skills.iter().any(|s| s.name == "council"), "council ships");
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
    }

    #[test]
    fn council_carries_its_ceilings() {
        let c = skill_named("council").expect("council");
        assert_eq!(c.tools, ["think", "semantic_search", "sql_query", "dispatch_subagents"]);
        assert_eq!(c.max_steps, 40);
        assert!(c.body.starts_with("<council>"));
        assert!(skill_named("chat").is_none(), "ordinary chat is not a skill");
    }
}
