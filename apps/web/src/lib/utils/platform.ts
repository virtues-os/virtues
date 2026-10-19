/**
 * Platform detection utilities for Tauri desktop app
 */

/**
 * Check if running inside Tauri desktop app
 */
export const isTauri = typeof window !== 'undefined' && '__TAURI__' in window;

/** The raw platform string, lowercased (empty when no navigator). */
const platformStr =
	typeof navigator !== 'undefined' ? navigator.platform.toLowerCase() : '';

/**
 * Check if running on macOS (only relevant in Tauri)
 */
export const isMacOS = isTauri && platformStr.includes('mac');

/**
 * Apple keyboard layout, Tauri or not — a Mac user in a plain browser still
 * expects ⌘ and still reads ⌥ as Option. Shortcut binding and rendering key off
 * this, never off `isMacOS` (which is deliberately Tauri-gated).
 */
export const isAppleKeyboard =
	platformStr.includes('mac') ||
	platformStr.includes('iphone') ||
	platformStr.includes('ipad');

/**
 * Running on iOS or iPadOS in the Tauri shell.
 *
 * The `macintel` clause is iPadOS, which has reported a desktop `platform`
 * since iPadOS 13 — so an iPad would otherwise satisfy `isMacOS` above and be
 * handed a Mac-only screen. `maxTouchPoints` is what separates them; a Mac
 * reports 0.
 *
 * Deliberately a SEPARATE flag rather than narrowing `isMacOS`, which is read
 * in a dozen places and whose meaning ("the Mac desktop app") is right for all
 * of them. Anywhere both could match, test this one first.
 */
export const isIOS =
	isTauri &&
	(platformStr.includes('iphone') ||
		platformStr.includes('ipad') ||
		(platformStr.includes('macintel') &&
			typeof navigator !== 'undefined' &&
			navigator.maxTouchPoints > 1));

/** The iOS app on an iPad rather than an iPhone (same shell, other name). */
export const isIPad =
	isIOS &&
	(platformStr.includes('ipad') || (platformStr.includes('macintel') && navigator.maxTouchPoints > 1));

/**
 * A phone's own browser (Safari on an iPhone, Chrome on Android), not the
 * app. A page reached this way is on the phone it would otherwise ask to
 * "add".
 */
export const isPhoneBrowser =
	!isTauri &&
	typeof navigator !== 'undefined' &&
	/iphone|android.+mobile/i.test(navigator.userAgent);

/** Running on Windows (only meaningful in Tauri). */
export const isWindows = isTauri && platformStr.includes('win');

/** Running on Linux (only meaningful in Tauri; excludes Android). */
export const isLinux = isTauri && platformStr.includes('linux') && !platformStr.includes('android');

/**
 * A human label for "this computer", by OS. Falls back to "this computer"
 * when the OS is unknown (e.g. plain browser).
 */
export const thisComputerLabel: string = isMacOS
	? 'this Mac'
	: isWindows
		? 'this PC'
		: isLinux
			? 'this Linux machine'
			: 'this computer';

/**
 * Check if running in browser (not Tauri)
 */
export const isBrowser = typeof window !== 'undefined' && !isTauri;

/**
 * Get platform info
 */
export function getPlatformInfo() {
	return {
		isTauri,
		isMacOS,
		isBrowser,
		userAgent: typeof navigator !== 'undefined' ? navigator.userAgent : '',
		platform: typeof navigator !== 'undefined' ? navigator.platform : ''
	};
}
