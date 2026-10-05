//! Built-in tool registry
//!
//! This module defines BUILT-IN tools that are part of Virtues core.
//! These are executed as native Rust functions via the ToolExecutor.
//!
//! # Tool Types
//!
//! - `builtin` - Native Rust implementation (web_search, sql_query, create_page, get_page_content, edit_page)
//! - `mcp` - MCP protocol (user-connected servers, stored in Postgres)

use serde::{Deserialize, Serialize};

/// Tool type - distinguishes built-in vs MCP tools
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolType {
    /// Built-in tool - native Rust implementation
    Builtin,
    /// MCP tool - executed via MCP protocol
    Mcp,
}

/// Tool category for UI grouping
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolCategory {
    Search,
    Data,
    Edit,
}

/// Built-in tool configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ToolConfig {
    /// Unique tool identifier
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Short description for UI
    pub description: String,
    /// Detailed description for LLM (helps model decide when to use)
    pub llm_description: String,
    /// JSON Schema for parameters
    pub parameters: serde_json::Value,
    /// Tool type (builtin for registry tools)
    pub tool_type: ToolType,
    /// Category for grouping in UI
    pub category: ToolCategory,
    /// Iconify icon name
    pub icon: String,
    /// Display order in UI
    pub display_order: i32,
    /// Whether this tool is system-only (not shown to users in regular chats).
    /// System tools are only available to action runners and internal callers.
    pub is_system: bool,
}

/// Get default built-in tool configurations
///
/// These are the core tools that ship with Virtues:
/// - web_search: Search the web (Parallel)
/// - sql_query: Read-only SQL queries against user data
/// - code_interpreter: Execute Python code for calculations and analysis
/// - create_page: Create a new page with content
/// - get_page_content: Read current page content
/// - edit_page: Apply edits using find/replace
/// - update_memory: Persist notes across conversations
pub fn default_tools() -> Vec<ToolConfig> {
    vec![
        think_tool(),
        propose_narrative_identity_tool(),
        write_it_up_tool(),
        revise_article_tool(),
        skip_step_tool(),
        record_introductions_tool(),
        update_memory_tool(),
        web_search_tool(),
        semantic_search_tool(),
        sql_query_tool(),
        sql_write_tool(),
        shell_tool(),
        code_interpreter_tool(),
        dispatch_subagents_tool(),
        create_page_tool(),
        get_page_content_tool(),
        edit_page_tool(),
        setup_applet_tool(),
        update_applet_memory_tool(),
        list_applets_tool(),
        get_applet_tool(),
        edit_applet_tool(),
        delete_applet_tool(),
        run_applet_tool(),
        get_project_item_tool(),
        generate_image_tool(),
        read_asset_tool(),
    ]
}

/// Generate Image tool — text-to-image via the gateway image model.
fn generate_image_tool() -> ToolConfig {
    ToolConfig {
        id: "generate_image".to_string(),
        name: "Generate Image".to_string(),
        description: "Generate an image from a text description".to_string(),
        llm_description: r#"Generate an image. Write a vivid, specific prompt: subject, style, composition, lighting, mood. The user sees the image; follow it with a brief caption, not a description."#.to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "required": ["prompt"],
            "properties": {
                "prompt": { "type": "string" }
            }
        }),
        tool_type: ToolType::Builtin,
        category: ToolCategory::Edit,
        icon: "ri:image-add-line".to_string(),
        display_order: 22,
        is_system: false,
    }
}

/// Propose an addition to the user's narrative identity — never write one.
fn propose_narrative_identity_tool() -> ToolConfig {
    ToolConfig {
        id: "propose_narrative_identity_edit".to_string(),
        name: "Propose identity note".to_string(),
        description: "Suggest something for the user's narrative identity".to_string(),
        llm_description: r#"Propose an addition to the user's narrative identity, the document of who they are that goes into every conversation. It is not yours to edit: this leaves a note they Add or Dismiss. Use it rarely: only something durable about who they are or what they are for; never facts, preferences or passing remarks. One or two sentences in their voice ("I'd rather ship early than polish in private."). If unsure, don't."#.to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "required": ["text", "why"],
            "properties": {
                "text": { "type": "string" },
                "why": { "type": "string", "description": "What prompted it; the user sees this" }
            }
        }),
        tool_type: ToolType::Builtin,
        category: ToolCategory::Edit,
        icon: "ri:compass-3-line".to_string(),
        display_order: 0,
        is_system: false,
    }
}

