// See https://svelte.dev/docs/kit/types#app.d.ts
// for information about these interfaces

// Build-time constants injected by Vite (see vite.config.ts)
declare const __BUILD_COMMIT__: string;

declare global {
	namespace App {
		// interface Error {}
		// interface Locals {} - No server-side locals in static build
		// interface PageData {}
		// interface PageState {}
		// interface Platform {}
	}
}

export {};
