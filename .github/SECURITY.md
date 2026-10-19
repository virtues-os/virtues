# Security

Virtues holds the record of someone's life, so a vulnerability here is
personal before it is technical. Please report one privately.

## Reporting

Use GitHub's private reporting: **Security → Report a vulnerability**, or go
straight to
[the advisory form](https://github.com/virtues-os/virtues/security/advisories/new).
The report and the conversation that follows stay private between you and
the maintainers until a fix ships.

Please do not open a public issue, pull request or Discord thread for a
vulnerability.

A useful report says:

- what an attacker can do, and from where: on the LAN, as a paired device,
  through the relay, as an applet, or from the internet;
- the release it affects: `virtues --version` on the server, or which app
  and its version;
- the steps that reproduce it.

Reproduce against a test server with invented data, never against a real
record. Leave real names, messages, locations and keys out of
the report; a fictional stand-in shows the same bug.

## What is in scope

Everything in this repository:

- the server: `virtues-core`, the installer and `bootstrap.sh`, applets and
  the applet runner;
- the clients: the web app, the Mac, iOS, Windows and Linux desktop apps, and
  the Mac collector;
- the services: `virtues-api` and `virtues-atlas`;
- the pairing and reach path, including what the relay can observe.

[The reach page](../docs/operate/reach.md) says what the relay can and cannot
see, and [the privacy boundary](../agents/build/privacy-boundary.md) says
where data leaves a server. A way to make either statement false is in scope.

## Supported releases

Fixes ship in the next release on each channel: the newest stable release
and the current prerelease line. Earlier releases are not patched, so a fix
reaches a server through `sudo virtues upgrade`.
