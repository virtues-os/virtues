/**
 * Tauri integration module
 *
 * Re-exports all Tauri-related utilities and types.
 * Safe to import in browser environments - all functions are no-ops when not in Tauri.
 */

export * from './bridge';
export { isTauri, isMacOS, isBrowser } from '$lib/utils/platform';
