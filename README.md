![Virtues](.github/images/cover3.png)

# Virtues

A private intelligence that connects your digital life — health, finance, location, conversations — into a coherent, queryable picture of who you are. Self-hosted or cloud.

> **Status**: v1 — single-user, **pair-only auth** (no passwords, no email, no
> magic links). The only way in is to walk to the box. LAN-first by default;
> the v0.2 desktop daemon (`virtues-client`, Linux preview) pairs over
> WireGuard so any browser on a paired machine sees the box at
> `http://localhost:8000`. Expect rough edges.

[![License: BUSL-1.1 + MIT](https://img.shields.io/badge/License-BUSL--1.1%20%2B%20MIT-blue.svg)](LICENSE)
[![Discord](https://img.shields.io/badge/Discord-Join%20Us-7289da?logo=discord&logoColor=white)](https://discord.gg/sSQKzDWqgv)

## What It Does

Virtues replaces a fragmented app ecosystem with a single, unified system:

- **Ingest** your data from APIs (Google, Notion, Plaid, Strava, GitHub) and devices (iOS sensors, Mac activity)
- **Build** a living knowledge graph — people, places, organizations, events — linked to your raw data
- **Write** an autobiography that maintains itself — daily summaries, narrative arcs, temporal navigation
- **Query** your life with an AI that has real context — not a chatbot guessing, but an agent with access to your actual data via SQL, web search, and code execution

All of it runs on a single Rust server with a Postgres database and S3 storage. Your data stays on your infrastructure.

## Architecture

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

## Data Sources

| Source | Streams | Method |
|--------|---------|--------|
| Google | Calendar, Gmail | OAuth |
| Notion | Pages | OAuth |
| Plaid | Transactions, Accounts, Investments, Liabilities | OAuth |
| Strava | Activities | OAuth |
| GitHub | Events | OAuth |
| iOS | HealthKit, Location, Microphone, Contacts, FinanceKit, EventKit | Device |
| macOS | Apps, Browser, iMessage | Device |

Extensible: add a new source as an action in `actions/<name>/` with a `manifest.toml` — see [`actions/AUTHORING.md`](actions/AUTHORING.md).

## Overview

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

## Install (Linux home server)

### Requirements

| | |
|---|---|
| **Host OS** | Debian 13+, Ubuntu 24.04 LTS+, or Fedora 40+. Debian 13 and Ubuntu 26.04+ ship Postgres 18 natively; on Ubuntu 24.04/25.04 the installer adds the [PGDG repo](https://www.postgresql.org/download/linux/) automatically. x86_64 or aarch64. |
| **Hardware** | 8 GB RAM, an SSD. GPU optional. |
| **Network** | Standard residential ISP. v1 is LAN-first. The web UI is reachable from a browser on the box itself (Chromium on the Jetson → `http://localhost:8000`) or from any machine running the v0.2 desktop daemon (see [Connect from another machine](#connect-from-another-machine-v02-preview) below). Linux client only in v0.2; macOS lands in v0.2.2. |
| **Mac / Windows** | Not supported as host — Virtues needs root, native Postgres, and full SSD ownership. Use a Linux box. |

### Install in one command

```bash
curl -sSL https://get.virtues.com | sudo sh
```

That:
- Downloads the latest `virtues` binary into `/usr/local/bin/`
- Installs Postgres 18 + pgvector, Avahi (mDNS), and the rest of the system deps via your package manager
- Configures `/etc/avahi/services/virtues.service` so the box advertises itself on the LAN as `virtues.local`
- Mints the box's WG identity (its SPKI fingerprint) and rendezvous identity, and enables the `virtues.service` systemd unit
- Prints a one-time URL — open it in Chromium on the Jetson to land in a logged-in session

```bash
sudo systemctl enable --now virtues
sudo -u virtues virtues link   # prints the one-time login URL for the box's browser
```

After that you're in the web UI on `http://localhost:8000` (run Chromium on the Jetson). Connect a source, and optionally `sudo -u virtues virtues subscribe` to enable AI chat through the Virtues cloud (or set up a [BYO provider key](docs/auth-model.md) under Settings).

| Command | What it does |
|---|---|
| `virtues link` | Print a one-time URL to log in to the web UI |
| `virtues sudo` | Approve a pending sensitive action from a paired browser |
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

## Connect from another machine (v0.2 preview)

The desktop daemon (`virtues-client`) pairs a Linux laptop to your box over
WireGuard and exposes the box's web UI on `http://localhost:8000` — a Secure
Context origin with no cert warnings.

```bash
# On the laptop, one-time setup
curl -L -o virtues-client \
  https://github.com/virtues-os/virtues/releases/latest/download/virtues-client-$(uname -m)-linux
chmod +x virtues-client && sudo mv virtues-client /usr/local/bin/
sudo setcap cap_net_admin+ep /usr/local/bin/virtues-client

# On the box, mint a pair URL
sudo -u virtues virtues link    # copy the printed https://…/pair#t=… URL

# On the laptop, pair + bring the tunnel up
virtues-client pair "<paste-pair-url>"
sudo virtues-client up          # → "proxy listening on http://localhost:8000"
```

Open `http://localhost:8000` in any browser on the laptop and you're talking
to the box.

**Honest scope of the v0.2 preview:**

- Linux client only. macOS lands in v0.2.2.
- The WireGuard server-side daemon (`virtues-wireguard`) is **v0.2.1 work** —
  pair succeeds today, but the tunnel won't reach the box until that lands.
- Strict-symmetric NAT (mostly enterprise) is not supported; cone NAT works.

## Development

For contributors working on the codebase itself (not for running Virtues in production):

### Prerequisites

- Rust 1.75+
- Node.js 18+ and pnpm
- Docker (for local S3 via MinIO, optional)

### Setup

```bash
git clone https://github.com/virtues-os/virtues
cd virtues
cp .env.example .env
# Edit .env with your API keys (see comments in .env.example)
```

### Run

```bash
# Terminal 1: Start Core server
cd virtues-core && cargo run -- server

# Terminal 2: Build and serve web UI (production mode)
cd apps/web && pnpm install && pnpm build

# Or for development with hot reload:
cd apps/web && pnpm dev
```

Access: `http://localhost:8000` (Core serves the built web UI) or `http://localhost:5173` (dev server with hot reload).

### virtues-api (required for AI features)

```bash
# Terminal 3: Start virtues-api sidecar
cd services/virtues-api && cargo run
```

virtues-api runs on port 9002. Core connects to it via `VIRTUES_API_URL=http://localhost:9002`. See `.env.example` for required API keys (`AI_GATEWAY_API_KEY`, `VIRTUES_API_INTERNAL_SECRET`).

## Cloud sidecar

**Cloud (managed)**: Virtues Cloud provisions a dedicated, isolated instance for each user — your own server, your own database, your own encryption keys. No shared infrastructure, no pooled data. Managed by [Atlas](https://github.com/virtues-os/atlas), our open-source orchestration layer.

## iOS App

The iOS companion app pairs with your box from `/virtues/devices → Add device` (scan the QR with the camera). See [docs/auth-model.md](docs/auth-model.md) for the pairing model. Source: `apps/ios/`.

## Project Structure

```
virtues/
├── virtues-core/            # The heart: Rust daemon (crate `virtues`) — API, ingestion, agent, wiki, dayline
│   ├── src/
│   │   ├── agent/           # AI agent loop, prompts, tool execution
│   │   ├── api/             # HTTP route handlers
│   │   ├── action_runner/   # Spawns & supervises action subprocesses
│   │   ├── entity_resolution/  # People, places extraction from raw data
│   │   ├── dayline/         # Day-page scoring + autobiography
│   │   ├── storage/         # S3 and local filesystem abstraction
│   │   └── tools/           # AI tool implementations (SQL, search, code, pages)
│   └── migrations/          # Postgres schema migrations
├── actions/                 # Extension system: ingestion + behavior as supervised subprocesses (functions/services/views)
├── crates/                  # Shared Rust libraries
│   ├── virtues-helpers/     # Shared helpers for action authors (auth, db, crypto, wire contract)
│   ├── virtues-wg/          # WireGuard engine (privileged daemon)
│   └── virtues-registry/    # Static config: models, sources, tools, personas, ontologies
├── services/                # Backend Rust services (the metered cloud edge)
│   ├── virtues-api/         # API proxy: LLM/web/bank/oauth gateway + per-user budgets
│   └── atlas/               # Identity, billing (Stripe), entitlement issuance
├── apps/                    # Client applications
│   ├── web/                 # SvelteKit web UI (static; served by the box)
│   ├── desktop/             # virtues-client daemon: pair + WG tunnel + localhost proxy (Linux; macOS/Windows planned)
│   ├── ios/                 # iOS companion app (Swift)
│   └── mac-source/          # macOS data source: HealthKit / EventKit / activity collector
├── deploy/                  # Model-fetch + sandbox scripts (cloud Docker lives under services/)
├── tools/                   # bootstrap.sh + virtues-installer (get.virtues.com)
├── docs/                    # Architecture + concept docs (flat)
└── .data/                   # Gitignored runtime state (Postgres cluster, drive files)
```

## Database Schema

| Prefix | Purpose | Examples |
|--------|---------|----------|
| `elt_*` | Pipeline infrastructure | `elt_source_connections`, `elt_stream_connections` |
| `scheduled_tasks` / `task_runs` | Unified scheduler | Task config + execution history |
| `data_*` | Normalized ontology data | `data_health_heart_rate`, `data_communication_email`, `data_calendar_event` |
| `app_*` | Application state | `app_chat_sessions`, `app_user_profile` |
| `wiki_*` | Entity graph | `wiki_people`, `wiki_places`, `wiki_orgs`, `wiki_events` |
| `narrative_*` | Life narrative | `narrative_telos`, `narrative_acts`, `narrative_chapters` |

## Daily Context & Scoring System

The daily context system transforms raw ontology data into two measurable signals: **how completely a day is observed** (7-dimension coverage) and **how unusual a day is** (chaos/order score). Think of the chaos score as a **VIX for your persona** — a single number that captures the volatility of your daily experience relative to your recent baseline.

### 7-Dimension Context Model

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

### Ontology Weight Matrix

Each of the 17 ontologies carries a 7-dimensional weight vector indicating how much it contributes to each context dimension. Weights follow a strict assignment principle: **0.0 unless the ontology genuinely informs that dimension**.

For example, `health_heart_rate` weights `[0.8, 0.0, 0.0, 0.8, 0.0, 0.0, 0.8]` — it tells you about self-awareness (who), temporal coverage (when), and physical state (how), but nothing about relationships, content, space, or intent. Meanwhile `communication_message` weights `[0.0, 1.0, 0.4, 0.0, 0.0, 0.0, 0.0]` — it's the strongest signal for relational resolution (whom) with modest content (what).

### Coverage Formula

For each of the 7 dimensions:

```
coverage[dim] = sum(weights[dim] for present ontologies) / sum(weights[dim] for ALL ontologies)
```

This produces a 0.0–1.0 score per dimension — the **ContextVector** displayed on each DayPage. A day with health data, location, and messages but no speech or knowledge will show high coverage in who/whom/when/where/how but low coverage in what/why.

### Daily Summary Generation

When "Generate Summary" is triggered on a DayPage, the system:

1. Gathers structured day sources (calendar, locations, transactions, messages, etc.)
2. Adds supplemental data: full transcription text, app usage, web browsing, knowledge documents, AI conversations
3. Builds a text prompt with all sections, truncated to fit token limits
4. Calls an LLM via virtues-api to generate a first-person daily narrative
5. Computes the 7-dim context vector from ontology data presence
6. Generates per-domain embeddings and computes the chaos/order score
7. Saves everything (autobiography, context vector, chaos score) to the wiki_days record

### Chaos/Order Scoring

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

### Domain Groupings

| Domain | Ontologies |
|--------|-----------|
| communication | communication_email, communication_message, communication_transcription |
| calendar | calendar_event |
| health | health_heart_rate, health_steps, health_sleep, health_workout, health_hrv |
| location | location_point, location_visit |
| financial | financial_transaction, financial_account |
| activity | activity_app_usage, activity_web_browsing |
| content | content_document, content_conversation, content_bookmark |

## Features

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

**macOS Source Collector** — Swift LaunchAgent (`apps/mac-source/`) that streams app usage, browser history, and iMessage data from a Mac into your box. Manages Full Disk Access and Accessibility permissions. Pairs over the v0.2 desktop daemon's tunnel.

**SSH into Your Server** — Built-in terminal for direct server access from the web UI. Developer tools include an interactive SQL console, data lake browser, task/run inspector, and sitemap viewer.

**Feedback & Changelog** — Built-in feedback submission and a changelog view for tracking what's new. Onboarding checklist guides new users through connecting sources, pairing devices, and starting their first chat.

## License

Virtues uses a hybrid model:

- **Server, web app, and infrastructure** — [Business Source License 1.1](LICENSE) (BUSL-1.1): source-available, free to self-host for personal or internal organizational use, **not** for offering a hosted service or commercial hardware product. Each file converts to Apache 2.0 four years after release.
- **Native apps and the data model** — MIT (see the `LICENSE` file in those directories, e.g. `apps/ios/`, `apps/mac-source/`).

The repository default is BUSL-1.1 unless a directory contains its own `LICENSE` stating otherwise.

---

<p align="center"><i>Your data. Your infrastructure. Your narrative.</i></p>
