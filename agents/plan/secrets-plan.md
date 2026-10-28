# Secrets plan

Written 2026-10-25, revised 2026-10-28. Status: slices 1–5 and the box and
page halves of slice 6 are built, and so are the fixes for the 2026-10-27
audit's now tier. Slice 1 (`72b40f7e3`) and slice 2's `cf27154c8` are on
staging and shipped in the `v0.2.0-staging.6` prerelease. The rest, slice
2's connect form (`163af1ed8`) included, is on `wave` only and has not run on
a box or a device. The audit's "Status on 2026-10-28" table lists the
now-tier items still open. The Mac pane's half of slice 6 and slice 7 are
planned. Before `0f5feff2b` reaches staging, read "Release order" below.

The box keeps every secret its owner has, in one table: the sessions and
tokens its syncs use, the box's own keys, named keys that applets need (API
keys, webhook tokens), sign-ins for other services' MCP servers, and the
owner's saved passwords. Settings → Keys shows the syncs' sign-ins and the
named keys, Sources shows the MCP connections, Drive → Passwords shows saved
logins, and Settings → Billing shows the key for the owner's own AI
provider. A model never sees a value, with the one exception under "Who can
read what". It started from the AI chat sync spike
([record/ai-chat-sync-spike-2026-10-23.md](../record/ai-chat-sync-spike-2026-10-23.md)),
which needed somewhere to keep site sessions.

## Where secrets live today

- **`credentials`** ([0001_initial.sql:795](../../virtues-core/migrations/0001_initial.sql),
  reshaped by `0062_one_secrets_table.sql`): one row per secret, with `kind`
  saying what it is (the table below). `box` and `env` rows are read and
  written through `crate::secrets` (`Kind::Box`, `Kind::Env`), `source` rows
  through `virtues_helpers::auth::vault`, `login` rows through
  `api/passwords.rs`, and `connection` rows through `tools/connection.rs`.
- **`virtues.env`**: `VIRTUES_ENCRYPTION_KEY`, which encrypts the table, and
  `DATABASE_URL`, which reaches it. Both stay there: neither can live inside
  the table it unlocks. `box_env::path` is the one resolver for the file
  (`<data dir>/virtues.env`, from the install manifest). The applet runtime
  (`applets/src/lib.rs`) and `relay.rs` still read the default path, under a
  dated TODO in `box_env.rs`.
- **Developer overrides in the environment.** `VIRTUES_API_KEY` is a dev
  override of the box's linked key, read at five call sites in four files:
  `virtues_api/client.rs`, `api/subscription.rs` (twice), `maps/sync.rs`, and
  `applet_runner/mod.rs`, which passes it to shipped applets only. A box keeps
  the real key as the `virtues_api` source row. `AI_GATEWAY_API_KEY` is read
  in core only by tests that spend real money. These stay environment
  variables. They are not owner secrets.
- **Per-box files.** The door key and shared pages, a sandboxed run's env and
  credential files (`<data dir>/applet-env`), a backup or restore's staging
  folder, and the backup recipient. `box_files.rs` lists them, and
  deprovision, reset and image-check read that list.

## The table

`credentials`, with `kind` saying what a row is for:

| `kind` | Holds | `key` | Value | Read by |
|---|---|---|---|---|
| `source` | a sync's session or tokens | source id (`x`) | JSON: `{jar}`, named fields, or OAuth tokens | that source's applets |
| `box` | the box's own keys | name (`iroh_secret_key`) | the bare string | core only |
| `env` | a named key an applet needs | variable name (`OPENAI_API_KEY`) | the bare string | applets that declare it; Settings → Keys through `GET /api/keys/:name/value`, with the owner proof |
| `login` | a saved password | site origin (`https://example.com`) | JSON: `{username, password, notes}` | Drive → Passwords through `GET /api/passwords/:id/secret`, with the owner proof |
| `connection` | the sign-in for another service's MCP server | the connection's short name, the prefix of its tool names | OAuth tokens, refreshed by rmcp | `tools/connection.rs`, when the chat or an allowed applet calls that service's tools |

