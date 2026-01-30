# Nomad Job Specification Template for Virtues Tenant
# Variables are substituted by setup.sh during provisioning
#
# Usage: This template shows the structure. The actual job file is generated
# by setup.sh with environment-specific values.

variable "subdomain" {
  type    = string
  default = "demo"
}

variable "tier" {
  type    = string
  default = "standard"
}

variable "ghcr_repo" {
  type    = string
  default = "ghcr.io/virtues-os"
}

variable "tag" {
  type    = string
  default = "latest"
}

# Resource configurations per tier
locals {
  tier_config = {
    free = {
      memory     = 256
      memory_max = 768    # Allow swap up to 512MB
      cpu        = 100    # ~10% of 1 core
    }
    standard = {
      memory     = 2048
      memory_max = 2048   # No swap
      cpu        = 1000   # 1 full core
    }
    pro = {
      memory     = 8192
      memory_max = 8192   # No swap, guaranteed RAM
      cpu        = 4000   # 4 cores
    }
  }

  resources = local.tier_config[var.tier]
}

job "virtues-tenant-${var.subdomain}" {
  datacenters = ["dc1"]
  type        = "service"

  # Schedule on appropriate tier node
  constraint {
    attribute = "${node.class}"
    value     = "${var.tier}-tier"
  }

  # Update strategy - rolling updates with canary
  update {
    max_parallel     = 1
    min_healthy_time = "30s"
    healthy_deadline = "5m"
    auto_revert      = true
    canary           = 0
  }

  # Migrate strategy for node drains
  migrate {
    max_parallel     = 1
    health_check     = "checks"
    min_healthy_time = "30s"
    healthy_deadline = "5m"
  }

  group "virtues" {
    count = 1

    # Restart policy
    restart {
      attempts = 3
      interval = "5m"
      delay    = "15s"
      mode     = "fail"
    }

    # Reschedule policy
    reschedule {
      attempts       = 3
      interval       = "1h"
      delay          = "30s"
      delay_function = "exponential"
      max_delay      = "1h"
      unlimited      = false
    }

    # Network configuration - CNI bridge mode
    network {
      mode = "bridge"

      port "http" {
        to = 8000
      }
    }

    # Host volumes
    volume "tenant_data" {
      type      = "host"
      source    = "tenant_data"
      read_only = false
    }

    volume "infinite_drive" {
      type      = "host"
      source    = "infinite_drive"
      read_only = false
    }

    # Ephemeral disk for scratch space
    ephemeral_disk {
      size    = 500  # MB
      migrate = false
      sticky  = false
    }

    # Main task - Rust core serving API + static files
    task "core" {
      driver = "containerd-driver"

      config {
        image   = "${var.ghcr_repo}/virtues-core:${var.tag}"
        runtime = "io.containerd.runsc.v1"

        # gVisor-specific options are configured in /etc/containerd/runsc.toml
      }

      # Mount volumes
      volume_mount {
        volume      = "tenant_data"
        destination = "/data"
        read_only   = false
      }

      volume_mount {
        volume      = "infinite_drive"
        destination = "/home/user/drive"
        read_only   = false
      }

      # Environment variables
      env {
        DATABASE_URL = "sqlite:/data/virtues.db"
        STATIC_DIR   = "/app/static"
        RUST_LOG     = "warn,virtues=info"
        TIER         = var.tier
        SUBDOMAIN    = var.subdomain
      }

      # Secrets template (injected from environment or Vault)
      template {
        data        = <<-EOF
          VIRTUES_ENCRYPTION_KEY={{ env "VIRTUES_ENCRYPTION_KEY" }}
          STREAM_ENCRYPTION_MASTER_KEY={{ env "STREAM_ENCRYPTION_MASTER_KEY" }}
          S3_ENDPOINT={{ env "S3_ENDPOINT" }}
          S3_BUCKET={{ env "S3_BUCKET" }}
          S3_ACCESS_KEY={{ env "S3_ACCESS_KEY" }}
          S3_SECRET_KEY={{ env "S3_SECRET_KEY" }}
          GOOGLE_CLIENT_ID={{ env "GOOGLE_CLIENT_ID" }}
          GOOGLE_CLIENT_SECRET={{ env "GOOGLE_CLIENT_SECRET" }}
          EXA_API_KEY={{ env "EXA_API_KEY" }}
        EOF
        destination = "secrets/env"
        env         = true
      }

      # Resource limits
      resources {
        cpu        = local.resources.cpu
        memory     = local.resources.memory
        memory_max = local.resources.memory_max
      }

      # Service registration for Traefik
      service {
        name = "virtues-${var.subdomain}"
        port = "http"

        tags = [
          "traefik.enable=true",
          "traefik.http.routers.${var.subdomain}.rule=Host(`${var.subdomain}.virtues.com`)",
          "traefik.http.routers.${var.subdomain}.entrypoints=websecure",
          "traefik.http.routers.${var.subdomain}.tls.certresolver=hetzner",
          # Security headers
          "traefik.http.middlewares.${var.subdomain}-headers.headers.stsSeconds=31536000",
          "traefik.http.middlewares.${var.subdomain}-headers.headers.stsIncludeSubdomains=true",
          "traefik.http.middlewares.${var.subdomain}-headers.headers.stsPreload=true",
          "traefik.http.middlewares.${var.subdomain}-headers.headers.frameDeny=true",
          "traefik.http.middlewares.${var.subdomain}-headers.headers.contentTypeNosniff=true",
          "traefik.http.routers.${var.subdomain}.middlewares=${var.subdomain}-headers"
        ]

        # Health check
        check {
          name     = "health"
          type     = "http"
          path     = "/health"
          interval = "30s"
          timeout  = "5s"
        }
      }
    }
  }
}
