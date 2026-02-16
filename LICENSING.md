# Licensing

Virtues uses a hybrid licensing model. Our native apps and data model are
fully open source under the MIT License. Our server, web application, and
infrastructure are source-available under the Business Source License 1.1,
free to self-host for personal use, with all code converting to Apache 2.0
after four years.

## Default License

The default license for this repository is the **Business Source License 1.1**
(BUSL-1.1), as specified in the root [LICENSE](./LICENSE) file. This applies
to all code unless a directory contains its own LICENSE file stating otherwise.

### What BSL 1.1 allows

- **Personal self-hosting**: Any individual can run Virtues on their own
  hardware or infrastructure for personal, non-commercial use.
- **Organizational self-hosting**: Any company or organization can self-host
  Virtues for its own internal operations.
- **Consulting and integration**: Contractors and consultants can install,
  configure, and maintain Virtues on behalf of individuals or organizations.
- **Education and research**: Non-commercial academic and research use is
  permitted.

### What BSL 1.1 restricts

- **Hosted services**: You may not offer Virtues as a hosted or managed
  service to third parties without a commercial license.
- **Hardware resale**: You may not distribute Virtues pre-installed on
  hardware devices or appliances for commercial resale without a commercial
  license.

### Conversion to open source

Each version of the Licensed Work automatically converts to the
**Apache License, Version 2.0** four years after its first public release.

## Open Source Components (MIT License)

The following directories are licensed under the **MIT License** and contain
their own LICENSE file:

| Directory | Description |
|-----------|-------------|
| `apps/ios/` | Virtues iOS application |
| `apps/mac/` | Virtues macOS application |
| `packages/virtues-registry/` | Data model, type definitions, and registry |

These components are fully open source. You may use, modify, and distribute
them without restriction under the terms of the MIT License.

## BSL-Licensed Components

The following directories fall under the default BSL 1.1 license:

| Directory | Description |
|-----------|-------------|
| `core/` | Server, API, storage, source connectors, agent, and tools |
| `apps/web/` | Web application (SvelteKit) |
| `apps/tollbooth/` | AI inference proxy |
| `apps/oauth-proxy/` | OAuth proxy service |
| `deploy/` | Sandbox runtime configuration |

## How License Resolution Works

The license for any file is determined by the nearest LICENSE file in the
directory hierarchy:

1. A LICENSE file in the same directory as the file
2. A LICENSE file in the nearest parent directory
3. The root LICENSE file (BSL 1.1)

## Third-Party Dependencies

Dependencies in `node_modules/`, `target/`, and `vendor/` directories retain
their original upstream licenses.

## Questions

For licensing questions, commercial license inquiries, or hardware reseller
partnerships, contact us at hello@virtues.com.
