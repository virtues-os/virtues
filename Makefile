# Virtues — Mac dev quickstart + cloud-service deploys.
#
# Mac dev uses native brew Postgres (no Docker daemon).
# Linux home box is installed natively via tools/bootstrap.sh (no Docker).
# Cloud services (virtues-atlas / virtues-api) deploy as Docker images to ECR.

.DEFAULT_GOAL := help
.PHONY: help init commit migration dev seed dev-info dev-core dev-api dev-web dev-embed _embed-ensure _embed-run \
        dev-link dev-reset dev-wipe-mac dev-clean dev-pull dev-real db db-stop deploy-atlas deploy-virtues-api _ecr-push mac-app web-test \
        iroh-ffi-ios iroh-ffi-mac ios-release

AWS_REGION ?= us-east-1

# Mac dev is FULLY LOCAL by default and never touches prod: `make dev` runs a
# local virtues-api (seeded wallet → AI works with no checkout) and points atlas
# at localhost (nothing there — so a stray "Connect" can't mint rows in prod).
#
# There is no staging stack. `STAGING=1` used to point these at
# {api,atlas}-staging.virtues.com, but no such host was ever deployed: both names
# resolved only by way of a `*.virtues.com` wildcard aimed at an Elastic IP
# attached to no instance, so the "real billing flow" was dialing a blackhole
# that swallowed connections instead of refusing them. A Stripe test-mode webhook
# pointed there for long enough to start mailing about failed deliveries.
# Wildcard and IP were both removed 2026-07-31. Override VIRTUES_API_URL /
# VIRTUES_ATLAS_URL individually for prod or a custom host.
VIRTUES_API_URL   ?= http://localhost:9002
VIRTUES_ATLAS_URL ?= http://localhost:9100
DEV_WEB_PORT ?= 5173

