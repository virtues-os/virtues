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

Two ways to run it. **Do it yourself** is the software on your own Linux
machine, installed with one command — the path you can take today, and the one
this README is written for. **The appliance** is the same binary on a small
board we flash, with a screen on the front; it is what we are building toward
and not yet something you can buy. Same binary either way, and you can always
leave with your data.

> **Status: early, in the way that word should mean.** Stable releases ship on
> the `virtues.com/sh` channel and prereleases on `sh-pre`; the
> [releases page](https://github.com/virtues-os/virtues/releases) is the only
> honest statement of what exists at any given moment, so read it there rather
> than here. Boxes do not update themselves — an upgrade is a command you run.
> The appliance path (flashing, first boot, pairing over Bluetooth, the case
> button) has been walked end to end on real boards; DIY is the path under
> daily use.
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
there. Search runs on your side of the wire — the embedding and reranking
endpoints must be local, and the installer refuses a public address for either
(see [Setting up inference](docs/inference.md)). GPS is reduced to distance and
pace before anything is sent anywhere.

The exception is the assistant, and it is not a leak, it is the feature: asking a
question, or letting the box write your day, sends the relevant part of your
record to a model provider — and a voice recording is sent as **audio**, not as a
transcript, to be transcribed. Point Virtues at a local model and that stops too;
the trade is quality and speed, and it is yours to make.

We will not tell you this is private by construction. The relay genuinely cannot
read your data — that is physics. The inference boundary rests on a contract with
a provider, which is a different kind of promise, and
[the privacy model](agents/record/privacy-model.md#the-inference-boundary-where-your-data-does-leave)
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
                       ├────────────────► ┌──────────────────────────────────┐
                       │                  │ Inference endpoints — YOURS      │
                       │                  │ /v1/embeddings  :18181           │
                       │                  │ /v1/rerank      :18182           │
                       │                  │ loopback / LAN / VPN only        │
                       │                  └──────────────────────────────────┘
                       ▼
┌──────────────────────────────────────────────────────────────┐
│  virtues-api (Rust sidecar · port 9002)                       │
│  API proxy with per-user budget enforcement                 │
│  Routes to 100+ LLM providers via Vercel AI Gateway         │
│  Holds all external API keys (AI, search, Plaid, Google)    │
└──────────────────────────────────────────────────────────────┘
```

**Core** handles data ingestion, entity resolution, the wiki, pages, chat, and serves the web UI. **virtues-api** is a sidecar proxy that mediates all external API calls — LLM requests, web search, bank connections — with budget tracking and key isolation. Core never touches API keys directly.

**Retrieval is two HTTP contracts, not code we ship.** Core consumes an OpenAI-style `/v1/embeddings` endpoint (required) and a `/v1/rerank` endpoint (optional), and cannot tell what is behind them. On our own board the installer provisions them on the NPU; on your machine you run them, and the installer probes what you point it at and pins the model's fingerprint so a silent model swap can't quietly corrupt your index. That is the whole composability story — see [Setting up inference](docs/inference.md).

**Remote access** has no public surface at all. The box is an iroh endpoint whose Ed25519 key *is* its identity, so a paired device reaches it by key — LAN-direct, hole-punched, or bounced off our relay — with no inbound port opened at home and no hostname anyone can type. The relay forwards sealed, end-to-end-encrypted bytes it has no key to read; it does see which two keys are talking, the addresses they connect from, and how much traffic passes. See **[Privacy &amp; security model](agents/record/privacy-model.md)** (who holds which secret, and who deliberately doesn't) and the [visual walkthrough](agents/archive/relay-walkthrough.html).

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
- `web_search` — web research through the gateway
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
| **Host OS** | Debian 13+, Ubuntu 24.04 LTS+, or Fedora 40+, `x86_64` or `aarch64`, with systemd and root. Debian 13 and Ubuntu 26.04+ ship Postgres 18 natively; on Ubuntu 24.04/25.04 the installer adds the [PGDG repo](https://www.postgresql.org/download/linux/) automatically, and on Fedora it takes Postgres as shipped. A VM is fine; a container is not. |
| **Hardware** | 8 GB RAM and an SSD. **No GPU required** — the model that writes is remote, embedding is faster on CPU than on an fp16 GPU path for the model we ship, and only the reranker meaningfully gains from a GPU. |
| **Storage** | NVMe or SATA SSD. The installer classifies *and measures* the disk before provisioning anything: eMMC is workable to ~100k items, microSD is slow and wears out under database load, a USB bridge may lie about cache flushes, NFS/SMB is a corruption risk. |
| **Inference** | Two endpoints you run: `/v1/embeddings` (required) and `/v1/rerank` (optional), on loopback, LAN, or VPN — never a public address. [Setting up inference](docs/inference.md) has the commands and the models. A bundled CPU-only trial exists for kicking the tires. |
| **Network** | Standard residential ISP — outbound 443 only, no port forwarding, no inbound rule. LAN-first: the web UI is reachable from a browser on the box itself (Chromium on the box → `http://localhost:8000`) or anywhere else from a paired client — the mobile app, or the desktop helper at `http://localhost:7117` (see [Reaching your box](#reaching-your-box) below). |
| **Mac / Windows** | Not supported as host — Virtues needs root, native Postgres, and full SSD ownership. Use a Linux box. |

Full detail, and the reasoning behind each number, in
[What to run it on](docs/setup/requirements.md).

<a id="inference"></a>
### <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h3-inference-dark.svg"><img alt="Inference" src=".github/images/headings/h3-inference-light.svg" height="22"></picture>

**Start this before you install.** The first thing the installer asks — before
it touches a package, a service, or a disk — is where the retrieval models
live. We provision inference on exactly one board, our own; we do not install
GPU or NPU inference software on hardware we can't test, so on your machine
you own the endpoints and we validate them at the door.

[llama.cpp](https://github.com/ggml-org/llama.cpp)'s `llama-server` speaks
both contracts. These are the invocations our own systemd units use:

```bash
# embedder — CPU is the right answer here (fp32 activations)
llama-server --embedding --pooling mean -m embeddinggemma-300m-qat-Q8_0.gguf \
  --host 127.0.0.1 --port 18181 -c 2048 -b 2048 -ub 2048 -np 1 --cache-ram 0 -ngl 0

# reranker — the half that wants a GPU; drop -ngl for CPU
llama-server --rerank --pooling rank -m gte-reranker-modernbert-base-Q8_0.gguf \
  --host 127.0.0.1 --port 18182 -c 2048 -b 2048 -ub 2048 -np 1 --cache-ram 0 -ngl 99
```

`-np 1` and `--cache-ram 0` are what take each server from ~2.5 GB resident to
~1 GB. Known-good embedders: **embeddinggemma-300m** (768, Matryoshka to 256),
gte-small (384), bge-small-en-v1.5 (384), e5-small-v2 (384),
nomic-embed-text-v1.5 (768). Known-good rerankers:
**gte-reranker-modernbert-base**, bge-reranker-v2-m3, jina-reranker-v2. Any
server will do — Ollama, vLLM, a vendor's NPU runtime — provided it answers
the two POST routes *and* `GET /health`, which the box probes at startup and
which is the requirement people trip over.

[Setting up inference](docs/inference.md) is the full page: the exact contract,
the prompt-prefix and dimension rules, what the fingerprint pin protects you
from, and how to change model later without losing your record.

<a id="install-in-one-command"></a>
### <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h3-install-in-one-command-dark.svg"><img alt="Install in one command" src=".github/images/headings/h3-install-in-one-command-light.svg" height="22"></picture>

```bash
curl -sSL https://virtues.com/sh | sudo sh
```

> `virtues.com/sh` serves the newest **stable** release. For the prerelease
> channel — same installer, cut from the branch we test on:
>
> ```bash
> curl -sSL https://virtues.com/sh-pre | sudo sh
> ```
>
> Pin an exact build with `curl -sSL https://virtues.com/sh | sudo VIRTUES_VERSION=vX.Y.Z sh`.
> The channel you install on is remembered, so later upgrades follow the same
> line. On the appliance you run neither: the boot medium arrives flashed, and
> first boot mints the box its own identity.

That:
- Asks how inference should run — your endpoints (validated and fingerprinted before anything is written) or the bundled CPU-only trial — and does it first, so a broken endpoint costs a prompt rather than a half-finished install
- Measures the disk your record will live on and tells you, with numbers, what you're in for
- Downloads the latest `virtues` binary into `/usr/local/bin/`
- Installs Postgres 18 + pgvector, Avahi (mDNS), and the rest of the system deps via your package manager
- Configures `/etc/avahi/services/virtues.service` so the box advertises itself on the LAN as `virtues.local`
- Enables the `virtues.service` systemd unit (the box mints and keeps its own iroh secret — that key *is* its identity, and nothing external issues it)
- Prints a **6-digit pair code** to type into the app

```bash
sudo -u virtues virtues pair   # prints the code again, any time
```

Enter that code in the desktop or mobile app ([virtues.com/downloads](https://virtues.com/downloads)). **A browser cannot pair** — authentication is a held Ed25519 key and a browser tab has none, so `/pair` in a browser only explains itself. The one exception is a browser running *on the box*, which is trusted as the loopback console and lands straight in the UI at `http://localhost:8000`.

Then connect a source, and optionally `sudo -u virtues virtues subscribe` to enable AI chat through the Virtues cloud (or set up a [BYO provider key](agents/record/auth-model.md) under Settings).

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
| `virtues upgrade` | Self-update from the latest release on this box's channel |
| `virtues channel` | Print (or set) the release channel this box follows |
| `virtues configure-inference` | Re-validate the embedding endpoint after a model change, and offer to re-embed |
| `virtues reindex` | Rebuild the derived search index from source data |

**When something breaks:** [When something breaks](docs/operate/recovery.md)
is the owner's page; [agents/build/recovery.md](agents/build/recovery.md) is
the longer runbook behind it — unreachable box, lost session, last device
revoked, Postgres won't start, restore from backup, BYO key reset.

**The manual** lives in [`docs/`](docs/) and publishes to
[virtues.com/docs](https://virtues.com/docs): what to run it on, inference,
installing, reaching your box, upgrading, backup and restore, the CLI.

<a id="reaching-your-box"></a>
<a id="connect-from-another-machine-v02-preview"></a>
## <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h2-reaching-your-box-dark.svg"><img alt="Reaching your box" src=".github/images/headings/h2-reaching-your-box-light.svg" height="28"></picture>

Reach your box from anywhere — no tunnel, no port forwarding, no DNS name.
**There is no URL that reaches your box.** Reaching it requires holding a paired
Ed25519 key, not knowing an address.

Your box is an **iroh endpoint**, and its Ed25519 `EndpointId` *is* its identity —
mutual-key auth, no certificate authority. A paired device dials that identity
and iroh finds a path: direct on your LAN, hole-punched across NATs, or bounced
off our relay when neither works. The box holds an outbound connection to
the relay, so nothing inbound is ever opened at home. It works behind CGNAT,
coworking/café wifi, and IPv6-only ISPs — anywhere outbound 443 reaches, which is
everywhere. The relay moves sealed bytes it has no key to read — it does see
which two keys are talking and how much passes between them, which is why we
describe it as encrypted end-to-end rather than as blind. See
**[Privacy &amp; security model](agents/record/privacy-model.md)** and the
[visual walkthrough](agents/archive/relay-walkthrough.html).

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

- A recent stable Rust toolchain
- Node.js 22+ and pnpm
- Homebrew, on macOS — `make db` provisions `postgresql@18` + pgvector through it, and `make dev` installs `llama.cpp` for the local embed/rerank sidecars
- Docker (for local S3 via MinIO, optional)

<a id="setup"></a>
### <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h3-setup-dark.svg"><img alt="Setup" src=".github/images/headings/h3-setup-light.svg" height="22"></picture>

```bash
git clone https://github.com/virtues-os/virtues
cd virtues
make init   # copies .env.example → .env with a fresh encryption key
```

Then read the comments in `.env.example` for the keys you'll want. Note that
there is **no `./target` in this repo** — the virtues repos share one cargo
target directory, set per-repo by an untracked `.cargo/config.toml`.

<a id="run"></a>
### <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h3-run-dark.svg"><img alt="Run" src=".github/images/headings/h3-run-light.svg" height="22"></picture>

```bash
make dev
```

One command runs the whole local stack — Postgres, virtues-api on `:9002`,
Core on `:8000`, the web dev server on `:5173`, and the embed/rerank sidecars
on `:18181`/`:18182` (first run fetches ~480 MB of GGUFs into `.data/`).
Ctrl-C stops all of it.

```bash
make dev WITH_EMBED=0   # skip the sidecars: UI-only or low-RAM session (search off)
make dev-info           # the per-tab commands, when you want split logs
make dev-link           # print a login URL for the local stack
```

Access `http://localhost:5173` for hot reload, or `http://localhost:8000`
where Core serves the built web UI. `make help` lists every target.

<a id="virtues-api-required-for-ai-features"></a>
### <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h3-virtues-api-required-for-ai-features-dark.svg"><img alt="virtues-api (required for AI features)" src=".github/images/headings/h3-virtues-api-required-for-ai-features-light.svg" height="22"></picture>

`make dev` already starts it when `VIRTUES_API_URL` points at localhost. To
run it alone:

```bash
make dev-api    # virtues-api on :9002, dev-seeded wallet
```

Core connects to it via `VIRTUES_API_URL=http://localhost:9002`. Real upstream
spend applies even against the fake wallet. See `.env.example` for the required
keys (`AI_GATEWAY_API_KEY`, `VIRTUES_API_INTERNAL_SECRET`).

<a id="cloud-sidecar"></a>
## <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h2-cloud-sidecar-dark.svg"><img alt="Cloud sidecar" src=".github/images/headings/h2-cloud-sidecar-light.svg" height="28"></picture>

Two small services run off-box, and neither holds your record.

**virtues-api** is the metered edge. It holds the external keys — the AI gateway, web search, Plaid, OAuth — and enforces a per-account budget, so a box never carries a credential that could be lifted off it. **atlas** owns identity and funding: Stripe on one side, an opaque account id on the other, and it never sees usage. Both live in this repo under `services/`.

There is **no managed hosting**. Nobody runs a box for you; the two services above exist so that the box you run doesn't have to hold anyone's API keys. Point Virtues at your own provider key, or at your own local model, and you can use less of this — see [the auth model](agents/record/auth-model.md).

<a id="ios-app"></a>
## <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h2-ios-app-dark.svg"><img alt="iOS App" src=".github/images/headings/h2-ios-app-light.svg" height="28"></picture>

The companion app pairs with a 6-digit code from `virtues pair`, typed into the app while you're standing next to the box — that code putting the device's key on the box's allowlist *is* the authentication. See [agents/record/auth-model.md](agents/record/auth-model.md) for the model. Source: the cross-platform Tauri app in `apps/web/src-tauri/` (macOS/Windows/Linux/iOS/Android), with on-device collection provided by the native plugins in `apps/web/plugins/`.

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
├── docs/                    # The public manual — publishes to virtues.com/docs
├── agents/                  # The workshop: build contracts, records, plans (never published)
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
<a id="the-day-pipeline"></a>
## <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h2-the-day-pipeline-dark.svg"><img alt="The Day Pipeline" src=".github/images/headings/h2-the-day-pipeline-light.svg" height="28"></picture>

After a day ends, the box reconstructs it — nightly, on the completed day. Two LLM passes build the record; three scoring passes measure how unusual it was, each against the owner's own history rather than any population norm. Every embedding involved is computed **on the box** by the local embedding sidecar (`virtues-embed.service`, a llama-server speaking OpenAI-compatible `/v1/embeddings` on localhost — currently EmbeddingGemma-300M). Nothing is sent anywhere to be embedded. The full design doc is [agents/record/the-day.md](agents/record/the-day.md).

<a id="segmentation--narration"></a>
### <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h3-segmentation-narration-dark.svg"><img alt="Segmentation & Narration" src=".github/images/headings/h3-segmentation-narration-light.svg" height="22"></picture>

**Segment** — the detective. A best-model pass reads a compact dossier of the day's evidence — location visits, movement, device presence, audio sessions, messages, purchases, sleep, health — and adjudicates it into a gapless timeline of events (`wiki_events`). Its central doctrine: **plans are not evidence.** A calendar entry or a message arranging something never names a stretch of the day on its own; it needs a physical trace — a visit, movement toward it, a purchase there, matching audio — to corroborate it. An uncorroborated plan yields an honest "Unknown", not a confident memory of a day that didn't happen.

**Narrate** — a second pass writes the day's first-person diary and epigraph, and rates the day's data quality (`{coverage, overall, note}`) as it goes. All of it lands on `wiki_days`.

<a id="event-novelty"></a>
### <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h3-event-novelty-dark.svg"><img alt="Event Novelty" src=".github/images/headings/h3-event-novelty-light.svg" height="22"></picture>

Each event's summary is embedded locally and scored against a baseline of the owner's own recent events — two orthogonal z-scores, deliberately never blended into one number:

- **Global novelty** (`novelty_z`) — cosine distance from a kernel-weighted centroid of the baseline. Each baseline event contributes by exponential recency decay (42-day half-life) × von Mises kernels on hour-of-day and weekday, so "unusual for a Tuesday morning" is a continuous notion, not a hard window. This answers *rare in your life at all*.
- **Local novelty** (`local_novelty_z`) — a density-relative Local Outlier Factor over the same baseline, log-transformed onto the same σ axis. This answers *off-pattern for its kind*: the first cardio session when you always lift, which global novelty misses because both are "just a workout".

With fewer than three distinct baseline days, nothing is scored — the box is calibrating, and NULL is more honest than noise.

<a id="topic--entity-novelty"></a>
### <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h3-topic-entity-novelty-dark.svg"><img alt="Topic & Entity Novelty" src=".github/images/headings/h3-topic-entity-novelty-light.svg" height="22"></picture>

The same question asked of two different structures. **Topics** are semantic: each topic string is embedded (and cached), and novelty is z-scored cosine distance from the trailing 12-week topic centroid. **Entities** are structural, with no embedding at all: a binomial z-score on how many baseline days the entity appeared — the person you see daily is routine; the one who has appeared three times in twelve weeks is news.

<a id="live-rhythm"></a>
### <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h3-live-rhythm-dark.svg"><img alt="Live Rhythm" src=".github/images/headings/h3-live-rhythm-light.svg" height="22"></picture>

The scores above are written by the end-of-day pass, so they cannot answer for the day in progress. The home page computes a live measure client-side, with no model at all: today's hour-by-hour activation raster is normalized into a shape and compared — total-variation distance — against the median shape of the trailing twelve weeks, using only the hours today has actually lived. Too little history, or a history where every day looks alike, and it renders nothing rather than scoring noise.

<a id="features"></a>
## <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h2-features-dark.svg"><img alt="Features" src=".github/images/headings/h2-features-light.svg" height="28"></picture>

**Spaces & Workspaces** — Arc-browser-style multi-space system. Each space has its own tabs, theme, and accent color. Swipeable sidebar carousel for switching between spaces. Organize your life into contexts — work, health, finance — each with its own look and layout.

**Tab System** — URL-native tab management with split-pane support. Tabs persist across sessions, serialize/deserialize automatically, and support side-by-side viewing. Every entity in the system has a URL, and every URL can be a tab.

**Rich Editor** — ProseMirror-based document editor with real-time collaboration via Yjs and WebSocket sync. Slash commands (`/`) for inserting blocks, `[[entity]]` linking for connecting to people/places/orgs, drag handles, table toolbar, code syntax highlighting (Shiki), markdown shortcuts, and media paste. IndexedDB persistence for offline support.

**AI Agent Modes** — Three distinct modes: **Agent** (full tool access — SQL, search, code, page editing), **Chat** (conversation only, no tools), and **Research** (read-only tools). Customizable personas let you shape the AI's behavior. Multi-model support across Claude, GPT, Gemini, and more.

**Semantic Search** — Two-stage retrieval, on your side of the wire: a bi-encoder (EmbeddingGemma-300M on our own boxes) generates embeddings, then a cross-encoder reranker (gte-reranker-modernbert) re-scores the candidates for precision. Both are HTTP endpoints rather than linked code — llama.cpp sidecars we provision on our board, whatever you run on yours ([Setting up inference](docs/inference.md)). Per-ontology text extraction ensures every data type is searchable. Cmd+K modal for quick actions and cross-entity search.

**Entity Resolution** — Automatic extraction of people, places, and organizations from your raw data. The "Sarah" in your calendar, the "Sarah" in your contacts, and the "Sarah" in your messages all resolve to one person. Dedicated wiki pages for each entity type with specialized views.

**Smart Views & Manual Folders** — Smart views are query-based dynamic collections that auto-update as your data changes. Manual folders let you curate your own groupings. Three-level sidebar hierarchy: Section → Folder → Item, all with drag-and-drop reordering via SortableJS.

**Automated Autobiography** — Daily summaries generated from your data — calendar, messages, health, location, transactions, transcriptions. Temporal navigation by day and year. Narrative structure: Telos (life purpose) → Acts (multi-year arcs) → Chapters → Days. Hit "Generate Summary" and the system writes your day for you.

**Novelty Scoring** — Every reconstructed event is scored against your own recent life, with embeddings computed on the box: global novelty (distance from a recency- and time-of-day-weighted centroid of your past events) and local novelty (off-pattern for its kind, via Local Outlier Factor). Topics and entities get their own novelty signals, and the home page shows a live rhythm measure for the day in progress. See [The Day Pipeline](#the-day-pipeline).

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