/// The narrative interview's ONE tool: turn the transcript into the person's
/// document and chapters. Interview-mode only (see ChatMode::tools);
/// is_system keeps it out of every other room's tool set.
fn revise_article_tool() -> ToolConfig {
    ToolConfig {
        id: "revise_article".to_string(),
        name: "Revise a wiki article".to_string(),
        description: "Hand back a revised wiki article; the record applies what changed"
            .to_string(),
        // The HOW-TO-WRITE lives in the editor's constitution and brief
        // (virtues-core/prompts/wiki/), in one place. This describes only the
        // arguments and what the box does with them, so the two cannot drift.
        llm_description: r#"Hand back the WHOLE article as it should now read. You do not patch it and you do not describe the change: you write the finished document, and the record works out what actually changed and applies only that. History therefore shows a small diff, not a rewrite.

Arguments:
- subject_type / subject_id: the article you were asked to revise.
- article: the complete new text. Everything still true must appear again, unchanged — anything you leave out is deleted.
- summary: one line on WHAT you changed and WHY, in plain words ("added the spring recital and three lessons in March"). Never "improved the article". The box appends its own count of what moved, so do not pad this with numbers.

A refused call is not an error. It returns the sentence to act on — most often that your text dropped or reworded something the owner wrote, which you may not do even to improve it. Fix that and call again."#.to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "subject_type": {
                    "type": "string",
                    "description": "The article's subject type, e.g. person, place, organization, year."
                },
                "subject_id": {
                    "type": "string",
                    "description": "The article's subject id, exactly as given to you."
                },
                "article": {
                    "type": "string",
                    "description": "The complete revised article. Anything omitted is deleted."
                },
                "summary": {
                    "type": "string",
                    "description": "One plain line: what changed and why."
                }
            },
            "required": ["subject_type", "subject_id", "article", "summary"]
        }),
        tool_type: ToolType::Builtin,
        category: ToolCategory::Edit,
        icon: "ri:draft-line".to_string(),
        display_order: 0,
        is_system: true,
    }
}

fn write_it_up_tool() -> ToolConfig {
    ToolConfig {
        id: "write_it_up".to_string(),
        name: "Write it up and close the interview".to_string(),
        description: "Close the interview: arrange it into the person's document and chapters".to_string(),
        // The WHEN lives in the interview prompt ("Pacing and the close"),
        // in one place. This describes only the arguments and the outcome,
        // so the two cannot drift apart again.
        llm_description: r#"Close the narrative interview. A separate drafter reads the transcript and writes the "In your own words" document and the chapters. This runs ONCE and cannot be undone: the composer retires and the person cannot reply here afterwards. The interview prompt says when to call it; never compose the document yourself in the chat.

Arguments state your claim, and the box checks it:
- territories_covered: the territory numbers (1-6) the person has actually answered.
- their_words: the person's own words asking to close or saying yes to closing, verbatim. If they have not said so, do not call this; ask.
- close_early_confirmed: true only when territories are uncovered, you told the person which, and they said yes anyway.

A refused call is not an error: it returns the sentence to act on, and the interview goes on."#.to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "territories_covered": {
                    "type": "array",
                    "items": { "type": "integer", "minimum": 1, "maximum": 6 },
                    "description": "Territory numbers (1-6) the person has answered."
                },
                "their_words": {
                    "type": "string",
                    "description": "The person's own words asking to close or agreeing to it, verbatim."
                },
                "close_early_confirmed": {
                    "type": "boolean",
                    "description": "True only when territories are uncovered and the person, told which, said yes to closing anyway."
                }
            },
            "required": ["territories_covered", "their_words"]
        }),
        tool_type: ToolType::Builtin,
        category: ToolCategory::Edit,
        icon: "ri:quill-pen-line".to_string(),
        display_order: 0,
        is_system: true,
    }
}

/// Getting started's tools. Mode-only (see ChatMode::tools);
/// is_system keeps them out of every other room. None writes anything: they
/// return markers the client renders as cards, and the cards do the work.

fn skip_step_tool() -> ToolConfig {
    ToolConfig {
        id: "skip_step".to_string(),
        name: "Skip a getting-started step".to_string(),
        description: "Mark one getting-started step skipped, on the person's ask".to_string(),
        llm_description: "Skip one getting-started step because the person asked to. Only on their ask, never suggested. connect_ai cannot be skipped here. Pass skipped=false to un-skip.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "step": { "type": "string", "enum": ["introductions", "connect_world", "interview"] },
                "skipped": { "type": "boolean", "default": true }
            },
            "required": ["step"]
        }),
        tool_type: ToolType::Builtin,
        category: ToolCategory::Edit,
        icon: "ri:skip-forward-line".to_string(),
        display_order: 0,
        is_system: true,
    }
}

fn record_introductions_tool() -> ToolConfig {
    ToolConfig {
        id: "record_introductions".to_string(),
        name: "Write the introductions down".to_string(),
        description: "Record what the person said about themselves and show it back".to_string(),
        llm_description: "Write down what the person said about themselves: their full name, what to call them, what they will call you, the city they live in with the IANA time zone you resolve from it, and their birth date (pass it EXACTLY as they wrote it — June 6 1997, 6/6/97, whatever they typed; it is parsed on the other side, and dropping a date you were unsure how to format is the one failure that matters here). Include only what they gave; leave the rest out rather than guessing, and each field you omit is left as it was. Call this the moment you have anything — it writes immediately and shows what it wrote under your turn, so do not ask them to confirm and do not list the fields back in your own words. If they gave only a first name, or left out the birth date, or gave a year without a day, ask once for what is missing. The result tells you what is still missing after the write; if it names anything, ask for that in one short question rather than moving on. If they correct something afterwards, call this again with only what changed.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "full_name": { "type": "string", "description": "First and last, as they gave it." },
                "preferred_name": { "type": "string", "description": "What they like to be called." },
                "assistant_name": { "type": "string" },
                "home_place": { "type": "string", "description": "The city as they said it, for the card's label." },
                "home_timezone": { "type": "string", "description": "IANA time zone, e.g. America/Chicago." },
                "birth_date": { "type": "string", "description": "Exactly as they wrote it — \"March 1962\", \"the 3rd\", \"1987-04-02\". Do not reformat or complete it." }
            }
        }),
        tool_type: ToolType::Builtin,
        category: ToolCategory::Edit,
        icon: "ri:user-smile-line".to_string(),
        display_order: 0,
        is_system: true,
    }
}

