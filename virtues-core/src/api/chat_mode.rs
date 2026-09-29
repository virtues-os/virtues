//! What kind of turn a chat request is: resolved once, asked everywhere.
//!
//! The wire carries `agentMode` as a string ("chat" or "sudo" from today's
//! client), and some chats override it by id: the interview's chat is always
//! the interviewer, the getting-started room is the setup guest or, once the
//! interview has begun inside it, the interviewer. [`ChatMode::resolve`] makes
//! that decision once, in the handler; every mode-dependent choice after it
//! (prompt, tools, ceilings, model slot, pin, sudo) is a question asked of the
//! resulting value rather than another string match on the request.

use std::time::Duration;

use sqlx::PgPool;
use virtues_registry::models::ModelSlot;
use virtues_registry::skills::Skill;

use crate::agent::{Thinking, TurnBudget};
use crate::api::getting_started;

/// Per-turn tool budgets in plain chat. Four searches: Anthropic puts a
/// simple factual question at one to three, and the chat prompt asks for one
/// parallel batch plus at most one more — and says "four" in words, which
/// `the_chat_search_cap_is_the_one_the_prompt_states` holds it to. Deep
/// research, sudo and skills are uncapped here; their ceilings are steps,
/// cost and time.
pub(crate) const CHAT_TOOL_CAPS: &[(&str, u32)] = &[("web_search", 4)];

#[derive(Debug, Clone)]
pub enum ChatMode {
    /// The default: every tool (write/act tools confirm first), low effort,
    /// capped searches. Also what an unknown wire string means — a client
    /// ahead of this box gets ordinary chat.
    Chat,
    /// The owner's bypass: everything chat has plus `shell`, nothing asks
    /// first. See `tools::shell`.
    Sudo,
    /// Read-only research tools + fan-out + `create_page` for the report.
    DeepResearch,
    /// The narrative interview: its standalone prompt, one tool, no pin.
    Interview,
    /// The getting-started room: its own prompt and tools, no data.
    GettingStarted,
    /// A skill file (`skills/*/SKILL.md`): its tools, ceilings and body.
    Skill(Skill),
    /// A small model on the Dragon's NPU, and nowhere else. The handler hands
    /// the whole turn to `local_model` before any cloud step: no slot, no
    /// tools, no prompt from here, nothing stored. See `api::local_chat`.
    Local,
}

/// A turn's ceilings. See [`ChatMode::limits`].
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_steps: u32,
    pub budget: TurnBudget,
    pub thinking: Thinking,
    pub tool_timeout: Duration,
}

impl ChatMode {
    /// The mode the client asked for. Unknown strings are ordinary chat.
    pub fn from_wire(requested: &str) -> Self {
        match requested {
            "chat" => Self::Chat,
            "sudo" => Self::Sudo,
            "deep_research" => Self::DeepResearch,
            "interview" => Self::Interview,
            "local" => Self::Local,
            getting_started::AGENT_MODE => Self::GettingStarted,
            other => virtues_registry::skills::skill_named(other).map_or(Self::Chat, Self::Skill),
        }
    }

    /// The mode this turn runs in. Some chats are a mode by id — never by
    /// what the client sent — so no client can opt the interview into tools
    /// by sending a different `agentMode`.
    pub async fn resolve(pool: &PgPool, chat_id: &str, requested: &str) -> Self {
        if chat_id == crate::api::narrative_draft::INTERVIEW_CHAT_ID {
            return Self::Interview;
        }
        // Getting started is the same kind of room: its mode is the chat id's
        // — and, once the interview has begun inside it, the interviewer's.
        if chat_id == getting_started::GETTING_STARTED_CHAT_ID {
            return match getting_started::compute(pool).await {
                Ok(s) if s.interview_underway() => Self::Interview,
                Ok(_) => Self::GettingStarted,
                Err(e) => {
                    tracing::warn!(error = %e, "getting-started state unavailable; setup mode");
                    Self::GettingStarted
                }
            };
        }
        Self::from_wire(requested)
    }

    /// The mode's name: `app_ai_calls.feature`, logs, and what the client
    /// sends to ask for it.
    pub fn wire_name(&self) -> &str {
        match self {
            Self::Chat => "chat",
            Self::Sudo => "sudo",
            Self::DeepResearch => "deep_research",
            Self::Interview => "interview",
            Self::GettingStarted => getting_started::AGENT_MODE,
            Self::Skill(skill) => &skill.name,
            Self::Local => "local",
        }
    }

