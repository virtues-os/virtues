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
 * Open a URL in the user's default *system* browser.
 *
 * In a plain browser this is just `window.open(url, '_blank')`. But inside the
 * Tauri desktop shell the webview **silently drops** `window.open`/`target=_blank`
 * to external origins, so Stripe checkout, the billing portal, social links and
 * citations never open — the user clicks and nothing happens. Route every
 * external open through here: under Tauri it calls the opener plugin (granted by
 * the `opener:default` capability), and falls back to `window.open` everywhere
 * else (and if the plugin call ever throws).
 */
export async function openExternal(url: string): Promise<void> {
	const invoke = await getInvoke();
	if (!invoke) {
		window.open(url, '_blank', 'noopener');
		return;
	}
	try {
		await invoke('plugin:opener|open_url', { url });
	} catch (e) {
		console.error('[tauri] openExternal failed; falling back to window.open', e);
		window.open(url, '_blank', 'noopener');
	}
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
	/**
	 * Whether the two permission flags above describe the DAEMON.
	 *
	 * macOS TCC grants are per-process, so a `status` probe run inside another
	 * process (this app, a terminal) reports that process's access, not the
	 * collector daemon's — which is how a revoked grant once showed as granted
	 * while collection was silently dead. False means the collector is too old
	 * to self-report, or hasn't recently, and the flags are a best guess.
	 */
	permissionsReportedByDaemon: boolean;
	/** When the daemon last checked, if it ever has. */
	permissionsCheckedAt: string | null;
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
			permissions_reported_by_daemon?: boolean;
			permissions_checked_at?: string | null;
		}>('get_collector_status');

		// Convert snake_case to camelCase
		return {
			running: status.running,
			paused: status.paused,
			pendingEvents: status.pending_events,
			pendingMessages: status.pending_messages,
			lastSync: status.last_sync,
			hasFullDiskAccess: status.has_full_disk_access,
			hasAccessibility: status.has_accessibility,
			// Absent on collector builds predating the self-report — treat as
			// "not from the daemon" rather than assuming the flags are authoritative.
			permissionsReportedByDaemon: status.permissions_reported_by_daemon ?? false,
			permissionsCheckedAt: status.permissions_checked_at ?? null
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
