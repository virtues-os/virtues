import { defineConfig } from 'vitest/config';

// Unit tests only, in Node, with none of SvelteKit's Vite plugins: the one
// suite here feeds the box's recorded UI-message stream through the AI SDK's
// own transport and parser, which needs nothing from the app build.
export default defineConfig({
	test: {
		include: ['src/**/*.test.ts'],
		environment: 'node'
	}
});
