/**
 * Mobile layout state.
 *
 * "Mobile" means the phone shell (Tauri iOS/Android), which renders a
 * bottom-tab bar instead of the desktop sidebar. Two signals decide it:
 *  - the shell injects `window.__VIRTUES_MOBILE__ = true` (authoritative), and
 *  - a viewport-width fallback (< 768px) so the mobile chrome can be exercised
 *    in a desktop browser during development without the native shell.
 *
 * The "You" sheet (device toggles, stream logs, long-tail nav, settings) is a
 * full-height overlay owned here so the tab bar and any deep link can open it.
 */

const MOBILE_BREAKPOINT = 768;

function detectShellFlag(): boolean {
	if (typeof window === "undefined") return false;
	return (window as unknown as { __VIRTUES_MOBILE__?: boolean }).__VIRTUES_MOBILE__ === true;
}

function detectViewport(): boolean {
	if (typeof window === "undefined") return false;
	return window.innerWidth < MOBILE_BREAKPOINT;
}

// Shell flag is sticky (a phone never becomes a desktop mid-session); the
// viewport fallback is reactive so dev-browser resizing flips the chrome.
const shellMobile = detectShellFlag();
let viewportMobile = $state(detectViewport());
let youSheetOpen = $state(false);

if (typeof window !== "undefined" && !shellMobile) {
	window.addEventListener("resize", () => {
		viewportMobile = detectViewport();
	});
}

export const mobileLayout = {
	/** True when the bottom-tab mobile chrome should render. */
	get isMobile(): boolean {
		return shellMobile || viewportMobile;
	},
	/** True only in the native phone shell (gates native-plugin surfaces). */
	get isNativeShell(): boolean {
		return shellMobile;
	},
	get youSheetOpen(): boolean {
		return youSheetOpen;
	},
	set youSheetOpen(val: boolean) {
		youSheetOpen = val;
	},
	openYou() {
		youSheetOpen = true;
	},
	closeYou() {
		youSheetOpen = false;
	},
};
