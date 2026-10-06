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

## Deploy

```sh
sh apps/loader/stage.sh     # after the build above; writes apps/loader/dist/
```

`stage.sh` puts each build's script and wasm under `wasm/<version>/` and
points `index.html` at that version. `index.html` is served uncached and the
wasm is cached, so a fixed path would let a new `index.html` load an old
cached script (which is exactly what broke the first live-pages deploy for
anyone who had opened a link in the previous hour). Copy `dist/` to the
served directory, keep older `wasm/<version>/` folders for an hour or so,
and update the table below.

## Link previews

Every link shows the same card ("Shared page", from the tags in
`index.html`). A link preview service fetches the page without the part after
`#`, so it cannot tell which page or whose server, and that is on purpose: a
per-link card would need something in the visible part of the URL, and then
`s.virtues.ch` would learn which link was opened.

## Published files

What `s.virtues.ch` serves, so anyone can check it against a build of this
directory (`sha256`):

| File | sha256 |
|---|---|
| `index.html` | `44c897660f5d2829bc0be331b7a78638a09640490ad10b7267b80d58c9a69ace` |
| `wasm/e6e52a7f74a5/virtues_loader.js` | `cf35b309e1efbb32f553545371517e58b52c0070b2dc401c2df2a3d657030efe` |
| `wasm/e6e52a7f74a5/virtues_loader_bg.wasm` | `cd96d7ff2c1cf0f0cf97b6f2590b33c749d7677c416babb62d531c5c596e1feb` |

Update this table with every deploy. A build is not yet reproducible
byte-for-byte across machines, so for now the hashes say what is served, not
that anyone can rebuild it exactly.