/// Look at a file — actually look, not read a description of it.
fn read_asset_tool() -> ToolConfig {
    ToolConfig {
        id: "read_asset".to_string(),
        name: "Read file".to_string(),
        description: "Look at an image or file the user has stored".to_string(),
        llm_description: r#"Look at one file in the user's drive; an image comes back for you to see. Take file_id from its ref URL: /drive/dr_abc123 is "dr_abc123".

Use it when answering means seeing the file: "this screenshot", a layout that matters, or a project member marked text="none", which semantic_search cannot see inside, so an empty search is no evidence about it. One known file per call, never a sweep: images cost context. If the file cannot be shown, say so; never describe a file you were not shown."#.to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "required": ["file_id"],
            "properties": {
                "file_id": { "type": "string" }
            }
        }),
        tool_type: ToolType::Builtin,
        category: ToolCategory::Search,
        icon: "ri:image-line".to_string(),
        display_order: 0,
        is_system: false,
    }
}

/// Think tool - structured reasoning scratchpad
fn think_tool() -> ToolConfig {
    ToolConfig {
        id: "think".to_string(),
        name: "Think".to_string(),
        description: "Plan your approach before acting".to_string(),
        llm_description: r#"Plan before a multi-step or ambiguous task: which tools, in what order. No side effects. The user sees the thought, so keep it clear and short."#.to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "required": ["thought"],
            "properties": {
                "thought": { "type": "string" }
            }
        }),
        tool_type: ToolType::Builtin,
        category: ToolCategory::Data,
        icon: "ri:lightbulb-line".to_string(),
        display_order: 0,
        is_system: false,
    }
}

/// Update Memory tool — persist notes across conversations
fn update_memory_tool() -> ToolConfig {
    ToolConfig {
        id: "update_memory".to_string(),
        name: "Memory".to_string(),
        description: "Keep, revise, or retire one memory that persists across conversations".to_string(),
        llm_description: r#"Keep one note in <memory>. The user reads and edits every note: write only what you want them to read. Lanes: facts (their world), manner (how to speak with them), practices (what they hold to, how you help). add: one durable fact, at most 500 chars. revise / retire: your own notes by id; [theirs] notes are the user's. Never store their narrative identity or secrets."#.to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "op": {
                    "type": "string",
                    "enum": ["add", "revise", "retire"],
                    "description": "Default add"
                },
                "lane": {
                    "type": "string",
                    "enum": ["facts", "manner", "practices"],
                    "description": "Default facts"
                },
                "note_id": { "type": "integer" },
                "content": { "type": "string" }
            }
        }),
        tool_type: ToolType::Builtin,
        category: ToolCategory::Data,
        icon: "ri:brain-line".to_string(),
        display_order: 1,
        is_system: false,
    }
}

/// Web Search tool (Parallel)
fn web_search_tool() -> ToolConfig {
    ToolConfig {
        id: "web_search".to_string(),
        name: "Web Search".to_string(),
        description: "Search the web for current information".to_string(),
        llm_description: r#"Search the web; you synthesize the results. Always set `objective`. Send several angles on one question as parallel calls in one step, not one after another. For a list (events, options, places), ask for 10 results instead of a second query. For news, scores, odds, prices and other live data, set max_age_hours=1."#.to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "required": ["query"],
            "properties": {
                "query": { "type": "string" },
                "num_results": {
                    "type": "integer",
                    "default": 8,
                    "minimum": 1,
                    "maximum": 10
                },
                "objective": { "type": "string", "description": "What you are trying to learn, in a sentence" },
                "max_age_hours": {
                    "type": "integer",
                    "description": "Max age of a cached result; omit for stable info",
                    "minimum": 0
                }
            }
        }),
        tool_type: ToolType::Builtin,
        category: ToolCategory::Search,
        icon: "ri:search-line".to_string(),
        display_order: 1,
        is_system: false,
    }
}

