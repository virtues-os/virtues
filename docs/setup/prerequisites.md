---
title: Prerequisites
description: What you need before installing Virtues - a server that meets the system requirements, terminal and root access, curl, internet, and a device to connect with.
updated: 2026-10-23
---

To install Virtues, you need a Linux machine that will become your server,
connected to the internet over Wi-Fi or Ethernet, with root access and a
terminal you can reach over SSH or with a keyboard and monitor. Before you
begin, run each check below on your server. If every check passes, you're
ready to install.

## Check your server

- **Terminal access to your server.** Connect a keyboard and monitor to it, or
  connect from another computer with `ssh you@your-server.local`. If you've
  never used SSH, use a keyboard and monitor for the install. In most Linux
  terminals, paste with Ctrl+Shift+V, not Cmd+V or Ctrl+V.
- **Root access.** Run `sudo -v`. If it asks for your password and returns
  without an error, you have root.
- **curl, the download tool.** Run `curl --version`. If it doesn't print a
  version, install curl with `sudo apt update && sudo apt install curl`. The first half refreshes
  Ubuntu's list of available software, which on a fresh machine can be out of
  date and make the download fail.
- **An internet connection.** Run `curl -sI https://github.com`. If it prints
  lines starting with `HTTP`, your server is online. You don't need to open
  any ports on your router.
- **A server that meets the [System requirements](/docs/setup/requirements).**
  That page lists the RAM, disk, processor and operating system Virtues needs,
  with the command to check each one.

## Get the Virtues app

You pair your server with the Virtues app on a Mac, a Windows PC or an iPhone.
Download the desktop app from
[virtues.com/downloads](https://virtues.com/downloads). A web browser on
another machine can't connect. [Reaching your server](/docs/operate/reach)
explains why.
