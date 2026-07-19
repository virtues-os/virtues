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

// On the native phone shell, lock the viewport so it behaves like an app:
// no pinch-to-zoom, and no auto-zoom when a text input (< 16px) is focused
// (WKWebView honours maximum-scale=1 for both). Scoped to mobile so the
// desktop browser keeps normal zoom/accessibility. viewport-fit=cover makes
// the page paint edge-to-edge (behind the Dynamic Island / home indicator) —
// without it iOS letterboxes the webview and the bands show the bare native
// window, not the theme. It also activates env(safe-area-inset-*), which the
// tab bar / layout / settings sheet already use to keep content clear.
if (typeof window !== 'undefined' && (window as unknown as { __VIRTUES_MOBILE__?: boolean }).__VIRTUES_MOBILE__) {
	let vp = document.querySelector('meta[name="viewport"]');
	if (!vp) {
		vp = document.createElement('meta');
		vp.setAttribute('name', 'viewport');
		document.head.appendChild(vp);
	}
	vp.setAttribute(
		'content',
		'width=device-width, initial-scale=1, maximum-scale=1, minimum-scale=1, user-scalable=no, viewport-fit=cover'
	);

	// Native appearance bridge: themes are user-picked, so the iOS status bar
	// can't follow the system light/dark mode — tell the shell the active
	// theme's darkness (it flips UIWindow.overrideUserInterfaceStyle, which the
	// status bar, keyboard, and native sheets all resolve from). Fire once for
	// the cached theme and again on every theme change.
	const syncAppearance = async () => {
		try {
			const [{ invoke }, { getTheme, isThemeDark }] = await Promise.all([
				import('@tauri-apps/api/core'),
				import('$lib/utils/theme')
			]);
			await invoke('set_appearance', { dark: isThemeDark(getTheme()) });
		} catch {
			// Not running in the Tauri shell (or command missing) — cosmetic only.
		}
	};
	syncAppearance();
	window.addEventListener('themechange', syncAppearance);

	// Marks the native phone shell for global CSS (e.g. suppressing the iOS
	// long-press callout so it doesn't fight the app's own context menus).
	document.documentElement.classList.add('native-mobile');
}

export {};