    /// The turn's ceilings. A step ceiling alone bounds nothing a person
    /// feels — twenty steps of a large model over a long context is real
    /// money, and twenty thirty-second tools is ten minutes — so beside it
    /// are dollars the gateway reports and wall-clock. First figures,
    /// 2026-09-21; the journal line "turn stopped at its budget" is how they
    /// get revised.
    ///
    /// Plain chat (and the two rooms, which ride its ceilings) also thinks at
    /// low effort and has a search budget. Effort governs how many tool calls
    /// a model makes as well as how long it thinks, and chat ran at the
    /// provider default — high, on most — which is how "what's on tonight"
    /// became thirteen searches. The other modes keep the model's default and
    /// no caps.
    pub fn limits(&self) -> Limits {
        let spend = |micros: i64, minutes: u64| TurnBudget {
            max_cost_micros: Some(micros),
            max_wall_clock: Some(Duration::from_secs(minutes * 60)),
            tool_caps: &[],
        };
        let (max_steps, budget, thinking) = match self {
            // A skill's ceilings come from its file.
            Self::Skill(skill) => (
                skill.max_steps,
                spend((skill.max_cost_usd * 1_000_000.0).round() as i64, skill.max_minutes),
                Thinking::Default,
            ),
            Self::DeepResearch => (50, spend(10_000_000, 25), Thinking::Default),
            // The owner's bypass: ceilings high enough that no real admin
            // session meets them. The dollar cap stays as the one thing
            // between a looping model and the bill.
            Self::Sudo => (500, spend(50_000_000, 4 * 60), Thinking::Default),
            // One generation, no tools, no gateway, so no cost. The runner
            // holds its own wall-clock limit (`local_model::TURN_TIME_LIMIT`).
            Self::Local => (1, spend(0, 12), Thinking::Default),
            Self::Chat | Self::Interview | Self::GettingStarted => (
                20,
                TurnBudget { tool_caps: CHAT_TOOL_CAPS, ..spend(2_500_000, 8) },
                Thinking::Low,
            ),
        };
        // Sudo: a long restore or migration through sql_* must not be cut
        // off at 30s. The shell has its own ceiling in agent::executor.
        let tool_timeout = if self.is_sudo() {
            Duration::from_secs(crate::tools::shell::MAX_TIMEOUT_SECS)
        } else {
            Duration::from_secs(30)
        };
        Limits { max_steps, budget, thinking, tool_timeout }
    }

    /// The tool definitions the model is offered this turn.
    pub fn tools(&self) -> Vec<serde_json::Value> {
        use crate::tools::{get_tool_definitions_for_llm, tools_named};
        match self {
            // Write/act tools confirm before running.
            Self::Chat => get_tool_definitions_for_llm(),
            // Everything chat has, plus `shell`, and nothing asks first.
            Self::Sudo => {
                let mut tools = get_tool_definitions_for_llm();
                tools.extend(tools_named(crate::tools::SUDO_ONLY_TOOLS));
                tools
            }
            // No other edit/act tools — see `DEEP_RESEARCH_TOOLS`.
            Self::DeepResearch => tools_named(crate::tools::DEEP_RESEARCH_TOOLS),
            // A listener, not an agent. Exactly one tool — the finisher that
            // turns the transcript into the document and chapters. No search,
            // no data, no pages: it must not read the record mid-confession
            // or claim capabilities.
            Self::Interview => tools_named(&["write_it_up"]),
            // The room is about the box, not the record. Skip a step, play
            // introductions back. No search, no data.
            Self::GettingStarted => tools_named(getting_started::TOOLS),
            // The tools its file declares.
            Self::Skill(skill) => {
                tools_named(&skill.tools.iter().map(String::as_str).collect::<Vec<_>>())
            }
            // A 0.6B model can't use tools, and a tool would reach the record.
            Self::Local => Vec::new(),
        }
    }

    /// The owner's bypass: `ToolContext.sudo`, the shell, no confirmations.
    pub fn is_sudo(&self) -> bool {
        matches!(self, Self::Sudo)
    }

