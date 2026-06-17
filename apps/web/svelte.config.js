import adapterStatic from '@sveltejs/adapter-static';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

// Cosmetic compiler warnings we don't want flooding `make dev`. These are
// style/a11y nags, not correctness issues — kept out of the dev console so the
// genuinely-actionable ones below (SSR placement, non-reactive state,
// hydration) stay visible. Run `pnpm build` for the full unfiltered list.
const SILENCED_WARNINGS = new Set([
	'css_unused_selector',
	'a11y_no_static_element_interactions',
	'a11y_no_noninteractive_element_interactions',
	'a11y_interactive_supports_focus',
	'a11y_click_events_have_key_events',
	'state_referenced_locally',
	'slot_snippet_conflict'
]);

/** @type {import('@sveltejs/kit').Config} */
const config = {
	// Consult https://svelte.dev/docs/kit/integrations
	// for more information about preprocessors
	preprocess: vitePreprocess(),

	onwarn(warning, handler) {
		if (SILENCED_WARNINGS.has(warning.code)) return;
		handler(warning);
	},

	kit: {
		// Static SPA build - served by Rust backend
		adapter: adapterStatic({
			pages: 'build',
			assets: 'build',
			fallback: '200.html', // SPA fallback for client-side routing
			precompress: false,
			strict: true
		})
		// No CSRF config needed - static SPA has no server-side form handling
	}
};

export default config;
