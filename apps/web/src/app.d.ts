// See https://svelte.dev/docs/kit/types#app.d.ts
// for information about these interfaces

declare global {
	// Build-time constants injected by Vite (see vite.config.ts). Declared inside
	// `declare global` (not at top level) because this file is a module (`export
	// {}`), so a top-level `declare const` would be module-scoped, not global.
	const __BUILD_COMMIT__: string;
	const __BUILD_VERSION__: string;

	namespace App {
		// interface Error {}
		// interface Locals {} - No server-side locals in static build
		// interface PageData {}
		// interface PageState {}
		// interface Platform {}
	}
}

export {};
