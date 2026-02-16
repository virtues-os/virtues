![Virtues](.github/images/cover3.png)

# Virtues

A private intelligence that connects your digital life — health, finance, location, conversations — into a coherent, queryable picture of who you are. Self-hosted or cloud.

> **Status**: Public beta. Actively developed. Expect rough edges.

[![License: MIT + ELv2](https://img.shields.io/badge/License-MIT%20%2B%20ELv2-blue.svg)](LICENSE)
[![Discord](https://img.shields.io/badge/Discord-Join%20Us-7289da?logo=discord&logoColor=white)](https://discord.gg/sSQKzDWqgv)

## What It Does

Virtues replaces a fragmented app ecosystem with a single, unified system:

- **Ingest** your data from APIs (Google, Notion, Plaid, Strava, GitHub) and devices (iOS sensors, Mac activity)
- **Build** a living knowledge graph — people, places, organizations, events — linked to your raw data
- **Write** an autobiography that maintains itself — daily summaries, narrative arcs, temporal navigation
- **Query** your life with an AI that has real context — not a chatbot guessing, but an agent with access to your actual data via SQL, web search, and code execution

All of it runs on a single Rust server with a SQLite database and S3 storage. Your data stays on your infrastructure.

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
│  Storage: SQLite (metadata + ontologies) · S3 (raw streams) │
└──────────────────────┬──────────────────────────────────────┘
                       ▼
┌──────────────────────────────────────────────────────────────┐
│  Tollbooth (Rust sidecar · port 9002)                       │
│  API proxy with per-user budget enforcement                 │
│  Routes to 100+ LLM providers via Vercel AI Gateway         │
│  Holds all external API keys (AI, Exa, Plaid, Google, etc.) │
└──────────────────────────────────────────────────────────────┘
```

**Core** handles data ingestion, entity resolution, the wiki, pages, chat, and serves the web UI. **Tollbooth** is a sidecar proxy that mediates all external API calls — LLM requests, web search, bank connections — with budget tracking and key isolation. Core never touches API keys directly.

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

Extensible: implement the `Stream` trait in `core/src/sources/{provider}/` to add new sources.

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

## Quick Start

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
cd core && cargo run -- server

# Terminal 2: Build and serve web UI (production mode)
cd apps/web && pnpm install && pnpm build

# Or for development with hot reload:
cd apps/web && pnpm dev
```

Access: `http://localhost:8000` (Core serves the built web UI) or `http://localhost:5173` (dev server with hot reload).

### Tollbooth (required for AI features)

```bash
# Terminal 3: Start Tollbooth sidecar
cd apps/tollbooth && cargo run
```

Tollbooth runs on port 9002. Core connects to it via `TOLLBOOTH_URL=http://localhost:9002`. See `.env.example` for required API keys (`AI_GATEWAY_API_KEY`, `TOLLBOOTH_INTERNAL_SECRET`).

## Deployment

**Self-hosted**: Run Core + Tollbooth on any machine. SQLite for the database, local filesystem or S3 for storage. Single binary, no external dependencies beyond what you choose to connect.

**Cloud (managed)**: Virtues Cloud provisions a dedicated, isolated instance for each user — your own server, your own database, your own encryption keys. No shared infrastructure, no pooled data. Managed by [Atlas](https://github.com/virtues-os/atlas), our open-source orchestration layer.

## iOS App

The iOS companion app streams real-time sensor data to your Virtues instance:

```bash
# Expose your local server via HTTPS for iOS development
cd core && cargo run -- tunnel
```

The tunnel command starts a Cloudflare quick tunnel and auto-sets `BACKEND_URL` to the generated HTTPS URL. Requires `brew install cloudflared`. See `apps/ios/` for the Xcode project.

## Project Structure

```
virtues/
├── core/                    # Rust backend (API server, ingestion, AI agent, wiki)
│   ├── src/
│   │   ├── agent/           # AI agent loop, prompts, tool execution
│   │   ├── api/             # HTTP route handlers
│   │   ├── sources/         # Data source implementations (Google, iOS, Plaid, etc.)
│   │   ├── entity_resolution/  # People, places extraction from raw data
│   │   ├── storage/         # S3 and local filesystem abstraction
│   │   └── tools/           # AI tool implementations (SQL, search, code, pages)
│   └── migrations/          # SQLite schema migrations
├── apps/
│   ├── web/                 # SvelteKit web UI
│   ├── tollbooth/           # Rust API proxy sidecar
│   └── ios/                 # iOS companion app (Swift)
├── packages/
│   └── virtues-registry/    # Shared Rust crate (models, sources, tools, personas)
└── deploy/                  # Sandbox runtime config
```

## Database Schema

| Prefix | Purpose | Examples |
|--------|---------|----------|
| `elt_*` | Pipeline infrastructure | `elt_source_connections`, `elt_stream_connections`, `elt_jobs` |
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
4. Calls an LLM via Tollbooth to generate a first-person daily narrative
5. Computes the 7-dim context vector from ontology data presence
6. Generates per-domain embeddings and computes the chaos/order score
7. Saves everything (autobiography, context vector, chaos score) to the wiki_days record

### Chaos/Order Scoring

The chaos score measures how **novel** or **routine** a day is compared to your recent 30-day baseline.

**Algorithm:**

1. The day's data is grouped into 7 embedding domains: communication, calendar, health, location, financial, activity, content
2. Each domain's text content is embedded via Tollbooth `/v1/embeddings` (text-embedding-3-small)
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

## Features (last updated: Feb 12)

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

**macOS Desktop App** — Tauri-based native app with a collector daemon that runs as a LaunchAgent. Streams app usage, browser history, and iMessage data in the background. Manages Full Disk Access and Accessibility permissions. Pairs with your server instance via a 6-digit code.

**SSH into Your Server** — Built-in terminal for direct server access from the web UI. Developer tools include an interactive SQL console, data lake browser, job inspector, and sitemap viewer.

**Feedback & Changelog** — Built-in feedback submission and a changelog view for tracking what's new. Onboarding checklist guides new users through connecting sources, pairing devices, and starting their first chat.

## License

MIT (core components) + Elastic License v2 (enterprise features). Self-host freely. See [LICENSE](LICENSE) for details.

---

<p align="center"><i>Your data. Your infrastructure. Your narrative.</i></p>
