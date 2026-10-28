//! What kind of turn a chat request is: resolved once, asked everywhere.
//!
//! The wire carries `agentMode` as a string ("chat", or the name of the
//! applet `/name` picked), and some chats override it by id: the interview's chat is always
//! the interviewer, the getting-started room is the setup guest or, once the
//! interview has begun inside it, the interviewer. [`ChatMode::resolve`] makes
//! that decision once, in the handler; every mode-dependent choice after it
//! (prompt, tools, ceilings, model slot, pin, sudo) is a question asked of the
//! resulting value rather than another string match on the request.

use std::time::Duration;

use sqlx::PgPool;
use virtues_registry::models::ModelSlot;
use crate::api::chat_applets::AppletTurn;

use crate::agent::{Thinking, TurnBudget};
use crate::api::getting_started;

/// Per-turn tool budgets in plain chat. Four searches: Anthropic puts a
/// simple factual question at one to three, and the chat prompt asks for one
/// parallel batch plus at most one more — and says "four" in words, which
/// `the_chat_search_cap_is_the_one_the_prompt_states` holds it to. Applets
/// are uncapped here; their ceilings are steps, cost and time.
pub(crate) const CHAT_TOOL_CAPS: &[(&str, u32)] = &[
    ("web_search", 4),
    // A message is sent the moment it is called. A turn that wants more than
    // this is a loop, not a request.
    (crate::tools::send_imessage::TOOL, 3),
];

/// What the chat can do through the Mac app: its browser whenever an app is
/// connected, and Messages only when that app says it can send. An app
/// released before sending existed connects too, and would refuse every send.
fn mac_tools(connected: bool, can_send: bool) -> Vec<&'static str> {
    let mut tools = Vec::new();
    if connected {
        tools.push(crate::browse::TOOL);
        if can_send {
            tools.push(crate::tools::send_imessage::TOOL);
        }
    }
    tools
}

/// `mac_tools` for the app connected right now.
fn connected_mac_tools() -> Vec<&'static str> {
    mac_tools(crate::browser::connected(), crate::browser::supports(crate::outbox::SEND_OP))
}

/// Chat's tools: every model-facing tool but `think`. The chat model reasons
/// natively, so a `think` call buys nothing it would not have thought anyway
/// and costs a whole extra step, with the prompt and history re-sent. An
/// applet that wants it lists it, as `deep-research` does.
fn chat_tools() -> Vec<serde_json::Value> {
    crate::tools::get_tool_definitions_for_llm()
        .into_iter()
        .filter(|t| t["function"]["name"] != "think")
        .collect()
}

