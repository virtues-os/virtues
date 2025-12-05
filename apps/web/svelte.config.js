import adapterAuto from '@sveltejs/adapter-auto';
import adapterNode from '@sveltejs/adapter-node';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

// Use adapter-node for Docker/self-hosted deployments (when ADAPTER=node is set)
// Otherwise use adapter-auto which auto-detects Vercel, Cloudflare, etc.
const useNodeAdapter = process.env.ADAPTER === 'node';

/** @type {import('@sveltejs/kit').Config} */
const config = {
	// Consult https://svelte.dev/docs/kit/integrations
	// for more information about preprocessors
	preprocess: vitePreprocess(),

	kit: {
		// adapter-auto only supports some environments, see https://svelte.dev/docs/kit/adapter-auto for a list.
		// For Docker deployments, set ADAPTER=node to use adapter-node
		adapter: useNodeAdapter ? adapterNode() : adapterAuto(),

		// CSRF protection - enabled by default, explicit configuration for clarity
		csrf: {
			checkOrigin: true
		}
	}
};

export default config;
