# Secrets and passwords audit, 2026-10-27

This audit covers everything on and around the box that stores, moves or
reveals a secret: the `credentials` table and its crypto, source sign-ins and
OAuth, applets, what the chat model can reach, the HTTP surface and the
Keys and Passwords pages, backup and restore, and secrets that live outside
the table. It follows slices 1–5 of the secrets plan, the Passwords page, and
the EnvironmentFile change for jailed applets
([plan/secrets-plan.md](../plan/secrets-plan.md)).

**Method.** Seven readers each mapped one subsystem from the code and
proposed findings with file and line. A completeness critic then looked for
files nobody read and paths that cross subsystems. That gave 146 candidate
findings. Two more readers checked each one: the first opened the cited code
and tried to refute the claim, and the second traced a real actor step by step
to the harm and assigned a severity. A finding survived only if the code
check confirmed it and the second reader judged it worth acting on. 110
survived and 36 were dropped. Five more readers brainstormed where the design
should go, each from one angle, and one ranker merged their 50 ideas into
tiers. The code read was `wave` between f74266d3f and 9128b0024. Nothing ran
on a box. The paths behind the top findings were re-read by hand afterwards
(`proxy.rs`, `auth.rs`, `parse_page`, `sudo_shell.rs`, `restricted::begin`,
the backup and restore staging). Severities are the second reader's.

Owner decisions taken as given, and not argued here: one table, one box key in
`virtues.env`, simplicity before security, passwords kept on the box, no
browser extension, no box-side browser, a model never sees a value, and an
applet runs with the owner's authority.

## 1. Summary

**What holds.**
- The AEAD use itself is sound: AES-256-GCM through `ring`, a fresh random
  nonce per seal, and a 32-byte key check
  (`crates/virtues-helpers/src/crypto/mod.rs:189-239`).
- In the dev catalog, only the owner role `virtues` has grants on
  `credentials`. The face reader and the applet writer have none.
- Chat, MCP and the CLI cannot write an applet that declares a secret. The
  chat context's keys line carries names only.
- The applet egress proxy refuses the box's own loopback, so a sandboxed
  applet cannot call `/api/passwords` through it.
- Bookmark and page fetches cannot read the JSON reveal routes, even under
  DNS rebinding.

**What breaks.** Most of the serious findings trace back to two causes.

1. **The Mac app's loopback port is the owner.** Any connection to
   `127.0.0.1:7117` on a paired Mac is spliced onto the Mac's own device
   identity, so the box treats it as the owner (section 2). This predates
   Passwords. It used to mean "can act as the owner" and now also means "can
   read every saved password and key".
2. **"A model never sees a value" is enforced at each source separately, and
   four sources miss it:** Browser pane snapshots, Browser pane navigation,
   the sudo shell's environment, and applet output fed back into a prompt
   (section 3).

Next come plaintext copies that outlive their purpose (section 4) and a
credential lifecycle that leaves silent gaps in the record when a sign-in
lapses (section 5). The plan and several docs claim protections the code
lacks (section 8).

| Severity | Count |
|---|---|
| Critical | 1 |
| High | 13 |
| Medium | 38 |
| Low or info | 58 |

## 2. The Mac's loopback port is the owner

When the Mac app is paired, its reach plugin binds `127.0.0.1:7117`
(`apps/web/plugins/reach/src/lib.rs:675-677`, port at `lib.rs:79`).
`serve_on_provider` accepts every TCP connection, discards the peer address,
and copies the bytes onto an iroh stream signed with the Mac's device key
(`crates/virtues-reach-client/src/proxy.rs:115-152`). The box maps the proven
iroh key to the paired device and returns the owner
(`virtues-core/src/middleware/auth.rs:124-145`). Nothing checks who opened the
local connection or what `Host` it named. The reveal routes,
`GET /api/passwords/:id/secret` and `GET /api/keys/:name/value`, sit behind
that extractor and nothing else (`virtues-core/src/server/api/settings.rs:34,41`).

These actors reach the port:

- **The chat model, through the Browser pane (high).** `parse_page` checks
  only that the scheme is http or https
  (`apps/web/src-tauri/src/browser_host.rs:572-578`), and the pane has no
  navigation policy. A browse step can open
  `http://127.0.0.1:7117/api/passwords`, then the secret route for each id,
  and read the result with `browser_snapshot`. The browse worker is offered
  whenever a Mac app is connected (`virtues-core/src/api/chat_mode.rs:36-44`),
  so text planted in an email or a web page can start this.
