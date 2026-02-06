![Virtues](.github/images/cover3.png)

# Virtues

A private intelligence that connects your digital life — health, finance, location, conversations — into a coherent, queryable picture of who you are. Self-hosted or cloud.

> **Status**: Public beta. Actively developed. Expect rough edges.

[![License: MIT + ELv2](https://img.shields.io/badge/License-MIT%20%2B%20ELv2-blue.svg)](LICENSE)
[![Discord](https://img.shields.io/badge/Discord-Join%20Us-7289da?logo=discord&logoColor=white)](https://discord.gg/sSQKzDWqgv)

## What It Does

Virtues replaces a fragmented app ecosystem with a single, unified system:

- **Ingest** your data from APIs (Google, Notion, Plaid) and devices (iOS sensors, Mac activity)
- **Build** a living knowledge graph — people, places, organizations, events — linked to your raw data
- **Write** an autobiography that maintains itself — daily summaries, narrative arcs, temporal navigation
- **Query** your life with an AI that has real context — not a chatbot guessing, but an agent with access to your actual data via SQL, web search, and code execution

All of it runs on a single Rust server with a SQLite database and S3 storage. Your data stays on your infrastructure.

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│  Sources                                                    │
│  OAuth: Google Calendar · Notion · Plaid                    │
│  Device: HealthKit · Location · Microphone · Contacts       │
│          Barometer · Battery · FinanceKit · EventKit        │
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
| Google | Calendar | OAuth |
| Notion | Pages | OAuth |
| Plaid | Transactions, Accounts | OAuth |
| iOS | HealthKit, Location, Microphone, Contacts, Battery, Barometer, FinanceKit, EventKit | Device |
| macOS | Apps, Browser, iMessage | Device |

Extensible: implement the `Stream` trait in `core/src/sources/{provider}/` to add new sources.

## Features

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
cd core && cargo run -- ngrok
```

The ngrok command outputs an HTTPS URL. Enter it in the iOS app settings to pair your device. See `apps/ios/` for the Xcode project.

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
└── deploy/                  # Nomad job specs, cluster config, deployment scripts
```

## Database Schema

| Prefix | Purpose | Examples |
|--------|---------|----------|
| `elt_*` | Pipeline infrastructure | `elt_source_connections`, `elt_stream_connections`, `elt_jobs` |
| `data_*` | Normalized ontology data | `data_health_heart_rate`, `data_location_point`, `data_financial_transaction` |
| `app_*` | Application state | `app_chat_sessions`, `app_user_profile` |
| `wiki_*` | Entity graph | `wiki_people`, `wiki_places`, `wiki_orgs`, `wiki_events` |
| `narrative_*` | Life narrative | `narrative_telos`, `narrative_acts`, `narrative_chapters` |

## License

MIT (core components) + Elastic License v2 (enterprise features). Self-host freely. See [LICENSE](LICENSE) for details.

---

<p align="center"><i>Your data. Your infrastructure. Your narrative.</i></p>
