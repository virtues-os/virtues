# Virtues — Mac dev quickstart + cloud-service deploys.
#
# Mac dev uses native brew Postgres (no Docker daemon).
# Linux home box is installed natively via tools/bootstrap.sh (no Docker).
# Cloud services (virtues-atlas / virtues-api) deploy as Docker images to ECR.

.DEFAULT_GOAL := help
.PHONY: help init dev dev-info dev-core dev-api dev-web dev-embed _embed-ensure _embed-run \
        dev-link dev-reset dev-wipe-mac dev-clean db db-stop deploy-atlas deploy-virtues-api _ecr-push mac-app \
        iroh-ffi-ios iroh-ffi-mac

AWS_REGION ?= us-east-1

# Mac dev is FULLY LOCAL by default and never touches prod: `make dev` runs a
# local virtues-api (seeded wallet → AI works with no checkout) and points atlas
# at localhost (nothing there — so a stray "Connect" can't mint rows in prod).
#
# To exercise the real billing/onboarding flow against the deployed staging
# services: `make dev STAGING=1` (test-mode Stripe, real claim). Or override
# VIRTUES_API_URL / VIRTUES_ATLAS_URL individually for prod or a custom host.
ifeq ($(STAGING),1)
VIRTUES_API_URL   ?= https://api-staging.virtues.com
VIRTUES_ATLAS_URL ?= https://atlas-staging.virtues.com
else
VIRTUES_API_URL   ?= http://localhost:9002
VIRTUES_ATLAS_URL ?= http://localhost:9100
endif
DEV_WEB_PORT ?= 5173

