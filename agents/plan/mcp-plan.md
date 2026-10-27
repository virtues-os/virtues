# MCP: connections in, the box as a server

**Written 2026-10-26. Nothing built.** Two directions over the Model Context
Protocol. Inbound, the box connects to other services' MCP servers and uses
their tools: actions and live lookups only, with no ingest. Outbound, the box
serves its own MCP endpoint, so agents the owner already uses (Claude Desktop,
Claude Code, Cursor, and later claude.ai and ChatGPT) can work over their life.

## Why

The box should not write and maintain an integration for every service an
owner uses. Most large services now publish an official MCP server, and the
spec settled in July 2026 (2026-07-28: stateless requests, Client ID
Metadata Documents for OAuth, Multi Round-Trip Requests, Tasks and Apps as
extensions). An MCP client gives the assistant a write path into Linear,
Notion, GitHub, Slack and the rest without a native integration for each.

The outbound side was already planned as slice 5 of
[cli-data-verbs-plan.md](cli-data-verbs-plan.md). This plan replaces that
slice.

## The three nouns

| Noun | What it is | Where it lives |
|---|---|---|
| **Tool** | A verb the model or a person can call. Native, or provided by a connection. | Native: `crates/virtues-registry/src/tools.rs`. Connection: listed from its MCP server at runtime. |
| **Connection** | An account the box holds a credential for, plus the tools it provides. Some connections also sync data; an MCP connection only provides tools. | A source entry (`applets/sources.toml`, or the state root for one the owner adds by URL) and a row in `credentials`. |
| **Applet** | A skill folder (`applet-shape.md` in this directory): instructions, plus an optional trigger, run step, tables and view. It uses the tools its `allowed-tools` names. | `applets/` when shipped, the state root when authored. |

