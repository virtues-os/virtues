# CLI data verbs

**Written 2026-10-01, decisions 2026-10-05.** The `virtues` CLI drives the
box's lifecycle (`pair`, `upgrade`, `backup`, `doctor`) and cannot touch a
single record. This plan gives it the verbs the chat agent and the applet
runner already have, so an agent the owner already uses (Claude Code, Codex)
can work on their box without the box building more authoring surface.
Applets stay as they are; see [record/applets.md](../record/applets.md).

## Why

Most of what an applet is (a prompt on a cron, a face, a table, memory) is
also what a general agent product ships as routines, artifacts and skills.
What only the box has is the data, the roles that fence it, and the gate. The
CLI is the cheapest way to offer exactly that and nothing more: the owner's
own agent brings the reasoning, the box brings the record and the rules.

**The split.** Applets are what the box runs by itself. An outside agent is
how the owner builds and inspects, interactively, while present. Nothing on
the outside agent's side (a routine, a scheduled task) wakes anything on the
box; the box's schedule is the box's.

What it is not: a replacement for applets. A headless 7am run with a cheaper
model and nobody watching still needs flat fields, the check and the limits.
The verbs make applets thinner over time (a field the outside agent can reach
through a verb does not need to become a manifest key); they do not remove
them. Data triggers and Persona ([applets-next-plan.md](applets-next-plan.md))
wait on the test in slice 4 for the same reason.

## CLI first, MCP as an adapter

The CLI is the interface. An agent with a shell runs `virtues query "…"`
directly: models are fluent in shells, and `--help` costs no context until it
is read, where an MCP tool list sits in every prompt. `virtues mcp` exists for
agents that cannot open a shell, and lists the same verbs; it is an adapter
over the CLI's verbs, never a second definition of them.

## The one door

Every verb is a registry tool run through `ToolExecutor::execute`. No verb
gets its own SQL, its own role switch or its own validation:

- `sql_query` already drops to `virtues_face_reader` with a statement timeout
  (`tools/sql_query.rs`); `sql_write` to `virtues_applet_writer`
  (`tools/sql_write.rs`). The CLI runs as the `virtues` service user, whose
  pool role is a superuser, so a verb that bypassed the executor would read
  `box_secrets` and `virtues.env`. **That is the failure this door exists to
  prevent.**
- Arguments and help text come from `virtues_registry::tools::ToolConfig`
  (`parameters`, `description`). The CLI, the MCP tool list and the chat tool
  list are three renderings of one table.
- Which tools a caller may use is a named allowlist beside
  `APPLET_RUN_ALLOWED_TOOLS` in `tools/mod.rs`, not a second list in `cli/`.

## Verbs

| Verb | Tool | Notes |
|---|---|---|
| `virtues query <sql>` | `sql_query` | |
| `virtues search <text>` | `semantic_search` | `--entities`, `--from/--to` pass through |
| `virtues page new/get/edit` | `create_page`, `get_page_content`, `edit_page` | `edit` reads the body from stdin |
| `virtues applet ls/get` | `list_applets`, `get_applet` | |
| `virtues applet check <dir>` | `setup_applet` with `check_only` | the LSP, standalone: no disk write, no row; `-` reads JSON arguments |
| `virtues applet put <dir>` | `setup_applet` | |
| `virtues applet on/off <id>` | the app's switch, `PATCH /api/applets/:id` | |
| `virtues applet run <id>` | `run_applet` | "Run now": exempt from count caps, not spend |
| `virtues write <sql>` | `sql_write` | `applet_*` schemas only |
| `virtues schema [table]` | the `sql_catalog` description `sql_query` already generates | what an outside agent needs first |