/// Semantic Search tool — meaning-based retrieval across user data
fn semantic_search_tool() -> ToolConfig {
    ToolConfig {
        id: "semantic_search".to_string(),
        name: "Semantic Search".to_string(),
        description: "Search personal data by meaning".to_string(),
        llm_description: r#"Search the user's own data by meaning; `domains` lists what it holds.

Omit `domains` to search everything; always omit it in a project chat, or its materials drop out. Pass 2-4 phrasings in `queries` for a vague need, one for a precise lookup. Rank is order, not match quality. Cite each `ref` as returned; read a full row with sql_query by its id.

Naming a person, place or org, or dates spanning a week or less, puts `from_your_wiki` first: an article, or a card whose id fits `entities`."#.to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "queries": {
                    "type": "array",
                    "items": { "type": "string" },
                    "minItems": 1,
                    "maxItems": 4
                },
                "query": { "type": "string", "description": "Alias for one query" },
                "domains": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "document, email, message, calendar, chat, transaction, transcription, page"
                },
                "date_after": { "type": "string", "description": "ISO 8601" },
                "date_before": { "type": "string", "description": "ISO 8601" },
                "entities": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Entity ids (person_, place_, org_) from results or <circumstances>; beats matching a name"
                },
                "num_results": {
                    "type": "integer",
                    "default": 10,
                    "minimum": 1,
                    "maximum": 50
                }
            }
        }),
        tool_type: ToolType::Builtin,
        category: ToolCategory::Search,
        icon: "ri:mind-map".to_string(),
        display_order: 2,
        is_system: false,
    }
}

/// SQL Query tool (read-only data access)
fn sql_query_tool() -> ToolConfig {
    ToolConfig {
        id: "sql_query".to_string(),
        name: "Query Data".to_string(),
        description: "Query user's personal data with SQL".to_string(),
        llm_description: r#"Read-only SQL (PostgreSQL) over the user's personal data. For meaning across sources use semantic_search; then read the hit whole here by id.

Operations: 'query' runs a SELECT; always LIMIT (max 200; with save_as up to 10,000 rows go to a CSV in the code_interpreter workspace and you see 20). 'get_schema' gives every column of the named tables with types and joins. 'list_tables' gives row counts.

Write against the column names listed, not names that sound right. Every table also has id, created_at, updated_at; data_* also carry metadata (jsonb) and source_stream_id. For any column not listed, call get_schema.

<<TABLE_CATALOG>>
Rules:
- In a JOIN, qualify every column (m.occurred_at); bare shared names like occurred_at, date, id fail as ambiguous.
- A table is an instant (occurred_at) or a span (started_at/ended_at), never both.
- created_at/updated_at are when WE wrote the row; filter time on occurred_at or started_at/ended_at.

Data row -> person (or wiki_places / wiki_orgs):
SELECT p.name, COUNT(*) FROM data_communication_message m
JOIN wiki_refs r ON r.source_table = 'data_communication_message' AND r.source_id = m.id AND r.role = 'sender'
JOIN wiki_people p ON p.id = r.entity_id
GROUP BY p.name ORDER BY 2 DESC LIMIT 10"#
            .replace("<<TABLE_CATALOG>>", &crate::sql_catalog::prompt_block()),
        parameters: serde_json::json!({
            "type": "object",
            "required": ["operation"],
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["query", "list_tables", "get_schema"]
                },
                "sql": { "type": "string", "description": "SELECT, for query" },
                "tables": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "For get_schema"
                },
                "limit": { "type": "integer", "description": "Default 50; 10,000 with save_as" },
                "save_as": { "type": "string", "description": "CSV name, e.g. 'sleep.csv' (saved chats only)" }
            }
        }),
        tool_type: ToolType::Builtin,
        category: ToolCategory::Data,
        icon: "ri:database-2-line".to_string(),
        display_order: 3,
        is_system: false,
    }
}

/// Code Interpreter tool - execute Python code in a sandbox
fn code_interpreter_tool() -> ToolConfig {
    ToolConfig {
        id: "code_interpreter".to_string(),
        name: "Python".to_string(),
        description: "Execute Python code for calculations and data analysis".to_string(),
        llm_description: r#"Run Python 3 in a sandbox for anything past a couple of steps in your head: arithmetic over many numbers, statistics, date math, finance, analysis of query results. Not for a single lookup or a fact you can state.

print() what you want back; only stdout and stderr return. Packages: numpy, pandas, scipy, matplotlib, numpy-financial, python-dateutil and the standard library; a result saying packages are not installed yet means standard library only for that call.

The working directory is this chat's workspace and persists between calls; variables do not. A sql_query save_as file is here as a CSV. Save charts to out/ (plt.savefig('out/x.png')): each one shows with your reply, so say what it shows and never link it. No network, 1 GB memory. On a traceback, fix the code and retry."#.to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "required": ["code"],
            "properties": {
                "code": { "type": "string" },
                "timeout": {
                    "type": "integer",
                    "description": "Seconds",
                    "default": 60,
                    "minimum": 5,
                    "maximum": 120
                }
            }
        }),
        tool_type: ToolType::Builtin,
        category: ToolCategory::Data,
        icon: "ri:code-s-slash-line".to_string(),
        display_order: 4,
        is_system: false,
    }
}