    /// Whether the turn may honor the person's pin, or must ride the slot
    /// default.
    ///
    /// The interview may not. Its prompt promises "a no-retention agreement"
    /// in as many words, and the Virtues-curated slot map is what keeps that
    /// true: a pinned grok (`zdr: none`) or a BYO endpoint would silently void
    /// it. Same doctrine as the drafter in `narrative_draft.rs` — a pin
    /// governs the chats a person watches, not a room built on a retention
    /// promise.
    pub fn honors_pin(&self) -> bool {
        !matches!(self, Self::Interview)
    }

    /// Which model slot the turn belongs to.
    ///
    /// Every mode is the Chat slot today: they answer the same kind of hard
    /// turn and differ only in tools and prompt. It is a method anyway
    /// because it is the seam the mode/slot question belongs at. The Coding
    /// slot is pinnable in Settings while NOTHING in the box asks for it
    /// (`get_coding_model` has no callers), so the day applet authoring
    /// becomes a mode, this is the one line that wires it up.
    pub fn slot(&self) -> ModelSlot {
        ModelSlot::Chat
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn tool_names(mode: &ChatMode) -> BTreeSet<String> {
        mode.tools()
            .iter()
            .filter_map(|t| t["function"]["name"].as_str().map(str::to_string))
            .collect()
    }

    fn all_modes() -> Vec<ChatMode> {
        ["chat", "sudo", "deep_research", "interview", "getting_started", "council", "local"]
            .into_iter()
            .map(ChatMode::from_wire)
            .collect()
    }

    #[test]
    fn the_chat_search_cap_is_the_one_the_prompt_states() {
        let cap = CHAT_TOOL_CAPS.iter().find(|(t, _)| *t == "web_search").map(|(_, n)| *n);
        assert_eq!(cap, Some(4), "change the <web> block's \"Four searches\" with it");
        assert!(crate::agent::prompt::AGENT_MODE_PROMPT.contains("Four searches is the most a turn gets"));
    }

    #[test]
    fn every_known_wire_name_round_trips() {
        for name in ["chat", "sudo", "deep_research", "interview", "getting_started", "council", "local"] {
            assert_eq!(ChatMode::from_wire(name).wire_name(), name);
        }
        assert!(matches!(ChatMode::from_wire("sudo"), ChatMode::Sudo));
        assert!(matches!(ChatMode::from_wire("deep_research"), ChatMode::DeepResearch));
        assert!(matches!(ChatMode::from_wire("interview"), ChatMode::Interview));
        assert!(matches!(ChatMode::from_wire("local"), ChatMode::Local));
        assert!(matches!(ChatMode::from_wire("getting_started"), ChatMode::GettingStarted));
        assert!(matches!(ChatMode::from_wire("council"), ChatMode::Skill(ref s) if s.name == "council"));
    }

    /// A client ahead of this box gets ordinary chat, not an error.
    #[test]
    fn an_unknown_mode_is_chat() {
        for name in ["", "agent", "default", "anything-else", "CHAT"] {
            assert!(matches!(ChatMode::from_wire(name), ChatMode::Chat), "{name:?}");
        }
    }

    /// No skill file may shadow a built-in mode's name.
    #[test]
    fn every_skill_resolves_to_itself() {
        for skill in virtues_registry::skills::default_skills() {
            assert!(matches!(ChatMode::from_wire(&skill.name), ChatMode::Skill(ref s) if s.name == skill.name));
        }
    }

    #[test]
    fn limits_are_todays_figures() {
        let secs = |m: u64| Some(Duration::from_secs(m * 60));
        let chat_like = [ChatMode::Chat, ChatMode::Interview, ChatMode::GettingStarted];
        for mode in chat_like {
            let l = mode.limits();
            assert_eq!(l.max_steps, 20, "{mode:?}");
            assert_eq!(l.budget.max_cost_micros, Some(2_500_000));
            assert_eq!(l.budget.max_wall_clock, secs(8));
            assert_eq!(l.budget.tool_caps, CHAT_TOOL_CAPS);
            assert_eq!(l.thinking, Thinking::Low);
            assert_eq!(l.tool_timeout, Duration::from_secs(30));
        }

        let l = ChatMode::DeepResearch.limits();
        assert_eq!((l.max_steps, l.budget.max_cost_micros, l.budget.max_wall_clock), (50, Some(10_000_000), secs(25)));
        assert!(l.budget.tool_caps.is_empty());
        assert_eq!(l.thinking, Thinking::Default);
        assert_eq!(l.tool_timeout, Duration::from_secs(30));

        let l = ChatMode::Sudo.limits();
        assert_eq!((l.max_steps, l.budget.max_cost_micros, l.budget.max_wall_clock), (500, Some(50_000_000), secs(240)));
        assert!(l.budget.tool_caps.is_empty());
        assert_eq!(l.thinking, Thinking::Default);
        assert_eq!(l.tool_timeout, Duration::from_secs(crate::tools::shell::MAX_TIMEOUT_SECS));

        // Council's file: 40 steps, $5, 15 minutes.
        let l = ChatMode::from_wire("council").limits();
        assert_eq!((l.max_steps, l.budget.max_cost_micros, l.budget.max_wall_clock), (40, Some(5_000_000), secs(15)));
        assert!(l.budget.tool_caps.is_empty());
        assert_eq!(l.thinking, Thinking::Default);
        assert_eq!(l.tool_timeout, Duration::from_secs(30));
    }

    #[test]
    fn tools_are_todays_sets() {
        let set = |ids: &[&str]| ids.iter().map(|s| s.to_string()).collect::<BTreeSet<_>>();
        let every: BTreeSet<String> = crate::tools::get_tool_definitions_for_llm()
            .iter()
            .filter_map(|t| t["function"]["name"].as_str().map(str::to_string))
            .collect();

        assert_eq!(tool_names(&ChatMode::Chat), every);
        let mut sudo = every.clone();
        sudo.insert("shell".into());
        assert_eq!(tool_names(&ChatMode::Sudo), sudo);
        assert_eq!(
            tool_names(&ChatMode::DeepResearch),
            set(&["think", "web_search", "semantic_search", "sql_query", "code_interpreter", "dispatch_subagents", "create_page"])
        );
        assert_eq!(tool_names(&ChatMode::Interview), set(&["write_it_up"]));
        assert_eq!(tool_names(&ChatMode::GettingStarted), set(&["skip_step", "record_introductions"]));
        assert_eq!(
            tool_names(&ChatMode::from_wire("council")),
            set(&["think", "semantic_search", "sql_query", "dispatch_subagents"])
        );
    }

    /// Every model step re-sends the mode's tool definitions ahead of the
    /// person's message. At ~41k characters chat's were most of the prompt
    /// and the model visibly deliberated over them, so every tool belongs to
    /// a group with a ceiling, and every mode has a total, in characters of
    /// the JSON exactly as `ChatMode::tools()` serializes it. Long how-to
    /// belongs behind a call (`setup_applet {"guide": true}` returns
    /// applets/AGENTS.md), not in the definition. A tool in any mode — chat,
    /// sudo, deep research, interview, getting started, local or a skill —
    /// must join a group here, and a new mode or skill must join `MODES`:
    /// unbudgeted tools and modes fail the test.
    #[test]
    fn tool_definitions_stay_inside_their_budgets() {
        // (group, tools, ceiling). A group's size is measured per mode: the
        // tools of it that mode offers.
        const GROUPS: &[(&str, &[&str], usize)] = &[
            (
                "applets",
                &["setup_applet", "edit_applet", "list_applets", "run_applet", "get_applet", "delete_applet"],
                3_000,
            ),
            ("analysis", &["code_interpreter", "think", "read_asset", "generate_image"], 2_400),
            ("pages", &["edit_page", "get_page_content", "create_page", "get_project_item"], 2_400),
            ("search", &["semantic_search", "web_search"], 2_000),
            ("self", &["update_memory", "propose_narrative_identity_edit"], 1_400),
            ("sql_write", &["sql_write"], 600),
            // The most-used tool, and its table block is generated from the
            // catalog (sql_catalog::prompt_block), so a new table or a longer
            // note lands here. Trimmed from 9.6k on 2026-09-29 with no column
            // dropped: failures were 6 unknown columns and 5 ambiguous joins
            // in 1,422 calls, so the columns stay and the prose went.
            ("sql_query", &["sql_query"], 6_500),
            // Mode-only tools, each set just above its 2026-09-29 size.
            ("shell", &["shell"], 1_500),
            ("dispatch", &["dispatch_subagents"], 3_300),
            ("interview", &["write_it_up"], 1_500),
            ("getting_started", &["skip_step", "record_introductions"], 2_200),
        ];
        // (mode's wire name, total ceiling). Chat's is the sum of its group
        // ceilings; the rest sit just above their 2026-09-29 size.
        const MODES: &[(&str, usize)] = &[
            ("chat", 18_300),
            ("sudo", 18_500),
            ("deep_research", 13_100),
            ("interview", 1_500),
            ("getting_started", 2_200),
            ("local", 0),
            ("council", 11_000),
        ];

        let size = |t: &serde_json::Value| serde_json::to_string(t).unwrap().chars().count();
        let name = |t: &serde_json::Value| t["function"]["name"].as_str().unwrap_or("").to_string();
        let mut modes = vec![
            ChatMode::Chat,
            ChatMode::Sudo,
            ChatMode::DeepResearch,
            ChatMode::Interview,
            ChatMode::GettingStarted,
            ChatMode::Local,
        ];
        modes.extend(virtues_registry::skills::default_skills().into_iter().map(ChatMode::Skill));

        let mut report = String::new();
        let mut failures = Vec::new();
        for mode in &modes {
            let mode_name = mode.wire_name();
            let tools = mode.tools();
            let total: usize = tools.iter().map(size).sum();
            report.push_str(&format!("{mode_name}: {total}\n"));
            for t in &tools {
                if !GROUPS.iter().any(|(_, names, _)| names.contains(&name(t).as_str())) {
                    failures.push(format!("{mode_name}: tool {} ({} chars) has no budget group", name(t), size(t)));
                }
            }
            for (group, names, ceiling) in GROUPS {
                let members: Vec<_> = tools.iter().filter(|t| names.contains(&name(t).as_str())).collect();
                if members.is_empty() {
                    continue;
                }
                let n: usize = members.iter().map(|t| size(t)).sum();
                report.push_str(&format!("  {group}: {n}\n"));
                for t in &members {
                    report.push_str(&format!("    {}: {}\n", name(t), size(t)));
                }
                if n > *ceiling {
                    failures.push(format!("{mode_name}: group {group} is {n} chars, budget {ceiling}"));
                }
            }
            match MODES.iter().find(|(m, _)| *m == mode_name) {
                None => failures.push(format!("{mode_name}: no total budget in MODES ({total} chars today)")),
                Some((_, ceiling)) if total > *ceiling => {
                    failures.push(format!("{mode_name}: tools are {total} chars, budget {ceiling}"))
                }
                Some(_) => {}
            }
        }
        eprintln!("{report}");
        assert!(failures.is_empty(), "{}\n{report}", failures.join("\n"));
    }

    #[test]
    fn only_the_interview_refuses_a_pin_and_only_sudo_is_sudo() {
        for mode in all_modes() {
            assert_eq!(mode.honors_pin(), !matches!(mode, ChatMode::Interview), "{mode:?}");
            assert_eq!(mode.is_sudo(), matches!(mode, ChatMode::Sudo), "{mode:?}");
            assert_eq!(mode.slot(), ModelSlot::Chat, "{mode:?}");
        }
    }

    /// The chat id beats the wire: the interview's chat is the interviewer,
    /// the getting-started room is setup until the interview begins in it.
    #[sqlx::test]
    async fn some_chats_are_a_mode_by_id(pool: PgPool) {
        let interview = crate::api::narrative_draft::INTERVIEW_CHAT_ID;
        let room = getting_started::GETTING_STARTED_CHAT_ID;

        assert!(matches!(ChatMode::resolve(&pool, interview, "sudo").await, ChatMode::Interview));
        assert!(matches!(ChatMode::resolve(&pool, room, "sudo").await, ChatMode::GettingStarted));
        assert!(matches!(ChatMode::resolve(&pool, "chat_other", "sudo").await, ChatMode::Sudo));
        assert!(matches!(ChatMode::resolve(&pool, "chat_other", "nope").await, ChatMode::Chat));

        sqlx::query("UPDATE app_user_profile SET interview_started_at = now()")
            .execute(&pool)
            .await
            .unwrap();
        assert!(matches!(ChatMode::resolve(&pool, room, "chat").await, ChatMode::Interview));
    }
}
