# Tollbooth - AI Budget Proxy
#
# Runs as a system job (one instance per host).
# Handles budget enforcement and billing for AI API requests.
# Routes requests directly to providers (OpenAI, Anthropic, Cerebras).
#
# Architecture:
#   Tenant Container (172.17.0.x) → Tollbooth (9000) → Provider API

job "tollbooth" {
  datacenters = ["dc1"]
  type        = "system"

    group "tollbooth" {
    restart {
      attempts = 3
      interval = "2m"
      delay    = "15s"
      mode     = "delay"
    }

    network {
      mode = "host"

      port "http" {
        static = 9000
      }
    }

    task "tollbooth" {
      driver = "containerd-driver"

      config {
        image   = "${GHCR_REPO}/tollbooth:${TAG}"
        runtime = "io.containerd.runsc.v1"  # gVisor for security
      }

      env {
        # Required: Shared secret for Core backend authentication
        TOLLBOOTH_INTERNAL_SECRET = "${TOLLBOOTH_INTERNAL_SECRET}"

        # Required: Database for budget persistence
        DATABASE_URL             = "${DATABASE_URL}"

        # LLM Provider API Keys (at least one required)
        OPENAI_API_KEY           = "${OPENAI_API_KEY}"
        ANTHROPIC_API_KEY        = "${ANTHROPIC_API_KEY}"
        CEREBRAS_API_KEY         = "${CEREBRAS_API_KEY}"

        # Model routing defaults
        DEFAULT_SMART_MODEL      = "openai/gpt-4o"
        DEFAULT_INSTANT_MODEL    = "cerebras/llama-3.3-70b"

        # Budget flush interval (seconds)
        TOLLBOOTH_FLUSH_INTERVAL = "30"

        # Default budget for new users (USD)
        TOLLBOOTH_DEFAULT_BUDGET = "5.0"

        # Logging
        RUST_LOG                 = "tollbooth=info"

        # Plaid Configuration
        PLAID_CLIENT_ID          = "${PLAID_CLIENT_ID}"
        PLAID_SECRET             = "${PLAID_SECRET}"
        PLAID_ENV                = "${PLAID_ENV}"
      }

      resources {
        cpu    = 200
        memory = 128
      }

      service {
        name = "tollbooth"
        port = "http"

        check {
          name     = "health"
          type     = "http"
          path     = "/health"
          interval = "10s"
          timeout  = "2s"

          check_restart {
            limit = 3
            grace = "60s"
            ignore_warnings = false
          }
        }
      }
    }
  }
}
