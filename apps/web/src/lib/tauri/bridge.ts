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
export async function installCollector(token: string): Promise<void> {
	const invoke = await getInvoke();
	if (!invoke) {
		throw new Error("Desktop bridge unavailable — open this in the Virtues app, not a browser.");
	}

	try {
		await invoke('install_collector', { token });
	} catch (e) {
		// The Tauri command returns the collector's real stderr as the error
		// string (and "program not found" when the sidecar isn't bundled).
		// Surface it instead of collapsing every cause into a bare `false` —
		// "The collector failed to install" with no detail was undiagnosable.
		console.error('[Tauri] Failed to install collector:', e);
		const msg = e instanceof Error ? e.message : String(e);
		throw new Error(msg?.trim() || "The collector failed to install.");
	}
}

/**
 * Disconnect this Mac from its box: clears the stored pairing (keychain +
 * bundle) and the proxy LaunchAgent. Local-only — doesn't need the box
 * reachable. Tauri-only (no-op/false in a browser). After this, reload to the
 * pairing screen.
 */
export async function forgetPairing(): Promise<boolean> {
	const invoke = await getInvoke();
	if (!invoke) return false;
	try {
		await invoke('forget_pairing');
		return true;
	} catch (e) {
		console.error('[Tauri] forget_pairing failed:', e);
		return false;
	}
}

/** Relaunch the desktop app (after disconnecting, so it comes back up on the
 *  pairing screen). Tauri-only. */
export async function restartApp(): Promise<void> {
	const invoke = await getInvoke();
	if (!invoke) return;
	try {
		await invoke('restart_app');
	} catch (e) {
		console.error('[Tauri] restart_app failed:', e);
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
