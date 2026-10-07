# Playwright's injected script

`injected.js` is Playwright's in-page script, unmodified. It is the
`source` string of `packages/playwright-core/src/generated/injectedScriptSource.ts`,
as bundled in the `playwright-core@1.63.0` npm package. The shell runs it in an
isolated WebKit content world, where its `ariaSnapshot(…, { mode: 'ai' })` gives
the assistant the same refs-per-element page outline Playwright MCP gives a model
(`src/browser.rs`, measured in `agents/record/webkit-agent-spike.md`).

Apache-2.0; `LICENSE` and `NOTICE` are Playwright's own, shipped with it.

Only the snapshot and its ref lookup are used. Copying the source files for
those alone (`packages/injected/src/ariaSnapshot.ts`, `roleUtils.ts`,
`domUtils.ts` and their isomorphic imports) would drop most of the 320 KB; until
then, re-sync by extracting the same string from a newer `playwright-core`.
