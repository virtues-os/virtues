# Where "applet" is depended on

Written on 2026-11-22, from an audit of a plan to take the word "applet" out
of the database and the code. The owner decided that day to keep one row per
folder, so no rename is planned. This is the inventory the audit produced,
kept for whoever weighs a rename again: it is what such a change would have
to reach.

The lists come from three reviews of the code on `wave`. The claims marked
in the audit as checked were read in the code a second time: the fan-out of
a sync folder, the on and off check in the runner, the schema named in a
shipped prompt, the gate the tool's guidance teaches, the web's fallback,
the owner of `app_tasks`, how a role's password is made, the phone's stored
ids, and the release leaving the two guides out. The rest are as the
reviews reported them, each with its file.

## Where the word is today

Measured on `wave` on 2026-11-22.

- **One table does four jobs.** `app_applets` has 24 columns: what the thing
  is (`name`, `description`, `agent`, `command`, `config`), when it starts
  (`schedule`, `watch`, `watched_through`, `until`, `triggers`, `run_sql`,
  `last_slot_at`, `next_due_at`), whose account it serves (`credential_id`,
  `device_id`, `owner`), and its state (`enabled`, `archived_at`, `state`,
  `memory`, `seen_at`).
- **What a row is gets worked out twice.** `kind_of` in
  `scheduler/applets.rs` and `kindOf` in `apps/web/src/lib/applets/kind.ts`
  each derive site, server task, assistant task or skill from which columns
  are filled.
- **A repeating task of the assistant's is stored twice**: its applet row,
  and a row in `app_tasks` that stands for it (`applet_id`). The two audits
  of 2026-11-22 found most of their defects in the code that keeps the two
  in step (`api/repeating_tasks.rs`).
- **A row changes kind** when it gains a schedule or a view.
- **Two runners.** `applet_runner/` runs applets. `task_runner.rs` runs
  one-time requests to the assistant from `app_tasks`.
- **Four tables hang off it**: `app_applet_runs` (`applet_id`, set null on
  delete), `app_applet_run_outputs` and `app_applet_run_requests` (cascade
  from a run), and `app_applet_schema_migrations` (cascade from an applet).
- **The word is stored in other places**: `app_ai_calls.applet_run_id` and
  its `feature` value `applet`, `app_publications.producer_kind`,
  `app_visits.kind`, one Postgres schema per applet named `applet_<name>`,
  the roles `virtues_applet_writer` and `virtues_face_reader`, and ids of
  the form `applet_<name>`, `applet_user__<name>` and, for a sync,
  `applet_<name>_<account or device id>`.
- **On disk**: the shipped folder `applets/` (46 folders, each a `SKILL.md`),
  the programs' crate `virtues-applets`, the installed
  `/usr/local/share/virtues/applets`, the programs in
  `/usr/local/libexec/virtues` (`applets-bin` in the release archive), the
  state root
  `/var/lib/virtues/applets`, `.applet-state/` in a checkout, and the
  variables `VIRTUES_APPLETS_DIR`, `VIRTUES_APPLETS_BIN_DIR`,
  `VIRTUES_APPLET_STATE_DIR` and `VIRTUES_APPLET_RUN_ID`. The names from the
  rename before this one are still accepted too: `VIRTUES_ACTIONS_DIR`,
  `VIRTUES_ACTIONS_BIN_DIR` and `/usr/local/share/virtues/actions`.
- **What the assistant and outside agents call**: the tools `applet`,
  `use_applet` and `update_applet_memory`, the command `virtues applet`, and
  the routes under `/api/applets`, `/api/face` and `/face/`.
- **In the web app**: about 124 files, the tab types `applet`, `applets` and
  `applet-view`, and the address forms from before `/site`.
- **In a page**: a site placed in a page is a block whose tag is
  `virtues-applet` and whose `ref` is the applet's id
  (`crates/virtues-document/contract.json`).
- **In a paired collector**: the Mac collector keeps a map of webhook
  targets by id in its own config (`appletIds` in
  `apps/mac-source/Sources/Core/Config.swift`), read from the pairing reply
  and from `GET /api/devices/applet-ids`.

In all, about 370 tracked files and 4,800 lines outside `agents/` and the
migrations.

## What else depends on the word or on an id

Each of these was missing from the first list and was found by the audit.

