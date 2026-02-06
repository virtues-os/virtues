# Tollbooth

**Open Source AI Budget Proxy**

Tollbooth is a "prepaid arcade card" model for AI API access. It enforces per-user budgets with 0ms latency by checking balances in RAM, not the database.

## Privacy Guarantee

This code is open source so you can verify we don't log your data.

### What Tollbooth Does

- Validates internal requests from Core via shared secret header
- Checks budget balance in RAM (instant)
- Routes requests to LLM providers (OpenAI, Anthropic, Cerebras)
- Extracts token usage from responses for billing
- Batches budget updates to database every 30 seconds

### What Tollbooth Does NOT Do

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
│  Tollbooth (Port 9002)                                         │
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

Tollbooth uses header-based authentication for internal service communication:

```
X-Internal-Secret: <shared_secret>   # Required - validates request origin
X-User-Id: <user_id>                 # Optional - defaults to "system"
```

Security model:
- Network isolation ensures only Core can reach Tollbooth (host sidecar)
- Shared secret validates request origin
- User ID tracks budget usage

## Configuration

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `TOLLBOOTH_INTERNAL_SECRET` | Yes | - | Shared secret for internal auth (min 32 chars) |
| `OPENAI_API_KEY` | * | - | OpenAI API key for GPT models |
| `ANTHROPIC_API_KEY` | * | - | Anthropic API key for Claude models |
| `CEREBRAS_API_KEY` | * | - | Cerebras API key for Llama models |
| `DEFAULT_SMART_MODEL` | No | `gpt-4o` | Default model for "smart" requests |
| `DEFAULT_INSTANT_MODEL` | No | `cerebras/llama-3.3-70b` | Default model for "instant" requests |
| `TOLLBOOTH_FLUSH_INTERVAL` | No | `30` | Seconds between budget flushes |
| `TOLLBOOTH_DEFAULT_BUDGET` | No | `5.0` | Default budget for new users (USD) |
| `TOLLBOOTH_PORT` | No | `9002` | Port to listen on (9000 used by MinIO) |

\* At least one provider API key is required.

## Model Routing

Tollbooth automatically routes requests to the appropriate provider based on the model name:

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

Tollbooth uses a "check in RAM, charge in DB" model:

1. **On Boot**: Load all user budgets from database into a `DashMap`
2. **On Request**: Check balance in RAM (0ms, lock-free)
3. **After Response**: Deduct cost atomically in RAM
4. **Every 30s**: Batch flush deltas to database

This decouples authorization from persistence for maximum performance.

## Pricing

Tollbooth calculates costs based on actual token usage. Approximate pricing (per 1K tokens):

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
docker build -t tollbooth .

# Or with nerdctl (for containerd)
nerdctl build -t tollbooth .
```

## Running Locally

```bash
# Set required environment variables
export TOLLBOOTH_INTERNAL_SECRET="your-32-character-or-longer-secret!"
export OPENAI_API_KEY="sk-..."  # Optional
export ANTHROPIC_API_KEY="sk-..."  # Optional
export CEREBRAS_API_KEY="..."  # Optional

# Run
cargo run
```

## Deployment (Nomad)

See `deploy/nomad/tollbooth.nomad` for production deployment.

```bash
nomad job run deploy/nomad/tollbooth.nomad
```

## License

MIT
