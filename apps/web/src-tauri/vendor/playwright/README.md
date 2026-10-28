# Playwright's injected script

`injected.js` is Playwright's in-page script with one change. It is the
`source` string of `packages/playwright-core/src/generated/injectedScriptSource.ts`,
as bundled in the `playwright-core@1.63.0` npm package. The shell runs it in an
isolated WebKit content world, where its `ariaSnapshot(…, { mode: 'ai' })` gives
the assistant the same refs-per-element page outline Playwright MCP gives a model
(`src/browser_host.rs`, measured in `agents/record/webkit-agent-spike.md`).

The change: a field's value reaches the outline only through
`virtuesFieldValue`, which shows a password, one-time-code or card field as
`(filled, value hidden)`. The outline goes to a model, which must never see
those values. `virtuesSecretField`, next to `toAriaNode`, decides which fields
count, and the shell also asks it before typing and before a screenshot
(`src/browser_host.rs`). Search for `Virtues:` to find the change, and apply it
again after a re-sync. A test in `browser_host.rs` fails until you do, but
only on a Mac: run `cargo test --bin virtues-app` in `apps/web/src-tauri`. CI
does not run it.

Apache-2.0; `LICENSE` and `NOTICE` are Playwright's own, shipped with it.

Only the snapshot and its ref lookup are used. Copying the source files for
those alone (`packages/injected/src/ariaSnapshot.ts`, `roleUtils.ts`,
`domUtils.ts` and their isomorphic imports) would drop most of the 320 KB; until
then, re-sync by extracting the same string from a newer `playwright-core`.
