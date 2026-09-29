/**
 * The vocabulary two screens have to agree on: the devices list and a single
 * device's page. Extracted when the detail page landed (2026-08-17) — a second
 * copy of `kindLabel` is the kind of thing that stays correct for a month and
 * then quietly disagrees about what a "sensor" is called.
 */
import {
	openFullDiskAccess,
	openAccessibilitySettings,
	type CollectorStatus
} from '$lib/tauri/bridge';
import { confirmAction } from '$lib/stores/dialog.svelte';
import { isTauri } from '$lib/utils/platform';
import { windowShellStore } from '$lib/stores/window-shell.svelte';
import { toast } from 'svelte-sonner';

export type DeviceKind = 'mobile_app' | 'desktop_app' | 'sensor' | 'cli';

export type DevicePermissions = {
	full_disk_access?: boolean;
	safari_library?: boolean;
	accessibility?: boolean;
	denied?: string[];
	checked_at?: string;
	stale?: boolean;
} | null;

export type Device = {
	id: string;
	permissions: DevicePermissions;
	// Mirrors the CHECK on `app_device.kind`. No "browser": the allowlisted
	// iroh key is the credential (middleware/auth.rs), and a bare browser holds
	// none — it cannot be a paired device, only the loopback console.
	kind: DeviceKind;
	/**
	 * Catalog source this device ingests as (`ios`, `mac`), or null for a
	 * device that only views — the Tauri shell, a CLI. The join that lets a
	 * device page point back at what it feeds.
	 */
	source_id: string | null;
	label: string;
	paired_at: string;
	last_seen_at: string | null;
	paired_from_ip: string | null;
	// Reported build identity (X-Virtues-Client header). Null until the device
	// has checked in on a build that reports it. version/sha/channel describe
	// the UI BUNDLE the device's requests come from — which for a paired
	// desktop is the box-served SPA, i.e. it mirrors the box. `app_version` is
	// the native shell's own release and is what a person means by "what
	// version is that device on"; for a collector, `version` IS the binary's
	// release and `app_version` stays null.
	version: string | null;
	sha: string | null;
	channel: string | null;
	app_version: string | null;
	// Device id of the app that installed this collector (the minter of its
	// pair token). The join that lets the list fold a machine's collector
	// under its app instead of showing one Mac as two unrelated rows.
	installed_by: string | null;
	is_current: boolean;
	/**
	 * When this device last told the box where to reach it with a push, or
	 * null if it never has or the address has since been cleared.
	 *
	 * The address itself is deliberately not here: a push token is a
	 * capability to reach the owner's phone, and a list response has no use
	 * for one. Null on a phone is the state worth showing, because a device
	 * the box cannot reach fails silently — Apple accepts a push for a phone
	 * whose owner turned notifications off and returns success, so nothing
	 * else anywhere would ever say so.
	 */
	push_address_at: string | null;
};

export type DevicesResponse = { devices: Device[] };

export function kindLabel(k: DeviceKind): string {
	switch (k) {
		case 'mobile_app':
			return 'Mobile';
		case 'desktop_app':
			return 'Desktop';
		case 'sensor':
			return 'Sensor';
		case 'cli':
			return 'CLI';
	}
}

export function kindIcon(k: DeviceKind): string {
	switch (k) {
		case 'mobile_app':
			return 'ri:smartphone-line';
		case 'desktop_app':
			return 'ri:macbook-line';
		case 'sensor':
			return 'ri:cpu-line';
		case 'cli':
			return 'ri:terminal-line';
	}
}

/**
 * A denied macOS permission, in the owner's terms: what it costs and how to fix
 * it. The collector reports raw capability names; a name alone ("accessibility")
 * tells you nothing about what stopped working.
 *
 * `open` takes the person straight to the pane. macOS buries these two four
 * levels down and the pane cannot be reached by description alone — the
 * previous copy asked someone to navigate there themselves, then to "restart
 * the collector", which names a background daemon they have never heard of and
 * cannot see (2026-08-13).
 */
export type PermissionCopy = {
	label: string;
	costs: string;
	/** What to do once the pane is open, in System Settings' own words. */
	fix?: string;
	/** Which pane `open` goes to, for its button: "Open Full Disk Access". */
	pane?: string;
	open?: () => Promise<boolean>;
};

/**
 * The name System Settings lists the collector under. Its grants are keyed to
 * the binary's path, so the row says `virtues-collector`, not "Virtues" (that
 * row is the app, and turning it on grants the collector nothing).
 */
export const COLLECTOR_LIST_NAME = 'virtues-collector';
/** Where the binary lives, for adding it with "+" when it isn't listed. */
export const COLLECTOR_BINARY_PATH = '~/.virtues/bin/virtues-collector';

const FULL_DISK_ACCESS_FIX = `Turn on ${COLLECTOR_LIST_NAME} in the list. Not listed? Click + and add ${COLLECTOR_BINARY_PATH}.`;

