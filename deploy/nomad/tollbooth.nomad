# Tollbooth - AI Budget Proxy
#
# Runs as a system job (one instance per host).
# Handles budget enforcement and billing for AI API requests.
# Routes requests directly to providers (OpenAI, Anthropic, Cerebras).
#
# Architecture:
#   Tenant Container (172.17.0.x) -> Tollbooth (9000) -> Provider API
#
# Usage:
#   nomad job run -var="tag=abc123" tollbooth.nomad

variable "ghcr_repo" {
  type    = string
  default = "ghcr.io/virtues-os"
}

variable "tag" {
  type    = string
  default = "latest"
}

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
        image   = "${var.ghcr_repo}/tollbooth:${var.tag}"
        runtime = "io.containerd.runsc.v1"  # gVisor for security
      }

      # Secrets injected from Nomad server environment via template
      template {
        data        = <<-EOF
          TOLLBOOTH_INTERNAL_SECRET={{ env "TOLLBOOTH_INTERNAL_SECRET" }}
          AI_GATEWAY_API_KEY={{ env "AI_GATEWAY_API_KEY" }}
          ATLAS_URL={{ env "ATLAS_URL" }}
          ATLAS_SECRET={{ env "ATLAS_SECRET" }}
          EXA_API_KEY={{ env "EXA_API_KEY" }}
          GOOGLE_API_KEY={{ env "GOOGLE_API_KEY" }}
          UNSPLASH_ACCESS_KEY={{ env "UNSPLASH_ACCESS_KEY" }}
          PLAID_CLIENT_ID={{ env "PLAID_CLIENT_ID" }}
          PLAID_SECRET={{ env "PLAID_SECRET" }}
          PLAID_ENV={{ env "PLAID_ENV" }}
        EOF
        destination = "secrets/env"
        env         = true
      }

      env {
        # Non-secret configuration
        TOLLBOOTH_PORT            = "9000"
        TOLLBOOTH_REPORT_INTERVAL = "30"
        TOLLBOOTH_DEFAULT_BUDGET  = "5.0"
        RUST_LOG                  = "tollbooth=info"
      }

      resources {
        cpu    = 100
        memory = 64
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
