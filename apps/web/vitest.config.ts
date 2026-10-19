import { fileURLToPath } from 'node:url';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { configDefaults, defineConfig } from 'vitest/config';

// Two projects, neither with SvelteKit's Vite plugins nor the app build.
//
// `unit`: in Node. The chat suites feed the box's recorded UI-message stream
// through the AI SDK's own transport and parser; the editor suites mount a
// real CodeMirror or Tiptap view under happy-dom (each of those files opts
// in with `// @vitest-environment happy-dom`). Rune stores (`*.svelte.ts`)
// cannot be compiled here; these tests `vi.mock` those modules.
//
// `components`: a component mounted under happy-dom, with the Svelte plugin
// and Svelte's browser build (`mount`), in files named `*.svelte.test.ts`,
// which may use runes themselves.
//
// `$lib` and `$contract` are resolved here by hand because the SvelteKit
// plugin that normally provides them (svelte.config.js `kit.alias`) is not
// loaded.
export default defineConfig({
	resolve: {
		alias: {
			$lib: fileURLToPath(new URL('./src/lib', import.meta.url)),
			$contract: fileURLToPath(
				new URL('../../crates/virtues-document/contract.json', import.meta.url),
			),
		},
	},
	test: {
		projects: [
			{
				extends: true,
				test: {
					name: 'unit',
					include: ['src/**/*.test.ts'],
					exclude: [...configDefaults.exclude, 'src/**/*.svelte.test.ts'],
					environment: 'node',
					// The editor converts a paste through the crate's converter, built
					// to WebAssembly: every editor test pastes through the real one.
					globalSetup: ['./scripts/vitest-setup.mjs'],
				},
			},
			{
				extends: true,
				plugins: [svelte({ configFile: false, emitCss: false })],
				resolve: { conditions: ['browser'] },
				test: {
					name: 'components',
					include: ['src/**/*.svelte.test.ts'],
					environment: 'happy-dom',
				},
			},
		],
	},
});
