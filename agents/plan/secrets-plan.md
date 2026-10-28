# Secrets plan

Written 2026-10-25, revised 2026-10-27. Status: slices 1–5 and the box and
page halves of slice 6 built on `wave`, not yet run on a box; the Mac pane's
half of slice 6 and slice 7 planned.

The box keeps every secret its owner has, in one table: the sessions and
tokens its syncs use, the box's own keys, named keys that applets need (API
keys, webhook tokens), and the owner's saved passwords. One module reads and
writes them, one Settings page shows them, and a model never sees a value.
It started from the AI chat sync spike
([record/ai-chat-sync-spike-2026-10-23.md](../record/ai-chat-sync-spike-2026-10-23.md)),
which needed somewhere to keep site sessions.

## Where secrets live today

- **`credentials`** ([0001_initial.sql:795](../../virtues-core/migrations/0001_initial.sql),
  reshaped by `0062_one_secrets_table.sql`): `source` rows, one per connected
  source, and `box` rows for the box's own keys (the iroh secret, the door
  key, the link code, the relay config). Both kinds are read through
  `crate::box_secrets` (box) and `virtues_helpers::auth::vault` (source).
- **`virtues.env`**: `VIRTUES_ENCRYPTION_KEY`, which encrypts the table, and
  `DATABASE_URL`, which reaches it. Both stay there: neither can live inside
  the table it unlocks.
- **Developer overrides in the environment.** `VIRTUES_API_KEY` is read in four
  places as a dev override of the box's linked key (`virtues_api/client.rs`,
  `api/subscription.rs`); a box keeps the real one as the `virtues_api`
  source row. `AI_GATEWAY_API_KEY` is read on the box only by tests that spend
  real money. These stay environment variables. They are not owner secrets.
- **`/var/lib/virtues/secrets/`**, created `0700` by the installer
  (`install.rs`, the data dir step). No code reads or writes it.

## The table

`credentials`, with `kind` saying what a row is for:

| `kind` | Holds | `key` | Value | Read by |
|---|---|---|---|---|
| `source` | a sync's session or tokens | source id (`x`) | JSON: `{jar}`, named fields, or OAuth tokens | that source's applets |
| `box` | the box's own keys | name (`iroh_secret_key`) | the bare string | core only |
| `env` | a named key an applet needs | variable name (`OPENAI_API_KEY`) | the bare string | applets that declare it |
| `login` | a saved password | site origin (`https://example.com`) | JSON: `{username, password, totp_secret, notes}` | the Mac browser's fill, by row id |

The other columns are as they are today: `id`, `name` (the label the owner
sees), `secrets_ciphertext`, `metadata` (non-secret context), `status`,
`status_reason`, `expires_at`, `next_refresh_at`, `last_seen_at`,
`created_at`, `updated_at`. `box` and `env` rows stay `active` with no expiry.

`box` and `env` keys are unique within their kind. A site can have several
`source` or `login` rows, one per account. A login's username sits inside the
ciphertext, so a database dump does not list which accounts the owner has.

## One key

Every row is encrypted with the box key (`VIRTUES_ENCRYPTION_KEY`).
Decided 2026-10-25: simplicity comes before security for now. The box reads
its own secrets at any time, with nobody present, including right after a
reboot.

What that costs, stated once so it is a known trade:

- A stolen box, or a copy of its disk, gives up every secret on it, saved
  passwords included. The key is in `virtues.env` on the same unencrypted
  disk, and the Dragon has no TPM to seal it to.
- Anything that runs with the owner's authority on the box can read them.

If that changes later, the step up is a second key for `login` rows that the
owner's device holds and the box keeps only in memory. The table does not
change for it, because `kind` already says which rows it would cover.

## Who can read what

- **The owner, and code acting as the owner.** The position written above
  `build_command` in `virtues-core/src/applet_runner/mod.rs` (2026-09-14)
  stays: an applet can do what the owner can do. This plan does not change
  it.
- **An applet gets the values it declares, as environment variables.** A
  manifest lists `secrets = ["OPENAI_API_KEY"]`; the runner reads those `env`
  rows and sets them for that run alone, the way `needs_vault_key` already
  grants the box key (`declares_vault_key_need` in `applet_templates`). An
  undeclared value is never passed. A declared one that is missing fails the
  run with "Add OPENAI_API_KEY in Settings, under Keys." Decided 2026-10-27:
  an unshipped applet also needs the owner to allow each declared key for
  that applet on its page, the way network hosts work. Shipped applets get
  their declared keys without that step. Not built yet.
- **A model sees names, never values.** It can know that a key or a login
  exists, so a chat can say "you have a GitHub token" or ask the owner to add
  one. The owner types the value into Settings. `sql_query` runs as
  `virtues_face_reader`, which has no grant on `credentials`. No test pins
  that yet, and the `SET LOCAL ROLE` fallback in `database/restricted.rs`
  can be undone from inside a query
  ([record/secrets-audit-2026-10-27.md](../record/secrets-audit-2026-10-27.md),
  section 3).