# Local virtues-api (`make dev-api`): its own logical db + a known dev api_key.
# The key is funded by the gated seed in virtues-api (ENVIRONMENT=dev), and
# virtues-core presents it via VIRTUES_API_KEY — but only when pointing at a
# LOCAL api. Against prod we must use the real vault key, so DEV_API_KEY
# stays empty there (the client override no-ops on an empty value).
VIRTUES_API_DATABASE_URL ?= postgres://virtues:virtues@localhost:5432/virtues_api
VIRTUES_API_KEY ?= dev-local-key
DEV_API_KEY := $(if $(filter http://localhost%,$(VIRTUES_API_URL)),$(VIRTUES_API_KEY),)

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

# Brew Postgres binaries (formula installs to opt/postgresql@17/bin).
PG_BIN ?= $(shell brew --prefix postgresql@17 2>/dev/null)/bin

help: ## Show this help
	@grep -hE '^[a-zA-Z_-]+:.*?## ' $(MAKEFILE_LIST) \
	  | awk 'BEGIN{FS=":.*?## "}{printf "  \033[36m%-12s\033[0m %s\n", $$1, $$2}'

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

db: ## Ensure brew postgres@17 is installed + running, db exists with pgvector
	@command -v brew >/dev/null || { echo "error: brew not installed — https://brew.sh"; exit 1; }
	@brew list postgresql@17 >/dev/null 2>&1 || { echo "→ installing postgresql@17"; brew install postgresql@17; }
	@brew list pgvector     >/dev/null 2>&1 || { echo "→ installing pgvector";     brew install pgvector;     }
	@brew services list | grep -q "postgresql@17.*started" || { echo "→ starting postgresql@17"; brew services start postgresql@17 >/dev/null; sleep 2; }
	@$(PG_BIN)/psql -d postgres -tAc "SELECT 1 FROM pg_database WHERE datname='virtues'" | grep -q 1 || { echo "→ creating db 'virtues'"; $(PG_BIN)/createdb virtues; }
	@$(PG_BIN)/psql -d virtues -c "CREATE EXTENSION IF NOT EXISTS vector" >/dev/null
	@$(PG_BIN)/psql -d postgres -tAc "SELECT 1 FROM pg_database WHERE datname='virtues_api'" | grep -q 1 || { echo "→ creating db 'virtues_api' (local virtues-api entitlements)"; $(PG_BIN)/createdb virtues_api; }
	@$(PG_BIN)/psql -d postgres -tAc "SELECT 1 FROM pg_roles WHERE rolname='virtues'" | grep -q 1 || { echo "→ creating role 'virtues' (password 'virtues' for .env compat)"; $(PG_BIN)/psql -d postgres -c "CREATE ROLE virtues WITH LOGIN SUPERUSER PASSWORD 'virtues'" >/dev/null; }
	@echo "✓ postgres ready on :5432, dbs 'virtues' (pgvector) + 'virtues_api', role 'virtues'"

db-stop: ## Stop the brew postgres service (preserves data)
	@brew services stop postgresql@17

# Run a local virtues-api as part of `make dev` whenever core points at a
# localhost api (the default). Empty when pointing at staging/prod.
DEV_LOCAL_API := $(filter http://localhost%,$(VIRTUES_API_URL))

dev: db ## Run the full LOCAL dev stack: postgres + api (:9002) + core (:8000) + web + embed (Ctrl-C stops all). `make dev STAGING=1` for the real billing flow.
	@if [ "$(WITH_EMBED)" = "1" ]; then $(MAKE) _embed-ensure; fi
	@echo "→ starting$(if $(DEV_LOCAL_API), virtues-api (:9002) +,) virtues-core (:8000) + web (:$(DEV_WEB_PORT))$(if $(filter 1,$(WITH_EMBED)), + embed :18181/rerank :18182,). Ctrl-C stops all."
	@echo "  api: $(VIRTUES_API_URL)  atlas: $(VIRTUES_ATLAS_URL)$(if $(DEV_LOCAL_API),  (fully local — AI works, no checkout), (staging/prod — real billing flow))"
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
	@echo ""
	@echo "To exercise the real billing/onboarding flow against staging:"
	@echo "  tab 1:  make dev-core STAGING=1   (real claim, test-mode Stripe; skip dev-api)"

dev-core: ## Run virtues-core on the host (HTTP :8000, auto-migrates + prod-seeds). WATCH=1 to auto-restart on .rs changes
	@if [ "$(WATCH)" = "1" ] && [ -z "$(CARGO_WATCH)" ]; then \
	  echo "→ WATCH=1 but cargo-watch not found — running once. Install: cargo install cargo-watch"; \
	fi
	RUST_LOG="$(RUST_LOG),noq_udp=error" \
	SQLX_OFFLINE="$(SQLX_OFFLINE)" \
	VIRTUES_DEV_SKIP_SETUP="$(VIRTUES_DEV_SKIP_SETUP)" \
	ENVIRONMENT=dev \
	DATABASE_URL=postgres://virtues:virtues@localhost:5432/virtues \
	VIRTUES_API_URL=$(VIRTUES_API_URL) \
	VIRTUES_API_KEY=$(DEV_API_KEY) \
	VIRTUES_ATLAS_URL=$(VIRTUES_ATLAS_URL) \
	VIRTUES_HTTPS_PORT=0 \
	VIRTUES_WEB_PORT=$(DEV_WEB_PORT) \
	VIRTUES_MODELS_DIR=$(VIRTUES_MODELS_DIR) \
	$(DEV_CORE_RUN)

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

iroh-ffi-ios: ## Build VirtuesIroh.xcframework for the iOS app (run before Xcode)
	crates/virtues-iroh-ffi/build-ios.sh

iroh-ffi-mac: ## Build VirtuesIrohMac.xcframework for the Mac collector
	crates/virtues-iroh-ffi/build-macos.sh

# ── macOS desktop app (one signed DMG: app + both helper sidecars) ───────────

# Auto-launch the freshly-built app after `make mac-app` (OPEN=0 to skip). We
# HARD-KILL any running instance first: the app hides-on-close (doesn't quit),
# and `open` on a live app just re-activates the OLD in-memory binary — so a
# polite `osascript quit` left you staring at stale code after every rebuild.
# pkill -9 guarantees the new binary actually loads.
OPEN ?= 1
mac-app: ## Build the macOS app (Virtues.app + sidecars) and open it (OPEN=0 to skip)
	tools/build-mac-app.sh
	@if [ "$(OPEN)" = "1" ]; then \
	  app=$$(find apps/web/src-tauri/target -maxdepth 6 -path '*/bundle/macos/Virtues.app' -print -quit 2>/dev/null); \
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
	aws ecr get-login-password --region $(AWS_REGION) | docker login --username AWS --password-stdin "$$reg"; \
	docker build --platform linux/amd64 -f $(DOCKERFILE) -t "$$reg/$(SVC):latest" .; \
	docker push "$$reg/$(SVC):latest"
