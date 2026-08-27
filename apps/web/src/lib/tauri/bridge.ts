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

// ─── Command-surface negotiation ────────────────────────────────────────────
//
// This file is served by the BOX but calls into the APP — two artifacts with
// separate version lines and, until now, nothing negotiated between them. A box
// newer than the installed app `invoke()`s a command that does not exist and
// throws inside whatever feature needed it. That is a live defect today, not a
// future one; it stayed quiet only because the command list stopped changing.
//
// `shellSurface()` asks the app what it supports. `shellSupports(n)` is the
// gate: check it and degrade the feature deliberately, rather than letting an
// unknown command reject somewhere the user cannot interpret.
//
// The Rust side is `COMMAND_SURFACE_VERSION` in src-tauri/src/main.rs; the two
// must move together. See docs/spa-delivery-plan.md.

/** Surface version of a shell too old to answer the question at all. */
const SURFACE_UNKNOWN = 0;

let surfaceCache: number | null = null;

/**
 * The running shell's command-surface version, or 0 in a plain browser and in
 * any shell predating the command itself (which is indistinguishable from — and
 * treated identically to — "supports nothing new"). Cached; the shell cannot
 * change under a running page.
 */
export async function shellSurface(): Promise<number> {
	if (surfaceCache !== null) return surfaceCache;
	const invoke = await getInvoke();
	if (!invoke) return (surfaceCache = SURFACE_UNKNOWN);
	try {
		const v = await invoke<number>('command_surface_version');
		return (surfaceCache = typeof v === 'number' ? v : SURFACE_UNKNOWN);
	} catch {
		// Command absent → a shell older than this negotiation. Not an error:
		// it is exactly the case this mechanism exists to detect.
		return (surfaceCache = SURFACE_UNKNOWN);
	}
}

/**
 * Whether the running shell is new enough for a feature needing surface `min`.
 *
 * Use this to decide, not to assert — the caller should show something honest
 * when it returns false ("update the app to use this"), never a thrown IPC
 * error.
 */
export async function shellSupports(min: number): Promise<boolean> {
	return (await shellSurface()) >= min;
}

/** The app updater's state, mirrored from the shell. See `UpdateStateView` in
 *  main.rs. `null` everywhere the self-updater doesn't exist — plain browsers,
 *  mobile, Windows/Linux shells, and shells predating the command — which the
 *  UI must treat as silence, never as up-to-date. */
export interface AppUpdateState {
	/** A downloaded release waiting for a relaunch, e.g. "1.0.24". */
	stagedVersion: string | null;
	lastCheck:
		| { outcome: 'up_to_date' }
		| { outcome: 'staged'; version: string }
		| { outcome: 'failed'; error: string }
		| null;
}

/**
 * What the shell's self-updater knows. Until this command, a staged update was
 * visible ONLY in the menu-bar tray — nothing the SPA rendered could see it,
 * which is how "is my app current" became unanswerable from inside the app.
 */
export async function appUpdateState(): Promise<AppUpdateState | null> {
	const invoke = await getInvoke();
	if (!invoke) return null;
	try {
		const r = await invoke<{
			staged_version: string | null;
			last_check: AppUpdateState['lastCheck'];
		} | null>('update_state_cmd');
		if (!r) return null;
		return { stagedVersion: r.staged_version ?? null, lastCheck: r.last_check ?? null };
	} catch {
		return null;
	}
}

/** Ask the shell to run an update check now. Fire-and-forget: re-read
 *  appUpdateState() for the verdict. Silently nothing in browsers/mobile/old
 *  shells — same contract as every other addition. */
export async function checkAppUpdate(): Promise<void> {
	const invoke = await getInvoke();
	if (!invoke) return;
	try {
		await invoke('check_app_update_cmd');
	} catch {
		// Shell predates the command — the caller's state read shows nothing new.
	}
}

/** Restart into a staged app update. No-op when nothing is staged. */
export async function applyAppUpdate(): Promise<void> {
	const invoke = await getInvoke();
	if (!invoke) return;
	try {
		await invoke('apply_update_cmd');
	} catch {
		// Shell too old for the command — the chip that calls this only renders
		// when appUpdateState() answered, so this is belt-and-braces.
	}
}

/** What the native shell reports about itself. See `ShellIdentity` in lib.rs. */
export interface ShellIdentity {
	/** The native app's version — `tauri.conf.json > version`. */
	appVersion: string;
	/** Command contract this shell exposes. */
	commandSurface: number;
	/** Active OTA bundle's content hash, or null when running the baked build. */
	activeBundle: string | null;
	/** What the last update check concluded; null if none has run. */
	lastCheck: OtaCheck | null;
}

