/**
 * Mobile layout state.
 *
 * "Mobile" means the phone shell (Tauri iOS/Android), which is chat-first: one
 * window opening on the chat, with a left-edge drawer as the only navigation
 * chrome. Two signals decide it:
 *  - the shell injects `window.__VIRTUES_MOBILE__ = true` (authoritative), and
 *  - a viewport-width fallback (< 768px) so the mobile chrome can be exercised
 *    in a desktop browser during development without the native shell.
 *
 * The drawer's open state lives here rather than in MobileShell because more
 * than the shell has an interest in it: the shell drives it from the edge
 * gesture, rows inside the drawer close it after navigating, and anything that
 * raises the keyboard needs to know the drawer is in the way.
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
//
// `__VIRTUES_PAIRED__` is baked into the window's init script when the shell
// BUILDS the window, so on the very launch that pairs, a reload from
// connect.html still reads the stale `false` — and the first-run permission
// cards silently waited for the next cold launch. The connect shell therefore
// leaves a marker in localStorage (same `virtues://localhost` origin) the
// moment pairing finishes; it bridges exactly that one session. Once the baked
// flag itself says paired, the marker has done its job and is cleared —
// including after an unpair, so it can never resurrect a forgotten pairing.
const JUST_PAIRED_KEY = "virtues-just-paired";

function detectPaired(): boolean {
	if (typeof window === "undefined") return false;
	const baked =
		(window as unknown as { __VIRTUES_PAIRED__?: boolean }).__VIRTUES_PAIRED__ === true;
	try {
		if (baked) {
			localStorage.removeItem(JUST_PAIRED_KEY);
			return true;
		}
		return localStorage.getItem(JUST_PAIRED_KEY) === "true";
	} catch {
		return baked;
	}
}

/**
 * What the active chat wants the shell's top-right button to be.
 *
 * The button is modal: on an EMPTY chat "new chat" is a no-op (it is already
 * new), so the slot carries the temporary-chat (ghost) toggle instead; once a
 * conversation exists the mode is settled, the toggle would be dead weight,
 * and the slot goes back to compose. Only ChatView knows emptiness and owns
 * the ghost state, so it publishes this while it is the active mobile view and
 * the shell renders it. `null` (any non-chat view) means compose.
 */
export interface ChatChrome {
	empty: boolean;
	ghost: boolean;
	toggleGhost: () => void;
}

const ONBOARDING_KEY = "virtues-onboarding-done";

const shellMobile = detectShellFlag();
const shellPaired = detectPaired();
let viewportMobile = $state(detectViewport());
let drawerOpen = $state(false);
let chatChrome = $state<ChatChrome | null>(null);
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
	/** True when the chat-first mobile chrome should render. */
	get isMobile(): boolean {
		return shellMobile || viewportMobile;
	},
	/** True only in the native phone shell (gates native-plugin surfaces). */
	get isNativeShell(): boolean {
		return shellMobile;
	},
	get drawerOpen(): boolean {
		return drawerOpen;
	},
	openDrawer() {
		drawerOpen = true;
		// The drawer and the keyboard cannot share the screen: the drawer slides
		// out over a half-height viewport otherwise. Blurring here (rather than
		// in the shell's gesture handler) covers every way the drawer opens —
		// the hamburger, the edge swipe, a deep link.
		if (typeof document !== "undefined") {
			const el = document.activeElement;
			if (el instanceof HTMLElement) el.blur();
		}
	},
	closeDrawer() {
		drawerOpen = false;
	},
	get chatChrome(): ChatChrome | null {
		return chatChrome;
	},
	setChatChrome(chrome: ChatChrome | null) {
		chatChrome = chrome;
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