- **A web page, through DNS rebinding (high).** A page served at
  `http://evil.example:7117` rebinds its name to 127.0.0.1. The splice passes
  the request through unchanged. `build_app` has no `Host` check
  (`virtues-core/src/server/mod.rs:357-389`), so the box answers, and the
  browser treats the answer as same-origin. The server code already names
  this attack in a comment (`server/mod.rs:1168-1177`).
- **Any `*.virtues` origin (high).** `origin_is_ours` accepts any origin whose
  host ends in `.virtues`, whatever the request's `Host` was
  (`server/mod.rs:1303`). Anyone who controls the Mac's DNS, such as a café
  network or a captive portal, can serve such a name.
- **Any other process or macOS user on the Mac (high).** This includes a
  local MCP agent, which the box otherwise limits to `/api/mcp`
  (`auth.rs:135`). Through 7117 the agent is the owner.
- **The device seed (high).** `box.json` keeps `device_secret_hex` in a plain
  0600 file under `~/Library/Application Support/virtues/`
  (`apps/web/plugins/reach/src/lib.rs:401-470`). The store's own comment says
  Keychain.
- **Windows and Linux builds (low).** The Tauri capability grants the app's
  commands, pairing included, to `http://localhost:*/*`
  (`apps/web/src-tauri/capabilities/default.json:55`). The main window's
  navigation guard narrows this. Other windows do not.

Layered fixes, smallest first:
1. The pane refuses loopback, private and `.local` hosts.
2. The box refuses requests whose `Host` it does not serve, and the
   `.virtues` CORS arm requires `Origin` to match `Host`.
3. The reveal routes require something a plain local socket lacks: a
   per-launch token the app's webview presents, or the app's own Tauri
   channel. The seed moves into the Keychain.

## 3. Where "a model never sees a value" breaks

- **Browser pane snapshots carry field values (high).** The vendored
  Playwright script gives `<input type=password>` the role `textbox` and sets
  every input's value as its text
  (`apps/web/src-tauri/vendor/playwright/injected.js:3310-3322, 4523-4525,
  4709-4710`). The host refuses to let the model type into a password field
  and hands off to the owner (`browser_host.rs:802-804`). Once the owner has
  typed, the next snapshot sends the password, one-time code or card number to
  the model. The refusal itself checks the focused element and misses iframes
  and shadow DOM (`browser_host.rs:506`, low).
- **Sudo mode inherits the server's environment (high).** `sudo_shell` builds
  `bash -c` without clearing the environment
  (`virtues-core/src/tools/sudo_shell.rs:63-79`), so `env` prints
  `VIRTUES_ENCRYPTION_KEY` into a tool result. Even with the environment
  cleared, passwordless sudo can read `virtues.env`, and sudo SQL returns any
  table. `sudo_gate` asks only before deletes (`tools/sudo_gate.rs:44-51`).
  The tool's description says changes wait for the owner
  (`crates/virtues-registry/src/tools.rs:1067`), but only deletes do.
  `virtues backup --init-key` prints a fresh age identity into the tool result
  and is not on the ask list. Whether sudo mode is an exception to the rule is
  an owner decision (section 9).
- **Applet output goes into a prompt (medium).** A run's stdout `result` and
  the last 500 characters of stderr become the applet agent's context
  (`virtues-core/src/applet_runner/mod.rs:1204-1210, 482`;
  `virtues-core/src/agent/applet_runner.rs:259-263`). A declared key or a
  source token that the code prints reaches the provider. Tool results are
  also stored with the turn and resent on every later turn
  (`virtues-core/src/api/turn_recorder.rs:241`, low).
- **Model SQL can drop its role (high).** `restricted::begin` uses the
  restricted login when it can connect. Otherwise it runs `SET LOCAL ROLE` on
  the owner pool (`virtues-core/src/database/restricted.rs:205-212`), which SQL
  inside the transaction can undo with `RESET ROLE`. According to the
  verifier, fresh installs create both roles NOLOGIN. Only the upgrade-time
  `db-logins` repair gives them passwords, so a new box starts in the fallback.
  Unattended applet agents run `sql_write` without approval
  (`virtues-core/src/tools/mod.rs:130-133`, `executor.rs:364-366`). No test
  pins the grants, though the plan says one does.