/** Outcome of the shell's last OTA check. Mirrors `Outcome` in web_bundle.rs. */
export type OtaCheck =
	| { state: 'up_to_date' }
	| { state: 'applied'; contentHash: string }
	| { state: 'shell_too_old'; needs: number; have: number }
	| { state: 'no_bundle_on_box' }
	| { state: 'rolled_back'; contentHash: string };

/**
 * One line describing an update check, or null when there is nothing worth
 * saying. Deliberately silent for the ordinary states: "up to date" and "your
 * box serves no bundle" are not news. `shell_too_old` is the one that must
 * always speak — it is the case where everything is working correctly and the
 * user still sees stale UI, which without an explanation reads as a bug.
 */
export function describeOtaCheck(c: OtaCheck | null): string | null {
	if (!c) return null;
	switch (c.state) {
		case 'shell_too_old':
			return `Your box has newer UI that needs a newer app (needs ${c.needs}, this app has ${c.have}) — update from the App Store.`;
		case 'applied':
			return 'Newer UI downloaded — it will be used next time the app starts.';
		case 'rolled_back':
			// Also worth a word: the device is deliberately refusing the box's
			// bundle after a failed boot, which otherwise looks like OTA
			// silently not working.
			return 'A newer UI failed to start on this device and was set aside — the next box update clears it.';
		default:
			return null;
	}
}

/**
 * Ask the shell what it is. `null` in a plain browser or on a shell too old to
 * answer — both mean "no native identity to show", not an error.
 *
 * This is the third of three version lines. The SPA knows what it was built
 * from (`$lib/build`) and the box reports its own (`/health`), but only the
 * shell can say whether this UI arrived over the air or shipped inside the app.
 */
export async function shellIdentity(): Promise<ShellIdentity | null> {
	const invoke = await getInvoke();
	if (!invoke) return null;
	try {
		const r = await invoke<{
			app_version: string;
			command_surface: number;
			active_bundle: string | null;
			last_check: OtaCheck | null;
		}>('shell_identity_cmd');
		return {
			appVersion: r.app_version,
			commandSurface: r.command_surface,
			activeBundle: r.active_bundle ?? null,
			lastCheck: r.last_check ?? null
		};
	} catch {
		return null;
	}
}

/**
 * Ask the shell to check the box for a newer UI bundle.
 *
 * Best-effort by design: it needs command surface 2, and `bundle-contract.json`
 * deliberately still requires only 1, so on an older shell this is a no-op
 * rather than a reason to strand the client. Returns immediately — the shell
 * does the work on its own thread.
 *
 * Call this when the app returns to the foreground. The launch-time check alone
 * is not enough: the mobile app stays alive for days (the mic session doubles as
 * the background keepalive), so a phone that is never cold-started would never
 * check again.
 */
export async function otaCheckNow(): Promise<void> {
	const invoke = await getInvoke();
	if (!invoke) return;
	try {
		await invoke('ota_check_now');
	} catch {
		// Shell older than surface 2, or desktop. Nothing to do.
	}
}

/**
 * Tell the shell this UI booted successfully.
 *
 * An OTA bundle is *pending* until this lands. If the app starts and still
 * finds one pending, that bundle failed to come up and is abandoned — so
 * **failing to call this rolls back every update**, which is the intended
 * failure direction but a silent one if the call is simply forgotten.
 *
 * Call it once the app has actually rendered, not on module load: the point is
 * to prove the bundle works, and a module that parses is not a page that
 * renders. No-op outside Tauri and on shells without the command.
 */