Deliberately absent: `shell`, `sql_sudo`, `generate_image`, `code_interpreter`
(an outside agent has its own), `web_search` (same, and it spends the box's
gateway money), `update_memory` and the narrative-identity tools (they write
the owner's account of themselves).

### Output

The `gh` shape (clig.dev):

- stdout is a TTY: aligned tables for lists, plain text for one thing.
- stdout is piped: one record per line, no decoration.
- `--json` prints the tool's own JSON result; pipe it to `jq` to filter.
- Data on stdout, messages and errors on stderr, non-zero exit on failure.
- No colour when stdout is not a TTY or `NO_COLOR` is set.

MCP returns the JSON form.

## Permissions

**An outside agent gets full read and write over the verbs above.** A
"lands disabled, enable in the app" step was considered and rejected as poor
UX: the owner is present in the agent's session and approves its commands
there.

That changes an invariant in record/applets.md, which reads "no path from
model output to an enabled, scheduled row without a user-surface action". For
an outside agent, **the user-surface action is the owner's approval in the
agent's own session**. The record's wording is updated in the same change
that ships write verbs. The chat door is unchanged: an applet authored in the
box's own chat still waits on the Enable card.

What replaces the card as the backstop: **an applet put through the CLI with
a prompt gets `max_llm_cost_per_day = 1.00`** when it sets none, so a
scheduled applet a model wrote cannot run up a bill unattended. The owner can
raise or remove it like any limit. Both are built and in the record.

### The trap this has to close anyway

`check_tool_permission` gates only when `context.chat_id` is set: **"Headless
calls with no chat aren't gated either."** The CLI's verbs ride that headless
path, which is now intended. What says where a write came from is the door it
came through, not a field: writes arrive only at the console door, which
takes only the local console and logs each call, and the default spend cap is
applied by the CLI verb before the tool runs.

## Reaching the box

The credential decides what an agent can reach, and the CLI's role drops only
protect anything if the credential cannot go around the CLI.

| Way in | Opens | What the key can reach | Verdict |
|---|---|---|---|
| Plain SSH | nothing new | a full shell: `sudo -u virtues` is a database superuser, `virtues.env` holds the encryption key. The agent can skip the CLI, so the roles protect nothing and the only boundary is the agent's own permission prompts | **no** |
| SSH key with a forced command | nothing new | `command="virtues …",no-pty,no-port-forwarding` in `authorized_keys`: the key can run the `virtues` CLI and nothing else, so the executor's roles are the real boundary | **first** |
| iroh device key | nothing new (relay) | the box's API as a paired device, like the apps; works away from home; per-agent identity and `virtues device rm` revocation | **later** |
| An HTTP port with a token | an internet-facing listener | whatever the token reaches | **rejected**: the box has no internet-facing surface |

**First: `virtues agent-key add <pubkey>`** writes the forced-command line for
the `virtues-agent` user (slice 4 says why not the login user), and
`agent-key ls/rm` manage them. The forced command dispatches
`SSH_ORIGINAL_COMMAND` to the CLI's verbs only (lifecycle verbs like `upgrade`
or `reset` refused) and passes the key's name to the console door for the
audit line. Limits: reachable on the LAN or over Tailscale, and only on a box
running sshd.

**Later: agent devices over iroh.** A device kind `agent`, paired with the
standing code like any device, and a `virtues` client on the laptop that
speaks iroh. Designed when owners want access away from home; not before.

## Audit

Server logs only. Every write verb logs one structured event through the
existing tracing path (record/observability.md): `audit = "console_tool"`,
the tool, a hash of its arguments, and the status. The key name joins it with
`agent-key`. No table, no activity list in the app.

## Dev checkouts

No `virtues` OS user on a Mac. The verbs connect with the dev `DATABASE_URL`
and the executor's `SET LOCAL ROLE` does the same thing it does on a box,
which is the boundary that matters. `agent-key` is box-only.

## Slices

1. **Read verbs over the executor.** Built 2026-10-05 in `cli/data.rs`:
   `query` (`-` reads stdin), `search`, `schema`, `applet ls/get`, `page get`,
   with the output rules above. `CLI_TOOLS` in `tools/mod.rs` is the
   allowlist; `ToolExecutor::without_warmup` skips the code_interpreter
   package build `new` starts; `sql_query` results now carry `columns` in the
   SELECT's order; the verbs log at `error` so a failure prints once. Checked
   on the dev database: `box_secrets` and `pg_read_file` are refused,
   `DELETE` is refused, a misspelled column gets the did-you-mean.
   `page get` reads the saved page, not live Yjs state, so it can trail an
   open editor by the last save.