## 4. Plaintext copies outside the table

- **Backups stage a plaintext dump on the backup drive (high).**
  `write_archive` puts the `pg_dump` in a staging folder beside its output
  (`virtues-core/src/cli/backup.rs:203-207, 681-692`). For the nightly volume
  backup, that folder is on the drive (`backup_volume.rs:104-105`). Only `Drop`
  removes it, so an interrupted run leaves the whole database there in the
  clear.
- **Restore and `--verify` unpack the key beside the archive (high).**
  `mkstage` uses the archive's parent directory
  (`virtues-core/src/cli/restore.rs:633-645`). The stage holds
  `env/virtues.env` and the full dump, and Ctrl-C leaves them there. The
  manual tells owners to run `--verify` from time to time.
- **The backup recipient survives deprovision and reset (high).**
  `/var/lib/virtues/backup-recipient` is never removed
  (`virtues-core/src/cli/backup.rs:317`, `deprovision.rs`), and firstboot
  copies the card side onto each clone. A board handed on keeps encrypting its
  new owner's backups to the old owner's key. `image-check` does not look for
  the file.
- **Applet run files (medium).** Decrypted env values and source JSON for
  jailed runs sit in `<data_dir>/applet-env/` until `RunFile`'s `Drop`
  (`applet_runner/mod.rs:848-892`). A crash, SIGKILL or reboot leaves them. No
  boot sweep exists, and reset, deprovision and image-check ignore the folder.
- **The door key file (medium).** It is written and then chmodded
  (`virtues-core/src/door.rs:200-212`), it survives reset and deprovision, and
  a wiped database adopts it again (`api/publications.rs:120-145`).

## 5. The credential lifecycle

These matter most to the two primary personas, because each one shows up as
an unexplained gap in a reply or a chart.

- **A revoked Google or Strava grant never asks for a reconnect (medium).** The
  proxy drops the upstream body to keep tokens out of logs
  (`services/virtues-api/src/routes/oauth.rs:1430-1439`). As a result, the
  `invalid_grant` test at `oauth.rs:1378` never matches, and the box's
  `upstream 4` substring test never sees a 502 (`refresh.rs:141`). The row
  stays active and the sync just stops.
- **Reconnect mints a second row (medium).** The broken row keeps its stale
  tokens (`apps/web/src/lib/stores/connectFlow.svelte.ts:27`,
  `components/sources/connectDispatch.ts:55`). An api_key reconnect revives a
  revoked row, which PATCH says needs re-pairing (`api/source_auth.rs:449`,
  low).
- **Two processes can refresh one credential at once (medium).** The refresh
  lock is per process, and core and `credential_refresh` both refresh
  (`crates/virtues-helpers/src/auth/refresh.rs:31, 93`).
- **Revoking keeps the ciphertext (medium).** Disconnecting a source, or
  replacing or clearing the BYO AI key, only sets a status
  (`api/credentials.rs:223-251`, `api/settings_byo.rs:344-351, 464-482`). The
  BYO delete ignores its `UPDATE` result but still shows "Key removed."
- **Linking reports Ready without the key (medium).** The claimed arm returns
  Ready without checking that `store_api_key` succeeded
  (`virtues-core/src/virtues_api/link.rs:215-243`).
- **Four resolvers disagree about where the key file is (medium).** Backup
  prefers `/etc/virtues/env` without checking that it holds the key
  (`cli/backup.rs:57-75`). Restore falls back to writing there, where systemd
  never reads it (`cli/restore.rs:29-32`). `main.rs:50` and
  `applets/src/lib.rs:27` hardcode the default. `box_env::path()` is the
  right one.
- **Restore never checks that the key opens the table (medium).** It prints
  "restore complete" whether or not it does (`cli/restore.rs:233-308`).
- **One bad login hides them all (low).** `passwords::list` decrypts every row
  into a single `Result`, so one row sealed under another key fails the whole
  Passwords page (`virtues-core/src/api/passwords.rs:74-81`).

## 6. The cloud proxy

