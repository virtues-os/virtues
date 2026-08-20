<!--
  ┌─ HEADING CONVENTION (read before editing any # / ## / ### heading) ──────────┐
  Headings are NOT plain markdown text. GitHub's sanitizer strips CSS, so to get
  a serif (Times) heading we render each one as a theme-aware SVG image wrapped in
  a real markdown heading, e.g.:

    ## <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h2-foo-dark.svg"><img alt="Foo" src=".github/images/headings/h2-foo-light.svg" height="28"></picture>

  Because the heading's visible content is an image (no text), two things follow:
    • The GitHub "Outline" sidebar shows blank labels — accepted tradeoff.
    • GitHub can't auto-generate anchors, so each H2/H3 has an explicit
      `<a id="github-slug"></a>` on the line ABOVE it to keep deep links working.

  TO ADD OR EDIT A HEADING (do ALL of these, or you'll get a stale image / dead link):
    1. Edit the heading list in .github/images/headings/gen.py
       (font sizes: H1=34/h40, H2=24/h28, H3=20/h22; weight 400; light+dark each).
    2. Run:  python3 .github/images/headings/gen.py   (regenerates the SVGs).
    3. In this file, update the <picture> block's `alt`, both `srcset`/`src`
       filenames, and the `<a id>` anchor above it. The `<a id>` must equal the
       GitHub slug of the heading text (lowercase, punctuation stripped,
       spaces -> hyphens), so existing #deep-links keep resolving.

  Keep headings short — wide SVGs (>~360px) get scaled down on mobile, breaking
  the visual size hierarchy. Times is a system font; Linux/Android fall back to
  their default serif.
  └──────────────────────────────────────────────────────────────────────────────┘
-->
![Virtues](.github/images/cover3.png)

# <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h1-virtues-dark.svg"><img alt="Virtues" src=".github/images/headings/h1-virtues-light.svg" height="40"></picture>

**A computer for your own life.** It sits in your home, collects what you
already generate — health, location, money, calendar, mail, messages, what you
record and read — and turns it into a record you can actually ask questions of.
One household, one box, no account on somebody else's server.

Two ways to run it. **The appliance** is a small board we flash and ship, with a
screen on the front; you plug it in and pair a phone to it. **Do it yourself** is
the same software on your own Linux machine, installed with one command. The
appliance is the product; the DIY path is how it stays honest — it is the same
binary, and you can always leave with your data.

> **Status: 0.1.0, and early in the way that word should mean.** Exactly one box
> exists and it was built by hand. The appliance path — flashing, first boot,
> pairing over Bluetooth, the case button — has been walked end to end once, on
> that board. No stable release is published yet, so the install command below
> does not work today; see [Installing](#install-linux-home-server).
>
> Auth is **pair-only**: no passwords, no email, no magic links. The only way to
> get in the first time is to be standing next to it. A paired device then
> reaches the box from anywhere by its iroh key.
>
> Expect rough edges, and expect us to say where they are.

[![License: BUSL-1.1 + MIT](https://img.shields.io/badge/License-BUSL--1.1%20%2B%20MIT-blue.svg)](LICENSE)
[![Discord](https://img.shields.io/badge/Discord-Join%20Us-7289da?logo=discord&logoColor=white)](https://discord.gg/sSQKzDWqgv)

<a id="what-it-does"></a>
## <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h2-what-it-does-dark.svg"><img alt="What It Does" src=".github/images/headings/h2-what-it-does-light.svg" height="28"></picture>

Virtues replaces a fragmented app ecosystem with a single, unified system:

- **Ingest** your data from APIs (Google, Notion, Plaid, Strava, GitHub) and devices (iOS sensors, Mac activity)
- **Build** a living knowledge graph — people, places, organizations, events — linked to your raw data
- **Write** an autobiography that maintains itself — daily summaries, narrative arcs, temporal navigation
- **Query** your life with an AI that has real context — not a chatbot guessing, but an agent with access to your actual data via SQL, web search, and code execution

All of it runs as a single Rust binary against a Postgres database on the box's
own disk.

**Where your data goes, stated plainly.** It is stored on the box and it stays
there. Search runs on the box — embeddings and reranking never leave it. GPS is
reduced to distance and pace before anything is sent anywhere.

The exception is the assistant, and it is not a leak, it is the feature: asking a
question, or letting the box write your day, sends the relevant part of your
record to a model provider — and a voice recording is sent as **audio**, not as a
transcript, to be transcribed. Point Virtues at a local model and that stops too;
the trade is quality and speed, and it is yours to make.

We will not tell you this is private by construction. The relay genuinely cannot
read your data — that is physics. The inference boundary rests on a contract with
a provider, which is a different kind of promise, and
[the privacy model](docs/privacy-model.md#the-inference-boundary-where-your-data-does-leave)
says exactly what crosses it and what never does.

<a id="architecture"></a>
## <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h2-architecture-dark.svg"><img alt="Architecture" src=".github/images/headings/h2-architecture-light.svg" height="28"></picture>

```
┌─────────────────────────────────────────────────────────────┐
│  Sources                                                    │
│  OAuth: Google · Notion · Plaid · Strava · GitHub           │
│  Device: HealthKit · Location · Microphone · Contacts       │
│          FinanceKit · EventKit                              │
└──────────────────────┬──────────────────────────────────────┘
                       ▼
┌──────────────────────────────────────────────────────────────┐
│  Virtues Core (Rust · port 8000)                            │
│  ┌──────────┐  ┌───────────┐  ┌──────────┐  ┌───────────┐  │
│  │ Ingest   │  │ Transform │  │ Wiki &   │  │ AI Agent  │  │
│  │ Engine   │  │ Pipeline  │  │ Entities │  │ + Tools   │  │
│  └──────────┘  └───────────┘  └──────────┘  └───────────┘  │
│  Storage: Postgres (metadata + ontologies) · S3 (raw streams)│
└──────────────────────┬──────────────────────────────────────┘
                       ▼
┌──────────────────────────────────────────────────────────────┐
│  virtues-api (Rust sidecar · port 9002)                       │
│  API proxy with per-user budget enforcement                 │
│  Routes to 100+ LLM providers via Vercel AI Gateway         │
│  Holds all external API keys (AI, Exa, Plaid, Google, etc.) │
└──────────────────────────────────────────────────────────────┘
```

**Core** handles data ingestion, entity resolution, the wiki, pages, chat, and serves the web UI. **virtues-api** is a sidecar proxy that mediates all external API calls — LLM requests, web search, bank connections — with budget tracking and key isolation. Core never touches API keys directly.

**Remote access** has no public surface at all. The box is an iroh endpoint whose Ed25519 key *is* its identity, so a paired device reaches it by key — LAN-direct, hole-punched, or bounced off a blind relay — with no inbound port opened at home and no hostname anyone can type. The relay forwards only sealed, end-to-end-encrypted bytes it has no key to read. See **[Privacy &amp; security model](docs/privacy-model.md)** (who holds which secret, and who deliberately doesn't) and the [visual walkthrough](docs/relay-walkthrough.html).

<a id="data-sources"></a>
## <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h2-data-sources-dark.svg"><img alt="Data Sources" src=".github/images/headings/h2-data-sources-light.svg" height="28"></picture>

| Source | Streams | Method |
|--------|---------|--------|
| Google | Calendar, Gmail | OAuth |
| Notion | Pages | OAuth |
| Plaid | Transactions, Accounts, Investments, Liabilities | OAuth |
| Strava | Activities | OAuth |
| GitHub | Events | OAuth |
| iOS | HealthKit, Location, Microphone, Contacts, FinanceKit, EventKit | Device |
| macOS | Apps, Browser, iMessage | Device |

Extensible: add a new source as an applet in `applets/<name>/` with a `manifest.toml` — see [`applets/AUTHORING.md`](applets/AUTHORING.md).

<a id="overview"></a>
## <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h2-overview-dark.svg"><img alt="Overview" src=".github/images/headings/h2-overview-light.svg" height="28"></picture>

**Knowledge Graph** — People, places, organizations, and events extracted from your data. Entity resolution links mentions across sources (the "Sarah" in your calendar is the same one in your contacts).

**Autobiography** — Daily summaries written from your data. Temporal navigation by day and year. Narrative structure: Telos (life purpose) → Acts (multi-year arcs) → Chapters → Days.

**AI Chat** — Multi-model chat (Claude, GPT, Gemini, etc.) with tools:

- `sql_query` — read-only SQL against your ontology tables
- `web_search` — Exa-powered web research
- `code_interpreter` — Python sandbox (pandas, matplotlib, scipy)
- `create_page` / `edit_page` — AI-authored documents
- MCP server support for custom tool integrations

**Pages** — Rich documents with version history, cover images, and AI editing.

**Drive** — File storage with S3 backend. Upload, organize, and reference files in chat.

**Developer Tools** — SQL console, lake browser, job inspector, sitemap viewer.

<a id="install-linux-home-server"></a>
## <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h2-install-linux-home-server-dark.svg"><img alt="Install (Linux home server)" src=".github/images/headings/h2-install-linux-home-server-light.svg" height="28"></picture>

<a id="requirements"></a>
### <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h3-requirements-dark.svg"><img alt="Requirements" src=".github/images/headings/h3-requirements-light.svg" height="22"></picture>

| | |
|---|---|
| **Host OS** | Debian 13+, Ubuntu 24.04 LTS+, or Fedora 40+. Debian 13 and Ubuntu 26.04+ ship Postgres 18 natively; on Ubuntu 24.04/25.04 the installer adds the [PGDG repo](https://www.postgresql.org/download/linux/) automatically. x86_64 or aarch64. |
| **Hardware** | 8 GB RAM, an SSD. GPU optional. |
| **Network** | Standard residential ISP — outbound 443 only, no port forwarding, no inbound rule. LAN-first: the web UI is reachable from a browser on the box itself (Chromium on the box → `http://localhost:8000`) or anywhere else from a paired client — the mobile app, or the desktop helper at `http://localhost:7117` (see [Connect from another machine](#connect-from-another-machine-v02-preview) below). |
| **Mac / Windows** | Not supported as host — Virtues needs root, native Postgres, and full SSD ownership. Use a Linux box. |

<a id="install-in-one-command"></a>
### <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h3-install-in-one-command-dark.svg"><img alt="Install in one command" src=".github/images/headings/h3-install-in-one-command-light.svg" height="22"></picture>

```bash
curl -sSL https://virtues.com/sh | sudo sh
```

> **This does not work yet.** `virtues.com/sh` serves the newest *stable*
> release, and there is not one: the version line was reset to 0.1.0 before
> launch and the older tags were withdrawn. Until `v0.1.0` is cut, install the
> prerelease channel instead — same installer, same steps, cut from the branch
> we are actually testing:
>
> ```bash
> curl -sSL https://virtues.com/sh-pre | sudo sh
> ```
>
> On the appliance you will not run either: the boot medium arrives flashed, and
> first boot mints the box its own identity.

That:
- Downloads the latest `virtues` binary into `/usr/local/bin/`
- Installs Postgres 18 + pgvector, Avahi (mDNS), and the rest of the system deps via your package manager
- Configures `/etc/avahi/services/virtues.service` so the box advertises itself on the LAN as `virtues.local`
- Enables the `virtues.service` systemd unit (the box mints and keeps its own iroh secret — that key *is* its identity, and nothing external issues it)
- Prints a **6-digit pair code** to type into the app

```bash
sudo -u virtues virtues pair   # prints the code again, any time
```

Enter that code in the desktop or mobile app ([virtues.com/downloads](https://virtues.com/downloads)). **A browser cannot pair** — authentication is a held Ed25519 key and a browser tab has none, so `/pair` in a browser only explains itself. The one exception is a browser running *on the box*, which is trusted as the loopback console and lands straight in the UI at `http://localhost:8000`.

Then connect a source, and optionally `sudo -u virtues virtues subscribe` to enable AI chat through the Virtues cloud (or set up a [BYO provider key](docs/auth-model.md) under Settings).

| Command | What it does |
|---|---|
| `virtues pair` | Print the 6-digit code to connect a device (`login`/`link` are aliases) |
| `virtues sudo` | Approve a pending sensitive action — proves physical access to the box |
| `virtues status` | Health dashboard (identity / inference / subscription / devices) |
| `virtues status --json` | Machine-readable status snapshot for support tickets |
| `virtues subscribe` | Connect this box to your Virtues subscription via Stripe |
| `virtues init` | First-boot plumbing (migrations + pair-token handoff) — usually run by the installer, not by hand |
| `virtues doctor` | Hardware + inference resolution report |
| `virtues backup` / `virtues restore` | Snapshot + restore the box state |
| `virtues upgrade` | Self-update from the latest GitHub Release |

**When something breaks:** see [docs/recovery.md](docs/recovery.md) — covers
unreachable-box, lost-session, last-device-revoked, Postgres won't start,
restore from backup, BYO key reset, and more.

<a id="connect-from-another-machine-v02-preview"></a>
## <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h2-connect-from-another-machine-v0-2-preview-dark.svg"><img alt="Connect from another machine (v0.2 preview)" src=".github/images/headings/h2-connect-from-another-machine-v0-2-preview-light.svg" height="28"></picture>

Reach your box from anywhere — no tunnel, no port forwarding, no DNS name.
**There is no URL that reaches your box.** Reaching it requires holding a paired
Ed25519 key, not knowing an address.

Your box is an **iroh endpoint**, and its Ed25519 `EndpointId` *is* its identity —
mutual-key auth, no certificate authority. A paired device dials that identity
and iroh finds a path: direct on your LAN, hole-punched across NATs, or bounced
off our blind relay when neither works. The box holds an outbound connection to
the relay, so nothing inbound is ever opened at home. It works behind CGNAT,
coworking/café wifi, and IPv6-only ISPs — anywhere outbound 443 reaches, which is
everywhere. The relay moves sealed bytes it has no key to read. See
**[Privacy &amp; security model](docs/privacy-model.md)** and the
[visual walkthrough](docs/relay-walkthrough.html).

Pairing is **local-first**: you walk to the box, run `virtues pair` (or
`virtues device add`), and it prints a one-time code that puts the new device's
key on the allowlist. The box generates and keeps its own iroh secret, so its
identity is stable across restarts and nothing external mints it.

Access is enforced in two independent layers. iroh applies an `AllowPolicy` over
paired `EndpointId`s at the transport, so an unpaired key is refused *before HTTP
exists*; the app-layer bearer/cookie remains the authorization keystone on top.
Revocation is real — `virtues device rm <id>` de-allowlists a key and the next
dial is refused.

Day to day you don't type any of this. The mobile app and the desktop helper
(`virtues-reach-client`, which serves the box at `http://localhost:7117`) hold
the key and do the dialing.

**Honest scope today:**

- Reach needs a **paired client** that can hold a key — the mobile app or the
  desktop helper. An arbitrary browser on an unpaired machine cannot connect.
- On your home network the box is reachable directly on `:8000` without the
  relay.
- This replaces both the earlier WireGuard desktop tunnel and the per-box
  `*.virtues.ch` hostname, which have been removed.

<a id="development"></a>
## <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h2-development-dark.svg"><img alt="Development" src=".github/images/headings/h2-development-light.svg" height="28"></picture>

For contributors working on the codebase itself (not for running Virtues in production):

<a id="prerequisites"></a>
### <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h3-prerequisites-dark.svg"><img alt="Prerequisites" src=".github/images/headings/h3-prerequisites-light.svg" height="22"></picture>

- Rust 1.75+
- Node.js 18+ and pnpm
- Docker (for local S3 via MinIO, optional)

<a id="setup"></a>
### <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h3-setup-dark.svg"><img alt="Setup" src=".github/images/headings/h3-setup-light.svg" height="22"></picture>

```bash
git clone https://github.com/virtues-os/virtues
cd virtues
cp .env.example .env
# Edit .env with your API keys (see comments in .env.example)
```

<a id="run"></a>
### <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h3-run-dark.svg"><img alt="Run" src=".github/images/headings/h3-run-light.svg" height="22"></picture>

```bash
# Terminal 1: Start Core server
cd virtues-core && cargo run -- server

# Terminal 2: Build and serve web UI (production mode)
cd apps/web && pnpm install && pnpm build

# Or for development with hot reload:
cd apps/web && pnpm dev
```

Access: `http://localhost:8000` (Core serves the built web UI) or `http://localhost:5173` (dev server with hot reload).

<a id="virtues-api-required-for-ai-features"></a>
### <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h3-virtues-api-required-for-ai-features-dark.svg"><img alt="virtues-api (required for AI features)" src=".github/images/headings/h3-virtues-api-required-for-ai-features-light.svg" height="22"></picture>

```bash
# Terminal 3: Start virtues-api sidecar
cd services/virtues-api && cargo run
```

virtues-api runs on port 9002. Core connects to it via `VIRTUES_API_URL=http://localhost:9002`. See `.env.example` for required API keys (`AI_GATEWAY_API_KEY`, `VIRTUES_API_INTERNAL_SECRET`).

<a id="cloud-sidecar"></a>
## <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h2-cloud-sidecar-dark.svg"><img alt="Cloud sidecar" src=".github/images/headings/h2-cloud-sidecar-light.svg" height="28"></picture>

**Cloud (managed)**: Virtues Cloud provisions a dedicated, isolated instance for each user — your own server, your own database, your own encryption keys. No shared infrastructure, no pooled data. Managed by [Atlas](https://github.com/virtues-os/atlas), our open-source orchestration layer.

<a id="ios-app"></a>
## <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h2-ios-app-dark.svg"><img alt="iOS App" src=".github/images/headings/h2-ios-app-light.svg" height="28"></picture>

The companion app pairs with your box from `/virtues/devices → Add device` (scan the QR with the camera). See [docs/auth-model.md](docs/auth-model.md) for the pairing model. Source: the cross-platform Tauri app in `apps/web/src-tauri/` (macOS/Windows/Linux/iOS/Android), with on-device collection provided by the native plugins in `apps/web/plugins/`.

<a id="project-structure"></a>
## <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h2-project-structure-dark.svg"><img alt="Project Structure" src=".github/images/headings/h2-project-structure-light.svg" height="28"></picture>

```
virtues/
├── virtues-core/            # The heart: Rust daemon (crate `virtues`) — API, ingestion, agent, wiki, dayline
│   ├── src/
│   │   ├── agent/           # AI agent loop, prompts, tool execution
│   │   ├── api/             # HTTP route handlers
│   │   ├── applet_runner/   # Spawns & supervises applet subprocesses
│   │   ├── entity_resolution/  # People, places extraction from raw data
│   │   ├── dayline/         # Day-page scoring + autobiography
│   │   ├── storage/         # S3 and local filesystem abstraction
│   │   └── tools/           # AI tool implementations (SQL, search, code, pages)
│   └── migrations/          # Postgres schema migrations
├── applets/                 # Extension system: ingestion + behavior as subprocesses (functions/views)
├── crates/                  # Shared Rust libraries
│   ├── virtues-helpers/     # Shared helpers for applet authors (auth, db, crypto, wire contract)
│   ├── virtues-iroh/        # iroh transport: endpoint, AllowPolicy, serve/dial
│   ├── virtues-iroh-ffi/    # C ABI over virtues-iroh, for the mobile clients
│   ├── virtues-reach-client/# Reach proxy: holds the key, serves the box on localhost
│   ├── virtues-qnnd/        # Dragon NPU daemon (QNN, on-device embedding + rerank)
│   └── virtues-registry/    # Static config: models, sources, tools, personas, ontologies
├── services/                # Backend Rust services (the metered cloud edge)
│   ├── virtues-api/         # API proxy: LLM/web/bank/oauth gateway + per-user budgets
│   └── virtues-atlas/       # Identity, billing (Stripe), entitlement issuance
├── apps/                    # Client applications
│   ├── web/                 # SvelteKit web UI (static; served by the box) — also the Tauri iOS/desktop app
│   ├── desktop/             # Desktop helper: pair + localhost proxy over iroh (:7117)
│   ├── ios/                 # Vestigial xcodeproj shell; the live iOS app is the Tauri build of apps/web
│   └── mac-source/          # macOS data source: HealthKit / EventKit / activity collector
├── deploy/                  # Model-fetch + sandbox scripts (cloud Docker lives under services/)
├── tools/                   # bootstrap.sh + virtues-installer (virtues.com/sh)
├── vendor/                  # Vendored third-party sources
├── docs/                    # Architecture + concept docs (docs/archive/ holds superseded ones)
└── .data/                   # Gitignored runtime state (Postgres cluster, drive files)
```

<a id="database-schema"></a>
## <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h2-database-schema-dark.svg"><img alt="Database Schema" src=".github/images/headings/h2-database-schema-light.svg" height="28"></picture>

| Prefix | Purpose | Examples |
|--------|---------|----------|
| `elt_*` | Pipeline infrastructure | `elt_source_connections`, `elt_stream_connections` |
| `scheduled_tasks` / `task_runs` | Unified scheduler | Task config + execution history |
| `data_*` | Normalized ontology data | `data_health_heart_rate`, `data_communication_email`, `data_calendar_event` |
| `app_*` | Application state | `app_chat_sessions`, `app_user_profile` |
| `wiki_*` | Entity graph | `wiki_people`, `wiki_places`, `wiki_orgs`, `wiki_events` |
| `narrative_*` | Life narrative | `narrative_telos`, `narrative_acts`, `narrative_chapters` |

<a id="daily-context--scoring-system"></a>
## <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h2-daily-context-scoring-system-dark.svg"><img alt="Daily Context & Scoring System" src=".github/images/headings/h2-daily-context-scoring-system-light.svg" height="28"></picture>

The daily context system transforms raw ontology data into two measurable signals: **how completely a day is observed** (7-dimension coverage) and **how unusual a day is** (chaos/order score). Think of the chaos score as a **VIX for your persona** — a single number that captures the volatility of your daily experience relative to your recent baseline.

<a id="7-dimension-context-model"></a>
### <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h3-7-dimension-context-model-dark.svg"><img alt="7-Dimension Context Model" src=".github/images/headings/h3-7-dimension-context-model-light.svg" height="22"></picture>

Evolved from journalism's W5H framework, expanded to 7 dimensions by splitting "who" into self-awareness and relational resolution:

| Dim | Key | Meaning |
|-----|-----|---------|
| **Who** | `who` | Self-awareness — is the person's physical/digital state tracked? (health, location, device) |
| **Whom** | `whom` | Relational resolution — who else was involved? (messages, emails, calendar attendees) |
| **What** | `what` | Events & content — what happened? (transcriptions, calendar, documents) |
| **When** | `when` | Temporal coverage — how much of the 24h window is observed by continuous streams? |
| **Where** | `where` | Spatial awareness — do we know locations? (GPS points, named place visits) |
| **Why** | `why` | Intent & motivation — the rarest dimension, requires rich transcription or content data |
| **How** | `how` | Physical state — body metrics (sleep, workout, heart rate, HRV, steps) |

<a id="ontology-weight-matrix"></a>
### <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h3-ontology-weight-matrix-dark.svg"><img alt="Ontology Weight Matrix" src=".github/images/headings/h3-ontology-weight-matrix-light.svg" height="22"></picture>

Each of the 17 ontologies carries a 7-dimensional weight vector indicating how much it contributes to each context dimension. Weights follow a strict assignment principle: **0.0 unless the ontology genuinely informs that dimension**.

For example, `health_heart_rate` weights `[0.8, 0.0, 0.0, 0.8, 0.0, 0.0, 0.8]` — it tells you about self-awareness (who), temporal coverage (when), and physical state (how), but nothing about relationships, content, space, or intent. Meanwhile `communication_message` weights `[0.0, 1.0, 0.4, 0.0, 0.0, 0.0, 0.0]` — it's the strongest signal for relational resolution (whom) with modest content (what).

<a id="coverage-formula"></a>
### <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h3-coverage-formula-dark.svg"><img alt="Coverage Formula" src=".github/images/headings/h3-coverage-formula-light.svg" height="22"></picture>

For each of the 7 dimensions:

```
coverage[dim] = sum(weights[dim] for present ontologies) / sum(weights[dim] for ALL ontologies)
```

This produces a 0.0–1.0 score per dimension — the **ContextVector** displayed on each DayPage. A day with health data, location, and messages but no speech or knowledge will show high coverage in who/whom/when/where/how but low coverage in what/why.

<a id="daily-summary-generation"></a>
### <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h3-daily-summary-generation-dark.svg"><img alt="Daily Summary Generation" src=".github/images/headings/h3-daily-summary-generation-light.svg" height="22"></picture>

When "Generate Summary" is triggered on a DayPage, the system:

1. Gathers structured day sources (calendar, locations, transactions, messages, etc.)
2. Adds supplemental data: full transcription text, app usage, web browsing, knowledge documents, AI conversations
3. Builds a text prompt with all sections, truncated to fit token limits
4. Calls an LLM via virtues-api to generate a first-person daily narrative
5. Computes the 7-dim context vector from ontology data presence
6. Generates per-domain embeddings and computes the chaos/order score
7. Saves everything (autobiography, context vector, chaos score) to the wiki_days record

<a id="chaosorder-scoring"></a>
### <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h3-chaos-order-scoring-dark.svg"><img alt="Chaos/Order Scoring" src=".github/images/headings/h3-chaos-order-scoring-light.svg" height="22"></picture>

The chaos score measures how **novel** or **routine** a day is compared to your recent 30-day baseline.

**Algorithm:**

1. The day's data is grouped into 7 embedding domains: communication, calendar, health, location, financial, activity, content
2. Each domain's text content is embedded via virtues-api `/v1/embeddings` (text-embedding-3-small)
3. Each domain's embedding is compared to its **30-day exponentially-decayed centroid** via cosine similarity (decay rate: `exp(-0.1 * days_ago)`)
4. Per-domain chaos: `domain_chaos = 1 - similarity`
5. Domain chaos is **distributed across 7 dimensions** via the domain's ontology context weights
6. Final score: `chaos = sum(chaos[dim] * coverage[dim]) / sum(coverage[dim])`

The final normalization by coverage is the key insight: **sparse days don't appear artificially chaotic**. A day with only health data can't swing the chaos score wildly because its coverage is concentrated in just a few dimensions. The formula requires the chaos to be proportional to what was actually observed.

- **0.0** = Perfectly ordered/routine — every domain looks like your recent average
- **1.0** = Maximally chaotic/novel — every domain diverges from its centroid

<a id="domain-groupings"></a>
### <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h3-domain-groupings-dark.svg"><img alt="Domain Groupings" src=".github/images/headings/h3-domain-groupings-light.svg" height="22"></picture>

| Domain | Ontologies |
|--------|-----------|
| communication | communication_email, communication_message, communication_transcription |
| calendar | calendar_event |
| health | health_heart_rate, health_steps, health_sleep, health_workout, health_hrv |
| location | location_point, location_visit |
| financial | financial_transaction, financial_account |
| activity | activity_app_usage, activity_web_browsing |
| content | content_document, content_conversation, content_bookmark |

<a id="features"></a>
## <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h2-features-dark.svg"><img alt="Features" src=".github/images/headings/h2-features-light.svg" height="28"></picture>

**Spaces & Workspaces** — Arc-browser-style multi-space system. Each space has its own tabs, theme, and accent color. Swipeable sidebar carousel for switching between spaces. Organize your life into contexts — work, health, finance — each with its own look and layout.

**Tab System** — URL-native tab management with split-pane support. Tabs persist across sessions, serialize/deserialize automatically, and support side-by-side viewing. Every entity in the system has a URL, and every URL can be a tab.

**Rich Editor** — ProseMirror-based document editor with real-time collaboration via Yjs and WebSocket sync. Slash commands (`/`) for inserting blocks, `[[entity]]` linking for connecting to people/places/orgs, drag handles, table toolbar, code syntax highlighting (Shiki), markdown shortcuts, and media paste. IndexedDB persistence for offline support.

**AI Agent Modes** — Three distinct modes: **Agent** (full tool access — SQL, search, code, page editing), **Chat** (conversation only, no tools), and **Research** (read-only tools). Customizable personas let you shape the AI's behavior. Multi-model support across Claude, GPT, Gemini, and more.

**Semantic Search** — Two-stage retrieval pipeline: bi-encoder (nomic-embed, 768-dim) generates embeddings, then a cross-encoder reranker (BGE-reranker-v2-m3) re-scores results for precision. Per-ontology text extraction ensures every data type is searchable. Cmd+K modal for quick actions and cross-entity search.

**Entity Resolution** — Automatic extraction of people, places, and organizations from your raw data. The "Sarah" in your calendar, the "Sarah" in your contacts, and the "Sarah" in your messages all resolve to one person. Dedicated wiki pages for each entity type with specialized views.

**Smart Views & Manual Folders** — Smart views are query-based dynamic collections that auto-update as your data changes. Manual folders let you curate your own groupings. Three-level sidebar hierarchy: Section → Folder → Item, all with drag-and-drop reordering via SortableJS.

**Automated Autobiography** — Daily summaries generated from your data — calendar, messages, health, location, transactions, transcriptions. Temporal navigation by day and year. Narrative structure: Telos (life purpose) → Acts (multi-year arcs) → Chapters → Days. Hit "Generate Summary" and the system writes your day for you.

**W7H Context Score** — Every day is scored across 7 dimensions evolved from journalism's W5H framework: Who (self-awareness), Whom (relational resolution), What (events/content), When (temporal coverage), Where (spatial awareness), Why (intent/motivation), How (physical state). Each ontology carries a weight vector; coverage shows how completely a day is observed.

**Entropy Calculation** — A chaos/order score that measures how novel or routine your day is. Per-domain embeddings are compared against a 30-day exponentially-decayed centroid. The result is a single number — think VIX for your persona — normalized by coverage so sparse days don't appear artificially chaotic.

**Activity Heatmap** — GitHub-style contribution heatmap showing your data density over time. Visual at-a-glance view of which days have rich context and which are sparse.

**Movement Map** — Leaflet-based location visualization on day pages. See your geographic movement throughout the day rendered on an interactive map.

**Real-Time Collaboration** — Yjs CRDT backend with WebSocket sync. Multiple clients can edit the same page simultaneously with automatic conflict resolution. Version history with save/restore. Y.UndoManager for undo/redo.

**Drive & Trash** — Personal file storage with S3 backend. Folder hierarchy, drag-and-drop upload, breadcrumb navigation, storage quotas. Soft delete moves files to trash with restore and permanent purge options.

**macOS Source Collector** — Swift LaunchAgent (`apps/mac-source/`) that streams app usage, browser history, and iMessage data from a Mac into your box. Manages Full Disk Access and Accessibility permissions. Pairs over the desktop helper's tunnel.

**SSH into Your Server** — Built-in terminal for direct server access from the web UI. Developer tools include an interactive SQL console, data lake browser, task/run inspector, and sitemap viewer.

**Feedback & Changelog** — Built-in feedback submission and a changelog view for tracking what's new. Onboarding checklist guides new users through connecting sources, pairing devices, and starting their first chat.

<a id="license"></a>
## <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h2-license-dark.svg"><img alt="License" src=".github/images/headings/h2-license-light.svg" height="28"></picture>

Virtues uses a hybrid model:

- **Server, web app, and infrastructure** — [Business Source License 1.1](LICENSE) (BUSL-1.1): source-available, free to self-host for personal or internal organizational use, **not** for offering a hosted service or commercial hardware product. Each file converts to Apache 2.0 four years after release.
- **Native apps and the data model** — MIT (see the `LICENSE` file in those directories, e.g. `apps/web/plugins/`, `apps/mac-source/`).

The repository default is BUSL-1.1 unless a directory contains its own `LICENSE` stating otherwise.

---

<p align="center"><i>Your data. Your infrastructure. Your narrative.</i></p>