2. **`applet check`.** Built 2026-10-05. The check is one function,
   `applet_setup::check` over a parsed `Draft`, and `setup_applet` takes
   `check_only` to run it and create nothing (no "I allow" in chat, since it
   writes nothing). `virtues applet check <dir>` maps a folder to the tool's
   arguments: `manifest.toml`, `face/index.html`, and the schema versions this
   box has not applied, joined in order so an `ALTER` dry-runs after its
   `CREATE`; a manifest key an authored applet cannot set (`command`,
   `credential`) is a finding. `virtues applet check -` takes the JSON
   arguments on stdin, which is how a remote agent checks without copying a
   folder onto the box. Exits 1 on findings. Building it found the drift
   parser reading SQL comments as columns, a false finding on any schema that
   explains itself; fixed.
3. **Write verbs.** Built 2026-10-05: `page new/edit`, `applet put/on/off/run`,
   `write`. They do not run in the CLI process. A page edit has to go through
   the live Yjs document the server holds, `setup_applet` reloads the
   server's applet catalog, and `run_applet` dispatches through the server's
   runner; a write from another process lands in the database behind all
   three, and an open page's next save puts the old text back. So the CLI
   posts the tool to `POST /api/console/tool/:tool` (`server/api/console.rs`),
   which takes only the local console (loopback, the identity the on-box CLI
   already has) and only `CLI_WRITE_TOOLS`, runs it through the same
   executor chat uses, and logs one `audit = "console_tool"` line with the
   tool and a hash of its arguments. That line is the audit, in the server's
   own journal. `applet on/off` is the app's switch, `PATCH /api/applets/:id`.
   `page edit` saves the page at once (`YjsState::apply_text_edit` saves a
   machine edit without the debounce) so a read straight after sees it. A write needs the server running; the error says
   so. No caller field on `ToolContext` was needed: the door the call came
   through says who it is.
4. **`agent-key`.** Built 2026-10-05; the spare-box test below is still to
   run. The key does not go to the owner's login (a shell) or to `virtues`,
   which the installer gives passwordless `sudo ALL` and a `nologin` shell (a
   key to root, and one whose forced command could not run). It goes to its
   own user, `virtues-agent`: `/bin/sh` (sshd runs a forced command through
   the user's shell), no sudo, no groups, no database role. `sudo virtues
   agent-key add <key.pub> --name <n>` creates the user when missing and
   writes `restrict,command="/usr/local/bin/virtues agent-exec --key <n>"`;
   `ls` and `rm` touch only lines it wrote. `agent-exec` parses
   `SSH_ORIGINAL_COMMAND` (never executes it), allows only the data verbs, and
   since its user has no database, sends every verb, reads included, to the
   console door over loopback with the key's name in a header; the door logs
   it as `agent_key`. A folder path is refused there (it names a directory on
   the box); `applet check -` and `applet put -` take JSON. Tested locally
   with `SSH_ORIGINAL_COMMAND` set by hand against a scratch server; not yet
   through a real sshd.

   **The test on the spare box**: Claude Code builds three typical applets
   using only the verbs. **Success, written down before the run: it never
   needs a manifest field the verbs cannot reach.** Every field it does need
   is listed, and that list decides how big applets have to be, and whether
   data triggers and Persona are still wanted.
5. **`virtues mcp`** over the same verbs, for agents without a shell.
6. A manual page (`docs/`) only when slice 3 is in a released box.

## Open

- **Whether the chat agent's own tool calls should go through the same verb
  layer**, so there is one place a tool is defined, gated and logged.
- **The default spend cap's value**, and whether `max_runs_per_day` also gets
  one by default.