/// Dispatch Subagents tool - orchestrator fan-out for Deep Research.
///
/// The main (orchestrator) agent calls this to spawn independent read-only research
/// workers in parallel. Each worker runs its own agent loop over the data + web tools,
/// then returns a compressed findings summary plus its sources. Workers cannot dispatch
/// further subagents (no recursion).
fn dispatch_subagents_tool() -> ToolConfig {
    ToolConfig {
        id: "dispatch_subagents".to_string(),
        name: "Dispatch Researchers".to_string(),
        description: "Spawn parallel research workers".to_string(),
        llm_description: r#"Spawn independent research workers that run IN PARALLEL, each chasing one line of inquiry, then return their findings for you to synthesize. This is your fan-out tool for deep research.

When to use:
- The question has several INDEPENDENT sub-questions that can be investigated separately (e.g. "query my spending trend", "check my calendar load", "find external base rates").
- You want a skeptic: dispatch one worker whose objective is to find evidence AGAINST your leading hypothesis.

How to use well:
- Dispatch the FEWEST workers that cover the independent questions — usually 2-4, never more than 5. Don't split one question into redundant workers.
- Each worker is READ-ONLY (sql_query, semantic_search, web_search, code_interpreter, think) and cannot dispatch further workers. Give each a self-contained objective — workers do NOT see the conversation, only their objective.
- Tell each worker to compute real statistics with code_interpreter where relevant, cite the specific records/sources it used, and return a CONCISE findings summary (not a transcript).
- Pick each worker's `model` by difficulty: "fast" for simple lookups, "balanced" (default) for normal research, "strong" for hard quantitative analysis.

After they return, synthesize their findings yourself — weigh agreements, surface disagreements, and never assert causation from correlation.

Example:
{
  "missions": [
    {"title": "Spending trend", "objective": "Query data_financial_transaction for monthly spending by category over the last 6 months. Use code_interpreter to compute the trend and flag the categories that grew most. Cite the rows.", "model": "balanced"},
    {"title": "Sleep correlation", "objective": "Pull data_health_sleep and monthly spending for the same period. Compute the correlation with code_interpreter. Report n and how weak/strong it is. Cite sources.", "model": "strong"},
    {"title": "Counter-evidence", "objective": "Argue against the idea that spending rose due to one cause. Look for confounders (seasonality, one-off purchases, income changes) in the data. Cite what you find.", "model": "balanced"}
  ]
}"#.to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "required": ["missions"],
            "properties": {
                "missions": {
                    "type": "array",
                    "description": "1-5 independent research missions to run in parallel.",
                    "minItems": 1,
                    "maxItems": 5,
                    "items": {
                        "type": "object",
                        "required": ["title", "objective"],
                        "properties": {
                            "title": {
                                "type": "string",
                                "description": "Short label shown in the live panel, e.g. 'Sleep vs spending'."
                            },
                            "objective": {
                                "type": "string",
                                "description": "Self-contained instructions: what to find, which data/web to use, what to compute, what to cite. The worker sees only this."
                            },
                            "model": {
                                "type": "string",
                                "enum": ["fast", "balanced", "strong"],
                                "description": "Worker model tier by difficulty. Default balanced."
                            },
                            "style": {
                                "type": "string",
                                "enum": ["research", "voice"],
                                "description": "How the worker is framed. \"research\" (default): a read-only researcher that investigates and cites. \"voice\": a Council voice that speaks in first person as the perspective its objective describes (no tools but think). Use \"voice\" only in Council mode."
                            }
                        }
                    }
                }
            }
        }),
        tool_type: ToolType::Builtin,
        category: ToolCategory::Data,
        icon: "ri:team-line".to_string(),
        display_order: 6,
        // Internal orchestration tool: excluded from Chat (which filters out system tools) but
        // included in Deep Research and Council via their explicit tool allow-lists.
        is_system: true,
    }
}

/// Create Page tool - creates a new page with optional initial content
fn create_page_tool() -> ToolConfig {
    ToolConfig {
        id: "create_page".to_string(),
        name: "Create Page".to_string(),
        description: "Create a new page with content".to_string(),
        llm_description: r#"Create a page with a title and optional markdown content."#.to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "required": ["title"],
            "properties": {
                "title": { "type": "string" },
                "content": { "type": "string", "description": "Markdown" }
            }
        }),
        tool_type: ToolType::Builtin,
        category: ToolCategory::Edit,
        icon: "ri:file-add-line".to_string(),
        display_order: 5,
        is_system: false,
    }
}

/// Get Page Content tool - reads current content of a page
fn get_page_content_tool() -> ToolConfig {
    ToolConfig {
        id: "get_page_content".to_string(),
        name: "Get Page Content".to_string(),
        description: "Read the current content of a page".to_string(),
        llm_description: r#"Read a page's title and content. Call it before edit_page. A page arrives as a link, [Name](/page/page_abc123); page_id is its last segment, page_abc123."#.to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "required": ["page_id"],
            "properties": {
                "page_id": { "type": "string" }
            }
        }),
        tool_type: ToolType::Builtin,
        category: ToolCategory::Edit,
        icon: "ri:file-text-line".to_string(),
        display_order: 6,
        is_system: false,
    }
}

/// Edit Page tool - applies edits using simple find/replace
fn edit_page_tool() -> ToolConfig {
    ToolConfig {
        id: "edit_page".to_string(),
        name: "Edit Page".to_string(),
        description: "Edit a page using find/replace".to_string(),
        llm_description: r#"Edit a page: replace the text `find` with `replace`, and rename it with `title`. Call get_page_content first. page_id is the last segment of the page's link, [Name](/page/page_abc123).

`find` matches the plain text (formatting stripped): make it unique but short. `replace` is markdown. An empty `find` replaces the whole document; empty `find` and `replace` with a `title` only renames. Prefer a few large edits to many small ones. Changes apply immediately."#.to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "required": ["page_id", "find", "replace"],
            "properties": {
                "page_id": { "type": "string" },
                "title": { "type": "string" },
                "find": { "type": "string" },
                "replace": { "type": "string" }
            }
        }),
        tool_type: ToolType::Builtin,
        category: ToolCategory::Edit,
        icon: "ri:edit-line".to_string(),
        display_order: 7,
        is_system: false,
    }
}

