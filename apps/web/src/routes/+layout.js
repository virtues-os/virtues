// SvelteKit layout configuration
// When using adapter-static, SSR must be disabled for client-side routing to work

// These settings apply when using ADAPTER=static:
// - ssr: false - Disable server-side rendering (everything runs in browser)
// - csr: true - Enable client-side rendering (default, but explicit for clarity)
// - prerender: false - Don't prerender pages (we want dynamic SPA behavior)

// Note: When using adapter-node or adapter-auto, these settings are still fine
// as they allow the app to work as an SPA with proper fallback routing

export const ssr = false;
export const csr = true;
export const prerender = false;
