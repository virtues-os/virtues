// Client-side SvelteKit hooks.
//
// Intentionally empty. Auth is the device's proven, allowlisted iroh key
// (established by the transport, not the app), so there is no CSRF token to
// attach and no session cookie to guard — the old `window.fetch` CSRF wrapper
// that lived here was removed with the cookie/CSRF layer.
export {};