- **Sudo mode is the one exception** (decided 2026-10-27). A chat in sudo mode
  can reach every secret on the box, and the mode's copy says so. Its shell
  starts from a clean environment, so the server's variables are not
  inherited, and `virtues backup --init-key` asks first, the way deletes do.
  Reads never ask. Not built yet.
- **Four routes need the app's own signature** (decided 2026-10-27).
  Revealing a password, revealing a key, enrolling a device and minting a
  pair code each need a fresh owner proof: the box hands out a one-time
  nonce, and the app signs it with the device key, the same signing
  `crates/virtues-reach-client/src/owner.rs` does for an offline box. Every
  other route stays open to any connection on the Mac's `127.0.0.1:7117`,
  which is the accepted gap. The box's own console on its loopback is exempt.
  With it: the box refuses unknown `Host` names, the Browser pane refuses
  loopback, private and `.local` hosts, and the Mac keeps its device seed in
  the Keychain. Not built yet.
- **A password is filled by row id.** The Mac browser host decrypts the row
  and types the password into the page itself, so the value never appears in
  a tool result or a prompt. The shell's refusal to let the model type into
  password, one-time-code and card fields stays
  ([record/browser.md](../record/browser.md)).
- **The pool role is not a superuser** (since 2026-08-18; first boot runs
  `ALTER ROLE virtues NOSUPERUSER …`). Not yet checked on the live box.

## Built

1. **One table** (`72b40f7e3`). `kind` added, `source_id` renamed to `key`,
   `box_secrets` folded in as `kind = 'box'` and dropped. Every other
   statement on `credentials` names `kind = 'source'`, which also stopped an
   applet from being handed a box key through its `credential_id`.
2. **Sessions with an expiry** (`cf27154c8`; the connect form's half in
   `163af1ed8`). A sign-in in the Mac pane hands over `jar`, every cookie with its domain, path, expiry and flags,
   beside the old named fields, so boxes that have not upgraded keep working.
   The box drops Cloudflare's cookies, sets `expires_at` from the source's
   login cookies, and attention warns three days before a session ends.
   Applets read `jar` first and fall back to the old fields
   (`virtues_applets::cookie`, `cookie_header`). `domain` arrives without a
   leading dot, because the Mac's cookie store does not keep whether a
   cookie was host-only; anything loading `jar` into a browser treats a
   `__Host-` cookie as host-only.
3. **One module for named secrets** (`a14980007`). `crate::secrets` with
   `get`, `put`, `put_if_absent`, `delete` and `list` by `Kind::Box` or
   `Kind::Env`; every box caller moved onto it. Migration 0063 allows `env`
   and makes `(kind, key)` unique for both. The installer no longer creates
   the unused `secrets/` directory. `source` and `login` rows keep their own
   paths, since a site can have several of each.
4. **Declared keys** (`bac2a0657`; the applet page's "Keys it needs"
   section in `163af1ed8`). `secrets = [...]` in a manifest; the runner sets the declared `env` values for each run and fails a run
   missing one with the Settings sentence. The loader drops names the box
   sets itself. A jailed run gets its environment from a 0600
   `EnvironmentFile=` in a folder the unit cannot see, so values stay off
   sudo's command line and out of the journal (tested with a throwaway unit on systemd 255;
   `7dd15806e`).
5. **Keys in Settings** (`51a16fff9`, page in `163af1ed8`). `/api/keys`
   lists names only; `PUT`/`DELETE /api/keys/:name` set and remove a value,
   trimmed; `GET /api/keys/:name/value` is for when the owner asks to see
   one. The chat's context gets a `keys` line with names only. Settings →
   Keys has the sign-ins (each opens Sources) and the keys grid with the
   applets that use each.
6. **Saved logins, box and page** (`d2e306f5a`, page in `163af1ed8`).
   `login` rows: `key` is the site's origin, `name` the host without
   `www.`, and the sealed JSON holds username, password and notes.
   `/api/passwords` lists site, label and username; the password and notes
   come only from `GET /api/passwords/:id/secret`, which nothing a model
   calls can reach. An edit with an empty password keeps the saved one.
   Drive → Passwords lists them, shows and copies, edits and removes.

The cookie format and the order of the rollout are recorded in the commit
messages. Two rules outlive them: the Mac app updates before the box does,
so a request field is only ever added, and the old named fields leave the
app only once every supported box release reads `jar`.

## Slices

6. **Saved logins in the Mac pane.** After a sign-in in the pane, the app
   offers to save it. In the pane, a saved login for the page's site fills by
   row id.
7. **Importing logins.** CSV exports from 1Password, Bitwarden, Chrome and
   iCloud Keychain become `login` rows. A row matches on site and username,
   so importing the same file again adds only what is new or changed, as
   `chat_import` does for chats.

## Open

- Confirm on the live box that `virtues` is not a superuser and that the two
  reader roles have no grant on `credentials`.
- Whether a password manager fills inside the Mac app's WebKit view, which
  decides how much slice 6 matters to someone who already has one.
- Passkeys: platforms barely allow them to be exported, and the export
  standards were not checked for this plan.
- TOTP codes: whether the box should generate them from a stored
  `totp_secret`, which makes the box a second factor as well as the first.
