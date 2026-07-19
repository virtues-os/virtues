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