export async function reportBootOk(): Promise<void> {
	const invoke = await getInvoke();
	if (!invoke) return;
	try {
		await invoke('bundle_boot_ok');
	} catch {
		// Older shell without the command, or a desktop build that does not use
		// overlays. Nothing to report to, and nothing broken.
	}
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
	/** The installed collector binary's release, or null from one too old to say. */
	version: string | null;
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
 * The outcome of one status read.
 *
 * `null` used to be the answer to three different questions — "not running
 * under Tauri", "the daemon read failed", and "there is no daemon" — and This
 * Mac, which can only act on the third, treated all three as "keep waiting".
 * A box whose `virtues-collector status` exits non-zero (the Rust command
 * returns Err for exactly that case) therefore sat on "Checking this Mac…"
 * forever. Naming the three outcomes is what lets a caller say something true
 * about each.
 */
export type CollectorProbe =
	/** Read succeeded. */
	| { kind: 'ok'; status: CollectorStatus }
	/** Not inside the desktop app — there is no daemon to ask. */
	| { kind: 'unavailable' }
	/** The daemon was asked and could not answer. */
	| { kind: 'error'; message: string };

/**
 * Read the collector daemon's status, keeping the failure reason.
 */
export async function probeCollectorStatus(): Promise<CollectorProbe> {
	const invoke = await getInvoke();
	if (!invoke) return { kind: 'unavailable' };

	try {
		const status = await invoke<{
			version?: string | null;
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
			kind: 'ok',
			status: {
				version: status.version ?? null,
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
			}
		};
	} catch (e) {
		console.error('[Tauri] Failed to get collector status:', e);
		return { kind: 'error', message: e instanceof Error ? e.message : String(e) };
	}
}

/**
 * Get the current status of the collector daemon, or null if it can't be read.
 * For callers that only poll for a *good* answer and have nothing to say about
 * the difference between the failure modes (see `probeCollectorStatus`).
 */
export async function getCollectorStatus(): Promise<CollectorStatus | null> {
	const probe = await probeCollectorStatus();
	return probe.kind === 'ok' ? probe.status : null;
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

// ─────────────────────────────────────────────────────────────────────────────
// Global summon shortcut
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Default chord, mirrored from `DEFAULT_SUMMON_CHORD` in main.rs so Settings can
 * show it without a round-trip. ⌘Space is Spotlight (or Raycast, on the machines
 * our users have), ⌥Space is Alfred's default, ⌥⌘Space is Spotlight's Finder
 * window — ⌘⇧Space is the one that's actually free.
 */
export const DEFAULT_SUMMON_CHORD = 'CmdOrCtrl+Shift+Space';

const SUMMON_CHORD_KEY = 'virtues-summon-chord';

/** The stored chord, or the default. Per-device: it's about this keyboard. */
export function storedSummonChord(): string {
	try {
		return localStorage.getItem(SUMMON_CHORD_KEY) || DEFAULT_SUMMON_CHORD;
	} catch {
		return DEFAULT_SUMMON_CHORD;
	}
}

/**
 * Bind the OS-global summon chord. Returns the accelerator actually in force,
 * which is not always the one asked for — a chord another app already holds is
 * rejected, and the native side falls back to the default rather than leaving
 * the app with no summon at all.
 *
 * No-op outside Tauri: a browser tab has no business claiming an OS hotkey.
 */
export async function setSummonShortcut(accelerator: string): Promise<string | null> {
	const invoke = await getInvoke();
	if (!invoke) return null;

	try {
		const bound = await invoke<string>('set_summon_shortcut', { accelerator });
		try {
			localStorage.setItem(SUMMON_CHORD_KEY, bound);
		} catch {
			/* private mode — the binding holds for this session regardless */
		}
		return bound;
	} catch (e) {
		console.error('[tauri] could not bind summon chord:', e);
		return null;
	}
}

/**
 * Run `onSummon` when the OS-global chord fires. Returns an unlisten function.
 *
 * The native side has already shown and focused the window by the time this
 * runs; what summoning *means* beyond that is the frontend's call, which is why
 * the event carries no payload.
 */
export async function onSummon(handler: () => void): Promise<() => void> {
	if (!isTauri) return () => {};
	try {
		const { listen } = await import('@tauri-apps/api/event');
		return await listen('virtues://summon', () => handler());
	} catch (e) {
		console.error('[tauri] could not listen for summon:', e);
		return () => {};
	}
}

// ─── The pairing door ────────────────────────────────────────────────────────
//
// Pairing is structurally LAN-only (a device can't use iroh until it's
// allowlisted, and allowlisting happens at pairing), so a phone away from home
// can't enroll against the box — however reachable that box is to this
// already-paired computer. The door lets THIS machine stand in for the box at
// its own LAN address, for the length of one Add-device window: the phone
// types this address into its pairing screen and consumes the code normally.
// See crates/virtues-reach-client/src/pair_door.rs.

/** `host:port` for the phone to type, plus the window it has. */
export interface PairDoor {
	origin: string;
	ttlSecs: number;
}

/**
 * Open the pairing door on this computer's LAN address. Desktop-only —
 * resolves null in a browser, on a phone, or when this machine isn't paired
 * and connected (a door onto an unreachable box would only fail later).
 */
export async function openPairDoor(ttlSecs?: number): Promise<PairDoor | null> {
	const invoke = await getInvoke();
	if (!invoke) return null;
	try {
		return await invoke<PairDoor>('plugin:reach|pair_door_open', { ttlSecs });
	} catch (e) {
		// Expected on a phone and in a browser; the caller falls back to the
		// LAN-only QR rather than showing an error for a path that never applied.
		console.warn('[Tauri] pair_door_open unavailable:', e);
		return null;
	}
}

/** Close the pairing door. Safe to call when none is open. */
export async function closePairDoor(): Promise<void> {
	const invoke = await getInvoke();
	if (!invoke) return;
	try {
		await invoke('plugin:reach|pair_door_close');
	} catch (e) {
		console.warn('[Tauri] pair_door_close failed:', e);
	}
}
