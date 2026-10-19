<!--
  ┌─ HEADING CONVENTION (read before editing any # / ## / ### heading) ──────────┐
  Headings are NOT plain markdown text. GitHub's sanitizer strips CSS, so to get
  a serif (Times) heading we render each one as a theme-aware SVG image wrapped in
  a real markdown heading, e.g.:

    ## <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h2-foo-dark.svg"><img alt="Foo" src=".github/images/headings/h2-foo-light.svg" height="28"></picture>

  Because the heading's visible content is an image (no text), two things follow:
    • The GitHub "Outline" sidebar shows blank labels — accepted tradeoff, which
      is why this file carries a hand-written table of contents under the badges.
    • GitHub can't auto-generate anchors, so each H2/H3 has an explicit
      `<a id="…"></a>` on the line ABOVE it. The TOC links to those ids; some
      headings carry a second, older id so existing deep links keep resolving.

  TO ADD OR EDIT A HEADING (do ALL of these, or you'll get a stale image / dead link):
    1. Edit the heading list in .github/images/headings/gen.py
       (font sizes: H1=34/h40, H2=24/h28, H3=20/h22; weight 400; light+dark each).
    2. Run:  python3 .github/images/headings/gen.py   (regenerates the SVGs).
    3. In this file, update the <picture> block's `alt`, both `srcset`/`src`
       filenames, and the `<a id>` anchor above it. Note gen.py's filename slug
       turns an apostrophe into a hyphen ("Why it's shaped this way" →
       h2-why-it-s-shaped-this-way) while the anchor drops it
       ("why-its-shaped-this-way").
    4. Delete the SVG pair for any heading you removed.

  Keep headings short — wide SVGs (>~360px) get scaled down on mobile, breaking
  the visual size hierarchy. Times is a system font; Linux/Android fall back to
  their default serif.
  └──────────────────────────────────────────────────────────────────────────────┘
-->

![Virtues](.github/images/cover3.png)

# <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h1-virtues-dark.svg"><img alt="Virtues" src=".github/images/headings/h1-virtues-light.svg" height="40"></picture>

Self-hosted personal AI over the record of your own life.

[![CI](https://github.com/virtues-os/virtues/actions/workflows/ci.yml/badge.svg)](https://github.com/virtues-os/virtues/actions/workflows/ci.yml)
[![License: BUSL-1.1 + MIT](https://img.shields.io/badge/License-BUSL--1.1%20%2B%20MIT-blue.svg)](LICENSE)
[![Discord](https://img.shields.io/badge/Discord-Join%20Us-7289da?logo=discord&logoColor=white)](https://discord.gg/sSQKzDWqgv)

[Website](https://virtues.com) · [Docs](https://virtues.com/docs) · [Pre-order the server](https://virtues.com/pre-order) · [Discord](https://discord.gg/sSQKzDWqgv)

Virtues is a server you run at home. It pulls in what you already generate —
messages, calendar, health, location, transactions, recordings, files — keeps
it in one Postgres database on your own disk, and turns it into three things: a
written account of each day, a wiki of the people and places in it, and an
assistant that answers from your actual data. Search runs on your own
hardware. The model that writes is the only thing that leaves, through a
metered proxy that keeps nothing. No inbound port, no hostname: your devices
reach the server by key.

*Early software. The [releases page](https://github.com/virtues-os/virtues/releases)
says what exists; expect rough edges, and expect us to say where they are.*

<a id="diy-quickstart"></a>
## <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h2-diy-quickstart-dark.svg"><img alt="DIY quickstart" src=".github/images/headings/h2-diy-quickstart-light.svg" height="28"></picture>

Two ways to run it. **Do it yourself**: the software on your own Linux machine,
installed with one command — the path this README covers.
**[The Virtues Server](https://virtues.com/pre-order)**: the same software on a
board we build, open for pre-order. Either way you can leave with your data.

Five steps on a spare Linux machine — a VM is fine.

**1. Install.** When it asks how search should run, take the default: the
server's CPU, with the embedding model Virtues recommends and keeps current.

```bash
curl -sSL https://virtues.com/sh | sudo sh
```

**2. Pair a device.** The installer ends by printing a 6-digit code — a
browser cannot pair, so enter it in the desktop or phone app from
[virtues.com/downloads](https://virtues.com/downloads). Pairing puts your
device's key on the server's allowlist; that key is how you reach it from
anywhere, and a fresh code is one command away:

```bash
sudo virtues pair
```

**3. Connect an account.** The app asks for one after pairing. Search runs
locally, but everything that reaches outside goes through the account: the
model that writes your days and answers questions, web search, the sign-in
flows for Google, Strava and the rest, bank data, and the relay that lets your
devices reach the server from outside your home network. Bringing your own
model key replaces only the first of those. From the server itself, the same
step is:

```bash
sudo virtues subscribe
```

**4. Connect a source** in the app: Google, Plaid, Strava, or the phone app
for HealthKit and location. The record starts filling from there, and the
search index builds as data lands.

**5. Wait a day.** Each day is written after it ends, so the first page worth
reading arrives tomorrow. `virtues status` shows what is flowing in the
meantime; `virtues doctor` explains anything that isn't.

Search on the CPU is enough for everyday use. If the machine has a GPU or
NPU, the installer links to [the guide](docs/setup/accelerators.md) for
running search there; [the full install](#the-full-install) covers running
your own embedding server.

<a id="what-it-does"></a>
## <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h2-what-it-does-dark.svg"><img alt="What it does" src=".github/images/headings/h2-what-it-does-light.svg" height="28"></picture>

**It writes your days.** After a day ends, the server rebuilds it from evidence
— visits, movement, device presence, audio, messages, purchases, sleep — into a
timeline, then writes the diary. Plans are not evidence: a calendar entry with
no physical trace behind it becomes an honest *Unknown*, not a confident
memory. Each event is scored for novelty against your own history, never a
population norm. [Design notes](agents/build/the-day.md).

**It keeps a wiki of the people and places in them.** The Sarah in your
calendar, contacts and messages resolves to one person with a page of her own.

**It answers from your data.** The agent has read-only SQL over your tables,
semantic search over the record, a Python sandbox, web research, and page
editing. Claude, GPT or Gemini; three modes — full tools, conversation only,
read-only research.

**Where the data goes.** It stays on your server. Embedding must run on a
local address; the installer refuses a public one. GPS is reduced to
distance and pace before anything is sent. The assistant is the exception and
the point: a question, or the nightly day-writing, sends the relevant part of
your record to a model provider — voice as audio, not as transcript. Point it
at a local model and that stops too. The relay cannot read your traffic; that
is physics. The inference boundary is a contract with a provider, which is a
different kind of promise.
[The privacy boundary](agents/build/privacy-boundary.md)
says exactly what crosses and what never does.

<a id="why-its-shaped-this-way"></a>
## <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h2-why-it-s-shaped-this-way-dark.svg"><img alt="Why it's shaped this way" src=".github/images/headings/h2-why-it-s-shaped-this-way-light.svg" height="28"></picture>

**Virtues of the digital age.** The name does not mean *use technology to live
virtuously*. Virtues change with the age, and ours asks for ones the old lists
never named. Owning the data of your own life is the first: what is held about
you is what can be used to move you, and someone who cannot read their own
record cannot notice being moved.

**Digital subsidiarity** is that virtue as software. A matter belongs to the
smallest authority competent to handle it, so the record of a life belongs
where the life is lived — not in a datacenter that can read it, not with a
company that will outlive its interest in keeping it.

Every structural decision follows, and each is an incapability rather than a
promise: the record is on your disk; retrieval runs against local endpoints
only; nothing inbound is opened at home; the relay moves sealed bytes it has no
key to read; we are not on your allowlist.

Essays in [the Library](https://virtues.com/library). Engineering record in
[`agents/`](agents/).

<a id="the-ontology"></a>
## <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h2-the-ontology-dark.svg"><img alt="The ontology" src=".github/images/headings/h2-the-ontology-light.svg" height="28"></picture>

The schema is not an implementation detail here — it *is* the interface. It is
shown to a language model at runtime and it drives a table-driven UI, so a
column name is a term of art in two directions at once.

Everything ingested lands in one of a couple of dozen **registered
ontologies** — the list is
[`crates/virtues-registry/src/ontologies.rs`](crates/virtues-registry/src/ontologies.rs),
and it is what a life looks like once it is normalized:

| Family | Tables |
|---|---|
| Health | `data_health_sleep`, `data_health_workout`, `data_health_heart_rate`, `data_health_hrv`, `data_health_steps` |
| Place | `data_location_visit`, `data_location_point`, `data_environment_weather` |
| People | `data_communication_message`, `data_communication_email`, `data_communication_transcription`, `data_calendar_event` |
| Money | `data_financial_transaction`, `data_financial_account` |
| Attention | `data_activity_app_session`, `data_activity_web_browsing`, `data_content_document`, `data_content_bookmark`, `data_content_conversation` |
| Voice | `data_audio_session` |

Table prefixes are a namespace, not decoration: `data_` is ingested, `wiki_` is
the derived entity graph (`wiki_people`, `wiki_places`, `wiki_orgs`,
`wiki_events`, `wiki_days`), `narrative_` is the life structure
(`narrative_telos` → `narrative_acts` → `narrative_chapters` → days), `app_` is
product state, `search_` is the indexes, `elt_` is pipeline plumbing.

The column names follow a law, because the schema once had **seven** different
names for "when this happened":

- `occurred_at` — an instant, when the thing happened. `started_at` / `ended_at`
for a span.
- `created_at` / `updated_at` — when *we* wrote the row. Never the event; that
conflation is what produced `created_time` sitting beside `created_at`.
- `is_` / `has_` for booleans, never a bare adjective.
- A unit suffix on every quantity: `_cents`, `_ms`, `_bytes`, `_meters`.

The agent's `sql_query` tool reads a catalog of this schema and queries it
read-only, which is the concrete mechanism behind "an AI with real context".
Adding a data type means adding an ontology and a text extractor; the UI and
the search index follow from the descriptor.

<a id="architecture"></a>
## <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h2-architecture-dark.svg"><img alt="Architecture" src=".github/images/headings/h2-architecture-light.svg" height="28"></picture>

<!-- Replace with a light/dark SVG pair (same machinery as the headings). -->

<picture>
  <source media="(prefers-color-scheme: dark)" srcset=".github/images/architecture/architecture-dark.svg">
  <img alt="Sources flow into Virtues Core, one Rust binary on port 8000 holding Postgres and a file store and never an external API key. Core calls your own embedding endpoint — /v1/embeddings on 18181, loopback, LAN or VPN only — and reaches model providers solely through virtues-api on port 9002, which holds the external keys and enforces a budget." src=".github/images/architecture/architecture-light.svg" width="100%">
</picture>

**Core** is one Rust binary against one Postgres database: ingestion, entity
resolution, the wiki, pages, chat, and the web UI it serves. It never touches
an external API key.

**Retrieval is one HTTP contract, not code we ship.** Core consumes an
OpenAI-style `/v1/embeddings` endpoint and cannot tell what is behind it. On
your machine the installer runs the recommended model on the CPU and keeps it
current, or you run your own server and the installer probes what you point it
at and pins the model's fingerprint — so a silently swapped model can't
quietly corrupt your index. On our own board it runs on the NPU.

**virtues-api** is the metered edge, and **atlas** owns identity and funding
(Stripe on one side, an opaque account id on the other; it never sees usage).
Both live in this repo under `services/`. There is **no managed hosting** —
nobody runs a server for you; those two services exist so the one you run
doesn't have to hold anyone's API keys.

| Source | Streams | Method |
|---|---|---|
| Google | Calendar, Gmail | OAuth |
| Notion | Pages | OAuth |
| Plaid | Transactions, Accounts, Investments, Liabilities | OAuth |
| Strava | Activities | OAuth |
| GitHub | Events | OAuth |
| iOS | HealthKit, Location, Microphone, Contacts, FinanceKit, EventKit | Device |
| macOS | Apps, Browser, iMessage | Device |

<a id="the-full-install"></a>
<a id="install-properly"></a>
<a id="install-linux-home-server"></a>
## <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h2-the-full-install-dark.svg"><img alt="The full install" src=".github/images/headings/h2-the-full-install-light.svg" height="28"></picture>

<a id="what-to-run-it-on"></a>
<a id="requirements"></a>
### <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h3-what-to-run-it-on-dark.svg"><img alt="What to run it on" src=".github/images/headings/h3-what-to-run-it-on-light.svg" height="22"></picture>

| Requirement | What, and why |
|---|---|
| **Host OS** | Debian 13+, Ubuntu 24.04 LTS+, or Fedora 40+, `x86_64` or `aarch64`, with systemd and root. A VM is fine; a container is not. |
| **Hardware** | 8 GB RAM and an SSD. **No GPU required** — the model that writes is remote, and the one local retrieval model runs on the CPU. |
| **Storage** | NVMe or SATA SSD. The installer classifies *and measures* the disk first: eMMC is workable to ~100k items, microSD is slow and wears out, NFS/SMB is a corruption risk. |
| **Inference** | One `/v1/embeddings` endpoint, on loopback, LAN, or VPN — never a public address. The installer runs one on the CPU by default. |
| **Network** | Outbound 443 only. No port forwarding, no inbound rule, no hostname. |

Machines that clear the bar: a mini PC with 16 GB and an NVMe slot; a used
small-form-factor desktop; an aarch64 SBC with 8 GB and NVMe over PCIe — not
microSD. Full reasoning behind each number in
[What to run it on](docs/setup/requirements.md).

<a id="inference"></a>
### <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h3-inference-dark.svg"><img alt="Inference" src=".github/images/headings/h3-inference-light.svg" height="22"></picture>

**Only if you want your own server.** The first thing the installer asks,
before it touches a package or a disk, is how search should run. The default
runs the recommended embedding model on the CPU. We don't install GPU or NPU
inference software on hardware we can't test, so for those you run the
server and we validate it at the door.

[llama.cpp](https://github.com/ggml-org/llama.cpp)'s `llama-server` speaks the
contract. This is the invocation our own systemd unit uses:

```bash
# embedder — -ngl 0 keeps it on the CPU (its math overflows fp16 on some GPU paths)
llama-server --embedding --pooling mean -m embeddinggemma-2-Q8_0.gguf \
  --host 127.0.0.1 --port 18181 -c 2048 -b 2048 -ub 2048 -np 1 --cache-ram 0 -ngl 0
```

`-np 1` and `--cache-ram 0` take the server from ~2.5 GB resident to ~1 GB.
Known-good embedders: **embeddinggemma-2** (768; needs llama.cpp b11507+),
gte-small (384), bge-small-en-v1.5 (384), e5-small-v2 (384),
nomic-embed-text-v1.5 (768).

[Setting up inference](docs/inference.md) is the full page: the exact contract,
the prompt-prefix and dimension rules, what the fingerprint pin protects you
from, and how to change model later without losing your record.

<a id="the-command"></a>
<a id="install-in-one-command"></a>
### <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h3-the-command-dark.svg"><img alt="The command" src=".github/images/headings/h3-the-command-light.svg" height="22"></picture>

```bash
curl -sSL https://virtues.com/sh | sudo sh
```

`virtues.com/sh` is the newest stable release, `sh-pre` the prerelease channel,
and the one you install on is remembered —
[Installing](docs/setup/install.md) covers pinning a version and reading the
script before you run it. On a Virtues Server you run neither: the boot medium
arrives flashed.

In about ten minutes it asks how you want inference and validates your
endpoints, measures the disk, installs Postgres 18 + pgvector and Avahi,
advertises the machine on the LAN as `virtues.local`, starts `virtues.service`
— which mints and keeps the Ed25519 secret that *is* this server's identity —
and prints a **6-digit pair code**.

```bash
sudo virtues pair   # prints a fresh code, any time
```

Enter it in the desktop or mobile app
([virtues.com/downloads](https://virtues.com/downloads)). **A browser cannot
pair** — authentication is a held Ed25519 key and a browser tab has none. The
one exception is a browser running *on the server*, trusted as the loopback
console at `http://localhost:8000`.

The [releases page](https://github.com/virtues-os/virtues/releases) carries
more than the server. The tag says which line a release belongs to:

| Tag | What it is |
|---|---|
| `vX.Y.Z` | The server, stable. The only line `virtues.com/sh` and GitHub's *Latest* point at. |
| `vX.Y.Z-staging.N` | Server prereleases, cut from `staging`. `virtues upgrade --pre` installs the newest. |
| `mac-vX.Y.Z` | The Mac app. Its numbers are its own line and never compare with the server's. |
| `mac-latest` | The newest stable Mac build under a fixed URL, which the app's updater and the download page read. |
| `models-1` | The model and library files the installer downloads. Not a software release. |

<a id="commands"></a>
### <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h3-commands-dark.svg"><img alt="Commands" src=".github/images/headings/h3-commands-light.svg" height="22"></picture>

| Command | What it does |
|---|---|
| `virtues status` | Health in one screen: identity, inference, subscription, devices. `--json` for the same thing in a stable shape, which is what to paste into a support thread |
| `virtues doctor` | How inference resolved, whether each endpoint is actually serving, whether the server is reachable — every finding with the command that diagnoses it |
| `virtues pair` | Print a code to connect a device |
| `virtues device ls` / `add` / `rm <id>` | Who can reach this server. Revoking is real: the next dial is refused |
| `virtues subscribe` | Connect the server to a subscription, so the assistant can reach a model |
| `virtues sudo` | Approve a pending sensitive action — proves physical access |
| `virtues upgrade` / `rollback` | Move between releases on this server's channel — nothing updates itself |
| `virtues channel` | Print or set the release channel this server follows |
| `virtues backup` / `restore` | Snapshot and restore server state |
| `virtues configure-inference` | Re-validate the embedding endpoint after a model change, and offer to re-embed |
| `virtues reindex` | Rebuild the derived search index from source data |
| `virtues uninstall` | Leave. Prints the full manifest of what it found before touching anything |

One rule for all of them: `sudo virtues …`. The daemon runs as its own
`virtues` user and owns the database, so commands that touch it re-exec
themselves as that user; the ones that change the machine — `upgrade`,
`restore`, `uninstall` — stay root.

<a id="where-things-live"></a>
### <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h3-where-things-live-dark.svg"><img alt="Where things live" src=".github/images/headings/h3-where-things-live-light.svg" height="22"></picture>

| What | Where |
|---|---|
| Binary | `/usr/local/bin/virtues` |
| Config | `/var/lib/virtues/virtues.env` — edit, then `sudo systemctl restart virtues` |
| Data | `/var/lib/virtues` — the Postgres cluster and the file store. `DATA_DIR` at install moves it |
| Models | `/var/lib/virtues/models` |
| Units | `virtues`, plus `virtues-embed` when search runs on the CPU |
| Ports | `8000` server · `5432` Postgres · `18181` embedding |
| Logs | `journalctl -u virtues -f` |

**When something breaks:** [When something breaks](docs/operate/recovery.md) is
the owner's page; [agents/build/recovery.md](agents/build/recovery.md) is the
longer runbook behind it.

**The manual** is in [`docs/`](docs/) and publishes to
[virtues.com/docs](https://virtues.com/docs): what to run it on, inference,
installing, reaching your server, upgrading, backup and restore, the CLI.

<a id="reaching-your-server"></a>
<a id="reaching-your-box"></a>
<a id="connect-from-another-machine-v02-preview"></a>
## <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h2-reaching-your-server-dark.svg"><img alt="Reaching your server" src=".github/images/headings/h2-reaching-your-server-light.svg" height="28"></picture>

**There is no URL that reaches your server.** Reaching it requires holding a
paired Ed25519 key, not knowing an address.

It is an **iroh endpoint** whose Ed25519 `EndpointId` *is* its identity —
mutual-key auth, no certificate authority. A paired device dials that identity
and iroh finds a path: direct on your LAN, hole-punched across NATs, or bounced
off our relay when neither works. The server dials *out* to the relay, so nothing
inbound is ever opened at home, and it works behind CGNAT, café wifi, and
IPv6-only ISPs — anywhere outbound 443 reaches. The relay moves sealed bytes it
has no key to read; it does see which two keys are talking and how much passes
between them, which is why we call it encrypted end-to-end rather than blind
([design notes](agents/archive/networking-relay-tee.md)).

Pairing is **local-first** — no passwords, no email, no magic links: you walk to
the machine, run `virtues pair`, and the code puts the new device's key on the
allowlist. Access is enforced twice —
iroh refuses an unpaired key *before HTTP exists*, and the app-layer bearer
remains the authorization keystone on top. Revocation is real:
`virtues device rm <id>` de-allowlists a key and the next dial is refused. Day
to day you type none of this; the apps hold the key and do the dialing, and on
your home network it answers directly on `:8000`.

<a id="build-on-it"></a>
## <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h2-build-on-it-dark.svg"><img alt="Build on it" src=".github/images/headings/h2-build-on-it-light.svg" height="28"></picture>

**Applets** are the extension system — ingestion and behavior as supervised
subprocesses, in any language, declared by a `manifest.toml`. A new data source
is an applet; so is a scheduled job. See
[`applets/AUTHORING.md`](applets/AUTHORING.md).

**Pages** are collaborative documents (ProseMirror + Yjs) with `[[entity]]`
links into the graph, version history, and AI editing. **Drive** is file storage
on the server's own disk. Every entity has a URL, which is what makes the web UI's tabs and split panes work.

<a id="development"></a>
## <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h2-development-dark.svg"><img alt="Development" src=".github/images/headings/h2-development-light.svg" height="28"></picture>

For working on the codebase itself, not for running Virtues in production.

**Prerequisites:** a recent stable Rust toolchain, Node 22+ and pnpm. The dev
loop below is written for macOS on Apple Silicon: `make db` provisions
`postgresql@18` + pgvector through Homebrew, and the embedding sidecar is
llama.cpp's macOS arm64 build. `make dev` needs both.

On Linux, `make db` and `make dev` will not run. Install Postgres 18 with
pgvector yourself, create the role and databases the `db` target in the
Makefile creates, then run `make dev-core` and `make dev-web` in two terminals.
Search stays off unless you run an embedding server of your own.

```bash
git clone https://github.com/virtues-os/virtues
cd virtues
make init   # .env with a fresh encryption key
make dev    # postgres + api :9002 + core :8000 + web :5173 + embed
```

One command runs the whole stack; Ctrl-C stops it. First run fetches the ~320 MB
embedding model into `.data/`.

```bash
make dev WITH_EMBED=0   # skip the sidecar (search off) — UI-only or low-RAM
make dev-info           # per-tab commands, for split logs
make dev-link           # a login URL for the local stack
make help               # every target
```

Two things that surprise everyone: there is **no `./target`** — the virtues
repos share one cargo target directory set by an untracked `.cargo/config.toml`
— and the crate is `virtues`, not `virtues-core`, so tests are
`cargo test -p virtues --lib`.

Several agents work in this one checkout at the same time, which makes a few
ordinary git commands destructive here. [`CLAUDE.md`](CLAUDE.md) is the working
agreement: branch discipline, how to commit safely, and how to claim a
migration number.

<a id="the-repo"></a>
<a id="project-structure"></a>
## <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h2-the-repo-dark.svg"><img alt="The repo" src=".github/images/headings/h2-the-repo-light.svg" height="28"></picture>

```
virtues/
├── virtues-core/            # The heart: Rust daemon (crate `virtues`) — API, ingest, agent, wiki, dayline
├── applets/                 # Extension system: ingestion + behavior as subprocesses
├── crates/                  # Shared libraries: iroh transport, reach client, NPU daemon, registry
├── services/                # The metered cloud edge: virtues-api, atlas
├── apps/                    # Clients: web (also the Tauri iOS/desktop app), desktop helper, mac collector
├── docs/                    # The public manual — publishes to virtues.com/docs
├── agents/                  # The workshop: build contracts, records, plans (never published)
├── tools/                   # bootstrap.sh + virtues-installer (virtues.com/sh)
└── masters/, deploy/
```

<a id="security"></a>
## <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h2-security-dark.svg"><img alt="Security" src=".github/images/headings/h2-security-light.svg" height="28"></picture>

Please report vulnerabilities privately — GitHub → **Security** → *Report a
vulnerability* — rather than in a public issue. What the relay can and cannot
see is on [the reach page](docs/operate/reach.md); where data leaves the box is
[the privacy boundary](agents/build/privacy-boundary.md).

<a id="license"></a>
## <picture><source media="(prefers-color-scheme: dark)" srcset=".github/images/headings/h2-license-dark.svg"><img alt="License" src=".github/images/headings/h2-license-light.svg" height="28"></picture>

- **Everything by default** — [BUSL-1.1](LICENSE): source-available, free to
self-host for personal or internal organizational use, **not** for offering a
hosted service or commercial hardware product. Each version converts to Apache
2.0 four years after its first public release.
- **The native plugins, the Mac collector and the data registry** — MIT, under
the `LICENSE` in each directory: [`apps/web/plugins/`](apps/web/plugins/LICENSE),
[`apps/mac-source/`](apps/mac-source/LICENSE) and
[`crates/virtues-registry/`](crates/virtues-registry/LICENSE).

A directory is BUSL-1.1 unless it carries its own `LICENSE`. Vendored
third-party code keeps its upstream license, filed beside it; the models and
libraries a server downloads are listed in
[`THIRD_PARTY_NOTICES.txt`](THIRD_PARTY_NOTICES.txt).

---

<p align="center"><i>Your data. Your infrastructure. Your narrative.</i></p>