Two `source` keys are the box's own: `virtues_api` (the subscription key) and
`__byo_ai_key__` (the owner's AI provider key). Two hand-kept lists hide them
from the source list and setup status (`INTERNAL_SOURCE_IDS` in
`api/credentials.rs`, and `api/box_status.rs`). The chat's context uses
neither, so it names them as connected sources (`api/circumstances.rs`).
Making them `box` rows is in the audit's next tier.

The other columns are as they are today: `id`, `name` (the label the owner
sees), `secrets_ciphertext`, `metadata` (non-secret context), `status`,
`status_reason`, `expires_at`, `next_refresh_at`, `last_seen_at`,
`created_at`, `updated_at`. `box` and `env` rows stay `active` with no expiry.
A revoked row keeps its id and its payload becomes a sealed `{}`.

`box`, `env` and `connection` keys are unique within their kind. A site can
have several `source` or `login` rows, one per account. A login's username
sits inside the ciphertext, so a database dump does not list which accounts
the owner has. It does show each login's site and label.

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

- **Shipped code acts as the owner; other code runs in the sandbox.** Shipped
  applet code runs directly as the box user and can do what the owner can do.
  Code from an import or written by the assistant runs in a transient unit
  (`applet_runner/sandboxed.rs`): a throwaway user, the box's data hidden, no
  network unless the owner allowed hosts on its page, the restricted writer
  login for SQL, and only the keys it declares. A release build without
  systemd refuses to run it.
- **An applet gets the values it declares, as environment variables.** An
  applet's `SKILL.md` lists `secrets: OPENAI_API_KEY` under `metadata`; the
  runner reads those `env` rows and sets them for that run alone. The box key
  goes only to the shipped applets in `VAULT_KEY_APPLETS`
  (`applet_templates/skill.rs`), a list in the code that no folder can join.
  An undeclared value is never passed. A declared one that is missing fails
  the run with "Add OPENAI_API_KEY in Settings, under Keys." Decided 2026-10-27:
  an unshipped applet also needs the owner to allow each declared key for
  that applet on its page, the way network hosts work. Shipped applets get
  their declared keys without that step. That allow is not built yet.
- **A model sees names, never values.** It can know that a key or a login
  exists, so a chat can say "you have a GitHub token" or ask the owner to add
  one. The owner types the value into Settings. `sql_query` runs as
  `virtues_face_reader` and sandboxed applet SQL as `virtues_applet_writer`.
  Neither login has a privilege on `credentials`, `app_pair_token`,
  `app_setup_phrase`, `app_publications` or `app_device`, and tests in
  `database/restricted.rs` pin that, including SQL that resets its own role
  (`5b5c5f4b6`). On a box, model SQL refuses when its login does not connect;
  only dev and tests fall back to `SET LOCAL ROLE`. The db-logins repair sets
  the logins' passwords, and it runs from the installer, first boot,
  `virtues migrate` and `virtues restore`. The Browser pane shows a filled
  password, one-time-code or card field as "(filled, value hidden)" in a
  snapshot and as dots in a screenshot. Applet `schema_sql` is an open hole in
  this rule; see Open.
- **The admin applet is the one exception** (decided 2026-10-27). A chat turn
  of the admin applet (`/admin`, or agentMode `admin`, or `sudo` from an older
  app) runs bash on the box with passwordless sudo, and its SQL runs as the
  server's role, so it can reach every secret on the box. Its shell starts
  from a clean environment, so the server's variables are not inherited, and
  `virtues backup --init-key` asks first, the way deletes do (`37c526afa`).
  Reads never ask. The description the owner sees in the `/` menu and the
  chip says it can read every saved password and key and that what it reads
  goes to the AI model (`d053b6e61`). An unshipped applet named `sudo` is
  refused, and one named `admin` is left out of the `/` menu, so neither can
  start it (`d053b6e61`).
- **Six actions need the app's own signature** (decided 2026-10-27, built in
  `0f5feff2b`). Revealing a password, revealing a key, enrolling a device,
  minting a pair code (the collector's code too, since it redeems into a
  device), changing the AI key and importing an applet package each need a
  fresh owner proof. That is eight routes: `GET /api/passwords/:id/secret`,
  `GET /api/keys/:name/value`, `POST /api/devices/enroll-peer`,
  `POST /api/pair/mint`, `POST /api/pair/mint-collector`, `POST` and
  `DELETE /api/settings/byo-key`, and `POST /api/admin/applets/import-git`.
  The box hands out a one-time nonce (`POST /api/owner-proof/nonce`), and the
  app signs it with the device key, the same signing
  `crates/virtues-reach-client/src/owner.rs` does for an offline box, over a
  message with its own prefix (`api::owner_proof`). The box's own console on
  its loopback is exempt. Each reveal, enrollment and pair code also writes an
  `app_auth_event` row. With it: the box refuses unknown `Host` names, the
  Browser pane refuses loopback, private and `.local` hosts, and the Mac
  keeps its device seed in the Keychain. An app below command surface 15
  tells the owner to update it and sends nothing (`7a3054101`).

  The last two actions replaced the `virtues sudo` confirmation, which is
  removed. Migration 0070 empties its table and keeps it for one release,
  because the previous binary's sweeper fails without it after
  `virtues rollback`. A later migration drops `app_sudo_request`.

  The two ways to act as the box's admin need the proof too, since from
  either a process reaches the box's console, which the proof exempts, and
  `virtues.env`. `GET /ws/terminal` opens a shell as the service user, which
  has passwordless sudo. A browser socket cannot set a header, so the
  terminal alone may carry the proof in its `owner_proof` query parameter,
  signed over `GET /ws/terminal`; `POST /api/terminal/paste` takes the
  header. `POST /api/chat` takes the header when the turn resolves to the
  admin applet (`ChatMode::is_sudo`), and so does the Allow on one of its
  commands (`POST /api/chats/:id/permissions` with a `sudo:` grant). An app
  that still asks for `sudo` mode is told to update.

  Every other route stays open to any connection on a paired computer's
  `127.0.0.1:7117`, which is the accepted gap. Two parts of it are still
  open questions; see Open.
