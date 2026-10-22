import tailwindcss from '@tailwindcss/vite';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig, loadEnv, type Plugin } from 'vite';

/**
 * KaTeX's stylesheet names each of its fonts three times (woff2, woff, ttf),
 * and the build copies every file a stylesheet names. Every browser the app
 * runs in reads woff2, the first source, so the woff and ttf files (about
 * 800 KB, and gzip copies of the ttfs) went into every update and were never
 * fetched. This keeps the woff2 source alone. It fails the build when the
 * stylesheet no longer matches, so a KaTeX upgrade cannot quietly bring the
 * other formats back.
 */
function katexWoff2Only(): Plugin {
	const OTHER_FORMATS = /,url\([^)]*\.woff\) format\("woff"\),url\([^)]*\.ttf\) format\("truetype"\)/g;
	return {
		name: 'katex-woff2-only',
		enforce: 'pre',
		transform(code, id) {
			if (!/\/katex\/dist\/katex(\.min)?\.css(\?|$)/.test(id)) return;
			const faces = code.match(/@font-face/g)?.length ?? 0;
			const trimmed = code.match(OTHER_FORMATS)?.length ?? 0;
			if (faces === 0 || trimmed !== faces) {
				throw new Error(
					`katex-woff2-only: trimmed ${trimmed} of ${faces} @font-face sources in ${id}; the stylesheet's format changed`
				);
			}
			return { code: code.replace(OTHER_FORMATS, ''), map: null };
		}
	};
}

export default defineConfig(({ mode }) => {
	// Load env file from project root (../..)
	const env = loadEnv(mode, '../..', '');

	return {
		envDir: '../..', // Load .env from project root
		plugins: [katexWoff2Only(), tailwindcss(), sveltekit()],
		define: {
			// Build identity, baked at build time — mirrors the box's build.rs.
			// GIT_COMMIT = full sha; GIT_DESCRIBE = the release tag (CI sets
			// VIRTUES_BUILD_VERSION on shallow checkouts). channel is derived from
			// the tag in $lib/build. See agents/archive/update-identity-spine.md.
			'__BUILD_COMMIT__': JSON.stringify(process.env.GIT_COMMIT || 'dev'),
			'__BUILD_VERSION__': JSON.stringify(
				process.env.GIT_DESCRIBE || process.env.VIRTUES_BUILD_VERSION || 'dev'
			),
		},
		server: {
			fs: {
				// Allow Vite to serve files from the repo root, not just apps/web/.
				// View-runtime applet UIs live at applets/<name>/ui/ and are
				// imported via `import.meta.glob` from $lib/applet-views.
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
				// The box's liveness probe. Unproxied, this fell through to
				// SvelteKit and answered every request with index.html — so in
				// dev, /health returned 200 and "<!doctype html>" forever. That is
				// worse than a 404: anything asking "is the box up?" got a
				// confident yes with the box stopped. It already broke
				// SystemInfoView's version panel, and it would have broken the
				// update watcher, which waits to see the box go down before it
				// accepts it coming back. In both real deployments (a browser
				// served by the box, and the Tauri window on the :7117 reach
				// proxy) /health is same-origin and reaches the box; only dev was
				// the odd one out.
				'/health': {
					target: env.BACKEND_URL || 'http://localhost:8000',
					changeOrigin: true
				},
				// Applet face runtime — sandboxed-iframe HTML/JS/CSS served by
				// core (the `/api/face/query` bridge already rides `/api`). The
				// `/service` proxy fronts service-URL faces the same way.
				'/face': {
					target: env.BACKEND_URL || 'http://localhost:8000',
					changeOrigin: true
				},
				'/service': {
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
				// Proxy WebSocket connections: the page socket, the terminal.
				// Every socket on the box refuses a page whose Origin is not
				// one it serves (`refuse_foreign_socket`), so the Origin goes
				// to the box as the browser sent it, never rewritten to the
				// box's own: the box would take any page in the browser for
				// its own then, its terminal's shell included. `make dev`
				// lists this dev server's origin for the page socket alone
				// (`VIRTUES_PUBLIC_ORIGINS`); the terminal refuses it.
				'/ws': {
					target: env.BACKEND_URL || 'http://localhost:8000',
					changeOrigin: true,
					ws: true
				}
			}
		}
	};
});
