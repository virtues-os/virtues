# Archived Docker Compose Files

**Archived**: 2026-01-22
**Reason**: Migration to Nomad orchestration

## Why These Were Archived

Per the **Virtues Infrastructure Strategy**, we are migrating from Docker Compose to HashiCorp Nomad for orchestration. The key changes:

### Old Architecture (Docker Compose)
- Multi-container orchestration via `docker-compose.yml`
- Separate containers for: Postgres, MinIO, Core API, Web Frontend
- Local development and production managed via compose overrides

### New Architecture (Nomad + Single Image)
- **No Docker Compose**: Nomad handles all orchestration
- **No Postgres container**: SQLite embedded in Rust binary
- **No Node.js frontend container**: Static SvelteKit served by Rust
- **One Image per tenant**: Single optimized Dockerfile

## What's Still Needed

The **Dockerfiles** remain in the codebase (not archived) because Nomad still uses Docker/containerd to run containers:
- `core/Dockerfile` - Primary tenant container (Rust binary + embedded SQLite + Python)
- `apps/tollbooth/Dockerfile` - Host sidecar (AI budget proxy)
- `apps/oauth-proxy/Dockerfile` - Central OAuth broker

### Archived Dockerfile
- `Dockerfile.web` - **Archived**: In the new architecture, static SvelteKit is built and copied into the `core` container, so no separate web container is needed.

## Archived Files

| Original Path | Description |
|--------------|-------------|
| `docker-compose.yml` | Main development compose |
| `docker-compose.dev.yml` | Dev overrides (MinIO, Ollama) |
| `docker-compose.prod.yml` | Production overrides |
| `docker-compose.deploy.yml` | VPS tenant deployment (was in `deploy/`) |

## Transitional Note

⚠️ **`deploy/setup.sh` temporarily still uses Docker Compose** for VPS tenant provisioning until the Nomad migration is complete. That script generates its own `docker-compose.yml` inline (not using these archived files).

Once Nomad infrastructure is deployed, `deploy/setup.sh` will be migrated to use Nomad job specs instead.

## Reference

These files are preserved for:
- Historical reference
- Extracting environment variable patterns
- Understanding previous multi-container architecture
