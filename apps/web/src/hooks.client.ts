// Client-side SvelteKit hooks.
//
// Auth is the device's proven, allowlisted iroh key (established by the
// transport, not the app), so there is no CSRF token to attach and no session
// cookie to guard — the old `window.fetch` CSRF wrapper that lived here was
// removed with the cookie/CSRF layer.
//
// The one startup task: on the mobile (bundled) build the shell injects the
// box's loopback origin; wire `/api` + `/ws` to it. No-op on desktop, where the
// box serves the app same-origin.
import { initBackendFromShell } from '$lib/config/backend';

initBackendFromShell();

export {};
