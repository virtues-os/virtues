# Local access to the server: plan

**Status:** Planned. Decisions 1 to 3 below are for the owner.

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

## The design, option 1

- **Socket and permissions.** `/run/virtues/console.sock`, created by the
  server at start (`RuntimeDirectory=virtues` in the unit), mode 0660, group
  `virtues-console`. The installer creates the group and adds the login that
  ran it (`SUDO_USER`) and `virtues-agent`.
- **Who the socket admits.** The server reads the peer uid and maps it:
  - root, `virtues`, or a member of `virtues-console`: `local-console`, as
    today;
  - `virtues-agent`: an agent caller, admitted to `/api/console/tool` only.
    Today it gets the whole owner surface through the same door.
  - anyone else: refused. The socket mode stops them first; the uid check
    covers a mode someone loosened by hand.
- **TCP loopback.** It loses branch 2 in `middleware/auth.rs`. It keeps
  branch 3, so a developer's stack (`ENVIRONMENT=dev`) works as before.
  `owner_by_transport` follows the same change.
- **The CLI.** `cli/data.rs` and `agent-exec` talk to the socket.
  `reqwest` has no Unix-socket support, so this uses `hyper` with a
  `UnixStream` connector, or a small request writer like the door client's.
- **A browser over SSH.** This depends on decision 1. If it stays, the recipe
  becomes `ssh -L 8000:/run/virtues/console.sock user@host`. sshd connects as
  the person's login, which is in `virtues-console`, so it works for the
  owner and for nobody else with an account.
- **Upgrades.** An upgraded server needs the group and the unit change. Both
  belong in a system repair (`cli/system_repairs.rs`), which the new binary
  runs as root on the upgrade that first carries it.
  - Ordering: the new CLI must not try a socket the old server lacks. Until
    the repair has run, the CLI falls back to TCP and says the server needs
    `sudo virtues migrate`.
- **Mac and Windows servers (slice 6).** Unix sockets exist on both
  platforms. The peer check uses `LOCAL_PEERCRED` on macOS and the pipe's
  client token on Windows. Design that when slice 6 starts; nothing here
  blocks it.

## Slices

1. Socket listener, peer-uid mapping, and the auth change, with tests. The
   tests cover loopback TCP refused in production, loopback TCP admitted
   with `ENVIRONMENT=dev`, a socket caller as `virtues` admitted, a socket
   caller as `virtues-agent` admitted only to the console door, and the
   terminal refusing a loopback TCP request.
2. CLI and `agent-exec` move to the socket, with the TCP fallback for one
   release.
3. Installer and system repair: the group, membership for `SUDO_USER` and
   `virtues-agent`, the unit's `RuntimeDirectory`.
4. The SSH recipe in `cli/link.rs`, or its removal (decision 1), and the
   manual page that mentions it.
5. Comments and docs that state "physical access is the owner"
   (`middleware/auth.rs`, the sudoers comment in the installer, which also
   still says "WG/SPKI", and `agents/build/recovery.md`).

Each slice ships on its own. Slice 1 alone closes the hole, but the CLI data
verbs then fail until slice 2 lands, so slices 1 and 2 go in one release.

## Decisions for the owner

1. **Keep the browser over SSH?** It is the only way to use the web app with
   no paired device. Keep it as a socket forward that only the installer's
   login can use (recommended), or drop it and have every owner pair a
   device.
2. **Who joins `virtues-console`?** The login that ran the installer
   (recommended), or nobody, which leaves the console to root and `sudo -u
   virtues`.
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