- **OAuth grants are not bound to a box (medium).** `/start` accepts any
  private, `.local` or `*.virtues.com` return URL and records no box identity.
  `/exchange` checks only the HMAC and single use
  (`services/virtues-api/src/routes/oauth.rs:143-169, 333-366, 1286-1315`).
  `/refresh` runs the refresh grant with Virtues' client secret for any caller
  (`oauth.rs:1344`, low).
- **Plaid tokens work for any account (medium).** The proxy injects the
  Plaid secret into whatever body a funded account sends, and never checks
  that the `access_token` belongs to that account
  (`services/virtues-api/src/routes/plaid.rs:101-157`).
- **Tokens cross the proxy on connect and on every refresh (medium).** Nothing
  is kept at rest there. An unreachable-proxy error puts the exchange token
  in the journal and in the response (`crates/virtues-helpers/src/auth/proxy.rs:96`, low).

## 7. Other access control

- **`make dev` admits every peer as the owner (medium).** It binds `[::]:8000`
  with `ENVIRONMENT=dev`, and the dev fallback returns the owner without
  checking the peer (`virtues-core/src/middleware/auth.rs:199-205`,
  `main.rs:799`). Anyone on the developer's network can read the dev box's
  passwords.
- **Any owner request can enroll a permanent owner device (medium).**
  `POST /api/devices/enroll-peer` checks only the id and kind, then allowlists
  the peer. It asks for no confirmation, sends no notice and writes no audit
  row (`virtues-core/src/api/devices.rs:453-582`). With section 2, a rebinding
  page could enroll its own device.
- **The `virtues sudo` approval is reachable from the web terminal (medium).**
  The comments call it proof of physical access (`api/sudo.rs:1`). Any paired
  device can open `/ws/terminal` as the service user and approve its own
  pending request.
- **BLE setup sends the setup phrase and Wi-Fi password in cleartext
  (medium).** The Improv characteristic sets no encryption flags
  (`virtues-core/src/maintenance/ble_provision.rs:528-547`). The frozen phrase
  is stored as an unsalted SHA-256 of about 2^34 possibilities
  (`api/setup_phrase.rs:116`, low).
- **Message attachments are served under the app's CSP (high).** Attachments
  go out inline with the type the sender declared, at the box origin
  (`virtues-core/src/server/api/library.rs:145-167`), under a CSP that allows
  `script-src 'self'` with no `sandbox`. An attacker can send one iMessage
  holding an HTML file and a script. The message id can be computed from public
  code. When the owner later opens the attacker's web page in any browser on
  the Mac, that page navigates to the HTML attachment on `127.0.0.1:7117`. The
  HTML loads the script attachment as same-origin script, and the script runs
  as the owner and calls the reveal routes. This is a third way into
  section 2's port.
- **Reveals have no audit trail and no rate limit (medium).** The BYO key has
  a step-up, and Passwords and Keys do not (`server/api/settings.rs:34, 41`).
  Secret responses carry no `Cache-Control: no-store` (low). Copied values
  stay on the clipboard (`PasswordsView.svelte:93`, low).

## 8. Coherence: one table, one way

- **The box's own keys are `source` rows (medium).** `virtues_api` and
  `__byo_ai_key__` are `kind = 'source'` only because of the column default
  (`migrations/0062_one_secrets_table.sql:14-16`). Two hand-kept lists hide
  them (`api/credentials.rs:91-101`, `api/box_status.rs:715-729`). A third
  place uses neither list, so the chat's context lists the billing key as a
  connected source (`api/circumstances.rs:499-509`). A package's
  `sources.toml` can also claim a shipped source id.
- **Every shipped applet holds the box key (low).** The runner grants the key
  only to applets that declare `needs_vault_key`
  (`applet_runner/mod.rs:944-957`). `virtues_applets::init_tracing` then loads
  `virtues.env` in every shipped applet (`applets/src/lib.rs:26-27`), so the
  gate and its test protect nothing.
- **Seven modules seal or open `secrets_ciphertext` with their own SQL:**
  `secrets.rs`, `vault.rs`, `refresh.rs`, the applet runner, `settings_byo.rs`,
  `passwords.rs` and `connection.rs`. That gives two `CredentialStatus` enums,
  two readers of the billing key and four id schemes. The runner decrypts
  directly, so `last_seen_at` is never bumped.
