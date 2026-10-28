# MCP: connections in, the box as a server

**Written 2026-10-26. Slices 1 and 2 built the same day; slice 3 started, with Linear.** Two directions over the Model Context
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
| **Connection** | An account the box holds a credential for, plus the tools it provides. Some connections also sync data; an MCP connection only provides tools. | A `credentials` row of kind `connection`, keyed by the connection's short name, with the server's address in its metadata. |
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

### A connection is a credentials row

A connection is one `credentials` row of kind `connection` (migration 0066).
Its `key` is a short name such as `notion`, which prefixes the service's tool
names and is what the chat calls it. Its metadata holds the server's address,
its registry name when it has one, and its description. It is not a `source`,
so it does not count toward setup, the sources list, stream health or the
chat's list of sources. The servers the owner can pick from come from the
public MCP Registry, not from `sources.toml`; see "Which servers".

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

Only what the server itself labels. MCP tool annotations carry
`readOnlyHint` and `destructiveHint`, and the box acts on them and on
nothing else: it keeps no lists of tool names or words of its own for
deletes, money or messages (the owner, 2026-10-27).

- A tool labeled `destructiveHint: true` is left out of the worker's list
  and refused unless the chat called with `confirmed`, after the owner
  agreed in chat.
- Every other tool runs without asking.
- A tool labeled `readOnlyHint: true` is a read. Any other call is a write,
  and each write shows a receipt in the chat: which service, what was done,
  and a link to it there.

A server that leaves out the labels gets no prompt on a delete and a receipt
on every call. That is the server's choice, and the featured list is why it
matters which servers come first.

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

Any server in the public MCP Registry (registry.modelcontextprotocol.io) that
the box can connect with a sign-in alone: a streamable HTTP remote at a fixed
address, with no API key header. The box searches the registry for the owner
(`tools/connection_registry.rs`). The registry has no usage numbers, so the
order is a short featured list of first-party servers, then the registry's
own. The featured list is registry names, so addresses and descriptions stay
current. Any other address is a power-user option on the same page.

Anyone can publish to the registry, and much of it is unofficial or
throwaway, so the page tells the owner to connect only services they trust.
Google publishes no server for Gmail or Calendar; the registry's entries for
them are third parties that would read the mail. Zapier is on the featured
list as the way there until Google publishes one.

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

1. **`/api/mcp` and `MCP_TOOLS`.** Built 2026-10-26 in
   `virtues-core/src/server/api/mcp.rs`, on rmcp 3.8 with only its server
   and Streamable HTTP features. rmcp has served the 2026-07-28 spec since
   3.0 and still negotiates the older handshake, so both kinds of client
   work. Served statelessly with JSON responses. The route is in
   `protected_routes`, and a layer of its own puts the `AuthUser` into the
   request so the `audit = "mcp_tool"` line names the device. `MCP_TOOLS`
   and `mcp_allows` in `tools/mod.rs` allow the CLI's reads and writes and
   `applet` list, get, check, setup and run. Calls run with `console: true`,
   so an applet run is a manual run, and a setup gets the default daily spend
   cap, now one function (`applet_setup::with_default_spend_cap`) shared with
   `virtues applet put`. A request carrying any `Origin` is refused, which
   keeps browser pages off the route; the Host check is off, since the iroh
   key authenticates. Checked against a scratch core with MCP Inspector
   2.12.0: tools/list, a query, an applet list and a page created over MCP;
   a legacy `initialize` and a 2026-07-28 `server/discover`; `shell` and an
   applet delete refused; an `Origin` answered 403.
2. **The Mac bridge.** Built 2026-10-26.
   - Migration 0064 adds the `agent` device kind to `app_device` and to
     `app_pair_token.intended_kind`. In `middleware/auth.rs`, a proven peer
     whose device is an agent reaches `/api/mcp` and gets 403 on every other
     route, before the dev fallback can admit it. Pairing labels an agent by
     its name, files it under the device that minted its token
     (`installed_by`, as for the collector), and does not count it as a paired
     device for setup.
   - `crates/virtues-reach-client/src/mcp.rs` is the bridge: an `AgentStore`
     (`<app dir>/agents/<name>.json`, 0600, apart from the app's own
     pairing), `pair`, and `serve_stdio`, which relays each stdio JSON-RPC
     message to `/api/mcp` over iroh with the Streamable HTTP routing headers
     and turns JSON, chunked and event-stream answers back into lines.
   - The Mac app's binary is the bridge: `virtues mcp --name <name>` branches
     before any of the app starts. `connect_agent` (command surface 13) pairs
     an agent through the `:7117` loopback with a token minted for kind
     `agent`, and returns the command, arguments and, for a dev profile, its
     environment. Devices › Add device › An AI agent on this Mac
     (`AgentConnectModal.svelte`) runs it and shows the `claude mcp add` line
     and the `mcpServers` JSON.
   - `virtues mcp` on the box is the same bridge over loopback TCP
     (`cli/mcp.rs`, with `Loopback` beside the iroh client as a
     `pair_door::Forwarder`). The server takes it as the local console. An
     agent elsewhere runs it as `ssh virtues-agent@<box> mcp`: `agent-exec`
     allows the `mcp` verb and sends the key's name, which the `mcp_tool`
     audit line records as `agent_key`.
   - Checked against a scratch core: an agent paired with the same pairing
     call the app makes; MCP Inspector 2.12.0 drove the built app binary over
     stdio for tools/list, a query and a refused applet delete, with the audit
     line naming the agent device; the agent's key got 403 on `/api/pages`
     and `/api/devices`. `virtues mcp`, and `agent-exec` with
     `SSH_ORIGINAL_COMMAND=mcp`, were driven the same way, and the audit line
     named the key. Not yet checked: a real sshd in front of `agent-exec`,
     the sheet inside a built Mac app, and Claude Code building an applet
     through the bridge away from home, the test the CLI plan set for its
     verbs.