# Local virtues-api (`make dev-api`): its own logical db + a known dev api_key.
# The key is funded by the gated seed in virtues-api (ENVIRONMENT=dev), and
# virtues-core presents it via VIRTUES_API_KEY — but only when pointing at a
# LOCAL api. Against prod we must use the real vault key, so DEV_API_KEY
# stays empty there (the client override no-ops on an empty value).
VIRTUES_API_DATABASE_URL ?= postgres://virtues:virtues@localhost:5432/virtues_api
VIRTUES_API_KEY ?= dev-local-key
DEV_API_KEY := $(if $(filter http://localhost%,$(VIRTUES_API_URL)),$(VIRTUES_API_KEY),)

# Which Postgres `dev-core` reads. Defaults to the local seeded dev db; `dev-real`
# overrides it to a snapshot of your actual box (see `dev-pull` / `dev-real`).
DEV_DB_URL ?= postgres://virtues:virtues@localhost:5432/virtues
# Real-data dev: `dev-pull` snapshots the live box's Postgres into a THROWAWAY local
# db so you can browse your own life-log in the dev UI. Media (photos/audio/video)
# stays on the box — only the structured spine is copied. The box's encryption key
# is NEVER pulled: bulk life-data is plaintext, so browsing needs no secret (only
# credentialed actions, which you wouldn't run in a read-only browse, would want it).
DEV_BOX_SSH    ?= virtues-box
DEV_BOXCOPY_DB ?= virtues_boxcopy

# Quiet dev logs: warnings/errors only. Override for a noisy session, e.g.
#   make dev RUST_LOG=info        (or RUST_LOG=virtues=debug for targeted debug)
RUST_LOG ?= warn

# Auto-rebuild + restart virtues-core on .rs changes: `make dev WATCH=1` (or
# `make dev-core WATCH=1`). Needs cargo-watch (`cargo install cargo-watch`); if
# it's missing we print a hint and fall back to a plain one-shot run.
WATCH ?= 0
CARGO_WATCH := $(shell command -v cargo-watch 2>/dev/null)
DEV_CORE_RUN := cargo run -p virtues
ifeq ($(WATCH),1)
ifneq ($(CARGO_WATCH),)
DEV_CORE_RUN := cargo watch -x 'run -p virtues'
endif
endif

# Build against the committed `.sqlx` query cache (like CI) instead of probing the
# live DB at compile time. This decouples the build from DB state — a fresh/empty
# `virtues` (e.g. right after `make db-reset`) still compiles, then migrations run
# at runtime. If you EDIT a sqlx query!, regenerate the cache with
#   cargo sqlx prepare --workspace
# (or build that one session with `make dev-core SQLX_OFFLINE=` against a migrated DB).
SQLX_OFFLINE ?= true

# Dev convenience: pre-satisfy the required /setup wizard so `make dev` lands
# straight in the app shell (the loopback-console auth already logs the dev
# browser in). Clear it to walk the real wizard: `make dev VIRTUES_DEV_SKIP_SETUP=`.
VIRTUES_DEV_SKIP_SETUP ?= 1

# Local inference sidecars (`make dev-embed`). Models cache once under .data/
# (gitignored) and are reused — same GGUFs + llama-server flags the appliance
# installer pins (see tools/virtues-installer/src/install.rs). `dev-core` points
# at this dir so `virtues doctor` reports them baked.
# `make dev` runs the embed/rerank sidecars by default (~1 GB RAM, ~0% CPU
# idle). Skip them for a UI-only or low-RAM session: `make dev WITH_EMBED=0`.
WITH_EMBED ?= 1
VIRTUES_MODELS_DIR ?= $(CURDIR)/.data/models
# Must match the GGUFs virtues-core expects (see virtues-core/src/inference_report.rs):
# EmbeddingGemma-300M is 768-dim + mean-pooled + task-prompted (embedder.rs);
# serving bge-m3 here emits 1024-dim vectors that core rejects.
EMBED_GGUF  := embeddinggemma-300m-qat-Q8_0.gguf
RERANK_GGUF := gte-reranker-modernbert-base-Q8_0.gguf
EMBED_GGUF_URL  := https://huggingface.co/ggml-org/embeddinggemma-300m-qat-q8_0-GGUF/resolve/main/$(EMBED_GGUF)
RERANK_GGUF_URL := https://huggingface.co/keisuke-miyako/gte-reranker-modernbert-base-gguf-q8_0/resolve/main/$(RERANK_GGUF)

# Brew Postgres binaries (formula installs to opt/postgresql@$(PG_MAJOR)/bin).
# Postgres major version. PINNED, and pinned to match what the installer puts
# on a box (`install.rs`: postgresql-18 + postgresql-18-pgvector). Dev ran 17
# against a fleet on 18 for a while, which is invisible until it isn't:
# pg_dump/pg_restore refuse to read a newer server's output, so `virtues backup`
# taken on a box could not be restored on a laptop for diagnosis — exactly when
# you want it. Bump both together or neither.
PG_MAJOR ?= 18
PG_BIN ?= $(shell brew --prefix postgresql@$(PG_MAJOR) 2>/dev/null)/bin

help: ## Show this help
	@grep -hE '^[a-zA-Z_-]+:.*?## ' $(MAKEFILE_LIST) \
	  | awk 'BEGIN{FS=":.*?## "}{printf "  \033[36m%-14s\033[0m %s\n", $$1, $$2}'

# ── Git, for a shared checkout ───────────────────────────────────────────────
# Several agents work in this one tree at once, so the index is a shared mutable
# resource: two agents doing add→commit interleaved means whoever commits first
# takes both sets of changes. `git commit -- <paths>` sidesteps that entirely by
# ignoring the index and committing only the named paths. The flock serializes
# the add, so a concurrent `git add` can't land in between.
#
#   make commit MSG="fix(applets): the thing" FILES="a/one.rs b/two.rs"
#
# Only paths that still exist are staged: `git rm` clears a path from the index
# AND the worktree, so no pathspec can match it and `git add` would abort the
# whole command. `git commit -- <paths>` records those deletions by itself.
# `git add -A -- <paths>` is scoped BY the pathspec and stages only what is
# under those paths — not the same thing as a bare `git add -A`, which is what
# sweeps up other agents' work.
#
# See the Branching section of CLAUDE.md. Never bare `git add -A` in this repo.

commit: ## Safely commit only your files: MSG="..." FILES="path ..."
	@[ -n "$(MSG)" ]   || { echo "error: MSG is required  —  make commit MSG=\"fix(x): y\" FILES=\"a b\""; exit 1; }
	@[ -n "$(FILES)" ] || { echo "error: FILES is required (explicit paths only; never -A)"; exit 1; }
	@branch=$$(git rev-parse --abbrev-ref HEAD); \
	case "$$branch" in staging|main) \
	  echo "refusing to commit to $$branch — work on 'wave' (see CLAUDE.md)"; exit 1;; esac; \
	for f in $(FILES); do \
	  [ -e "$$f" ] && continue; \
	  git cat-file -e "HEAD:$$f" 2>/dev/null && continue; \
	  echo "error: no such path, and not tracked in HEAD: $$f"; exit 1; \
	done; \
	tools/with-lock.sh sh -c 'set -e; ex=""; \
	  for f in $(FILES); do if [ -e "$$f" ]; then ex="$$ex $$f"; fi; done; \
	  if [ -n "$$ex" ]; then git add -A -- $$ex; fi; \
	  git commit -m "$(MSG)" -- $(FILES)' \
	  || { echo "commit failed — nothing was committed"; exit 1; }; \
	echo "→ committed to $$branch:"; git show --stat --format='  %h %s' HEAD | head -20

# The claimed file is `.sql.pending`, NOT `.sql`, and that suffix is the whole
# point. `sqlx::migrate!` globs `*.sql`, so a bare placeholder is a VALID
# MIGRATION THAT DOES NOTHING — and any box that boots between claiming the
# number and writing the SQL records it as applied. The real SQL then never
# runs, and worse, its checksum no longer matches what the DB stored, so the
# next boot refuses to start. That happened on the dev box on 2026-08-04 and
# took the shared `make dev` with it.
#
# The number is still reserved the moment this commits (the counter below reads
# any filename starting with digits), but sqlx cannot see the file until you
# rename it, which is also the signal that the SQL is actually written.
migration: ## Claim the next migration number NOW, before writing SQL: NAME=add_foo
	@[ -n "$(NAME)" ] || { echo "error: NAME is required  —  make migration NAME=add_foo"; exit 1; }
	@tools/with-lock.sh sh -c '\
	  last=$$(ls virtues-core/migrations | sed -n "s/^\([0-9][0-9]*\).*/\1/p" | sort -n | tail -1); \
	  next=$$(printf "%04d" $$(expr $$(echo $$last | sed "s/^0*//") + 1)); \
	  f="virtues-core/migrations/$${next}_$(NAME).sql.pending"; \
	  if [ -e "$$f" ] || [ -e "virtues-core/migrations/$${next}_$(NAME).sql" ]; then echo "error: $${next}_$(NAME) exists"; exit 1; fi; \
	  printf -- "-- $${next}_$(NAME)\n-- Number claimed; SQL to follow.\n--\n-- Rename to .sql once written — sqlx ignores this file until you do,\n-- which is what stops a boot from applying it as an empty migration.\n" > "$$f"; \
	  git add -- "$$f" && \
	  git commit -q -m "chore(db): claim migration $${next} ($(NAME))" -- "$$f" && \
	  echo "→ claimed $$f"; \
	  echo "   1. write the SQL into that file"; \
	  echo "   2. mv $$f virtues-core/migrations/$${next}_$(NAME).sql"; \
	  echo "   3. make commit MSG=\"...\" FILES=virtues-core/migrations/$${next}_$(NAME).sql"'

# ── First-time setup ─────────────────────────────────────────────────────────

init: ## Create .env with a freshly-generated encryption key (idempotent)
	@command -v openssl >/dev/null || { echo "error: openssl not found"; exit 1; }
	@if [ -f .env ]; then \
	  echo ".env already exists — leaving it untouched."; \
	else \
	  cp .env.example .env; \
	  key=$$(openssl rand -base64 32); \
	  tmp=$$(mktemp); \
	  sed "s|^VIRTUES_ENCRYPTION_KEY=.*|VIRTUES_ENCRYPTION_KEY=$$key|" .env > $$tmp && mv $$tmp .env; \
	  echo "wrote .env with a fresh VIRTUES_ENCRYPTION_KEY"; \
	fi

# ── Mac dev loop ─────────────────────────────────────────────────────────────
# `make dev` starts brew Postgres and runs virtues-core + web together
# (Ctrl-C stops both). Use `make dev-info` if you'd rather run them in separate
# tabs for split logs. `make dev-link` prints a login URL when you need one.

db: ## Ensure brew postgres@$(PG_MAJOR) is installed + running, db exists with pgvector
	@command -v brew >/dev/null || { echo "error: brew not installed — https://brew.sh"; exit 1; }
	@brew list postgresql@$(PG_MAJOR) >/dev/null 2>&1 || { echo "→ installing postgresql@$(PG_MAJOR)"; brew install postgresql@$(PG_MAJOR); }
	@brew list pgvector     >/dev/null 2>&1 || { echo "→ installing pgvector";     brew install pgvector;     }
	@brew services list | grep -q "postgresql@$(PG_MAJOR).*started" || { echo "→ starting postgresql@$(PG_MAJOR)"; brew services start postgresql@$(PG_MAJOR) >/dev/null; sleep 2; }
	@$(PG_BIN)/psql -d postgres -tAc "SELECT 1 FROM pg_database WHERE datname='virtues'" | grep -q 1 || { echo "→ creating db 'virtues'"; $(PG_BIN)/createdb virtues; }
	@$(PG_BIN)/psql -d virtues -c "CREATE EXTENSION IF NOT EXISTS vector" >/dev/null
	@$(PG_BIN)/psql -d postgres -tAc "SELECT 1 FROM pg_database WHERE datname='virtues_api'" | grep -q 1 || { echo "→ creating db 'virtues_api' (local virtues-api entitlements)"; $(PG_BIN)/createdb virtues_api; }
# `vector` into template1, so every database created afterwards inherits it.
# This is what lets the dev role be a NON-superuser: pgvector is not a trusted
# extension (`pg_available_extensions.trusted = f`), so `CREATE EXTENSION vector`
# needs superuser — and `#[sqlx::test]` provisions a scratch database per test
# that runs migration 0001, which creates it. Inheriting from template1 makes
# that statement a no-op instead, and the tests stop needing the privilege.
	@$(PG_BIN)/psql -d template1 -c "CREATE EXTENSION IF NOT EXISTS vector" >/dev/null
# NOT a superuser. It was `CREATE ROLE virtues WITH LOGIN SUPERUSER PASSWORD
# 'virtues'` — a superuser whose password is its own name, on a machine where
# CLAUDE.md also tells you to open pg_hba to TCP on loopback. Any local process
# that guessed once owned the cluster. CREATEDB is what the test harness
# actually needs; ownership of its own database covers the rest.
	@$(PG_BIN)/psql -d postgres -tAc "SELECT 1 FROM pg_roles WHERE rolname='virtues'" | grep -q 1 || { echo "→ creating role 'virtues' (password 'virtues' for .env compat)"; $(PG_BIN)/psql -d postgres -c "CREATE ROLE virtues WITH LOGIN CREATEDB CREATEROLE PASSWORD 'virtues'" >/dev/null; }
# Idempotent downgrade for machines whose role predates the line above.
	@$(PG_BIN)/psql -d postgres -tAc "SELECT rolsuper FROM pg_roles WHERE rolname='virtues'" | grep -q t && { echo "→ dropping SUPERUSER from role 'virtues'"; $(PG_BIN)/psql -d postgres -c "ALTER ROLE virtues NOSUPERUSER CREATEDB CREATEROLE" >/dev/null; } || true
	@$(PG_BIN)/psql -d virtues -c "ALTER DATABASE virtues OWNER TO virtues" >/dev/null 2>&1 || true
# The two least-privileged roles `server/faces.rs` drops to, plus ADMIN OPTION
# so the non-superuser `virtues` can grant them to itself at runtime — which is
# what faces.rs does on every boot. Superuser used to make that implicit; in
# PG16+ a CREATEROLE role may only grant membership in roles it has ADMIN on,
# and these are created by the cluster owner, so the option has to be handed
# over explicitly. Without it the grant fails, no applet table is readable, and
# the failure looks like a permissions bug in the applet rather than in setup.
	@$(PG_BIN)/psql -d postgres -tAc "SELECT 1 FROM pg_roles WHERE rolname='virtues_face_reader'" | grep -q 1 || $(PG_BIN)/psql -d postgres -c "CREATE ROLE virtues_face_reader NOLOGIN" >/dev/null
	@$(PG_BIN)/psql -d postgres -tAc "SELECT 1 FROM pg_roles WHERE rolname='virtues_applet_writer'" | grep -q 1 || $(PG_BIN)/psql -d postgres -c "CREATE ROLE virtues_applet_writer NOLOGIN" >/dev/null
	@$(PG_BIN)/psql -d postgres -c "GRANT virtues_face_reader, virtues_applet_writer TO virtues WITH ADMIN OPTION" >/dev/null
	@echo "✓ postgres ready on :5432, dbs 'virtues' (pgvector) + 'virtues_api', role 'virtues'"

db-stop: ## Stop the brew postgres service (preserves data)
	@brew services stop postgresql@$(PG_MAJOR)

# Run a local virtues-api as part of `make dev` whenever core points at a
# localhost api (the default). Empty when VIRTUES_API_URL is overridden to prod.
DEV_LOCAL_API := $(filter http://localhost%,$(VIRTUES_API_URL))

dev: db ## Run the full LOCAL dev stack: postgres + api (:9002) + core (:8000) + web + embed (Ctrl-C stops all)
	@if [ "$(WITH_EMBED)" = "1" ]; then $(MAKE) _embed-ensure; fi
	@echo "→ starting$(if $(DEV_LOCAL_API), virtues-api (:9002) +,) virtues-core (:8000) + web (:$(DEV_WEB_PORT))$(if $(filter 1,$(WITH_EMBED)), + embed :18181/rerank :18182,). Ctrl-C stops all."
	@echo "  api: $(VIRTUES_API_URL)  atlas: $(VIRTUES_ATLAS_URL)$(if $(DEV_LOCAL_API),  (fully local — AI works, no checkout), (PROD — real billing, real rows))"
	@echo "  lands straight in the app (setup skipped).$(if $(filter 1,$(WITH_EMBED)),, search off — 'make dev WITH_EMBED=1' or 'make dev-embed' to enable.)"
	@trap 'kill 0' EXIT INT TERM; \
	$(if $(DEV_LOCAL_API),$(MAKE) dev-api & ,) \
	$(MAKE) dev-core & \
	$(MAKE) dev-web & \
	if [ "$(WITH_EMBED)" = "1" ]; then $(MAKE) _embed-run & fi; \
	wait

dev-info: db ## Print the manual multi-tab dev instructions (when you want split logs)
	@echo ""
	@echo "Fully-local stack (the default — AI works, no checkout):"
	@echo "  tab 0:  make dev-api            # local virtues-api on :9002 (seeded wallet)"
	@echo "  tab 1:  make dev-core           # points at localhost api by default"
	@echo "  tab 2:  make dev-web"
	@echo ""
	@echo "Then 'make dev-link' to get a login URL."

dev-core: ## Run virtues-core on the host (HTTP :8000, auto-migrates + prod-seeds). WATCH=1 to auto-restart on .rs changes
	@if [ "$(WATCH)" = "1" ] && [ -z "$(CARGO_WATCH)" ]; then \
	  echo "→ WATCH=1 but cargo-watch not found — running once. Install: cargo install cargo-watch"; \
	fi
	RUST_LOG="$(RUST_LOG),noq_udp=error" \
	SQLX_OFFLINE="$(SQLX_OFFLINE)" \
	VIRTUES_DEV_SKIP_SETUP="$(VIRTUES_DEV_SKIP_SETUP)" \
	ENVIRONMENT=dev \
	DATABASE_URL=$(DEV_DB_URL) \
	VIRTUES_API_URL=$(VIRTUES_API_URL) \
	VIRTUES_API_KEY=$(DEV_API_KEY) \
	VIRTUES_ATLAS_URL=$(VIRTUES_ATLAS_URL) \
	VIRTUES_HTTPS_PORT=0 \
	VIRTUES_WEB_PORT=$(DEV_WEB_PORT) \
	VIRTUES_MODELS_DIR=$(VIRTUES_MODELS_DIR) \
	$(DEV_CORE_RUN)

seed: db ## Load demo data (people/places/orgs/events) into the dev DB. Idempotent — safe to re-run.
	SQLX_OFFLINE="$(SQLX_OFFLINE)" \
	ENVIRONMENT=dev \
	DATABASE_URL=postgres://virtues:virtues@localhost:5432/virtues \
	cargo run -p virtues -- seed

dev-api: db ## Run virtues-api locally on :9002 (standalone, dev-seeded wallet)
	@echo "→ local virtues-api on :9002. Point dev-core at it with:"
	@echo "    make dev-core VIRTUES_API_URL=http://localhost:9002"
	@echo "  Real upstream spend applies (fake wallet, $$20/day + $$5/call caps)."
	SQLX_OFFLINE="$(SQLX_OFFLINE)" \
	ENVIRONMENT=dev \
	VIRTUES_API_DATABASE_URL=$(VIRTUES_API_DATABASE_URL) \
	VIRTUES_API_KEY=$(VIRTUES_API_KEY) \
	cargo run -p virtues-api

dev-web: ## Run the SvelteKit dev server on :$(DEV_WEB_PORT)
	cd apps/web && pnpm dev --port $(DEV_WEB_PORT)

# Ensure the llama-server binary + both GGUFs are present (idempotent; the
# download runs once, then the `test -s` guards skip it). `make dev` calls this
# up front so the ~480 MB fetch happens before the concurrent stack starts,
# rather than racing the cargo/vite output.
_embed-ensure:
	@command -v llama-server >/dev/null || { echo "→ installing llama.cpp (provides llama-server)"; brew install llama.cpp; }
	@mkdir -p "$(VIRTUES_MODELS_DIR)"
	@test -s "$(VIRTUES_MODELS_DIR)/$(EMBED_GGUF)" || { \
	  echo "→ downloading $(EMBED_GGUF) (~320 MB, one-time)…"; \
	  curl -fL --progress-bar "$(EMBED_GGUF_URL)" -o "$(VIRTUES_MODELS_DIR)/$(EMBED_GGUF).part" \
	    && mv "$(VIRTUES_MODELS_DIR)/$(EMBED_GGUF).part" "$(VIRTUES_MODELS_DIR)/$(EMBED_GGUF)"; }
	@test -s "$(VIRTUES_MODELS_DIR)/$(RERANK_GGUF)" || { \
	  echo "→ downloading $(RERANK_GGUF) (~160 MB, one-time)…"; \
	  curl -fL --progress-bar "$(RERANK_GGUF_URL)" -o "$(VIRTUES_MODELS_DIR)/$(RERANK_GGUF).part" \
	    && mv "$(VIRTUES_MODELS_DIR)/$(RERANK_GGUF).part" "$(VIRTUES_MODELS_DIR)/$(RERANK_GGUF)"; }
	@$(MAKE) --no-print-directory _pdfium-ensure

# libpdfium for document extraction (dev-only convenience: direct from
# pdfium-binaries; the box gets it SHA-verified from OUR models release via
# the installer). Version must track PDFIUM_VERSION in
# tools/virtues-installer/src/config.rs.
PDFIUM_VERSION := 7961
PDFIUM_DIR := $(VIRTUES_MODELS_DIR)/pdfium
_pdfium-ensure:
	@test -s "$(PDFIUM_DIR)/libpdfium.dylib" || { \
	  echo "→ downloading pdfium $(PDFIUM_VERSION) (mac-arm64, one-time)…"; \
	  mkdir -p "$(PDFIUM_DIR)"; \
	  curl -fL --progress-bar "https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F$(PDFIUM_VERSION)/pdfium-mac-arm64.tgz" -o "$(PDFIUM_DIR)/pdfium.tgz" \
	    && tar -xzf "$(PDFIUM_DIR)/pdfium.tgz" -C "$(PDFIUM_DIR)" lib/libpdfium.dylib \
	    && mv "$(PDFIUM_DIR)/lib/libpdfium.dylib" "$(PDFIUM_DIR)/libpdfium.dylib" \
	    && rm -rf "$(PDFIUM_DIR)/pdfium.tgz" "$(PDFIUM_DIR)/lib"; }

# Run the two sidecars (assumes models present; ~1 GB resident, ~0% CPU
# idle). `-lv 1` quiets llama.cpp's startup spam (device-info/slot/warmup) while
# keeping the "model loaded / listening" line, warnings, and errors.
# `--pooling mean` matches EmbeddingGemma (and the installer's unit); cls pooling
# would emit the wrong sentence vector.
_embed-run:
	@trap 'kill 0' INT TERM; \
	  llama-server -lv 1 --embedding --pooling mean -m "$(VIRTUES_MODELS_DIR)/$(EMBED_GGUF)"  --host 127.0.0.1 --port 18181 -c 2048 -b 2048 -ub 2048 & \
	  llama-server -lv 1 --rerank                  -m "$(VIRTUES_MODELS_DIR)/$(RERANK_GGUF)" --host 127.0.0.1 --port 18182 -c 8192 -b 8192 -ub 8192 & \
	  wait

dev-embed: _embed-ensure ## Run local embed (:18181) + rerank (:18182) llama-server sidecars (models cached in .data/)
	@echo "→ embed :18181 + rerank :18182 (Ctrl-C stops both)."
	@$(MAKE) _embed-run

dev-link: ## Print a login URL for the local dev stack (no prompts, no .env writes)
	ENVIRONMENT=dev \
	DATABASE_URL=postgres://virtues:virtues@localhost:5432/virtues \
	VIRTUES_API_URL=$(VIRTUES_API_URL) \
	VIRTUES_ATLAS_URL=$(VIRTUES_ATLAS_URL) \
	VIRTUES_WEB_PORT=$(DEV_WEB_PORT) \
	cargo run -p virtues -- link

dev-reset: ## Drop + recreate the dev dbs (DESTRUCTIVE, dev only)
	@echo "→ dropping dbs 'virtues' + 'virtues_api' (DESTRUCTIVE)"
	@$(PG_BIN)/dropdb --if-exists virtues
	@$(PG_BIN)/dropdb --if-exists virtues_api
	@$(PG_BIN)/createdb virtues
	@$(PG_BIN)/createdb virtues_api
	@$(PG_BIN)/psql -d virtues -c "CREATE EXTENSION IF NOT EXISTS vector" >/dev/null
	@echo "✓ fresh dbs. Run 'make dev-core' (+ 'make dev-api') to migrate + seed."

# ── Real-data dev (browse your actual box locally) ───────────────────────────
# `dev-pull` snapshots the live box's Postgres into a THROWAWAY local db
# ($(DEV_BOXCOPY_DB)); `dev-real` then runs the stack against it. The snapshot is
# slow (network-bound), so pull occasionally and run `dev-real` freely between pulls.
#
# OSS-SAFETY: the raw dump lands OUTSIDE the repo (in $$TMPDIR) and is deleted the
# instant the restore finishes — including on Ctrl-C — so no real data can ever be
# staged into git. The restored db lives in Postgres's own data dir, not the repo.
# `*.dump`/`*.pgdump` are also git-ignored as belt-and-suspenders.
#
# TODO(scale): today the whole box fits in one dump. When it outgrows that, add a
# `SINCE=` window (e.g. last 90 days) — a per-table time filter with referential
# integrity, so the pull stays ~constant regardless of total history. Media never
# rides along (it's on-disk on the box); fetch it lazily over the box loopback.
#
# EVERY APPLET IS DISABLED IN THE COPY, as the last step of the restore. The
# scheduler has no off switch, so whatever the box had armed starts firing here
# every 15 minutes against data it cannot act on:
#
#   · credentialed syncs (gmail, calendar, plaid) decrypt their OAuth tokens
#     with the BOX's key, which never leaves the box — so they can't work on a
#     laptop, not now and not after any amount of fixing;
#   · the embedding/extraction applets want the llama sidecars on :18181/:18182,
#     which `dev-real` doesn't start;
#   · and the ones that DO run would write into a snapshot nobody keeps.
#
# The result was a wall of ERROR lines every quarter hour, which trains you to
# ignore the log — the expensive kind of noise. Re-enable individually in the
# copy if you're actually working on an applet; it's a throwaway, and the next
# pull disarms them again.
dev-pull: db ## Snapshot the live box's Postgres into a throwaway local db (~min, network-bound). Run occasionally.
	@dump="$${TMPDIR:-/tmp}/virtues-box.$$$$.dump"; \
	trap 'rm -f "$$dump"' EXIT INT TERM; \
	echo "→ dumping box '$(DEV_BOX_SSH)' Postgres (structured spine only — media stays on the box)…"; \
	ssh $(DEV_BOX_SSH) 'sudo -u postgres pg_dump -Fc virtues' > "$$dump" || { echo "✖ box dump failed (is 'ssh $(DEV_BOX_SSH)' reachable?)"; exit 1; }; \
	echo "→ rebuilding local '$(DEV_BOXCOPY_DB)' from the snapshot…"; \
	$(PG_BIN)/dropdb --if-exists $(DEV_BOXCOPY_DB); \
	$(PG_BIN)/createdb $(DEV_BOXCOPY_DB); \
	$(PG_BIN)/psql -d $(DEV_BOXCOPY_DB) -c "CREATE EXTENSION IF NOT EXISTS vector" >/dev/null; \
	$(PG_BIN)/pg_restore --no-owner -d $(DEV_BOXCOPY_DB) "$$dump" || true; \
	echo "→ disarming applets in the copy…"; \
	$(PG_BIN)/psql -d $(DEV_BOXCOPY_DB) -tAc "UPDATE app_applets SET enabled = false WHERE enabled" >/dev/null 2>&1 || true; \
	echo "✓ '$(DEV_BOXCOPY_DB)' refreshed ($$($(PG_BIN)/psql -d $(DEV_BOXCOPY_DB) -tAc "SELECT pg_size_pretty(pg_database_size('$(DEV_BOXCOPY_DB)'))")), applets disabled. Raw dump deleted. Run 'make dev-real'."

dev-real: db ## Run dev-core + dev-web against your real-box snapshot (virtues_boxcopy). Auto-pulls on first use. Ctrl-C stops all.
	@$(PG_BIN)/psql -d postgres -tAc "SELECT 1 FROM pg_database WHERE datname='$(DEV_BOXCOPY_DB)'" | grep -q 1 || { \
	  echo "→ no '$(DEV_BOXCOPY_DB)' yet — pulling a fresh box snapshot first…"; $(MAKE) --no-print-directory dev-pull; }
	@echo "→ core + web against REAL data ('$(DEV_BOXCOPY_DB)'). Credentialed actions won't decrypt (box key stays on the box); data browsing works. Ctrl-C stops all."
	@trap 'kill 0' INT TERM; \
	  export VIRTUES_SKIP_MIGRATIONS=1; \
	  $(MAKE) --no-print-directory dev-core DEV_DB_URL="postgres://virtues:virtues@localhost:5432/$(DEV_BOXCOPY_DB)" & \
	  $(MAKE) --no-print-directory dev-web & \
	  wait

dev-wipe-mac: ## Unpair this Mac (clear keychain bundle + ~/.virtues/bundle.json + proxy LaunchAgent) to restart pairing
	@echo "→ unpairing this Mac from its box (keychain + bundle.json + LaunchAgent)"
	@launchctl bootout gui/$$(id -u)/com.virtues.client 2>/dev/null || true
	@rm -f $$HOME/Library/LaunchAgents/com.virtues.client.plist
	@pkill -f virtues-client 2>/dev/null || true
	@for a in default-box default-box-wg-private default-box-device-id default-box-credential-id default-box-server-pin; do \
		security delete-generic-password -s virtues-client -a "$$a" >/dev/null 2>&1 || true; \
	done
	@rm -f $$HOME/.virtues/bundle.json
	@echo "✓ Mac unpaired — reopen the app to get the code-entry screen"

dev-clean: dev-wipe-mac dev-reset ## Full local reset: unpair this Mac + drop/recreate dev dbs (fresh e2e from scratch)
	@echo "✓ clean slate — run 'make dev' to bring the local stack back up"

# ── iroh client FFI (uniffi Rust→Swift; consumed by the iOS app + Mac collector)
# The clients reach the box over iroh via a generated xcframework (gitignored —
# a build artifact). Regenerate it before building a client. `make mac-app` runs
# the macOS one automatically; iOS devs run `make iroh-ffi-ios` before opening
# Xcode. Both are idempotent.

web-test: ## Run the frontend checks CI runs (airlock router tests + svelte-check)
	@node --test "apps/web/src-tauri/ui/*.test.mjs"
	@cd apps/web && pnpm check

iroh-ffi-ios: ## Build VirtuesIroh.xcframework for the iOS app (run before Xcode)
	crates/virtues-iroh-ffi/build-ios.sh

iroh-ffi-mac: ## Build VirtuesIrohMac.xcframework for the Mac collector
	crates/virtues-iroh-ffi/build-macos.sh

# ── iOS release ─────────────────────────────────────────────────────────────
# The TestFlight path, which was tribal knowledge until 2026-08-05 and went two
# weeks stale unnoticed as a result. Encodes the two traps that each cost a
# build: the FFI framework must be rebuilt first, and the app icons must be
# flattened to RGB after any `tauri icon` run or App Store validation 409s on
# an alpha channel it rejects even when fully opaque.
#
# Upload stays manual — it publishes under your Apple ID.

ios-release: ## Build a signed iOS IPA for TestFlight (VERSION=1.2.6 to bump first)
	tools/ios-release.sh $(VERSION)

# ── icons ───────────────────────────────────────────────────────────────────
# Everything derives from apps/web/src-tauri/icons/AppIcon.icon. The outputs
# are committed because release CI runs `pnpm tauri build` directly and cannot
# compile them, and because handing tauri a `.icon` breaks the bundle step
# outright. Run this after any change to the mark, then commit what it wrote.

icons: ## Rebuild every icon artifact from AppIcon.icon (needs Xcode 26)
	tools/build-icons.sh

# ── macOS desktop app (one signed DMG: app + both helper sidecars) ───────────

# Auto-launch the freshly-built app after `make mac-app` (OPEN=0 to skip). We
# HARD-KILL any running instance first: the app hides-on-close (doesn't quit),
# and `open` on a live app just re-activates the OLD in-memory binary — so a
# polite `osascript quit` left you staring at stale code after every rebuild.
# pkill -9 guarantees the new binary actually loads.
#
# Ask cargo where the bundle is rather than globbing `src-tauri/target`: that
# path holds a pre-shared-target-dir build from July, so the find would have
# hit a months-old .app and cheerfully relaunched it as "freshly built".
OPEN ?= 1
mac-app: ## Build the macOS app (Virtues.app + sidecars) and open it (OPEN=0 to skip)
	tools/build-mac-app.sh
	@if [ "$(OPEN)" = "1" ]; then \
	  bundle=$$(cargo metadata --no-deps --format-version 1 --manifest-path apps/web/src-tauri/Cargo.toml | jq -r .target_directory)/release/bundle/macos/Virtues.app; \
	  app=$$([ -d "$$bundle" ] && echo "$$bundle"); \
	  if [ -n "$$app" ]; then \
	    echo "→ relaunching $$app"; \
	    pkill -9 -f "Virtues.app" >/dev/null 2>&1 || true; \
	    sleep 1; \
	    open "$$app"; \
	  else \
	    echo "⚠ built .app not found to open (check the build output above)"; \
	  fi; \
	fi

# ── Cloud-service deploy (Virtues-operated; not part of self-host) ───────────
# Build + push services/virtues-{atlas,api} images to ECR :latest. Rolling the
# running service is a separate step (the service pulls the new :latest).
# Needs AWS CLI v2 (configured) + Docker. No secrets are committed — auth comes
# from your AWS CLI config, account ID is discovered at run time.

deploy-atlas: ## Build + push services/virtues-atlas image to ECR :latest
	@$(MAKE) _ecr-push SVC=virtues-atlas DOCKERFILE=services/virtues-atlas/Dockerfile

deploy-virtues-api: ## Build + push services/virtues-api image to ECR :latest
	@$(MAKE) _ecr-push SVC=virtues-api DOCKERFILE=services/virtues-api/Dockerfile

_ecr-push:
	@command -v aws >/dev/null || { echo "error: AWS CLI not installed — https://docs.aws.amazon.com/cli/"; exit 1; }
	@acct=$$(aws sts get-caller-identity --query Account --output text 2>/dev/null) \
	  || { echo "error: AWS CLI not configured — run \`aws configure\`"; exit 1; }; \
	reg="$$acct.dkr.ecr.$(AWS_REGION).amazonaws.com"; \
	echo "→ pushing $(SVC) to $$reg"; \
	aws ecr get-login-password --region $(AWS_REGION) | docker login --username AWS --password-stdin "$$reg" && \
	docker build --platform linux/amd64 -f $(DOCKERFILE) -t "$$reg/$(SVC):latest" . && \
	docker push "$$reg/$(SVC):latest"
# NB: `&&`, not `;`. With `;` a FAILED build still ran the push, which happily
# re-uploaded whatever stale image was already tagged `:latest` locally and
# printed a digest — a deploy that looks successful and changes nothing. That
# is the worst possible failure mode for a manual deploy path, and it is how a
# broken build sat here unnoticed.
