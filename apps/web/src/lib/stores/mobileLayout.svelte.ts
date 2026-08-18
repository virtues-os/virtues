/**
 * Mobile layout state.
 *
 * "Mobile" means the phone shell (Tauri iOS/Android), which renders a
 * bottom-tab bar instead of the desktop sidebar. Two signals decide it:
 *  - the shell injects `window.__VIRTUES_MOBILE__ = true` (authoritative), and
 *  - a viewport-width fallback (< 768px) so the mobile chrome can be exercised
 *    in a desktop browser during development without the native shell.
 *
 * The Settings menu (a directory of every page/route not in the bottom bar) is
 * a full-height overlay owned here so the tab bar and any deep link can open
 * it. It has no sub-screens: it used to carry a `menuView` toggle for the
 * native "This device" dashboard, but that is a route now
 * (/virtues/devices/this), so the menu is a directory and nothing else.
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
function detectPaired(): boolean {
	if (typeof window === "undefined") return false;
	return (window as unknown as { __VIRTUES_PAIRED__?: boolean }).__VIRTUES_PAIRED__ === true;
}

const ONBOARDING_KEY = "virtues-onboarding-done";

const shellMobile = detectShellFlag();
const shellPaired = detectPaired();
let viewportMobile = $state(detectViewport());
let menuOpen = $state(false);
// First-run "Set up your streams" flow — shown once on the paired phone shell.
let onboardingOpen = $state(
	shellMobile &&
		shellPaired &&
		typeof localStorage !== "undefined" &&
		localStorage.getItem(ONBOARDING_KEY) !== "true"
);

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
	get menuOpen(): boolean {
		return menuOpen;
	},
	/** Open the Settings menu — the route directory, its only screen. */
	openMenu() {
		menuOpen = true;
	},
	closeMenu() {
		menuOpen = false;
	},
	/** First-run stream-setup flow (paired native shell, once). */
	get onboardingOpen(): boolean {
		return onboardingOpen;
	},
	/** Dismiss onboarding (Done or Skip) and don't show it again. */
	finishOnboarding() {
		onboardingOpen = false;
		if (typeof localStorage !== "undefined") localStorage.setItem(ONBOARDING_KEY, "true");
	},
	/** Re-open onboarding on demand (e.g. from the device screen). */
	openOnboarding() {
		onboardingOpen = true;
	},
};