- **Whether something is a site is a file on disk**, `view.html`
  (`server/api/applets.rs`, `face_for`). A SQL migration cannot fill a sites
  table.
- **A sync folder makes one row per connected account or device**
  (`applet_templates/mod.rs`, `per_credential`; 19 shipped folders have
  `source:`). Ids have a third form, `applet_<name>_<account or device id>`.
- **A run that belongs to no task.** A skill used from a chat makes a run,
  and its daily spending limit is summed by the applet's id
  (`applet_runner/limits.rs`). `app_runs.task_id` has no place for it.
  Programs have limits too and would have no skill row to hold them.
- **A skill has an on and off today**, and the runner refuses a run of one
  that is off (`applet_runner/mod.rs`). `app_skills` as drawn has no such
  column, so what was turned off would come back on.
- **Text saved on a server's disk names the old tables and ids.** The tool's
  guidance teaches a gate that reads `app_applet_runs` with an `applet_…`
  id, and the reader login is granted those columns for it
  (`server/faces.rs`). One shipped prompt and one shipped program name
  another folder's schema (`applet_daily_readings`). Table files create the
  schema by name and are checksummed (`tools/applet_schema.rs`).
- **The two database roles cannot be renamed by a migration.** Each role's
  password is derived from its name (`database/restricted.rs`), the lines
  in `pg_hba.conf` name them, the server's own login may not alter a role,
  and a role belongs to the whole cluster.
- **Code that reads meaning out of an id**: `applet_slug` and its callers,
  `template_for_applet_id`, the id built from the folder path
  (`applet_templates/skill.rs`), a prefix match in `google_push.rs`, and
  the web's address reader. Dropping `user__` from ids also lets a folder
  of yours and a shipped one take the same id.
- **Stored values**: tools the owner turned off, kept by tool id
  (`api/capabilities.rs`); a tool list inside each saved skill; the sidebar
  door `applets` in the profile; attention keys and routes; run outputs
  that name `applet_<name>.<table>`; the lake's kind `applet` and its
  `applets/<name>/` prefix; the `applet:` tag the crash reporter filters on.
- **What SQL cannot reach**: stored chats draw a tool call by its name;
  links inside pages and chat text; tabs kept in a browser's storage.
- **Lints that would stop checking without failing**: `arch-lint.sh` and
  `check-swallowed-queries.sh` test for a folder named `applets`.

## What a rename could not change in one release

Seven things sit outside the server, or hold data the server cannot rewrite
in SQL.

1. **The upgrade is run by the release before it.** The running binary
   unpacks the new release and looks for folders named `applets` and
   `applets-bin` (`cli/upgrade.rs`, `tools/virtues-installer/src/download.rs`).
   A release whose folders are named `packages` would install with no tasks
   at all. So one release teaches the upgrader the new names, and the next
   one uses them.
2. **Paired collectors keep ids.** A Mac or phone paired before the cut posts
   to ids of the form `applet_…` and reads the key `applet_ids`, which three
   replies carry (pairing, the device list, and the device-ids route). The
   Mac asks for its ids again when a post is refused. The phone never does:
   it keeps the ids from pairing until it is paired again. So the server
   keeps a `former_id` that the webhook route still answers to for as long
   as a phone paired before the cut is in use.
3. **Pages hold the block.** A page's content is a document the database
   cannot rewrite. The document contract would need a `site` block and a
   way to read the old block as one. No such mechanism exists today:
   `crates/virtues-document/src/migrate.rs` turns markdown into a document
   and does not rename blocks. The block's `ref` holds the id, so a site
   would need a `former_id` too.
4. **Backups.** Restore accepts an archive from an older schema, and its
   members are named `applets/…` (`cli/backup.rs`, `cli/restore.rs`).
5. **A server's own files**: its env file names the three variables, and
   `pg_hba.conf` names the roles.
6. **The native apps carry a web build of their own** and run it until the
   server's loads.
7. **A server that skips the preparing release.** Nothing makes an upgrade
   stop at a given release, so a server two releases behind would still
   install with no programs.

## What follows from it

- A table or column rename breaks text saved on servers: gates in `run`,
  table files, and one shipped prompt name the run table, a schema or an id.
- An id is not an opaque string today. Code reads the folder, the schema
  and whether it is yours out of its text.
- The two database roles are named in three places outside the database.
