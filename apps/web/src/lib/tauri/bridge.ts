/**
 * Tauri IPC Bridge
 *
 * Provides type-safe wrappers for communicating with the Tauri backend.
 * All functions are no-ops when running in a browser (non-Tauri environment).
 */

import { isTauri } from '$lib/utils/platform';

// Lazy load Tauri API to avoid errors in browser environment
async function getInvoke() {
	if (!isTauri) return null;
	const { invoke } = await import('@tauri-apps/api/core');
	return invoke;
}

/**
 * Collector daemon status
 */
export interface CollectorStatus {
	running: boolean;
	paused: boolean;
	pendingEvents: number;
	pendingMessages: number;
	lastSync: string | null;
	hasFullDiskAccess: boolean;
	hasAccessibility: boolean;
}

// ============================================================================
// Domain / Auth
// ============================================================================

/**
 * Get the stored user domain (e.g., "adam" for adam.virtues.com)
 */
export async function getUserDomain(): Promise<string | null> {
	const invoke = await getInvoke();
	if (!invoke) return null;

	try {
		return await invoke<string | null>('get_user_domain');
	} catch (e) {
		console.error('[Tauri] Failed to get user domain:', e);
		return null;
	}
}

/**
 * Set the user's domain after authentication
 * This will also navigate the WebView to the user's instance
 */
export async function setUserDomain(domain: string): Promise<boolean> {
	const invoke = await getInvoke();
	if (!invoke) return false;

	try {
		await invoke('set_user_domain', { domain });
		return true;
	} catch (e) {
		console.error('[Tauri] Failed to set user domain:', e);
		return false;
	}
}

/**
 * Clear stored domain (logout)
 */
export async function clearUserDomain(): Promise<boolean> {
	const invoke = await getInvoke();
	if (!invoke) return false;

	try {
		await invoke('clear_user_domain');
		return true;
	} catch (e) {
		console.error('[Tauri] Failed to clear user domain:', e);
		return false;
	}
}

// ============================================================================
// Collector Daemon
// ============================================================================

/**
 * Get the current status of the collector daemon
 */
export async function getCollectorStatus(): Promise<CollectorStatus | null> {
	const invoke = await getInvoke();
	if (!invoke) return null;

	try {
		const status = await invoke<{
			running: boolean;
			paused: boolean;
			pending_events: number;
			pending_messages: number;
			last_sync: string | null;
			has_full_disk_access: boolean;
			has_accessibility: boolean;
		}>('get_collector_status');

		// Convert snake_case to camelCase
		return {
			running: status.running,
			paused: status.paused,
			pendingEvents: status.pending_events,
			pendingMessages: status.pending_messages,
			lastSync: status.last_sync,
			hasFullDiskAccess: status.has_full_disk_access,
			hasAccessibility: status.has_accessibility
		};
	} catch (e) {
		console.error('[Tauri] Failed to get collector status:', e);
		return null;
	}
}

/**
 * Install the collector daemon as a LaunchAgent
 * This copies the binary to ~/.virtues/bin and creates a LaunchAgent plist
 */
export async function installCollector(token: string): Promise<boolean> {
	const invoke = await getInvoke();
	if (!invoke) return false;

	try {
		await invoke('install_collector', { token });
		return true;
	} catch (e) {
		console.error('[Tauri] Failed to install collector:', e);
		return false;
	}
}

/**
 * Uninstall the collector daemon
 * This stops the daemon and removes the LaunchAgent
 */
export async function uninstallCollector(): Promise<boolean> {
	const invoke = await getInvoke();
	if (!invoke) return false;

	try {
		await invoke('uninstall_collector');
		return true;
	} catch (e) {
		console.error('[Tauri] Failed to uninstall collector:', e);
		return false;
	}
}

/**
 * Pause data collection (daemon keeps running)
 */
export async function pauseCollector(): Promise<boolean> {
	const invoke = await getInvoke();
	if (!invoke) return false;

	try {
		await invoke('pause_collector');
		return true;
	} catch (e) {
		console.error('[Tauri] Failed to pause collector:', e);
		return false;
	}
}

/**
 * Resume data collection
 */
export async function resumeCollector(): Promise<boolean> {
	const invoke = await getInvoke();
	if (!invoke) return false;

	try {
		await invoke('resume_collector');
		return true;
	} catch (e) {
		console.error('[Tauri] Failed to resume collector:', e);
		return false;
	}
}

// ============================================================================
// System Settings
// ============================================================================

/**
 * Open System Preferences to Full Disk Access pane
 */
export async function openFullDiskAccess(): Promise<boolean> {
	const invoke = await getInvoke();
	if (!invoke) return false;

	try {
		await invoke('open_full_disk_access');
		return true;
	} catch (e) {
		console.error('[Tauri] Failed to open Full Disk Access settings:', e);
		return false;
	}
}

/**
 * Open System Preferences to Accessibility pane
 */
export async function openAccessibilitySettings(): Promise<boolean> {
	const invoke = await getInvoke();
	if (!invoke) return false;

	try {
		await invoke('open_accessibility_settings');
		return true;
	} catch (e) {
		console.error('[Tauri] Failed to open Accessibility settings:', e);
		return false;
	}
}