- **Any manifest can name any key (medium).** `declared_secrets` filters names
  only by format (`applet_templates/mod.rs:1040-1050`). There is no owner
  grant per applet and key, and nothing binds a key to a host. Re-importing a
  package can add `secrets` with no new owner action.
- **A connection's token goes to the URL in plaintext metadata (medium).**
  `tools/connection.rs:214-232, 349-356`. Only the owner role can write the
  table today.
- **Docs that contradict the code (medium).**
  - [secrets-plan.md](../plan/secrets-plan.md): it names a `crate::box_secrets`
    that does not exist and a `secrets/` folder the installer no longer
    creates. It lists four kinds where the code has five, and a `totp_secret`
    that nothing writes. It describes one Settings page and cites a test that
    does not exist.
  - `agents/build/recovery.md:151` resets the BYO key with SQL on a dropped
    column.
  - The manual's backup page does not say that an archive holds every saved
    password (`docs/operate/backup-and-restore.md:54`).
  - The sudo docs disagree with one another about which actions are gated.
  - The Rule of Two is documented as "checked at save", and no save-time check
    exists (`applet_templates/skill.rs:403`).
- **UI copy (low).** The Keys page header is a count (`KeysView.svelte:48`),
  which the recovery rule forbids. The connect form breaks the writing rules
  (`ApiKeyConnectModal.svelte:129`).

## 9. Decisions for the owner

1. **Sudo mode and secrets.** Is sudo mode a stated exception to "a model
   never sees a value", or should reading `virtues.env`, `/proc/*/environ`,
   the run files or `credentials` ask first? A middle path is a
   `virtues key exec NAME -- cmd` that puts one value in a child's environment
   and scrubs its output.
2. **What authenticates a local client of 7117.** Choose between a per-launch
   token the webview presents and routing the reveal routes over the app's
   Tauri channel. Either way, the seed moves into the Keychain.
3. **Per-applet key grants.** Keep the flat namespace, or record an owner
   approval per (applet, key) the way `net_hosts` already works.
4. **What `virtues sudo` proves.** Either approval needs something the web
   terminal cannot give, or the docs stop calling it physical access.

### What the owner decided, 2026-10-27

The rules that follow from these answers are in
[plan/secrets-plan.md](../plan/secrets-plan.md), under "Who can read what".

1. **Sudo mode is a stated exception.** Sudo mode can reach every secret, and
   the mode's copy says so when the owner turns it on. The shell starts from a
   clean environment, so `env` no longer prints the box key. Because
   `virtues backup --init-key` prints a recovery key, it asks first, the way
   deletes do. Reads never ask.
2. **Port 7117: the owner left the design open, as long as it is not
   over-engineered.** The chosen design reuses the owner-proof signature the
   offline-box Bluetooth path already uses
   (`crates/virtues-reach-client/src/owner.rs`):
   - The box refuses `Host` names it does not serve, and its `.virtues` CORS
     arm requires `Origin` to match `Host`.
   - The Browser pane refuses loopback, private and `.local` hosts.
   - Message attachments are served sandboxed, as downloads.
   - Four routes need a fresh owner proof: revealing a password, revealing a
     key, enrolling a device and minting a pair code. For each, the box issues
     a one-time nonce, the app signs it natively with the device key, and the
     request carries the signature. The box's own console on its loopback is
     exempt, since it already holds `virtues.env`.
   - The device seed moves from `box.json` into the Keychain.

   A token on every connection was rejected. It would change every request,
   and `<img>`, media and socket URLs would have to carry it. The gap that
   remains is accepted: another process on the Mac can still act as the owner
   for everything except those four routes.
3. **Keys are allowed per applet.** An unshipped applet gets a declared key
   only after the owner allows that key for that applet on its page, the way
   network hosts work. Shipped applets keep getting their declared keys
   automatically.
4. **`virtues sudo`: leaning toward removing the gate, not yet decided.** If
   it goes, the two gated actions (changing the AI key, importing a package)
   could require the owner proof from decision 2. That keeps a check that a
   rebinding page or another local process cannot pass, without a step on the
   box.

## 10. Where the design goes next

The ranker placed the merged ideas in four tiers. The ones marked as fixes
close findings above.

