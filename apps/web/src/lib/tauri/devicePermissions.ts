/**
 * The iPhone's permission sheets, asked for and read back.
 *
 * iOS shows each sheet once per install; after a refusal the only way back is
 * the app's page in Settings. So a screen that asks has to know three things
 * the plain `enable` commands never said: whether the sheet was answered, what
 * the answer was, and how to reach Settings when it was no. This module is
 * that, for location, microphone and Health.
 *
 * Off iOS — a browser, the Mac app — every read resolves `unavailable` and
 * nothing prompts. A native build older than a command resolves the closest
 * honest answer rather than throwing: the UI is served to shells older than
 * itself (see `bridge.ts`).
 *
 * Notifications are not here: PushRegistrar owns that sheet, behind
 * `plugin:location-probe|request_push` (MobileDeviceScreen).
 */

import { isIOS, isTauri } from '$lib/utils/platform';
import type { AudioStatus, HealthStatus, HealthPermission, MicPermission } from '$lib/mobile/deviceTypes';

export type { HealthPermission, MicPermission } from '$lib/mobile/deviceTypes';

/**
 * The location authorization.
 *
 * - `not_determined` — the sheet has not been answered.
 * - `when_in_use`    — While Using. Collects while the app is alive; the ask
 *                      for Always comes later, on its own (LocationProbe.swift).
 * - `always`         — collects even after iOS ends the app.
 * - `denied`         — refused, or Location Services is off. Only Settings helps.
 * - `restricted`     — blocked by Screen Time or a profile; the person may not
 *                      be able to change it.
 * - `unknown`        — a native build too old to report it; the sheet was
 *                      shown, the answer cannot be read.
 * - `unavailable`    — not the iPhone app.
 */
export type LocationAuth =
	| 'not_determined'
	| 'when_in_use'
	| 'always'
	| 'denied'
	| 'restricted'
	| 'unknown'
	| 'unavailable';

async function invokeIOS<T>(cmd: string): Promise<T> {
	const { invoke } = await import('@tauri-apps/api/core');
	return invoke<T>(cmd);
}

const onIOS = isTauri && isIOS;

/** The current location authorization. Never prompts. */
export async function locationStatus(): Promise<LocationAuth> {
	if (!onIOS) return 'unavailable';
	try {
		return (await invokeIOS<{ status: LocationAuth }>('plugin:location-probe|status')).status;
	} catch {
		return 'unknown';
	}
}

/**
 * Ask for location and wait for the answer: shows the sheet if it has never
 * been answered, starts collecting on a grant, and resolves once the person
 * has chosen (or after two minutes with no answer, with `not_determined`).
 * Already answered, it resolves at once with the standing answer — `denied`
 * means send them to `openAppSettings()`.
 */
export async function requestLocation(): Promise<LocationAuth> {
	if (!onIOS) return 'unavailable';
	try {
		return (await invokeIOS<{ status: LocationAuth }>('plugin:location-probe|request_location')).status;
	} catch {
		// A native build without the command: the old opt-in still shows the
		// sheet, but resolves before it is answered, so the answer is unknown.
		try {
			await invokeIOS('plugin:location-probe|start_probe');
		} catch {
			// Nothing to fall back to.
		}
		return 'unknown';
	}
}

/**
 * Open this app's page in the Settings app. Resolves whether iOS opened it —
 * false off iOS and on a native build without the command, where the caller
 * should say where to go instead ("Settings → Virtues").
 */
export async function openAppSettings(): Promise<boolean> {
	if (!onIOS) return false;
	try {
		return (await invokeIOS<{ opened: boolean }>('plugin:location-probe|open_settings')).opened;
	} catch {
		return false;
	}
}

/** Audio's status: `mic` is the permission, `enabled` whether the person left
 * it on, `recording` whether it is capturing now. Null off iOS or on failure. */
export async function audioStatus(): Promise<AudioStatus | null> {
	if (!onIOS) return null;
	try {
		return await invokeIOS<AudioStatus>('plugin:audio|status');
	} catch {
		return null;
	}
}

/** Show the microphone sheet if unanswered and start recording on a grant.
 * Asks for nothing else. Resolves after the answer. */
export async function enableAudio(): Promise<AudioStatus | null> {
	if (!onIOS) return null;
	try {
		return await invokeIOS<AudioStatus>('plugin:audio|enable');
	} catch {
		return null;
	}
}

/** The microphone permission alone, from `audio.status`. */
export async function micPermission(): Promise<MicPermission> {
	if (!onIOS) return 'unavailable';
	const s = await audioStatus();
	if (!s) return 'unavailable';
	// A native build predating `mic` still says whether it was granted.
	return s.mic ?? (s.authorized ? 'granted' : 'not_determined');
}

/** Health's status. `permission` is whether the sheet has been shown; what the
 * person allowed is not knowable. Null off iOS or on failure. */
export async function healthStatus(): Promise<HealthStatus | null> {
	if (!onIOS) return null;
	try {
		return await invokeIOS<HealthStatus>('plugin:health|status');
	} catch {
		return null;
	}
}

/** Show the Health sheet (iOS shows it only while some type is unasked) and
 * start collecting. Resolves once it closes. */
export async function enableHealth(): Promise<HealthStatus | null> {
	if (!onIOS) return null;
	try {
		return await invokeIOS<HealthStatus>('plugin:health|enable');
	} catch {
		return null;
	}
}

/** Health's `permission`, reading a native build that predates it as `unknown`. */
export function healthPermissionOf(s: HealthStatus | null): HealthPermission {
	if (!s) return 'unavailable';
	return s.permission ?? 'unknown';
}
