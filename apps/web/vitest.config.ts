import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vitest/config';

// Unit tests only, in Node, with none of SvelteKit's Vite plugins. The chat
// suites feed the box's recorded UI-message stream through the AI SDK's own
// transport and parser; the editor suites mount a real CodeMirror view under
// happy-dom (each of those files opts in with `// @vitest-environment
// happy-dom`). Neither needs the app build.
//
// `$lib` is resolved here by hand because the SvelteKit plugin that normally
// provides it is not loaded. Rune stores (`*.svelte.ts`) still cannot be
// compiled without the Svelte plugin; tests `vi.mock` those modules.
export default defineConfig({
	resolve: {
		alias: {
			$lib: fileURLToPath(new URL('./src/lib', import.meta.url)),
		},
	},
	test: {
		include: ['src/**/*.test.ts'],
		environment: 'node',
	},
});