- **The pool role is not a superuser** (since 2026-08-18; first boot runs
  `ALTER ROLE virtues NOSUPERUSER …`). Not yet checked on the live box.

## Release order

The app updates before the box, so a box change normally only adds what a
released app can ignore. `0f5feff2b` breaks that on purpose for the eight
owner-proof routes: a released shell has no `owner_proof_sign`, so a box that
carries `0f5feff2b` refuses its reveal, pair, AI key and import requests.

- The same holds for the terminal and admin turns: a box that requires the
  proof on them refuses a released shell's terminal and its `/admin` turns,
  and an app that sends the old `sudo` mode is told to update.
- A box build that contains `0f5feff2b` must not reach staging or stable
  until Mac, iOS and Windows shell builds with `owner_proof_sign` (command
  surface 15, `apps/web/src-tauri/src/lib.rs`) are published, and Linux and
  Android builds too if owners use them. The Mac updater, the App Store and
  the Windows channel each have to serve that build first. Check with
  `gh release list`, and record the order in the wave → staging PR.
- A surface-15 app against an older box works for reveals and pairing: the
  box issues no nonce, and the app sends the request without a proof. If the
  app's own UI runs against such a box, changing the AI key and importing a
  package fail with a message to update the server, because that box still
  asks for a `virtues sudo` approval (`explainOldServer` in
  `apps/web/src/lib/api/client.ts`).
- `services/virtues-api` may deploy before or after any box release. Its
  `/refresh` answers every provider failure with 502 and puts the reason in
  `code` (`955df8b38`). Released boxes retry a 502, and new boxes read
  `code`. A proxy built from `a96faee9d` up to `955df8b38` answers 401 for
  `invalid_grant` instead, which lets a released box mark a working Strava
  sign-in `reauth_required` when two refreshes race. If such a build is
  deployed, replace it with a current one.

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
   section in `163af1ed8`). `secrets` under `metadata` in an applet's
   `SKILL.md`; the runner sets the declared `env` values for each run and fails a run
   missing one with the Settings sentence. The loader drops names the box
   sets itself. A sandboxed run gets its environment from a 0600
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
   Drive → Passwords lists them, shows and copies, edits and removes. A
   login the box key cannot open is listed as unreadable and can be removed
   (`78aba6abc`).

The cookie format and the order of the rollout are recorded in the commit
messages. Two rules outlive them: the Mac app updates before the box does,
so a request field is only ever added, and the old named fields leave the
app only once every supported box release reads `jar`. The owner proof is
the exception to the first rule, handled under "Release order".

### The audit's now tier

Built on 2026-10-27 and 10-28. The audit's "Status on 2026-10-28" table maps
each item to its commit and says which are still open.

- The owner proof on eight routes, `virtues sudo` removed (`0f5feff2b`);
  an app below surface 15 says to update, and a pairing whose record does
  not load stays paired (`7a3054101`).
- The Browser pane refuses local hosts and hides secret fields; the Mac
  seed is in the Keychain (`2a3ebeeef`).
- The `Host` check, `no-store` on secret and pair routes, and message
  attachments served sandboxed (`9705ab8c5`, `078b0ad55`).
- The admin applet's clean shell and `--init-key` ask (`37c526afa`), its
  description and reserved name (`d053b6e61`), and the recovery key kept
  off the Terminal's history (`f6e541881`).
- Model SQL refuses without its login, installs and restores set the
  logins, and tests pin the grants (`5b5c5f4b6`, `10fe384cf`,
  `f6e541881`).
- Backups and restores stage on the box, restore and server start check
  the key, and one resolver finds `virtues.env` (`10fe384cf`,
  `f6e541881`).
- Per-box secret files in one list, and leftover run files swept at the
  first run in each server process (`aaf3d7b9f`, `f6e541881`).
- Revoking empties the payload, including rows revoked earlier, and a link
  is Ready only once the key is stored (`78aba6abc`, `955df8b38`).
- Refused grants become `reauth_required`, refreshes lock across
  processes, and Reconnect repairs the row it was pressed on
  (`a96faee9d`, `955df8b38`, `55bbefa72`). The box looks up a Google
  grant's account address itself, so a reconnect to another Google account
  adds a row; the proxy reads no Gmail or Calendar data (`fdd037f4a`).