export const PERMISSION_COPY: Record<string, PermissionCopy> = {
	full_disk_access: {
		label: 'Full Disk Access',
		costs: "your Mac can't read Messages or Safari history",
		fix: FULL_DISK_ACCESS_FIX,
		pane: 'Full Disk Access',
		open: openFullDiskAccess
	},
	// Granted by the same Full Disk Access toggle, but reported on its own: a
	// collector can read Messages and still be refused Safari, and one flag
	// for both only ever hid the Safari half.
	safari_library: {
		label: 'Safari history',
		costs: "your Mac can't read Safari history or bookmarks",
		fix: FULL_DISK_ACCESS_FIX,
		pane: 'Full Disk Access',
		open: openFullDiskAccess
	},
	accessibility: {
		label: 'Accessibility',
		costs: 'your Mac records app events without window titles',
		fix: `Turn on ${COLLECTOR_LIST_NAME} in the list.`,
		pane: 'Accessibility',
		open: openAccessibilitySettings
	}
};

/**
 * Every permission the collector is telling us it does NOT have.
 *
 * Two sources, deliberately unioned. `denied[]` is the collector's own list,
 * and a boolean field set to `false` says the same thing — but early builds
 * write one without the other, and reading only the array meant a capability
 * reported `full_disk_access: false` rendered as nothing at all. Nothing is how
 * "not applicable" looks, which is the opposite of what it means.
 *
 * `undefined` is left alone on purpose: it is "the collector never mentioned
 * this", which is neither granted nor denied and must not be drawn as either.
 */
export function deniedPermissions(device: Pick<Device, 'permissions'>) {
	const p = device.permissions;
	if (!p) return [];
	const names = new Set(p.denied ?? []);
	for (const key of Object.keys(PERMISSION_COPY)) {
		if ((p as Record<string, unknown>)[key] === false) names.add(key);
	}
	return [...names].map(
		(name) => PERMISSION_COPY[name] ?? { label: name, costs: "your Mac can't read some data" }
	);
}

/**
 * The mirror: what it says it DOES have. Worth stating positively — "Full Disk
 * Access ✓" is the sentence that ends the "is this a permission problem?"
 * question, and a panel that only ever shows problems can't answer it.
 *
 * A name in `denied[]` wins over a stale `true`, so the two lists can never
 * both claim the same capability.
 */
export function grantedPermissions(device: Pick<Device, 'permissions'>) {
	const p = device.permissions;
	if (!p) return [];
	const denied = new Set(p.denied ?? []);
	return Object.entries(PERMISSION_COPY)
		.filter(([key]) => (p as Record<string, unknown>)[key] === true && !denied.has(key))
		.map(([, copy]) => copy);
}

/** One macOS permission, as a Mac permissions panel shows it. */
export type MacPermission = PermissionCopy & {
	key: 'full_disk_access' | 'safari_library' | 'accessibility';
	/** Setup waits on it. Accessibility is optional. */
	required: boolean;
	/** true granted, false denied, null not reported (never draw null as either). */
	granted: boolean | null;
};

const MAC_PERMISSION_ORDER = [
	{ key: 'full_disk_access', required: true },
	{ key: 'safari_library', required: true },
	{ key: 'accessibility', required: false }
] as const;

function macPermissionList(
	read: (key: MacPermission['key']) => boolean | null
): MacPermission[] {
	const list: MacPermission[] = MAC_PERMISSION_ORDER.map(({ key, required }) => ({
		...PERMISSION_COPY[key],
		key,
		required,
		granted: read(key)
	}));
	// Safari gets its own row only when it has something to add: the collector
	// reports it, and Full Disk Access is on. Until then the Full Disk Access
	// row speaks for both, because one toggle grants both and two rows sending
	// the reader to the same switch read as two chores.
	const fullDisk = list[0].granted;
	return list.filter((p) => p.key !== 'safari_library' || (p.granted !== null && fullDisk === true));
}

/**
 * The Mac's permissions from the collector running here (`get_collector_status`).
 * Only the daemon's own report counts: `permissionsReportedByDaemon` false
 * means the flags describe some other process, so every row reads unknown.
 */
export function macPermissionsFromStatus(status: CollectorStatus | null): MacPermission[] {
	const trusted = !!status?.permissionsReportedByDaemon;
	return macPermissionList((key) => {
		if (!status || !trusted) return null;
		if (key === 'full_disk_access') return status.hasFullDiskAccess;
		if (key === 'accessibility') return status.hasAccessibility;
		return status.safariLibrary;
	});
}

/**
 * The Mac's permissions as its collector last reported them to the server.
 * Pass the COLLECTOR's row: the app's row never carries permissions.
 */