**Now (before the next release).**
- The Browser pane refuses loopback and private hosts, and snapshots blank
  password, one-time-code and card fields. Fixes section 2's pane path and
  section 3's snapshot leak.
- The box refuses foreign `Host` names, `.virtues` requires `Origin` to match
  `Host`, attachments get a sandbox CSP and `attachment` disposition, and the
  reveal routes get one rule: `no-store`, an audit row, and the local-client
  check.
- The Mac keeps its seed in the Keychain, and reveal, enroll and pairing go
  over the app's own channel.
- Sudo mode starts from a clean environment, `--init-key` joins the ask
  list, and the copy says what sudo can still reach.
- Backups and restores stage on the box's own disk and clean up after an
  interrupt.
- Runtime secret files move to `/run`, which a reboot clears, and one list of
  per-box secret files drives deprovision, reset and image-check.
- One resolver for `virtues.env`, a key check before anything is sealed or a
  restore reports success, and a Passwords list that marks an unreadable row
  instead of failing.
- Revoking a source or clearing the AI key also wipes its ciphertext.
- Revoked Google and Strava grants become `reauth_required`, and Reconnect
  repairs the row it was pressed on.
- Linking reports Ready only when the box holds the key.
- Model SQL fails closed on a box, installs set up the restricted logins, and a
  test pins the grants on `credentials`.
- Rewrite the secrets plan, the runbook and stale comments against the code.
  Delete the dead credential code and drop the Keys count.

**Next (the following few weeks).**
- A lapsed sign-in explains the gap it leaves. One helper sets a status and
  writes an `app_auth_event`, and coverage and charts draw a "not connected"
  band.
- A backup recovery kit minted on the owner's device. The box receives only
  the recipient, so it never holds the recovery key.
- Only the server holds the box key. The refresh sweep moves into core, and
  `init_tracing` stops loading the env file.
- The box's own keys become `box` rows, and a package catalog cannot claim a
  shipped source id.
- Ask for a key inside the reply: a sealed field drawn by `show` that posts to
  `PUT /api/keys/:name` and returns only "set" to the model. Also catch keys
  pasted into the composer.
- Fill a saved login in the Mac pane, with the host comparing the frame's
  origin to a copy of the site sealed with the password.
- One names-only list of every secret that says where each goes and when it
  last went.
- One scrubber where text enters a model, as a backstop behind the per-source
  fixes. It is never described as a boundary.
- Bind OAuth and Plaid grants to the box that started them.
- A salted slow hash for the setup phrase, and encrypted BLE writes.
- Native copy with a clipboard that marks the value concealed and clears it.
- Password imports parsed in memory, never through the `chat_import` applet
  path, whose summary a model reads.
- One module owns every statement on `credentials`, enforced by the arch lint.

**Later.**
- Seal each value with its row and destination as associated data, with a
  version prefix and a key id.
- Disconnect revokes the grant at the provider and offers to sign the pane
  out.
- Touch ID or Face ID before reveal, fill and export.
- Keys as host-bound placeholders that the egress proxy fills in.
- Offer to save a login after a pane sign-in. This should wait until fill and
  snapshot masking have held for a release.
- Reuse notes on the box, and a breach check that runs only when the owner
  asks, with no score or tally.
- An AutoFill credential provider on iOS and macOS. This is an OS extension,
  outside the no-browser-extension decision.

**Rejected.**
- The box generates the backup identity and hands it out once. The box would
  hold the recovery key, if only briefly.
- A `request_key` tool that pauses the turn on an approval card. The owner has
  ruled out new approval cards, and the sealed field above gives the same
  guarantee.

## 11. What each persona gets

- **Narrative seeker and quantified self.** The lifecycle fixes in section 5
  and the "not connected" band. Today a lapsed Google or Strava grant leaves a
  silent hole in a day article or a chart. After these fixes, the hole says
  why and offers the fix.
- **Tech prosumer.** A password manager on their own server that holds up
  under inspection: the Keychain seed, native copy, fill in the pane, imports,
  and later AutoFill.
- **Recovery.** Nothing that counts or scores. The Keys header count goes. A
  breach or reuse check shows per-row notes only.
- **Data sovereign.** The names-only list of every secret with where each
  goes, an audit row for every reveal, and the egress proxy's host list.

## Dropped findings

