# Loader

The page every shared link opens (`virtues.ch`). It is the same static files
for every link: iroh compiled to WebAssembly plus `public/index.html`. It
dials the sharer's door (`crates/virtues-door`) by the key in the link,
through the relay, and shows the page in a sandboxed frame. The design is in
`agents/plan/publishing-plan.md`.

A link is `<loader>/#<door-key>.<token>`. The door key is 43 characters of
base64url (64 of hex also opens). Nothing after `#` reaches a server.

## Build

`ring` needs an LLVM clang with a wasm32 target; Apple's clang has none
(`brew install llvm`).

```sh
cd apps/loader
CC_wasm32_unknown_unknown=/opt/homebrew/opt/llvm/bin/clang \
AR_wasm32_unknown_unknown=/opt/homebrew/opt/llvm/bin/llvm-ar \
  cargo build --target wasm32-unknown-unknown --release
wasm-bindgen target/wasm32-unknown-unknown/release/virtues_loader.wasm \
  --out-dir public/wasm --weak-refs --target web
```

`wasm-bindgen-cli` must match the pinned `wasm-bindgen` version
(`cargo install wasm-bindgen-cli --version 0.2.122`). `public/wasm/` is build
output and is not committed.

## What it shows

| Door's answer | The visitor sees |
|---|---|
| the page | the page, under a one-line bar naming where it came from |
| not found (never shared, revoked, expired) | "This link doesn't open a page" |
| no answer in 8 s (the server is off, or shares nothing so it runs no door) | "This page isn't available right now" |
| a malformed link | "This link is incomplete" |

The page runs its own scripts in an opaque origin under a policy that allows
no network requests, so a shared page cannot load trackers or reach the
visitor's other sites.

## Where it is served

`https://s.virtues.ch/`, static files behind Caddy, with no access log, so
visitors' addresses are not stored. The response headers are part of the
loader's security and are kept here so they can be reviewed with it:

```
Content-Security-Policy: default-src 'none'; script-src 'self' 'unsafe-inline' 'wasm-unsafe-eval';
  style-src 'unsafe-inline'; img-src data: blob:; media-src data: blob:; font-src data:;
  connect-src 'self' https://relay.virtues.ch wss://relay.virtues.ch;
  base-uri 'none'; form-action 'none'; frame-ancestors 'none'
Referrer-Policy: no-referrer
X-Content-Type-Options: nosniff
```

`connect-src 'self'` is there because the module fetches its own `.wasm`.
`'unsafe-inline'` scripts are allowed because the shared page renders in a
`srcdoc` frame, which inherits this policy, and applet pages use inline
scripts; the frame adds its own policy that blocks every network request.

## Published files

What `s.virtues.ch` serves, so anyone can check it against a build of this
directory (`sha256`):

| File | sha256 |
|---|---|
| `index.html` | `ad5d49a981c52638778c72f227f8d551fd2025c4486b7b019d8a621bfc5c9920` |
| `wasm/virtues_loader.js` | `59e4e16d34698376f9fabd72b5139eaa473bbbaf1ebcc377da9c63bea3b38cea` |
| `wasm/virtues_loader_bg.wasm` | `24675e7e7bfc3e76d501710bd410d502ddcbb79716ebbe2454ca5f83bb981d24` |

Update this table with every deploy. A build is not yet reproducible
byte-for-byte across machines, so for now the hashes say what is served, not
that anyone can rebuild it exactly.
