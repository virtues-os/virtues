/**
 * Platform detection utilities for Tauri desktop app
 */

/**
 * Check if running inside Tauri desktop app
 */
export const isTauri = typeof window !== 'undefined' && '__TAURI__' in window;

/**
 * Check if running on macOS (only relevant in Tauri)
 */
export const isMacOS = isTauri && navigator.platform.toLowerCase().includes('mac');

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
