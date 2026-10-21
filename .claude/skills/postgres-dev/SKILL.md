---
name: postgres-dev
description: Diagnose the local Postgres the Rust tests run against — a suite that fails a different random test every run, "rejected trust authentication" under #[sqlx::test] parallelism, CREATE EXTENSION vector needing superuser, or a CREATEROLE grant being refused. Use when the test suite is flaky rather than wrong, or when a Postgres permission error looks like an application bug.
---

# Diagnosing the dev Postgres

Two failures here look like bugs in your code but are caused by the local
Postgres setup. Both took real time to diagnose. The fixes below remove the
underlying *condition* instead of working around the symptom, so they are
written as step-by-step recipes.

The ordinary daily facts (where the target dir is, which commands to run) stay
in CLAUDE.md. This file covers what you need about twice a quarter, when
something behaves in a way that should be impossible.

## Random test failures under parallel `#[sqlx::test]`

**If the suite fails a different random test on every run, it is not your
change.** `#[sqlx::test]` provisions a scratch database per test, and under
parallelism Postgres.app's app-permission gate rejects passwordless (`trust`)
connections from processes it does not recognise — `rejected "trust"
authentication`. A failing suite then carries no signal, so a real regression can
slip through unnoticed.

This machine fixes it by requiring a password for the app role over TCP, which
removes the condition the gate checks for. In
`~/Library/Application Support/Postgres/var-18/pg_hba.conf`, *above* the
general `trust` lines (first match wins):

```
host    all   virtues   127.0.0.1/32   scram-sha-256
host    all   virtues   ::1/128        scram-sha-256
```

then `ALTER ROLE virtues WITH PASSWORD 'virtues';` to match `.env`, and
`SELECT pg_reload_conf();`. The rule is scoped to `virtues` because your macOS user and
`postgres` are login roles with no password, so a blanket rule locks them out
of TCP.

## Permission errors that are not application bugs

**The `virtues` role is NOT a superuser** (since 2026-08-18). It has exactly
`LOGIN CREATEDB CREATEROLE`: CREATEDB for `#[sqlx::test]`'s scratch databases,
CREATEROLE so `server/faces.rs` can provision `virtues_face_reader`. It was
`SUPERUSER` with the password `virtues`, which — with the pg_hba rule above
opening loopback TCP — let any local process that guessed the password once
take over the cluster. `make db` downgrades an existing role idempotently.

Two consequences worth knowing before you debug a permissions error:

- `pgvector` is not a trusted extension, so `CREATE EXTENSION vector` needs
  superuser. `make db` installs it into `template1` instead, and every database
  created afterwards inherits it — which is what makes migration 0001's
  `CREATE EXTENSION IF NOT EXISTS` a no-op rather than a failure.
- In PG16+ a CREATEROLE role may only grant membership in roles it has ADMIN
  on. `make db` grants `virtues_face_reader`/`virtues_applet_writer` to
  `virtues` WITH ADMIN OPTION for this reason. Without it, faces.rs cannot grant
  them to itself and the symptom looks like an applet permissions bug.
