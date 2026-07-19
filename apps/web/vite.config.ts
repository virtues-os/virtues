import tailwindcss from '@tailwindcss/vite';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig, loadEnv } from 'vite';

export default defineConfig(({ mode }) => {
	// Load env file from project root (../..)
	const env = loadEnv(mode, '../..', '');

	return {
		envDir: '../..', // Load .env from project root
		plugins: [tailwindcss(), sveltekit()],
		define: {
			// Build identity, baked at build time — mirrors the box's build.rs.
			// GIT_COMMIT = full sha; GIT_DESCRIBE = the release tag (CI sets
			// VIRTUES_BUILD_VERSION on shallow checkouts). channel is derived from
			// the tag in $lib/build. See docs/update-identity-spine.md.
			'__BUILD_COMMIT__': JSON.stringify(process.env.GIT_COMMIT || 'dev'),
			'__BUILD_VERSION__': JSON.stringify(
				process.env.GIT_DESCRIBE || process.env.VIRTUES_BUILD_VERSION || 'dev'
			),
		},
		server: {
			fs: {
				// Allow Vite to serve files from the repo root, not just apps/web/.
				// View-runtime action UIs live at actions/<name>/ui/ and are
				// imported via `import.meta.glob` from $lib/action-views.
				allow: ['../..']
			},
			proxy: {
				// Proxy all API and auth calls to Rust backend
				'/api': {
					target: env.BACKEND_URL || 'http://localhost:8000',
					changeOrigin: true
				},
				'/auth': {
					target: env.BACKEND_URL || 'http://localhost:8000',
					changeOrigin: true
				},
				// OAuth proxy redirects browser here after the dance — Rust handler
				// verifies signed state, fetches secrets via /exchange, then 302s
				// back to /sources?connected=...
				'/oauth': {
					target: env.BACKEND_URL || 'http://localhost:8000',
					changeOrigin: true
				},
				// Proxy WebSocket connections for Yjs real-time sync
				'/ws': {
					target: env.BACKEND_URL || 'http://localhost:8000',
					changeOrigin: true,
					ws: true
				}
			}
		}
	};
});