Of the 36 dropped findings, a few are worth recording so they are not raised
again:
- **Empty associated data on the AEAD.** True, but every principal that can
  write `credentials` already holds the key or is root.
- **Login sites and account ids in plaintext columns.** An HMAC of the origin
  would use a key that sits beside the dump, and the same account identities
  are already in the ingested `data_*` tables. `passwords.rs` and the plan
  should say that a dump shows each login's site and label.
- **Wi-Fi passwords on `nmcli`'s argv, and the kiosk unit loading
  `virtues.env`.** Only processes that already hold the key can read either.
- **Cookie lookup by name ignoring domain.** The jar is already filtered by
  host, path and the secure flag when it is captured (`browser.rs:46-57`), so
  only a duplicate name is left. Fix it when a source first declares
  `domains`.
- **Any applet can declare any key.** One copy was dropped: the Keys page
  already shows which applets use each key, and a package import goes through
  `virtues sudo`. Another copy survived in section 8, and decision 3 settles
  it. Either way, a comment should say why env keys need only a declaration
  while the vault key also needs shipped provenance
  (`applet_runner/mod.rs:949-953`).

## Appendix: low and info findings

- Migration 0066 converts legacy `mcp:` rows that the current code cannot read and that block reconnecting. `virtues-core/migrations/0066_add_connection_credential_kind.sql:14`
- The ciphertext has no format version, no key id and no rotation path, and a wrong key does not stop writes. `crates/virtues-helpers/src/crypto/mod.rs:208`
- Key files are written with default permissions and chmodded afterwards. `tools/virtues-installer/src/install.rs:1201`
- An env file without a key stays that way, and a missing key surfaces as 401 Unauthorized. `tools/virtues-installer/src/install.rs:1239`
- Sealed display copies of the pair code and setup phrase live outside the table. `virtues-core/src/api/pair.rs:252`
- The HMAC lint the crypto module cites does not run in CI, and `restricted.rs` uses HMAC directly. `crates/virtues-helpers/src/crypto/mod.rs:8`
- Unused credential types derive `Debug` over decrypted values, and `SetupConfig` copies the master key into a field nothing reads. `virtues-core/src/credentials/types.rs:61`, `virtues-core/src/setup/mod.rs:13`
- `kind` defaults to `'source'`, and the vault's inserts rely on it. `virtues-core/migrations/0062_one_secrets_table.sql:15`
- The iroh identity is minted with an overwriting put, and doctor swallows decrypt errors. `virtues-core/src/relay.rs:531`
- Pane sign-ins stay on the Mac and usable by the assistant after Disconnect, and the copy does not say so. `apps/web/src-tauri/src/browser_host.rs:560`
- The `last_seen_at` comment says it is bumped on every use, and the runner decrypts without the bump. `crates/virtues-helpers/src/auth/vault.rs:254`
- The warm-keeper comment says it refreshes ahead of expiry, and it refreshes only in the last 60 seconds. `applets/credential_refresh/main.rs:8`
- On a developer machine the chat's shell tool runs unsandboxed with the server's environment. `virtues-core/src/api/code.rs:282`
- The server sends `ALTER ROLE … PASSWORD` with the derived password on every start, and Postgres refuses it on a box. `virtues-core/src/database/restricted.rs:96`
- The restore comment says a missing increment fails loudly, and the code cannot detect one. `virtues-core/src/cli/restore.rs:1044`
- Dead code and a misleading count in reset and restore. `virtues-core/src/cli/reset.rs:197`
- Dead credential code and tables. `virtues-core/src/credentials/types.rs:12`, `virtues-core/src/api/credentials.rs:279`
- Swallowed errors on the BYO key status and delete paths. `virtues-core/src/api/settings_byo.rs:141`
- The setup flow keeps the atlas bearer token in `sessionStorage`, and the Mac collector ignores an RNG failure. `apps/web/src/lib/components/setup/prepair.svelte.ts:238`
- The egress proxy records host, method and byte counts only. `virtues-core/src/egress/mod.rs:37`
- Stale comments about pairing, devices, env paths, bearer and cookie auth, and the stopped box browser. `virtues-core/src/api/pair.rs:3`, `virtues-core/src/relay.rs:10`, `virtues-core/src/cli/deprovision.rs:23`