A view (`view.html`, called a face in today's code) is a file in an applet's
folder. It is not a fourth noun. A tool result from a connection can also
carry a view, under the MCP Apps extension; that is the same thing arriving
from a server.

Applets use a connection's tools. An applet does not call another applet:
applets share data through tables, as decided for data triggers.

## What exists today

- **No MCP code.** A client (`cf109c2b6`, about 700 lines no binary
  compiled) and a server (`dbfa46f3b`: `/mcp` mounted outside the auth layer,
  zero tools, the `rmcp` dependency) were both deleted. Migration 0010 dropped
  the `app_mcp_servers` and `app_mcp_tools` tables. The unused
  `ToolType::Mcp` variant in `tools.rs` and the "MCP tokens" mention on
  `SourceAuth::ApiKey` in `applet_templates/mod.rs` are what remain.
- **One tool table, one executor.** Every tool is a `ToolConfig`;
  `ToolExecutor::execute` (`virtues-core/src/tools/executor.rs`) dispatches by
  name. Callers get a named allowlist in `tools/mod.rs` (`CLI_TOOLS`,
  `CLI_WRITE_TOOLS`, `SUBAGENT_TOOLS`, the applet and request lists).
  `PERMISSION_REQUIRED` gates `applet`, `save_skill`, `sql_write` and
  `generate_image`; headless calls and applet runs are not gated.
- **A worker pattern for outside tools.** `browse` (`virtues-core/src/browse.rs`)
  gives chat one tool that runs a small `AgentLoop` with only the browser
  tools on the Lite slot, and reports its spend back to the turn.
- **Source auth kinds.** `self_issued_bearer`, `via_proxy` (the OAuth proxy in
  `services/virtues-api/src/routes/oauth.rs`, a client-secret custodian that
  hands tokens to the box in a signed 5-minute exchange token) and `api_key`.
- **How the box authenticates** (`virtues-core/src/middleware/auth.rs`): a
  proven iroh peer that is an allowlisted device, the loopback console, or
  dev. "There is no fourth class." Other tokens each open exactly one thing.
- **The Mac app already serves the box locally.** The desktop reach client
  (`crates/virtues-reach-client/src/proxy.rs`) listens on `127.0.0.1:7117` and
  splices each connection onto an iroh stream to the box, authenticated as the
  Mac's paired device.
- **CLI verbs for outside agents** (cli-data-verbs slices 1-4): read verbs,
  write verbs through `POST /api/console/tool/:tool`, and `agent-key`, an SSH
  forced-command key that can run only the data verbs.

## Part 1: the MCP client, tools only

### No ingest

A connection never syncs. MCP is a call interface for an agent: no cursors,
and change feeds are rarely implemented. Syncs stay native sources, exports
or the phone. A service with a native source (Google) can also be an MCP
connection later; the two do not share a credential row.

### A connection is a source entry

A curated server is a `[[source]]` entry with an `mcp` URL and a new auth
kind, `mcp_oauth`. A server that takes a pasted token uses the existing
`api_key` kind with an `mcp` URL. An owner adds an arbitrary server by URL
from Settings; that entry lives in the state root's catalog, which
[sources-packages-plan.md](sources-packages-plan.md) already wants
`load_catalog` to merge. Tokens go in `credentials` like any other source's.

### Signing in

MCP servers authorize with OAuth 2.1 and PKCE, and each server is its own
authorization server. The box is a public client: its `client_id` is a URL to
a Client ID Metadata Document we host (for example
`https://virtues.com/mcp/client.json`). There is no client secret to keep, so
the box redeems the code itself.

The box has no public URL, so the redirect goes to `auth.virtues.com`, which
forwards the code to the box in the same signed exchange shape the proxy uses
today. The PKCE verifier never leaves the box, so the proxy cannot redeem the
code and never holds a token. When the owner signs in from the Mac app, a
loopback redirect through `:7117` can skip the proxy entirely; the 2026-07-28
spec allows `localhost` redirects for native clients.

### Tools become registry rows

On first use after start, and again when a list's `ttlMs` runs out, the box
calls `tools/list` and keeps the result in memory. Each tool becomes a
`ToolConfig` with `tool_type: Mcp` and a name of the form
`<connection>__<tool>` (`linear__create_issue`). Function names may not
contain dots in the OpenAI-style definitions `llm_definition` builds.
`ToolExecutor::execute` gets one branch: a name with a connection prefix is a
`tools/call` on that server, with the credential refreshed first.

Everything else stays as it is for native tools: allowlists, the turn's
dollar and step ceilings, the tool card, the audit line.

### What the chat model sees

Not every connection's schemas. A large server's tool list runs to thousands
of tokens, and a tool list that changes mid-chat throws away the cached
prompt prefix.

Chat gets one tool, `connection(name, task)`, whose description lists each
connected service in one line ("Linear: issues, projects and cycles"). It
runs a worker loop on the `browse` pattern: a short prompt, only that
connection's tools, the Lite slot, spend reported back to the turn. The chat
passes everything the worker needs in `task`, as it does for `browse`. The
chat's own tool list stays the same whatever is connected.

If writes from the Lite model prove unreliable on the first server, the
worker moves to the Standard slot. That is a measurement for slice 2.

### What asks

The owner decided that assistant capabilities do not get approval cards.
Deletes and real money still ask. So:

- A call runs when the tool is read-only, or is a write without
  `destructiveHint: true`.
- A call to a tool that declares `destructiveHint: true` asks, once per call,
  through the existing `permission_needed` result.
- The owner can change either for one tool on the connection's page.
- Every write shows a receipt in the chat: which service, what was done, and a
  link to it there.
- The worker's prompt carries the rule chat already has: ask before anything
  irreversible.

Annotations are claims made by the server. A server that leaves out
`destructiveHint` on a delete gets no prompt. That is why the curated list
matters and why an arbitrary URL is the owner's choice, made on purpose.

### Applets and connections

An applet names a connection's tools in `allowed-tools`, by tool or by
connection (`linear__*`). For the Rule of Two in applet-shape.md, a
connection counts as both outside text (its results) and a way out (its
writes). As with `net(…)` hosts, only the owner can add a connection to an
applet that did not ship; the assistant's `applet` tool cannot.

Chat already holds all three legs through `browse` and `send_imessage`, and a
connection adds another. Applet-shape.md leaves chat's leak paths out of
scope, and so does this plan.

### Which servers

Official vendor servers on a short curated list in `sources.toml`. Any other
URL is a power-user option on the Settings page. Third-party registries carry
thousands of servers, many unofficial or unmaintained, and none of them is
the list.

## Part 2: the box as an MCP server

### One endpoint

The box serves MCP over Streamable HTTP at `/api/mcp`, stateless as in the
2026-07-28 spec. Every tier below reaches the same route. It sits behind the
auth extractor; the deleted server sat outside it.

Each call runs through `ToolExecutor::execute` with an allowlist,
`MCP_TOOLS`, beside `CLI_TOOLS` in `tools/mod.rs`. The tool definitions come
from the same `ToolConfig` rows, so the CLI, chat and MCP are three views of
one table, as the CLI plan intended.

What it exposes:

- **Tier 1** (the owner's own machine): the CLI's read and write sets,
  `applet` setup and run included, because the owner is present in that
  agent's session. That was the CLI plan's decision and it carries over.
- **Tier 2 and 3** (a lab's servers calling in): reads plus pages and tasks.
  No applet writes and no `sql_write`.
- **Never, at any tier:** `send_imessage`, `browse`, `shell`,
  `generate_image` or connection tools. An outside agent holds the data, reads outside text and
  has its own ways out, and the box cannot apply the Rule of Two inside
  someone else's agent loop.

Some sources' terms forbid this. Strava's API policy bars third-party MCP
servers, and Oura's agreement bars aggregators from passing its data to any AI
model. A source entry gets a flag for it, and `sql_query` under `/api/mcp`
drops to a role with no grants on those sources' tables.

### Tier 1: the Mac bridge (first)

Claude Desktop, Claude Code and Cursor all launch local stdio MCP servers. The
desktop reach client gains an `mcp` subcommand: a stdio shim that forwards
each request over iroh to `/api/mcp`.

The shim has its own device key, paired as an `agent` device from a "Connect
an agent" button in the Mac app. Pointing the agent at `127.0.0.1:7117` would
also work, but every call would then act as the Mac itself. A device of its
own gives each agent:

- its own identity in the audit line,
- revocation with `virtues device rm`,
- a tool set the box can tell apart from the Mac app's.

This is the "agent devices over iroh" row in the CLI plan's reach table,
reached sooner. It works away from home, because iroh does.

For agents on other machines, `virtues mcp` over the existing `agent-key`
forced command speaks the same protocol on stdio and posts to the same route
through the loopback console.

### Tier 2: remote, through our relay

claude.ai and ChatGPT call a connector from their own servers, over plain
HTTPS. They cannot run the iroh WASM loader publishing uses. So this tier
needs the Funnel from [publishing-plan.md](publishing-plan.md): an SNI router
beside iroh-relay forwards TLS for the box's hostname over iroh, and the box
holds the certificate. The relay forwards bytes it cannot decrypt. The lessons
recorded there apply unchanged: the CA rate limit per registered domain, the
Public Suffix List for a shared domain, CAA bound to our ACME account, and CT
monitoring as a real alert.

The box has to be the OAuth authorization server for its own endpoint. The
owner approves a connector the way a device flow works: the authorize page
shows a code, and the owner confirms it in the Virtues app, which is already
authenticated as a device. The box has no passwords for a stranger's browser
to type.

The access tokens are a new credential class in `middleware/auth.rs`. Like
the face token, each opens exactly one thing: `/api/mcp`, under the tier 2
allowlist. They never authenticate as a device, and the owner can revoke each
one on its own. The module's "no fourth class" paragraph is rewritten in the
same change.

### Tier 3: the owner's own HTTPS

A "public URL" setting for an owner who runs their own tunnel or domain. Same
endpoint and tokens as tier 2. Which tunnel they choose decides who else sees
plaintext: Tailscale Funnel terminates TLS on their machine, and Cloudflare
Tunnel terminates at Cloudflare. The manual says which is which.

`middleware/auth.rs` already refuses the loopback-console bypass when a
forwarding header is present, so a tunnel on the box itself cannot pass as
the console.

### Where the data goes

Whatever a connected agent reads reaches that lab under the owner's account
terms. Chat goes through the gateway with zero data retention; a consumer
connector does not. The sheet that connects an agent says so, with the lab's
name.

## Not in this plan

- **Native email and calendar writes.** Sending mail and creating events are
  resolved against people and written through to `communication_email` and
  `calendar_event`, with an undo window like `send_imessage`'s outbox. They
  need their own plan and wider Google scopes.
- **Ingest over MCP.** See above.
- **Applet views as MCP Apps.** Making `view.html` speak the MCP Apps
  protocol would let an applet's view render inside Claude or ChatGPT, and a
  connection's MCP App render in our chat. Do it when the view bridge next
  changes; it is listed as slice 6 so it is not lost.

## Slices

1. **`/api/mcp` and `MCP_TOOLS`.** Streamable HTTP, stateless, behind the
   auth extractor, tier 1 allowlist. Checked with the MCP Inspector against a
   dev box.
2. **The Mac bridge.** `agent` device kind, the "Connect an agent" button,
   the stdio shim in the reach client, and `virtues mcp` over `agent-key`.
   Done when Claude Code on a Mac away from home builds an applet through it,
   the test the CLI plan set for its verbs.
3. **One MCP connection.** `mcp_oauth` with a hosted Client ID Metadata
   Document and the code-only redirect, tool rows with `<connection>__`
   names, the executor branch, the `connection` worker, receipts and the
   `destructiveHint` ask. One official server, picked by the owner. Done when
   a chat files an issue on it and a delete asks.
4. **Connections in applets.** `allowed-tools` entries for connection tools,
   the Rule of Two legs, owner-only adding. Then owner-added URLs from
   Settings.
5. **Tier 2.** The SNI router (shared with publishing wave 3), the box as an
   authorization server, the approve-in-the-app flow, the token class, the
   tier 2 allowlist and the terms flag. Tier 3's setting follows from it.
6. **MCP Apps** for applet views and for connection results.

## Open

- **Which server goes first** in slice 3.
- **Whether `rmcp` supports the 2026-07-28 spec**, or whether the stateless
  protocol is small enough to serve directly from axum. Decide in slice 1.
- **Whether chat's own tool calls go through the same allowlist layer** (the
  CLI plan's open question), now that three callers share one table.
- **What the Lite slot gets wrong on writes**, measured in slice 3.