#[derive(Debug, Clone)]
pub enum ChatMode {
    /// The default: every tool (write/act tools confirm first), low effort,
    /// capped searches. Also what an unknown wire string means — a client
    /// ahead of this box gets ordinary chat.
    Chat,
    /// The narrative interview: its standalone prompt, one tool, no pin.
    Interview,
    /// The getting-started room: its own prompt and tools, no data.
    GettingStarted,
    /// An applet `/name` starts (`api::chat_applets`): its body, tools and
    /// ceilings shape the turn. Deep research and the owner's admin shell are
    /// the shipped applets `deep-research` and `admin`.
    Applet(AppletTurn),
    /// A small model on the Dragon's NPU, and nowhere else. The handler hands
    /// the whole turn to `local_model` before any cloud step: no slot, no
    /// tools, no prompt from here, nothing stored. See `api::local_chat`.
    // TODO(2026-10-27): the app no longer offers Local. Delete this mode,
    // `api::local_chat` and `/api/local-model`, or reach them from a model
    // picker; the owner has not decided which.
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

/// What the wire's mode field names before any applet: the modes, and the old
/// names a client from before admin and deep research were applets sends.
/// [`ChatMode::from_wire`] reads these first, so an applet the box did not
/// ship may not take one as its `/name`: an imported `sudo` would start the
/// real admin applet.
pub(crate) const RESERVED_CHAT_NAMES: &[&str] =
    &["chat", "interview", "local", getting_started::AGENT_MODE, "sudo", "deep_research"];

impl ChatMode {
    /// The mode the client asked for. Unknown strings are ordinary chat.
    pub fn from_wire(requested: &str) -> Self {
        use crate::api::chat_applets::{turn_named, ADMIN_APPLET, DEEP_RESEARCH_APPLET};
        let applet = match requested {
            "chat" => return Self::Chat,
            "interview" => return Self::Interview,
            "local" => return Self::Local,
            getting_started::AGENT_MODE => return Self::GettingStarted,
            // What a client from before these were applets sends.
            "sudo" => ADMIN_APPLET,
            "deep_research" => DEEP_RESEARCH_APPLET,
            other => other,
        };
        turn_named(applet).map_or(Self::Chat, Self::Applet)
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
            Self::Interview => "interview",
            Self::GettingStarted => getting_started::AGENT_MODE,
            Self::Applet(applet) => &applet.name,
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
    /// became thirteen searches. Applets keep the model's default and no
    /// caps.
    pub fn limits(&self) -> Limits {
        let spend = |micros: i64, minutes: u64| TurnBudget {
            max_cost_micros: Some(micros),
            max_wall_clock: Some(Duration::from_secs(minutes * 60)),
            tool_caps: &[],
        };
        let (max_steps, budget, thinking) = match self {
            // An applet's ceilings come from its folder.
            Self::Applet(applet) => (
                applet.max_steps,
                spend((applet.max_cost_usd * 1_000_000.0).round() as i64, applet.max_minutes),
                Thinking::Default,
            ),
            // One generation, no tools, no gateway, so no cost. The runner
            // holds its own wall-clock limit (`local_model::TURN_TIME_LIMIT`).
            Self::Local => (1, spend(0, 12), Thinking::Default),
            Self::Chat | Self::Interview | Self::GettingStarted => (
                20,
                TurnBudget { tool_caps: CHAT_TOOL_CAPS, ..spend(2_500_000, 8) },
                Thinking::Low,
            ),
        };
        // Admin: a long restore or migration through sql_* must not be cut
        // off at 30s. `sudo` has its own ceiling in agent::executor.
        let tool_timeout = if self.is_sudo() {
            Duration::from_secs(crate::tools::sudo_shell::MAX_TIMEOUT_SECS)
        } else {
            Duration::from_secs(30)
        };
        Limits { max_steps, budget, thinking, tool_timeout }
    }

    /// The tool definitions the model is offered this turn.
    pub fn tools(&self) -> Vec<serde_json::Value> {
        use crate::tools::tools_named;
        match self {
            // Write/act tools confirm before running. The owner's browser
            // only while an app that has one is connected: a tool the model
            // cannot use is one it will try anyway.
            Self::Chat => {
                let mut tools = chat_tools();
                tools.extend(tools_named(&connected_mac_tools()));
                // The applets it may load, when there are any.
                tools.extend(crate::tools::use_applet::definition());
                tools
            }
            // A listener, not an agent. Exactly one tool — the finisher that
            // turns the transcript into the document and chapters. No search,
            // no data, no pages: it must not read the record mid-confession
            // or claim capabilities.
            Self::Interview => tools_named(&["write_it_up"]),
            // The room is about the box, not the record. Skip a step, play
            // introductions back. No search, no data.
            Self::GettingStarted => tools_named(getting_started::TOOLS),
            // The tools its file declares, and its own files when it has any.
            Self::Applet(applet) => {
                let mut tools = tools_named(&applet.tools.iter().map(String::as_str).collect::<Vec<_>>());
                tools.extend(crate::tools::use_applet::own_files_definition(&applet.name));
                tools
            }
            // A 0.6B model can't use tools, and a tool would reach the record.
            Self::Local => Vec::new(),
        }
    }

    /// The owner's bypass, `ToolContext.sudo`: a turn of a shipped applet
    /// that lists `sudo`, which is `admin`. The reader refuses `sudo` from any
    /// other folder, and `shipped` is checked again here.
    pub fn is_sudo(&self) -> bool {
        matches!(self, Self::Applet(a) if a.shipped && a.tools.iter().any(|t| t == "sudo"))
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

    /// Which model tier the turn runs on.
    ///
    /// The mode decides, and nothing guesses per turn. An applet that
    /// declares `model: deep`, such as `deep-research`, is the rare, slow,
    /// high-stakes turn the strong tier exists for; everything else is
    /// Standard. Deciding per turn instead would mean switching models inside
    /// a chat, and every switch starts a cold prompt cache — most of a chat's
    /// input tokens are served from that cache.
    pub fn slot(&self) -> ModelSlot {
        match self {
            Self::Applet(applet) => applet.slot,
            _ => ModelSlot::Standard,
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn tool_names(mode: &ChatMode) -> BTreeSet<String> {
        mode.tools()
            .iter()
            .filter_map(|t| t["function"]["name"].as_str().map(str::to_string))
            .collect()
    }

    /// An applet turn as a folder would shape it, so the Applet arm is
    /// exercised through this.
    pub(crate) fn fixture_applet() -> ChatMode {
        ChatMode::Applet(AppletTurn {
            name: "fixture".into(),
            description: "d".into(),
            body: "<fixture>body</fixture>".into(),
            tools: vec!["think".into(), "semantic_search".into(), "sql_query".into()],
            max_steps: 40,
            max_cost_usd: 5.0,
            max_minutes: 15,
            slot: ModelSlot::Deep,
            icon: None,
            shipped: false,
        })
    }

    fn applet(name: &str) -> AppletTurn {
        match ChatMode::from_wire(name) {
            ChatMode::Applet(a) => a,
            other => panic!("{name} resolved to {other:?}"),
        }
    }

    #[test]
    fn send_imessage_is_offered_only_to_an_app_that_can_send() {
        let send = crate::tools::send_imessage::TOOL;
        assert!(mac_tools(false, false).is_empty());
        assert!(mac_tools(false, true).is_empty());
        let older_app = mac_tools(true, false);
        assert!(older_app.contains(&crate::browse::TOOL));
        assert!(!older_app.contains(&send));
        assert_eq!(mac_tools(true, true), vec![crate::browse::TOOL, send]);
    }

    fn all_modes() -> Vec<ChatMode> {
        ["chat", "admin", "deep-research", "interview", "getting_started", "local"]
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
        for name in ["chat", "admin", "deep-research", "interview", "getting_started", "local"] {
            assert_eq!(ChatMode::from_wire(name).wire_name(), name);
        }
        // A client from before the two modes were applets.
        assert_eq!(ChatMode::from_wire("sudo").wire_name(), "admin");
        assert_eq!(ChatMode::from_wire("deep_research").wire_name(), "deep-research");
        assert!(matches!(ChatMode::from_wire("interview"), ChatMode::Interview));
        assert!(matches!(ChatMode::from_wire("local"), ChatMode::Local));
        assert!(matches!(ChatMode::from_wire("getting_started"), ChatMode::GettingStarted));
        // Council was removed; a client that still sends it gets chat.
        assert!(matches!(ChatMode::from_wire("council"), ChatMode::Chat));
    }

    /// A client ahead of this box gets ordinary chat, not an error.
    #[test]
    fn an_unknown_mode_is_chat() {
        for name in ["", "agent", "default", "anything-else", "CHAT"] {
            assert!(matches!(ChatMode::from_wire(name), ChatMode::Chat), "{name:?}");
        }
    }

    /// No applet may shadow a built-in mode's name, and each shipped one
    /// `/name` starts resolves to itself.
    #[test]
    fn every_chat_applet_resolves_to_itself() {
        let names: Vec<String> =
            crate::applet_templates::chat_applets().into_iter().filter(|a| a.shipped).map(|a| a.name).collect();
        for name in ["admin", "deep-research", "meeting-prep", "morning-brief", "trip-planner", "weekly-review", "new-applet"] {
            assert!(names.iter().any(|n| n == name), "{name} is not offered");
        }
        for name in names {
            assert!(matches!(ChatMode::from_wire(&name), ChatMode::Applet(ref a) if a.name == name), "{name}");
        }
    }

    /// A shipped applet `/weekly-review` starts runs as a turn shaped by its
    /// folder: its body, its tools, its `limit`.
    #[test]
    fn weekly_review_runs_from_its_applet_folder() {
        let ChatMode::Applet(s) = ChatMode::from_wire("weekly-review") else { panic!("not resolved") };
        assert!(s.body.starts_with("<weekly_review>"), "{}", &s.body[..40.min(s.body.len())]);
        assert_eq!(s.tools, ["sql_query", "semantic_search", "get_project_item", "create_page"]);
        assert_eq!((s.max_cost_usd, s.max_minutes), (3.0, 10));
        assert_eq!(s.icon.as_deref(), Some("ri:calendar-check-line"));
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

        let l = ChatMode::Applet(applet("deep-research")).limits();
        assert_eq!((l.max_steps, l.budget.max_cost_micros, l.budget.max_wall_clock), (60, Some(10_000_000), secs(25)));
        assert!(l.budget.tool_caps.is_empty());
        assert_eq!(l.thinking, Thinking::Default);
        assert_eq!(l.tool_timeout, Duration::from_secs(30));

        let l = ChatMode::Applet(applet("admin")).limits();
        assert_eq!((l.max_steps, l.budget.max_cost_micros, l.budget.max_wall_clock), (60, Some(5_000_000), secs(30)));
        assert!(l.budget.tool_caps.is_empty());
        assert_eq!(l.thinking, Thinking::Default);
        assert_eq!(l.tool_timeout, Duration::from_secs(crate::tools::sudo_shell::MAX_TIMEOUT_SECS));

        // The fixture's: 40 steps, $5, 15 minutes, and no sudo: it did not ship.
        let l = fixture_applet().limits();
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
            .filter(|name| name != "think")
            .collect();

        let mut chat = every.clone();
        chat.insert("use_applet".into());
        assert_eq!(tool_names(&ChatMode::Chat), chat);
        assert_eq!(tool_names(&ChatMode::Applet(applet("admin"))), set(&["sudo", "sql_query", "sql_write"]));
        assert_eq!(
            tool_names(&ChatMode::Applet(applet("deep-research"))),
            set(&["think", "web_search", "semantic_search", "sql_query", "shell", "dispatch_subagents", "create_page"])
        );
        assert_eq!(tool_names(&ChatMode::Interview), set(&["write_it_up"]));
        assert_eq!(tool_names(&ChatMode::GettingStarted), set(&["skip_step", "record_introductions"]));
        assert_eq!(tool_names(&fixture_applet()), set(&["think", "semantic_search", "sql_query"]));
    }

    /// Every model step re-sends the mode's tool definitions ahead of the
    /// person's message. At ~41k characters chat's were most of the prompt
    /// and the model visibly deliberated over them, so every tool belongs to
    /// a group with a ceiling, and every mode has a total, in characters of
    /// the JSON exactly as `ChatMode::tools()` serializes it. Long how-to
    /// belongs behind a call (`applet {"action": "setup", "guide": true}`
    /// returns applets/AGENTS.md), not in the definition. A tool in any mode — chat,
    /// interview, getting started, local or an applet —
    /// must join a group here, and a new mode or shipped chat applet must join `MODES`:
    /// unbudgeted tools and modes fail the test.
    #[test]
    fn tool_definitions_stay_inside_their_budgets() {
        // (group, tools, ceiling). A group's size is measured per mode: the
        // tools of it that mode offers.
        const GROUPS: &[(&str, &[&str], usize)] = &[
            // One tool for every applet action, chosen by `action`. Raised 150
            // on 2026-10-27 for setup's invoke, tools and model, which write
            // an applet `/name` starts; how to use them is in the guide.
            ("applets", &["applet"], 2_950),
            // The applets chat may load, listed by name and description. The
            // shipped catalog has none; a box's grows to the cap in
            // `use_applet_tool`.
            ("use_applet", &["use_applet"], 4_800),
            ("analysis", &["shell", "think", "read_asset", "generate_image"], 2_400),
            ("pages", &["edit_page", "get_page_content", "create_page", "get_project_item"], 2_400),
            ("search", &["semantic_search", "web_search"], 2_000),
            ("self", &["update_memory"], 800),
            ("tasks", &["create_task", "update_task"], 1_500),
            ("sql_write", &["sql_write"], 600),
            // A service the owner connected over MCP (`tools/connection.rs`).
            // One tool whatever is connected; the services' own tools go to
            // its worker, never into this list.
            ("connections", &["connection"], 900),
            // Drawn inside the reply. The kinds and their column contracts are
            // the description; trimming it means dropping a kind.
            ("show", &["show"], 1_900),
            // The most-used tool, and its table block is generated from the
            // catalog (sql_catalog::prompt_block), so a new table or a longer
            // note lands here. Trimmed from 9.6k on 2026-09-29 with no column
            // dropped: failures were 6 unknown columns and 5 ambiguous joins
            // in 1,422 calls, so the columns stay and the prose went. Raised
            // 150 on 2026-10-20 for app_tasks, the one table the task tools
            // read through it.
            ("sql_query", &["sql_query"], 6_650),
            // Mode-only tools, each set just above its 2026-09-29 size. Sudo
            // raised on 2026-10-27 for its clean environment and the
            // recovery-key ask.
            ("sudo", &["sudo"], 1_800),
            ("dispatch", &["dispatch_subagents"], 3_300),
            ("interview", &["write_it_up"], 1_500),
            ("getting_started", &["skip_step", "record_introductions"], 2_200),
        ];
        // (mode's wire name, total ceiling). Chat's was the sum of its group
        // ceilings, and rose 1,300 on 2026-10-27 for `use_applet` listing the
        // four shipped chat applets; the applets a box adds grow it toward
        // that group's ceiling, which this test cannot see. The rest sit just
        // above their 2026-09-29 size.
        const MODES: &[(&str, usize)] = &[
            ("chat", 23_200),
            ("interview", 1_500),
            ("getting_started", 2_200),
            ("local", 0),
            // The shipped chat applets, each just above its 2026-10-22 size as a
            // skill; admin and deep research just above their size as modes,
            // less what admin no longer lists.
            ("admin", 9_000),
            ("deep-research", 13_250),
            ("meeting-prep", 8_400),
            ("morning-brief", 8_700),
            ("new-applet", 2_950),
            ("trip-planner", 9_000),
            ("weekly-review", 8_700),
        ];

        let size = |t: &serde_json::Value| serde_json::to_string(t).unwrap().chars().count();
        let name = |t: &serde_json::Value| t["function"]["name"].as_str().unwrap_or("").to_string();
        let mut modes = vec![
            ChatMode::Chat,
            ChatMode::Interview,
            ChatMode::GettingStarted,
            ChatMode::Local,
        ];
        modes.extend(
            crate::applet_templates::chat_applets()
                .into_iter()
                .filter(|a| a.shipped)
                .filter_map(|a| crate::api::chat_applets::turn_named(&a.name))
                .map(ChatMode::Applet),
        );

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
    fn only_the_interview_refuses_a_pin_and_only_admin_is_sudo() {
        for mode in all_modes() {
            let name = mode.wire_name().to_string();
            assert_eq!(mode.honors_pin(), !matches!(mode, ChatMode::Interview), "{name}");
            assert_eq!(mode.is_sudo(), name == "admin", "{name}");
            let want = if name == "deep-research" { ModelSlot::Deep } else { ModelSlot::Standard };
            assert_eq!(mode.slot(), want, "{name}");
        }
        assert_eq!(fixture_applet().slot(), ModelSlot::Deep, "an applet's `model: deep` is honoured");

        // An applet that lists sudo but did not ship is not a sudo turn, even
        // if a folder got past the reader.
        let ChatMode::Applet(mut not_shipped) = fixture_applet() else { unreachable!() };
        not_shipped.tools.push("sudo".into());
        assert!(!ChatMode::Applet(not_shipped.clone()).is_sudo());
        not_shipped.shipped = true;
        assert!(ChatMode::Applet(not_shipped).is_sudo());
    }

    /// The admin prompt moved from the chat prompt into `applets/admin`.
    /// The sudo shell starts from a clean environment (`tools::sudo_shell`),
    /// and the gate asks before the recovery key (`tools::sudo_gate`).
    #[test]
    fn the_two_old_modes_run_from_their_applet_folders() {
        let admin = applet("admin");
        assert!(admin.body.contains("virtues backup --init-key"));
        assert!(!admin.body.contains("$DATABASE_URL"));
        let research = applet("deep-research");
        assert!(research.body.contains("Start with the think tool to plan your research approach"));
        for a in [admin, research] {
            if let Some(line) = a.body.lines().find(|l| l.contains('—')) {
                panic!("em dash in {}: {line}", a.name);
            }
        }
    }

    /// The chat id beats the wire: the interview's chat is the interviewer,
    /// the getting-started room is setup until the interview begins in it.
    #[sqlx::test]
    async fn some_chats_are_a_mode_by_id(pool: PgPool) {
        let interview = crate::api::narrative_draft::INTERVIEW_CHAT_ID;
        let room = getting_started::GETTING_STARTED_CHAT_ID;

        assert!(matches!(ChatMode::resolve(&pool, interview, "admin").await, ChatMode::Interview));
        assert!(matches!(ChatMode::resolve(&pool, room, "admin").await, ChatMode::GettingStarted));
        assert!(ChatMode::resolve(&pool, "chat_other", "admin").await.is_sudo());
        assert!(matches!(ChatMode::resolve(&pool, "chat_other", "nope").await, ChatMode::Chat));

        sqlx::query("UPDATE app_user_profile SET interview_started_at = now()")
            .execute(&pool)
            .await
            .unwrap();
        assert!(matches!(ChatMode::resolve(&pool, room, "chat").await, ChatMode::Interview));
    }
}
