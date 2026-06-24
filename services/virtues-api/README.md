# virtues-api

**Open Source AI Budget Proxy**

virtues-api is a "prepaid arcade card" model for AI API access. It enforces per-user budgets with 0ms latency by checking balances in RAM, not the database.

## Privacy Guarantee

This code is open source so you can verify we don't log your data.

### What virtues-api Does

- Validates internal requests from Core via shared secret header
- Checks budget balance in RAM (instant)
- Routes requests to LLM providers (OpenAI, Anthropic, Cerebras)
- Extracts token usage from responses for billing
- Batches budget updates to database every 30 seconds

### What virtues-api Does NOT Do

- Log request bodies (your prompts)
- Log response bodies (AI completions)
- Store any content for training
- Analyze or inspect payloads
- Send data to third parties

We only extract the `usage` field from responses to calculate cost. The actual prompt and completion content is never read or logged.

## Architecture

```
┌────────────────────────────────────────────────────────────────┐
│  Core Backend                                                   │
│  App → http://localhost:9002 (X-Internal-Secret header)        │
└────────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌────────────────────────────────────────────────────────────────┐
│  virtues-api (Port 9002)                                         │
│                                                                │
│  1. Validate internal secret header                            │
│  2. Check budget in RAM                                        │
│  3. Route to provider ─────────────────────────────────────┐  │
│  4. Extract usage from response                             │  │
│  5. Deduct cost from budget                                 │  │
└─────────────────────────────────────────────────────────────│──┘
                                                              │
              ┌───────────────────┬───────────────────────────┘
              ▼                   ▼                   ▼
┌─────────────────────┐ ┌─────────────────────┐ ┌─────────────────────┐
│  OpenAI             │ │  Anthropic          │ │  Cerebras           │
│  GPT-4o (Smart)     │ │  Claude (Smart)     │ │  Llama (Instant)    │
└─────────────────────┘ └─────────────────────┘ └─────────────────────┘
```

## Authentication

virtues-api uses header-based authentication for internal service communication:

```
X-Internal-Secret: <shared_secret>   # Required - validates request origin
X-User-Id: <user_id>                 # Optional - defaults to "system"
```

Security model:
- Network isolation ensures only Core can reach virtues-api (host sidecar)
- Shared secret validates request origin
- User ID tracks budget usage

## Configuration

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `VIRTUES_API_INTERNAL_SECRET` | Yes | - | Shared secret for internal auth (min 32 chars) |
| `OPENAI_API_KEY` | * | - | OpenAI API key for GPT models |
| `ANTHROPIC_API_KEY` | * | - | Anthropic API key for Claude models |
| `CEREBRAS_API_KEY` | * | - | Cerebras API key for Llama models |
| `DEFAULT_SMART_MODEL` | No | `gpt-4o` | Default model for "smart" requests |
| `DEFAULT_INSTANT_MODEL` | No | `cerebras/llama-3.3-70b` | Default model for "instant" requests |
| `VIRTUES_API_FLUSH_INTERVAL` | No | `30` | Seconds between budget flushes |
| `VIRTUES_API_DEFAULT_BUDGET` | No | `5.0` | Default budget for new users (USD) |
| `VIRTUES_API_PORT` | No | `9002` | Port to listen on (9000 used by MinIO) |

\* At least one provider API key is required.

## Model Routing

virtues-api automatically routes requests to the appropriate provider based on the model name:

| Model Pattern | Provider | Example |
|---------------|----------|---------|
| `gpt-*` | OpenAI | `gpt-4o`, `gpt-4o-mini`, `gpt-3.5-turbo` |
| `claude-*` | Anthropic | `claude-3-5-sonnet-20241022`, `claude-3-opus-20240229` |
| `cerebras/*` or `*llama*` | Cerebras | `cerebras/llama-3.3-70b`, `llama-3.1-8b` |

## API Endpoints

### Health Checks

```bash
# Liveness probe
curl http://localhost:9002/health

# Readiness probe (includes provider status)
curl http://localhost:9002/ready
```

### AI Proxy (OpenAI-compatible)

All `/v1/*` routes are OpenAI-compatible:

```bash
curl -X POST http://localhost:9002/v1/chat/completions \
  -H "X-Internal-Secret: your-secret-here" \
  -H "X-User-Id: user-123" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-4o",
    "messages": [{"role": "user", "content": "Hello"}]
  }'
```

### List Available Models

```bash
curl http://localhost:9002/v1/models
```

Returns models based on which providers are configured.

## Budget Model

virtues-api uses a "check in RAM, charge in DB" model:

1. **On Boot**: Load all user budgets from database into a `DashMap`
2. **On Request**: Check balance in RAM (0ms, lock-free)
3. **After Response**: Deduct cost atomically in RAM
4. **Every 30s**: Batch flush deltas to database

This decouples authorization from persistence for maximum performance.

## Pricing

virtues-api calculates costs based on actual token usage. Approximate pricing (per 1K tokens):

| Model | Input | Output |
|-------|-------|--------|
| GPT-4o | $0.005 | $0.015 |
| GPT-4o mini | $0.00015 | $0.0006 |
| Claude 3.5 Sonnet | $0.003 | $0.015 |
| Claude 3 Opus | $0.015 | $0.075 |
| Claude 3 Haiku | $0.00025 | $0.00125 |
| Cerebras Llama | $0.0001 | $0.0001 |

## Building

```bash
# Build locally
cargo build --release

# Build container
docker build -t virtues-api .

# Or with nerdctl (for containerd)
nerdctl build -t virtues-api .
```

## Running Locally

```bash
# Set required environment variables
export VIRTUES_API_INTERNAL_SECRET="your-32-character-or-longer-secret!"
export OPENAI_API_KEY="sk-..."  # Optional
export ANTHROPIC_API_KEY="sk-..."  # Optional
export CEREBRAS_API_KEY="..."  # Optional

# Run
cargo run
```

## Deployment

Images are built and pushed to **ECR** by `make deploy-virtues-api` (and to GHCR by
CI on push to `main`/`staging`). The CI `register-version` step only POSTs a version
string to Atlas for display — **Atlas does not roll the container.** Rolling the
running service is a manual step on the EC2 host (reached via SSM RunCommand).

Environment is **not** passed inline at `docker run` time. It lives in a root-only
env-file on the host, mirroring `atlas.env`:

- prod    → `/etc/virtues/api.env`
- staging → `/etc/virtues/api-staging.env`

Both containers run with `--network host` (prod on `:9002`, staging on `:9003`), so
the old container must be stopped before the new one can bind. Canonical roll:

```bash
# on the EC2 host, as root (via: aws ssm start-session / RunShellScript)
img=<ECR repo>/virtues-api:latest
docker pull "$img"
docker rm -f virtues-api
docker run -d --name virtues-api --network host --restart unless-stopped \
  --env-file /etc/virtues/api.env "$img"
docker logs --tail 5 virtues-api    # expect: "External services: Exa=true" + "listening on 0.0.0.0:9002"
```

To add or change a secret (e.g. `EXA_API_KEY`), edit the env-file and re-run the
roll above — the file is the durable source of truth, so the value survives the next
image roll. Keep a timestamped `.bak` as `atlas.env` does.

## License

MIT
