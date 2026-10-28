---
title: Install script
description: Install Virtues on your own Linux machine with one command.
updated: 2026-10-23
---

## Read the install script (optional)

The install command further below runs the install script as root, with full control
of your machine. To instead read the script without running it, run this. The
script opens in a viewer in your terminal; **press `q` to close it**.

```bash
curl -sSL https://virtues.com/sh | less
```

[What the install script does](#what-the-install-script-does) explains each
step.

## Run the install command

Note: this is the recommended way to install. For a prerelease, an exact
version, your data on a different disk, or an unattended install, use one of
the [alternate ways to install](#alternate-ways-to-install) below instead.

Run this command on the machine that will become your server:

```bash
curl -sSL https://virtues.com/sh | sudo sh
```

That installs the newest stable release. Before it installs anything, the
installer reports:

- **Your disk type.** If it's a microSD card or a network share, stop and use
  a better disk. [System requirements](/docs/setup/requirements) explains
  each type.
- **Ports already in use.** The installer checks ports `5432`, `8000` and
  `18181`, for PostgreSQL, the server itself and the embedding server. On a
  fresh install, stop the program using that port before you continue. If
  you're reinstalling, this warning is expected.

## Choose how search runs

The installer asks: **How should search run?** (On hardware we build, it skips
the question.) Either way, search is designed to run on your own local
hardware.

- **On this computer's CPU** (recommended) is the standard setup: search runs
  on the CPU of the Linux machine you're installing Virtues on, and the
  installer sets it up for you.
- **On my own server**: choose this to use an existing custom embedding
  server, a separate program you run on this machine's GPU or NPU, or on
  another computer on your network. The installer asks for its URL and tests
  it. If you haven't set one
  up yet, follow [Set up a custom embedding server](/docs/inference) first.

If your server has a GPU or NPU and you want faster search, follow
[GPU and NPU acceleration](/docs/setup/accelerators), before or after you
install. If the installer finds a GPU or NPU, it also prints a link to that
page in your terminal.

After you answer, the installer finishes the install, which takes about ten
minutes, and ends by printing a six-digit pairing code.

## Next steps

With the pairing code on screen, pair your first device:
follow [Connecting your devices](/docs/setup/devices).

---

## Troubleshooting

If anything looks wrong during the install, run this first:

```bash
virtues doctor
```

It reports:

- How search is set up.
- Whether your embedding server is actually answering.
- Whether your server can be reached from outside your network.

Each finding includes the command to investigate it.
[The CLI](/docs/operate/cli) covers the other commands, and `virtues --help`
lists all of them.

## Alternate ways to install

To install differently, run one of these instead of the recommended command:

- **A prerelease.** [Upgrading](/docs/operate/upgrading) explains release
  channels.

  ```bash
  curl -sSL https://virtues.com/sh-pre | sudo sh
  ```

- **An exact version.**

  ```bash
  curl -sSL https://virtues.com/sh | sudo VIRTUES_VERSION=vX.Y.Z sh
  ```

- **With your data on a different disk.** Point `DATA_DIR` at a disk inside
  your server, not a network share. It defaults to `/var/lib/virtues`.

  ```bash
  curl -sSL https://virtues.com/sh | sudo DATA_DIR=/path/to/disk sh
  ```

- **Unattended, without answering "How should search run?"** Add
  `VIRTUES_INFERENCE=bundled` to choose **On this computer's CPU**, or
  `VIRTUES_INFERENCE=manual VIRTUES_EMBED_URL=<URL>` to choose
  **On my own server**. For example:

  ```bash
  curl -sSL https://virtues.com/sh | sudo VIRTUES_INFERENCE=bundled sh
  ```

## What the install script does

When you run the install command, the install script does five things:

1. Checks that you're on Linux, on a supported processor (`x86_64` or
   `aarch64`), and running as root.
2. Finds the newest stable release.
3. Downloads the installer and its checksum file (`.sha256`).
4. Checks that the installer matches the checksum.
5. Runs the installer.

If the checksum file is missing or doesn't match, the install script stops without
running the installer. It doesn't fall back to trusting HTTPS alone, because
a missing checksum file means a packaging bug, and the installer runs as root.
