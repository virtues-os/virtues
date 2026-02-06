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
			'__BUILD_COMMIT__': JSON.stringify(process.env.GIT_COMMIT || 'dev'),
		},
		server: {
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
