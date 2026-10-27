// `$app/environment` for the `components` test project, which runs without
// SvelteKit's plugin (vitest.config.ts). The app is a browser-only SPA, so a
// mounted component is in the browser.
export const browser = true;
export const dev = true;
export const building = false;
export const version = 'test';
