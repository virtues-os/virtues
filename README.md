![Virtues Cover](.github/images/cover3.png)

# Virtues

Personal data ELT platform. Extract data from Google, iOS, Mac, Notion → Store in SQLite + S3 → Query with SQL.

> **Status**: Active development, beware. Things will break.

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Discord](https://img.shields.io/badge/Discord-Join%20Us-7289da?logo=discord&logoColor=white)](https://discord.gg/sSQKzDWqgv)

## What It Is

Virtues is a single-user ELT pipeline for personal data:

- **Extract**: Pull from APIs (Google, Notion) and devices (iOS, Mac)
- **Load**: Store raw streams in SQLite + S3 with full fidelity
- **Transform**: Normalize into ontologies for cross-source analysis

Self-hosted, open source, Rust-based. Your data stays on your infrastructure.

## Why Monolithic Rust?

Unlike enterprise tools (Airbyte: Docker per source), Virtues uses **one Rust package** for all sources:

**Personal data is different:**

- **User experience first**: One person manages this (not a full-time ELT team). Must be maintain-less and simple.
- **Device coupling**: iOS/Mac apps need direct hardware access (HealthKit, Location, Microphone) that can't run in containers.
- **Real-time hot pipeline**: Sub-second latency for streaming data enables proactive personal AI (future).
- **Single-user**: No multi-tenancy overhead = simpler code, better performance.

**Extensibility**: Implement `Stream` trait in `core/src/sources/{provider}/` for custom sources.

## Implementation Status

| Source | Stream | Status |
|--------|--------|--------|
| Virtues | App Export | ✅ |
| Google | Calendar | ✅ |
| Google | Gmail | ✅ |
| iOS | HealthKit | ✅ |
| iOS | Location | ✅ |
| iOS | Microphone | ✅ |
| Mac | Apps | ✅ |
| Mac | Browser | ✅ |
| Mac | iMessage | ✅ |
| Notion | Pages | ✅ |

## Quick Start

```bash
# Clone and setup
git clone https://github.com/virtues-os/virtues
cd virtues

# Start everything (infrastructure + migrations + servers)
make dev

# In separate terminals, run:
cd apps/web && npm run build      # Build web app for production serving
cd core && cargo run -- server    # Terminal 1: API server (serves static files too)

# Optional: For hot reload during web development
cd apps/web && npm run dev        # Terminal 2: Web UI dev server (optional)
```

Access: `http://localhost:8000` (backend serves web UI) | `http://localhost:5173` (dev server - optional)

**First time setup**: `make dev` handles everything - Docker containers, database migrations, and SQLx cache generation.

## Architecture

```
Sources (OAuth: Google, Notion | Device: iOS, Mac)
   ↓
Ingest API / StreamWriter
   ↓
Storage (S3: raw JSONL streams | SQLite: metadata + ontologies)
   ↓
Ontologies (normalized domain primitives: health_*, location_*, social_*, etc.)
   ↓
Query with SQL
```

**Extract:**

- **Source** = Data origin (e.g., iOS, Google, Notion)
- **Stream** = Raw, non-normalized data from a source (e.g., iOS → healthkit, location, microphone)

**Load:**

- **Storage** = S3 for raw JSONL streams, SQLite for metadata + ontology tables

**Transform:**

- **Ontologies** = Normalized domain tables for cross-source analysis (e.g., health_heart_rate, location_point, social_email)

**Core**: Rust library with OAuth, schedulers, and device processors
**Clients**: iOS/Mac apps push real-time data to ingestion server

## Database Schema

### Table Naming Convention

| Prefix | Purpose | Examples |
|--------|---------|----------|
| `elt_*` | ELT pipeline infrastructure | `elt_source_connections`, `elt_stream_connections`, `elt_jobs` |
| `data_*` | Normalized ontology data | `data_health_heart_rate`, `data_location_point`, `data_financial_transaction` |
| `app_*` | Application config and state | `app_models`, `app_agents`, `app_user_profile`, `app_chat_sessions` |
| `wiki_*` | Entity graph | `wiki_people`, `wiki_places`, `wiki_orgs`, `wiki_things` |
| `narrative_*` | Narrative structure | `narrative_telos`, `narrative_acts`, `narrative_chapters` |

This convention separates concerns:
- **`elt_*`** tables are pipeline plumbing (sources, streams, jobs, checkpoints)
- **`data_*`** tables hold the actual user data (your ontologies)
- **`app_*`** tables manage application state (models, agents, user preferences)
- **`wiki_*`** tables form the entity graph (people, places, organizations)
- **`narrative_*`** tables structure your life story

## Development

```bash
# Start development environment
make dev              # Runs migrations and starts services

# Run tests
make test-rust        # Rust tests
make test-web         # Web tests

# Database commands
make migrate          # Run migrations
make prepare          # Regenerate SQLx cache (after schema changes)
make db-reset         # Reset database (WARNING: deletes data)

# View all commands
make help
```

### Testing Onboarding

When testing the onboarding flow, you can reset your user status directly from the CLI without wiping your entire database:

```bash
# Reset onboarding status to 'welcome' (keeps your data)
cd core && cargo run -p virtues -- onboarding reset

# Full Reset: Back to 'welcome' AND wipe user-generated data (telos, aspirations, sources, etc.)
cd core && cargo run -p virtues -- onboarding reset --full
```

## OAuth Testing Workflow

### Production-like Testing (Recommended)

For production-like OAuth flows where the backend serves the static web app:

1. Build the web app:
   ```bash
   cd apps/web && npm run build
   ```

2. Start the backend (serves static files from `apps/web/build`):
   ```bash
   cd core && cargo run -- server
   ```

3. Access the app at `http://localhost:8000`

4. OAuth flow:
   - Navigate to `http://localhost:8000/data/sources/add`
   - Authorize with Google/Notion
   - Google redirects to `http://localhost:8000/oauth/callback` (backend)
   - Backend processes tokens and returns HTML redirect
   - Browser redirects to `http://localhost:8000/data/sources/add?source_id=...&connected=true`

### Development with Hot Reload

For active web development with hot reload:

1. Start backend server:
   ```bash
   cd core && cargo run -- server
   ```

2. Start web dev server in a separate terminal:
   ```bash
   cd apps/web && npm run dev
   ```

3. Access the app at `http://localhost:5173` for hot reload

4. OAuth flow:
   - Navigate to `http://localhost:5173/data/sources/add`
   - Note: OAuth callbacks will still redirect to `http://localhost:8000/oauth/callback` (backend)
   - The dev server won't receive OAuth callbacks - the backend handles them directly
   - After OAuth, you'll be redirected to the production server at `http://localhost:8000`

**Important**: When using the dev server with hot reload, the OAuth flow will redirect you to the production backend (8000). This is expected behavior as the backend handles all OAuth callbacks in the backend-first paradigm.

**Requirements**:

- Docker & Docker Compose (for infrastructure)
- Rust 1.70+ (for core server)
- Node.js 18+ (for web UI)
- Make (for build commands)

## iOS/Mac Development with ngrok

iOS apps require HTTPS connections for production use. For local development, use ngrok to expose your Rust backend via a secure HTTPS tunnel:

```bash
# Install ngrok (one-time setup)
brew install ngrok

# Sign up for free account and add your authtoken
ngrok config add-authtoken YOUR_TOKEN

# Start Rust server with ngrok tunnel (automatically starts both services)
cd core && cargo run -- ngrok
```

**Get your HTTPS URL:**

The ngrok command will display your HTTPS URL in the terminal, for example:
```
🌐 HTTPS URL: https://abc123.ngrok-free.app
```

Use this URL in your iOS/Mac app settings.

**Note:** Free ngrok URLs change each time you restart. For persistent URLs, upgrade to a paid ngrok plan.

**Alternative commands:**

Start server separately without ngrok:
```bash
cd core && cargo run -- server
```

Start ngrok manually:
```bash
ngrok http 8000
```

## License

MIT (core library) + Elastic License v2 (ML modules)

See [LICENSE](LICENSE) for details.

---

<p align="center">
  <i>Your data. Your infrastructure. Your life.</i>
</p>