3. **One MCP connection: Linear** (the owner's pick, 2026-10-26). Done when
   a chat files an issue on it and the chat shows the receipt.
   - Built: `tools/connection.rs` and `server/api/connections.rs`, on rmcp's
     client and `auth` features over the reqwest 0.13 iroh already links.
     Sign-in is OAuth 2.1 with PKCE as a public client registered by dynamic
     client registration; the redirect is `/api/connections/callback` at the
     origin the owner's app sends, so no proxy holds the code. Tokens are a
     sealed `credentials` row (kind `connection`), which rmcp loads and
     refreshes. Chat gets one `connection(name, task, confirmed)` tool
     (its own 900-character budget group); a Lite-slot worker gets the
     service's tools as `<connection>__<tool>` definitions, built from
     `tools/list`, never registry rows. A tool whose annotations say
     `destructiveHint: true` is left out of the worker's list and refused by
     the executor branch unless the chat called with `confirmed`, which sets
     `ToolContext::owner_confirmed`.
   - Built after: any registry server or pasted address, not one constant
     (`tools/connection_registry.rs`); the featured services as rows in
     the Sources catalog, labeled "The assistant can act here" beside
     sources' "Syncs your data", with the table's search also searching the
     registry and an address field under it (`SourcesCatalog.svelte`);
     routes `GET /api/connections`, `GET
     /api/connections/search?q=`, `POST /api/connections/connect {origin,
     registry | url}`, `DELETE /api/connections/:id`; the connected services'
     short names in the chat's context.
   - Checked: each featured server starts a sign-in from the box (discovery,
     dynamic registration, a sign-in URL with PKCE S256 and `resource`).
     GitHub, Figma and Zoom refuse dynamic registration, so they are not
     featured until a Client ID Metadata Document is hosted on virtues.com,
     which rmcp prefers once it is passed in.
   - Run live on 2026-10-27 against a real Linear workspace: the owner
     approved the sign-in, the box stored the `connection` row, and a chat
     question about the owner's issues went through one `connection` call.
     The Lite-slot worker made five read calls (four `list_issues`, one
     `list_custom_views`) and spent about a quarter of a cent; the chat
     answered with linked issue identifiers.
   - Receipts: the worker loop builds one for each call to a tool its
     server does not label `readOnlyHint: true`, from the tool's title and
     the service's answer (its item and `url`), never the model's wording.
     They return in the `connection` result as `receipts` and the chat draws
     `ConnectionReceiptCard` for them; each is also logged as
     `audit="connection_write"`. Each connection's tool list is read once
     per session and shared by the definitions, the destructive check and
     the receipts. Checked live: Linear labels `get_issue` read-only, and a
     read question made no receipt.
   - Write run, 2026-10-27, against a real Linear workspace with the
     owner's OK for one test comment. Three findings, each fixed:
     - Linear labels 29 tools `destructiveHint`, every `save_*` among them,
       because each one creates or updates. So every Linear write waits for
       the owner. The worker and the chat now get the held-back tools by
       name (`held_back`), so the chat asks instead of reporting the
       connection as read-only.
     - The Lite worker filled every optional field of `save_comment` (empty
       strings, and an enum value Linear rejects). Optional top-level
       properties are now shown to the worker as also taking `null`, and
       the box drops optional ones left `null` or empty before the call.
       The comment then posted on the next try.
     - A comment's answer is the comment: no link, no issue, and an author
       object, whose name the receipt took as the item. The item now comes
       only from the answer itself or a nested object with its own
       `identifier`, else the written `body`.
   - Left: servers that take an API key in a header.
4. **Connections in applets.** `allowed-tools` entries for connection tools,
   the Rule of Two legs, owner-only adding. Then owner-added URLs from
   Settings.
5. **Tier 2.** The SNI router (shared with publishing wave 3), the box as an
   authorization server, the approve-in-the-app flow, the token class, the
   tier 2 allowlist and the terms flag. Tier 3's setting follows from it.
6. **MCP Apps** for applet views and for connection results.

## Open

- **Whether chat's own tool calls go through the same allowlist layer** (the
  CLI plan's open question), now that three callers share one table.
- **What the Lite slot gets wrong on writes**, measured in slice 3.