/// SQL Write tool — DML scoped to applet-owned schemas
fn sql_write_tool() -> ToolConfig {
    ToolConfig {
        id: "sql_write".to_string(),
        name: "SQL Write".to_string(),
        description: "Write to applet-owned tables".to_string(),
        llm_description: r#"One INSERT, UPDATE or DELETE on an applet's own tables (applet_* schemas; everything else is read-only). Create tables with setup_applet's schema_sql first. Add RETURNING to get rows back (max 500)."#.to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "required": ["sql"],
            "properties": {
                "sql": { "type": "string" }
            }
        }),
        tool_type: ToolType::Builtin,
        category: ToolCategory::Data,
        icon: "ri:database-2-line".to_string(),
        display_order: 7,
        is_system: false,
    }
}

/// Shell tool — a command on the server itself, as its admin account.
/// Sudo mode only: `ChatMode::tools` lists it for no other mode and
/// the executor refuses it outside a sudo turn.
fn shell_tool() -> ToolConfig {
    ToolConfig {
        id: "shell".to_string(),
        name: "Shell".to_string(),
        description: "Run a command on the server, with sudo".to_string(),
        llm_description: r#"Run a shell command on the server this assistant runs on, as its admin account, which has passwordless sudo. Anything a person could do over ssh is possible: read and change files anywhere, query or change any database, read logs, manage services, install packages. Reads run at once; changes wait for the owner to allow them.

The command runs through bash -c with no terminal and no stdin, so anything that prompts reads nothing: pass -y / --yes / --no-pager, and never open an editor or a pager. Returns exit_code, stdout and stderr; long output keeps its beginning and its end.

- Database: psql "$DATABASE_URL" -c '…', as the app's role.
- Logs: journalctl -u virtues --no-pager -n 200 (add -p warning, --since "1 hour ago").
- Long jobs: raise timeout_seconds (default 120, max 600), or run them in the background with output to a file (nohup … >/tmp/job.log 2>&1 &) and check the file later.

This server is running this conversation: restarting the virtues service, rebooting, or killing its process ends the turn mid-reply, so do that last."#.to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "required": ["command"],
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The command line, run with bash -c"
                },
                "timeout_seconds": {
                    "type": "integer",
                    "description": "Kill the command after this many seconds (default 120, max 600)"
                },
                "cwd": {
                    "type": "string",
                    "description": "Working directory (default: the account's home)"
                }
            }
        }),
        tool_type: ToolType::Builtin,
        category: ToolCategory::Edit,
        icon: "ri:terminal-box-line".to_string(),
        display_order: 8,
        is_system: false,
    }
}

/// Setup Applet tool — materialize a chat-authored applet as a folder
fn setup_applet_tool() -> ToolConfig {
    ToolConfig {
        id: "setup_applet".to_string(),
        name: "Setup Applet".to_string(),
        description: "Create or update an applet".to_string(),
        // The authoring contract is applets/AGENTS.md, returned by
        // `{"guide": true}`. It is one source for the in-box assistant and
        // for agents working in the repo, and it stays out of the definition
        // every chat step re-sends.
        llm_description: r#"Create or update an applet: a scheduled task, reminder, monitor, tracker or dashboard. First call with {"guide": true} and follow the guide it returns. The same name updates that applet. name and description are required except for the guide. On check_failed, fix the findings and retry; nothing was created. check_only runs the check alone."#.to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "guide": { "type": "boolean" },
                "check_only": { "type": "boolean" },
                "name": { "type": "string" },
                "description": { "type": "string" },
                "agent": { "type": "string", "description": "Run prompt; omit for a face-only dashboard" },
                "schedule": { "type": "string" },
                "triggers": {
                    "type": "array",
                    "items": { "type": "string", "enum": ["cron", "manual", "tool", "api", "webhook", "message"] }
                },
                "condition": { "type": "string" },
                "until": { "type": "string" },
                "schema_sql": { "type": "string" },
                "face_html": { "type": "string" },
                "limits": {
                    "type": "object",
                    "properties": {
                        "max_llm_cost":         { "type": "number"  },
                        "max_llm_cost_per_day": { "type": "number"  },
                        "max_runs_per_day":     { "type": "integer" },
                        "max_runs_per_hour":    { "type": "integer" },
                        "timeout_s":            { "type": "integer" }
                    }
                }
            }
        }),
        tool_type: ToolType::Builtin,
        category: ToolCategory::Edit,
        icon: "ri:robot-2-line".to_string(),
        display_order: 8,
        is_system: false,
    }
}

