# Local access to the server: plan

**Status:** Built on wave, not yet run on a server. Decision 1 is made (2026-11-16); 2 and 3 are open.

## What is wrong

The server treats any process on its own machine as the owner. A request
that reaches port 8000 from `127.0.0.1` or `::1` with no forwarding header
authenticates as `local-console` (`middleware/auth.rs`, branch 2), and the
owner proof waves `local-console` through (`api/owner_proof.rs`,
`Requester::Console => return true`). That opens every owner route,
including these two:

- `/ws/terminal`, a shell as the `virtues` user, who has passwordless sudo
  (`/etc/sudoers.d/virtues`). Any local process can reach root this way.
- `/api/pair/mint`, which mints a pair code, so a local process can add a
  device that stays paired after the process is gone.

The rule was written for hardware Virtues sold, where the reasoning was that
physical access is the owner, and that a process on the box could already
read the data folder anyway. Neither holds on a machine the person installed
Virtues on themselves:

- Another login on the machine, a container with host networking, or any
  service that can be made to fetch a loopback URL can open a TCP connection
  to `127.0.0.1:8000`. None of them can read the encryption key, which is
  in `virtues.env` at 0600, owned by `virtues` (`install.rs`).
- `virtues-agent`, the user an outside agent's SSH key runs as
  (`cli/agent_key.rs`), is meant to have no database role and no sudo. Its
  forced command keeps it from opening sockets today, but its design already
  assumes a local process should not get the owner's powers.

The server's own outbound fetches refuse loopback (`fetch/guard.rs`,
`egress/policy.rs`), so the AI's fetch tools are not a way in.

## Who uses loopback owner access today

| Caller | How | What it needs |
|---|---|---|
| `virtues query/search/write/...` (`cli/data.rs`) | `POST /api/console/tool/:tool` on `127.0.0.1` | the console door only |
| `virtues agent-exec`, as `virtues-agent` | the same console door | the console door only |
| A browser over `ssh -L 8000:localhost:8000` (`cli/link.rs` prints the recipe after the QR) | the whole web app | full owner, by design today |
| Applet webhooks driven from the machine (`server/webhook.rs`) | the console exemption from device ownership | the console's ownership exemption |
| `make dev`'s Vite proxy | loopback with no forwarding header | covered again by branch 3 (`ENVIRONMENT=dev`) |
| The installer's health check, `virtues doctor`, `virtues pair`, `virtues status` | public `/api/box/health`, or the database directly | nothing from the console |

## Options

1. **A Unix socket for the console, with the caller's uid checked
   (recommended).** The server listens on `/run/virtues/console.sock` as
   well as TCP. Only a request on that socket can be `local-console`, and the
   server reads the caller's uid with `SO_PEERCRED`. A loopback TCP request
   is then treated like any other network request: it needs a paired device.
   The door already listens on a Unix socket this way (`door.rs`), so the
   listener code exists.
2. **Look up the uid behind a loopback TCP connection.** Linux can map a
   connection to its socket owner through `/proc/net/tcp` or `inet_diag`.
   This keeps port 8000 as the only door, but it is racy, Linux-only, and
   new code nobody else here has.
3. **Keep the console rule and require the owner proof on the dangerous
   routes.** This closes the terminal and pair minting, but every other owner
   route stays open to any local process: every read of the person's data,
   and deleting devices. The CLI also cannot sign an owner proof without a
   device key.

Option 1 is the only one that limits the console to the people who should
have it.

## The design, option 1, as built

Built on wave 2026-11-16, unverified on a real server. The code is
`virtues-core/src/console_socket.rs`.

- **Socket and folder.** The server listens on `/run/virtues/console.sock`
  beside TCP. The folder comes from a tmpfiles rule,
  `/etc/tmpfiles.d/virtues.conf` (`d /run/virtues 0755 virtues virtues -`),
  because upgrades never rewrite the service unit, so `RuntimeDirectory=`
  would not reach existing servers. The socket is mode 0666: the uid check
  is the boundary, which lets `virtues-agent` connect without joining a
  group.
- **Who the socket admits** (`classify`, by `SO_PEERCRED`):
  - root, `virtues`, or a member of `virtues-console`: `local-console`;
  - `virtues-agent`: the console door, `PATCH /api/applets/<id>` (applet
    enable) and `/api/mcp` (`agent_may`), and nothing else;
  - anyone else: disconnected before the request is read.
- **TCP loopback** lost branch 2 in `middleware/auth.rs`. Branch 3 stays, so a
  developer's stack (`ENVIRONMENT=dev`) works as before. `owner_by_transport`
  follows the same change.
- **The CLI.** `cli/data.rs`'s `Console` (data verbs and `agent-exec`) and
  `cli/mcp.rs` (`virtues mcp`) use the socket when it exists, and loopback
  TCP otherwise, which only a dev server accepts. On TCP a 401 says to run
  `sudo virtues migrate`.
- **Group and folder on every server.** The `console-socket` system repair
  creates the group, writes the tmpfiles rule, makes the folder, and adds
  the login that ran it through sudo (`SUDO_USER`). It runs from the
  installer's `migrate --check` and from the new binary on the upgrade that
  first carries it, so the installing login, or whoever runs
  `sudo virtues upgrade`, joins the group. Membership applies from that
  login's next session.
- **SSH.** The `ssh -L 8000:localhost:8000` the installer prints stays: it is
  the app's route to pair, and pairing needs the code, not the console. The
  browser-as-owner route is `ssh -L 8000:/run/virtues/console.sock`,
  documented in `docs/operate/reach.md`.
- **Mac and Windows servers (slice 6).** Unix sockets exist on both. The
  peer check uses `LOCAL_PEERCRED` on macOS and the pipe's client token on
  Windows. Design that when slice 6 starts.

## What is left

- Run it on a real Linux server: a fresh install, an upgrade from a release
  without it, the data verbs as the installing login and through an agent
  key, `virtues mcp`, and a browser over SSH.

## Decisions for the owner

1. **Decided 2026-11-16: keep the browser over SSH.** Asked "Keep the
   browser-over-SSH path, limited to your own login?", the owner answered
   "Yes, keep it". It becomes a socket forward that only the owner's login
   can use.
2. **Who joins `virtues-console`?** Decision 1 needs the owner's login in the
   group, so the open part is which logins count as the owner's: the one that
   ran the installer (recommended), or a list `virtues console add <user>`
   keeps.
3. **The terminal's passwordless sudo.** With this plan only an owner reaches
   the terminal, so it can stay. Narrowing it is separate work.

## Not in this plan

- The standing pair code being multi-use (`api/pair.rs`). Its reasons were
  the panel and the Bluetooth flow, both deleted. Making it one-use is a
  small change of its own.
- `compute_setup_state` counting `local-console` as claimed
  (`api/box_status.rs`).
- The data folder's own mode. The installer creates it through `useradd`
  and sets no mode, so whether another login can list or read the lake
  depends on the distro's `HOME_MODE`. Check it, and set it to 0700 with a
  system repair if needed.