export function macPermissionsFromDevice(device: Pick<Device, 'permissions'> | null): MacPermission[] {
	const p = device?.permissions;
	const denied = new Set(p?.denied ?? []);
	return macPermissionList((key) => {
		if (!p) return null;
		if (denied.has(key)) return false;
		const v = (p as Record<string, unknown>)[key];
		return typeof v === 'boolean' ? v : null;
	});
}

/** The Mac permissions a device reports as off, one per row a panel draws. */
export function deniedMacPermissions(device: Pick<Device, 'permissions'> | null): MacPermission[] {
	return macPermissionsFromDevice(device).filter((p) => p.granted === false);
}

/**
 * Is this Mac collecting what Setup asks of it? The collector is running AND
 * the daemon itself reports Full Disk Access (Messages). Accessibility is
 * optional and never blocks.
 */
export function macReady(status: CollectorStatus | null): boolean {
	return !!status?.running && !!status.permissionsReportedByDaemon && status.hasFullDiskAccess;
}

/**
 * The collector row a desktop app installed, which is the row that carries
 * the Mac's permissions. `null` when there is none.
 */
export function collectorOf<D extends Pick<Device, 'id' | 'installed_by'>>(
	app: Pick<Device, 'id'> | null,
	devices: D[]
): D | null {
	if (!app) return null;
	return devices.find((d) => d.installed_by === app.id) ?? null;
}

/**
 * Where a device row's page lives.
 *
 * `this` rather than the row id for the device making the request, because on a
 * Mac that page is not built from the row at all — it is the local collector
 * daemon read over IPC, which knows things the box never receives. A stable
 * word also means Sources can deep-link to "this machine" without knowing an id.
 *
 * KNOWN GAP, and the reason this is a function rather than a template string:
 * one Mac is TWO rows — the app (`is_current`) and the collector (the `.local`
 * hostname, not current) — and only the app row resolves here. Click the
 * collector row and you get the thin box-side page, which is honest but not
 * what someone pointing at their own laptop expects. Closing it needs the box
 * to model a *machine* that credentials hang off; that is deliberately not in
 * this change.
 */
export function deviceHref(device: Pick<Device, 'id' | 'is_current'>): string {
	return device.is_current ? '/virtues/devices/this' : `/virtues/devices/${device.id}`;
}

/**
 * Back to the list, from any of the three screens that a device row can open
 * (the box-side detail, This Mac, This device). Shared because two of those
 * three are platform panels that predate the list having pages under it, and
 * arriving at one with no way back was the first thing wrong with the drill-down.
 */
export function backToDevices(): void {
	windowShellStore.navigate('/virtues/devices', { label: 'Settings' });
}

/**
 * Where to land after revoking THIS device — the one true "return to pairing"
 * flow. In the browser, pairing is the SPA's cookie-redeem `/pair` page. In the
 * Tauri app, pairing is the native shell's concern: reloading the webview root
 * re-runs the app's unpaired gate, which hands control back to the shell. (The
 * precise native handoff — a shell IPC that drops the proven iroh key — is the
 * one open seam; until it lands, the gate + re-pair covers it.)
 */
export function returnToPairing(): void {
	window.location.href = isTauri ? '/' : '/pair';
}

/**
 * Confirm, revoke, and report — shared because the list and the device page
 * both offer it and the 409 below is the part worth never getting wrong.
 *
 * 409 is the box refusing to strand you: revoking your last active device
 * leaves nothing that can reach it, and the only way back in is physical. It
 * needs `virtues sudo` ON the box, so the message names that rather than
 * pretending the click merely failed.
 *
 * Returns true when a device other than the current one was revoked, so the
 * caller can refresh or navigate. Revoking the current device never returns —
 * it hands off to pairing.
 */
export async function revokeDeviceFlow(device: Device): Promise<boolean> {
	const ok = await confirmAction({
		title: device.is_current ? 'Revoke this device?' : `Revoke "${device.label}"?`,
		body: device.is_current
			? `${device.label} is the device you're using. You'll be signed out immediately.`
			: 'It loses access to your server right away.',
		confirmLabel: 'Revoke',
		danger: true
	});
	if (!ok) return false;

	try {
		const resp = await fetch(`/api/devices/${device.id}`, { method: 'DELETE' });
		if (resp.status === 409) {
			toast.error('Cannot revoke the only active device', {
				description:
					'Run `virtues sudo` on your server to confirm before deleting your last paired device.'
			});
			return false;
		}
		if (!resp.ok) {
			const data = await resp.json().catch(() => ({}));
			toast.error('Revoke failed', { description: data.error ?? `HTTP ${resp.status}` });
			return false;
		}
		toast.success('Device revoked');
		if (device.is_current) {
			returnToPairing();
			return false;
		}
		return true;
	} catch (e) {
		toast.error('Revoke failed', {
			description: e instanceof Error ? e.message : 'Network error'
		});
		return false;
	}
}