/// List actions — lightweight catalog for chat-driven discovery
fn list_applets_tool() -> ToolConfig {
    ToolConfig {
        id: "list_applets".to_string(),
        name: "List Applets".to_string(),
        description: "List applets".to_string(),
        llm_description: "List applets and their last run.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "owner": { "type": "string", "enum": ["system", "user", "ai"], "description": "ai = made in a chat" },
                "enabled": { "type": "boolean" },
                "trigger": { "type": "string", "enum": ["cron", "manual", "tool", "api", "webhook", "message"] },
                "include_archived": { "type": "boolean" }
            }
        }),
        tool_type: ToolType::Builtin,
        category: ToolCategory::Data,
        icon: "ri:list-check".to_string(),
        display_order: 10,
        is_system: false,
    }
}

/// Get a single action's full details + recent runs
fn get_applet_tool() -> ToolConfig {
    ToolConfig {
        id: "get_applet".to_string(),
        name: "Get Applet".to_string(),
        description: "Fetch a single applet".to_string(),
        llm_description: "An applet's full config and last 10 runs. Read it before editing or debugging one.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "required": ["id"],
            "properties": {
                "id": { "type": "string" }
            }
        }),
        tool_type: ToolType::Builtin,
        category: ToolCategory::Data,
        icon: "ri:file-search-line".to_string(),
        display_order: 11,
        is_system: false,
    }
}

/// Edit an existing action — partial update with system-owner guard
fn edit_applet_tool() -> ToolConfig {
    ToolConfig {
        id: "edit_applet".to_string(),
        name: "Edit Applet".to_string(),
        description: "Update an action's configuration".to_string(),
        llm_description: "Change an applet: only the fields to change go in `patch`; null clears. `config` replaces the whole object. `schedule`: 6-field cron, local time. System applets take only enabled, schedule, config, memory.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "required": ["id", "patch"],
            "properties": {
                "id": { "type": "string" },
                "patch": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string" },
                        "agent": { "type": ["string", "null"] },
                        "schedule": { "type": ["string", "null"] },
                        "enabled": { "type": "boolean" },
                        "config": { "type": "object" },
                        "condition": { "type": ["string", "null"] },
                        "triggers": { "type": "array", "items": { "type": "string" } },
                        "memory": { "type": ["string", "null"] }
                    }
                }
            }
        }),
        tool_type: ToolType::Builtin,
        category: ToolCategory::Edit,
        icon: "ri:edit-2-line".to_string(),
        display_order: 12,
        is_system: false,
    }
}

/// Delete a user-owned action
fn delete_applet_tool() -> ToolConfig {
    ToolConfig {
        id: "delete_applet".to_string(),
        name: "Delete Applet".to_string(),
        description: "Delete a user-owned action".to_string(),
        llm_description: "Delete a user-owned applet; system ones refuse, so offer to disable. Confirm unless the ask was explicit.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "required": ["id"],
            "properties": {
                "id": { "type": "string" }
            }
        }),
        tool_type: ToolType::Builtin,
        category: ToolCategory::Edit,
        icon: "ri:delete-bin-line".to_string(),
        display_order: 13,
        is_system: false,
    }
}

/// Manually run an action
fn run_applet_tool() -> ToolConfig {
    ToolConfig {
        id: "run_applet".to_string(),
        name: "Run Applet".to_string(),
        description: "Trigger an action to run now".to_string(),
        llm_description: "Run an applet now; it needs the `tool` trigger.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "required": ["id"],
            "properties": {
                "id": { "type": "string" },
                "payload": { "description": "Context for the run" },
                "date": { "type": "string", "description": "YYYY-MM-DD for date-scoped applets" }
            }
        }),
        tool_type: ToolType::Builtin,
        category: ToolCategory::Edit,
        icon: "ri:play-circle-line".to_string(),
        display_order: 14,
        is_system: false,
    }
}

/// Update action memory — persistent markdown scratchpad for actions
fn update_applet_memory_tool() -> ToolConfig {
    ToolConfig {
        id: "update_applet_memory".to_string(),
        name: "Update Applet Memory".to_string(),
        description: "Update this action's persistent memory".to_string(),
        llm_description: r#"Save persistent memory that will be available on your next run.
Use this to remember facts, preferences, or state across runs. The content is markdown.
This tool is only available when running as an action."#.to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "required": ["content"],
            "properties": {
                "content": {
                    "type": "string",
                    "description": "Markdown content to save as this action's memory. Replaces previous memory entirely."
                }
            }
        }),
        tool_type: ToolType::Builtin,
        category: ToolCategory::Edit,
        icon: "ri:brain-line".to_string(),
        display_order: 9,
        is_system: true,
    }
}

/// Dayline event tool — structured event CRUD for hourly/EOD actions
/// Get Project Item tool - fetches the full content of a reference in an attached project.
fn get_project_item_tool() -> ToolConfig {
    ToolConfig {
        id: "get_project_item".to_string(),
        name: "Get Project Item".to_string(),
        description: "Read the full content of a referenced page, chat, space, or entity".to_string(),
        llm_description: r#"Fetch the full content of an item by its url: /page/, /chat/, /project/, /person/, /place/ or /org/. An @-mention in the user's message is such a link, e.g. [name](/chat/chat_xxx); fetch it only when its content matters to the answer. Also for one item an attached_project lists."#.to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "required": ["item_url"],
            "properties": {
                "item_url": { "type": "string" }
            }
        }),
        tool_type: ToolType::Builtin,
        category: ToolCategory::Data,
        icon: "ri:folder-open-line".to_string(),
        display_order: 5,
        is_system: false,
    }
}

