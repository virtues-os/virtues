# Virtues — Mac dev quickstart + cloud-service deploys.
#
# Mac dev uses native brew Postgres (no Docker daemon).
# Linux home box is installed natively via scripts/install.sh (no Docker).
# Cloud services (atlas / virtues-api) deploy as Docker images to ECR.

.DEFAULT_GOAL := help
.PHONY: help init dev dev-core dev-web dev-link dev-reset db db-stop \
        deploy-atlas deploy-virtues-api _ecr-push

AWS_REGION ?= us-east-1

# Mac dev points at PROD virtues-api by default — your real bearer + real sub.
# Override with VIRTUES_API_URL=http://localhost:9002 to run a local api too.
VIRTUES_API_URL ?= https://api.virtues.com
VIRTUES_ATLAS_URL ?= https://atlas.virtues.com
DEV_WEB_PORT ?= 5173

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
# `make dev` starts brew Postgres, then you open two terminal tabs:
#   tab 1:  make dev-core
#   tab 2:  make dev-web
#   tab 3:  make dev-link    (when you need a login URL)

db: ## Ensure brew postgres@17 is installed + running, db exists with pgvector
	@command -v brew >/dev/null || { echo "error: brew not installed — https://brew.sh"; exit 1; }
	@brew list postgresql@17 >/dev/null 2>&1 || { echo "→ installing postgresql@17"; brew install postgresql@17; }
	@brew list pgvector     >/dev/null 2>&1 || { echo "→ installing pgvector";     brew install pgvector;     }
	@brew services list | grep -q "postgresql@17.*started" || { echo "→ starting postgresql@17"; brew services start postgresql@17 >/dev/null; sleep 2; }
	@$(PG_BIN)/psql -d postgres -tAc "SELECT 1 FROM pg_database WHERE datname='virtues'" | grep -q 1 || { echo "→ creating db 'virtues'"; $(PG_BIN)/createdb virtues; }
	@$(PG_BIN)/psql -d virtues -c "CREATE EXTENSION IF NOT EXISTS vector" >/dev/null
	@$(PG_BIN)/psql -d postgres -tAc "SELECT 1 FROM pg_roles WHERE rolname='virtues'" | grep -q 1 || { echo "→ creating role 'virtues' (password 'virtues' for .env compat)"; $(PG_BIN)/psql -d postgres -c "CREATE ROLE virtues WITH LOGIN SUPERUSER PASSWORD 'virtues'" >/dev/null; }
	@echo "✓ postgres ready on :5432, db 'virtues' with pgvector + role 'virtues'"

db-stop: ## Stop the brew postgres service (preserves data)
	@brew services stop postgresql@17

dev: db ## Start postgres + print next steps
	@echo ""
	@echo "Open two terminal tabs and run:"
	@echo "  tab 1:  make dev-core"
	@echo "  tab 2:  make dev-web"
	@echo ""
	@echo "Then 'make dev-link' to get a login URL."

dev-core: ## Run virtues-core on the host (HTTP :8000, auto-migrates + prod-seeds)
	ENVIRONMENT=dev \
	DATABASE_URL=postgres://virtues:virtues@localhost:5432/virtues \
	VIRTUES_API_URL=$(VIRTUES_API_URL) \
	VIRTUES_ATLAS_URL=$(VIRTUES_ATLAS_URL) \
	VIRTUES_HTTPS_PORT=0 \
	VIRTUES_WEB_PORT=$(DEV_WEB_PORT) \
	cargo run -p virtues

dev-web: ## Run the SvelteKit dev server on :$(DEV_WEB_PORT)
	cd apps/web && pnpm dev --port $(DEV_WEB_PORT)

dev-link: ## Print a login URL for the local dev stack (no prompts, no .env writes)
	ENVIRONMENT=dev \
	DATABASE_URL=postgres://virtues:virtues@localhost:5432/virtues \
	VIRTUES_API_URL=$(VIRTUES_API_URL) \
	VIRTUES_ATLAS_URL=$(VIRTUES_ATLAS_URL) \
	VIRTUES_WEB_PORT=$(DEV_WEB_PORT) \
	cargo run -p virtues -- link

dev-reset: ## Drop + recreate the dev db (DESTRUCTIVE, dev only)
	@echo "→ dropping db 'virtues' (DESTRUCTIVE)"
	@$(PG_BIN)/dropdb --if-exists virtues
	@$(PG_BIN)/createdb virtues
	@$(PG_BIN)/psql -d virtues -c "CREATE EXTENSION IF NOT EXISTS vector" >/dev/null
	@echo "✓ fresh db. Run 'make dev-core' to migrate + seed."

# ── Cloud-service deploy (Virtues-operated; not part of self-host) ───────────
# Build + push services/{atlas,virtues-api} images to ECR :latest. Rolling the
# running service is a separate step (the service pulls the new :latest).
# Needs AWS CLI v2 (configured) + Docker. No secrets are committed — auth comes
# from your AWS CLI config, account ID is discovered at run time.

deploy-atlas: ## Build + push services/atlas image to ECR :latest
	@$(MAKE) _ecr-push SVC=virtues-atlas DOCKERFILE=services/atlas/Dockerfile

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
