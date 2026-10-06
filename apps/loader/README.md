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