/// Get default enabled tools configuration (for assistant profile)
pub fn default_enabled_tools() -> serde_json::Value {
    serde_json::json!({
        "think": true,
        "update_memory": true,
        "web_search": true,
        "semantic_search": true,
        "sql_query": true,
        "code_interpreter": true,
        "create_page": true,
        "get_page_content": true,
        "edit_page": true,
        "setup_applet": true,
        "get_project_item": true
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The sql_query description is generated: its table block is spliced in
    /// from the catalog when `default_tools()` runs. A placeholder that
    /// survived would be handed to the model AS the list of what it can query.
    #[test]
    fn sql_query_description_is_generated_from_the_catalog() {
        let desc = default_tools()
            .into_iter()
            .find(|t| t.id == "sql_query")
            .expect("sql_query is registered")
            .llm_description;
        assert!(!desc.contains("<<TABLE_CATALOG>>"), "placeholder reached the model");
        assert!(desc.contains("data_communication_message(body, "));
        assert!(desc.contains("wiki_day_prose(day_id, date, prose)"));
    }

    #[test]
    fn test_default_tools() {
        let tools = default_tools();
        // No exact count: it only ever fires when someone adds a tool, which is
        // not a bug, so it gets bumped without thought — or, as happened here,
        // left red for eight tools running. What matters is below: every tool is
        // well-formed, and the load-bearing ones are present.
        assert!(!tools.is_empty(), "the registry ships tools");

        // Verify all tools have required fields
        for tool in &tools {
            assert!(!tool.id.is_empty());
            assert!(!tool.name.is_empty());
            assert!(!tool.llm_description.is_empty(), "LLM description is required");
            assert!(tool.parameters.is_object(), "Parameters must be JSON object");
            assert_eq!(tool.tool_type, ToolType::Builtin, "Registry tools should be builtin type");
        }

        // Verify specific tools exist
        let ids: Vec<&str> = tools.iter().map(|t| t.id.as_str()).collect();
        assert!(ids.contains(&"think"));
        assert!(ids.contains(&"update_memory"));
        assert!(ids.contains(&"web_search"));
        assert!(ids.contains(&"semantic_search"));
        assert!(ids.contains(&"sql_query"));
        assert!(ids.contains(&"code_interpreter"));
        assert!(ids.contains(&"create_page"));
        assert!(ids.contains(&"get_page_content"));
        assert!(ids.contains(&"edit_page"));
        assert!(ids.contains(&"setup_applet"));
    }

    #[test]
    fn test_default_enabled_tools() {
        let enabled = default_enabled_tools();
        assert!(enabled.is_object());
        assert_eq!(enabled.get("think"), Some(&serde_json::json!(true)));
        assert_eq!(enabled.get("update_memory"), Some(&serde_json::json!(true)));
        assert_eq!(enabled.get("web_search"), Some(&serde_json::json!(true)));
        assert_eq!(enabled.get("semantic_search"), Some(&serde_json::json!(true)));
        assert_eq!(enabled.get("sql_query"), Some(&serde_json::json!(true)));
        assert_eq!(enabled.get("code_interpreter"), Some(&serde_json::json!(true)));
        assert_eq!(enabled.get("create_page"), Some(&serde_json::json!(true)));
        assert_eq!(enabled.get("get_page_content"), Some(&serde_json::json!(true)));
        assert_eq!(enabled.get("edit_page"), Some(&serde_json::json!(true)));
    }

    #[test]
    fn test_tool_parameters_have_type() {
        for tool in default_tools() {
            assert_eq!(
                tool.parameters.get("type"),
                Some(&serde_json::json!("object")),
                "Tool {} parameters should have type: object",
                tool.id
            );
        }
    }

    /// The web client keeps one presentation entry per tool (its noun, its
    /// depth, its line in the thinking block), and those tables drifted from
    /// this list unseen: a tool with no entry renders under its raw id. This
    /// writes the ids where a vitest diffs them against the client's table.
    /// Regenerate with `UPDATE_FIXTURES=1 cargo test -p virtues-registry tool_ids`.
    #[test]
    fn tool_ids_fixture_is_current() {
        let mut ids: Vec<String> = default_tools().into_iter().map(|t| t.id).collect();
        ids.sort();
        let want = serde_json::to_string_pretty(&ids).unwrap() + "\n";
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/web/src/lib/ai/fixtures/tool-ids.json");
        if std::env::var("UPDATE_FIXTURES").is_ok() {
            std::fs::write(&path, &want).expect("write fixture");
            return;
        }
        let have = std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!("{}: {e} (run with UPDATE_FIXTURES=1 to write it)", path.display())
        });
        assert_eq!(
            have, want,
            "{} is stale: the tool registry changed. Regenerate with UPDATE_FIXTURES=1, then give the new tool an entry in the web client's toolPresentation.ts.",
            path.display()
        );
    }
}