- Applet `schema_sql` limited to table and index statements
  (`2d24fd854`). This narrows the hole and does not close it; see Open.
- The runbook (`95eb83553`, `f6e541881`), the Keys page count
  (`95eb83553`), and the dead credential code: `credentials/types.rs`
  (`78aba6abc`), the unused `SetupConfig` fields (`95eb83553`), and the
  pairing-status helpers and the web client's unused pairing types
  (`9ebd715ef`).

## Slices

6. **Saved logins in the Mac pane.** After a sign-in in the pane, the app
   offers to save it. In the pane, a saved login for the page's site fills by
   row id: the box opens the row and hands the value to the Mac's browser
   host, which types it into the page itself and checks the frame's origin
   against the site saved with it. The value never appears in a tool result
   or a prompt. The shell's refusal to let the model type into password,
   one-time-code and card fields stays
   ([record/browser.md](../record/browser.md)).
7. **Importing logins.** CSV exports from 1Password, Bitwarden, Chrome and
   iCloud Keychain become `login` rows. A row matches on site and username,
   so importing the same file again adds only what is new or changed, as
   `chat_import` does for chats.

## Open

- **Applet schema DDL runs as the server's role on the main pool.** That
  includes a `check_only` setup the model can call with no allow, whose
  Postgres errors come back to the model. `2d24fd854` refuses the
  statements and functions known to read another table, but the DDL still
  runs as the server's role, so any function a `CHECK` or `DEFAULT` calls
  runs with that role's rights. A text check is not a boundary. The fix is a
  DDL login that owns only the `applet_*`
  schemas and has no privilege on `public`, `data_*`, `wiki_*` or `app_*`,
  connected the way `database::restricted` connects (never `SET ROLE`), with
  `EXECUTE` on functions in those schemas revoked from `PUBLIC`. That is
  slice 9 of [plan/applet-shape.md](applet-shape.md).
- **Writes from a web page on a paired computer.** There is no CSRF token,
  so a page on any site, open in a browser on a computer where the desktop
  app is paired, can send `127.0.0.1:7117` a request that needs no CORS
  preflight. CORS hides the reply, and nothing stops the write
  (`middleware/security.rs`).
- **`POST /api/pair/reopen-onboarding` has no owner proof.** It revokes
  every device. The owner left destructive routes inside the 7117 gap. Now
  that the terminal and admin turns need the proof, adding it here would
  close this one; the owner has not decided.
- **Per-applet key allows** (decision 3) are not built. Any applet's
  `SKILL.md` can still name any key.
- **Drive downloads with `?disposition=inline`** still serve the stored type
  at the box origin with no sandbox CSP (`server/api/drive.rs`). The chat
  file handler is sandboxed by its own rule. Both should move onto
  `inline_type` and `STORED_FILE_CSP`, as message attachments did, so there
  is one rule.
- **The Keychain move goes one way.** An older Mac build, for example after
  going from the prerelease channel back to stable, finds no seed in
  `box.json`, cannot connect, and shows the server as unreachable until the
  owner forgets it on the Mac and pairs again. On a current build the
  reconnect page shows the plugin's `problem` sentence and offers Pair again
  when the key is missing (`9ebd715ef`).
- **The seed outside a keychain.** Windows and Linux keep the seed in
  `box.json`, so a program running as the same user can read it and sign the
  owner proof (dated TODO in `apps/web/plugins/reach/src/store.rs`). On a Mac
  whose Keychain refused the move, the seed also stays in `box.json`.
  Android keeps it in the app's private storage.
- **Restore writes as root into folders the service user owns.** It
  recreates the lake and the applet folder at their paths, then renames
  staged files into them. Code running as that user can swap either folder
  for a link between the two, and root then puts files from the archive
  wherever the link points, still root's, since `chown -R` does not follow
  it. The service user writes the nightly drive backups, so it can also
  write an archive whose files a restore would place that way. The fix is
  the one the stage got: open each target when restore recreates it and
  move into it through that descriptor until it is handed back.
- **The start-time key check only reports.** It logs and shows attention
  when the key opens none of the sampled rows; nothing stops the box sealing
  new rows under the wrong key.
- **`make dev` serves Vite on every interface,** and its proxy reaches core
  from loopback, so a client on the developer's network that opens port 5173
  is the owner of the dev box.
- Confirm on the live box that `virtues` is not a superuser, that the reader
  and writer logins connect, and that neither has a grant on `credentials`.
- Whether a password manager fills inside the Mac app's WebKit view, which
  decides how much slice 6 matters to someone who already has one.
- Passkeys: platforms barely allow them to be exported, and the export
  standards were not checked for this plan.
- TOTP codes: whether a login should also hold a TOTP secret the box
  generates codes from, which makes the box a second factor as well as the
  first.
