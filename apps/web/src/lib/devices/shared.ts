/**
 * The vocabulary two screens have to agree on: the devices list and a single
 * device's page. A device's kind sentence, its platform and when it last
 * reached the server are worded here once, so the list and the page never
 * disagree about what a device is called.
 */
import {
	openFullDiskAccess,
	openAccessibilitySettings,
	type CollectorStatus,
	type PaneOpened
} from '$lib/tauri/bridge';
import { confirmAction } from '$lib/stores/dialog.svelte';
import { isTauri } from '$lib/utils/platform';
import { toast } from 'svelte-sonner';

export type DeviceKind = 'mobile_app' | 'desktop_app' | 'sensor' | 'cli' | 'agent';

export type DevicePermissions = {
	full_disk_access?: boolean;
	safari_library?: boolean;
	accessibility?: boolean;
	system_audio?: boolean;
	microphone?: boolean;
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
	/** The platform the pairing client reported: `ios`, `macos`, `windows` or
	 *  `linux`. Null for a phone added by the Mac's handoff code and for the
	 *  console. iPhone and iPad both report `ios`. */
	os: string | null;
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

/**
 * The `source_id` pairing stores for an app that collects nothing: the Mac,
 * Windows and Linux apps and an agent. It is not a data source, so a row
 * carrying it is never a collector.
 */
export const NO_SOURCE = '__device__';

/** A Mac collector: the desktop row that sends data as a source (`mac`). */
export function isCollector(d: Pick<Device, 'kind' | 'source_id'>): boolean {
	return d.kind === 'desktop_app' && !!d.source_id && d.source_id !== NO_SOURCE;
}

export type Platform = 'mac' | 'ios' | 'windows' | 'linux' | 'desktop' | 'agent' | 'console' | 'sensor';

/**
 * What a device is, for its sentence and its page. The desktop platform comes
 * from `os`; a Mac app paired before `os` was recorded still reads as a Mac
 * when it installed a collector, since only the Mac has one.
 */
export function platformOf(d: Pick<Device, 'kind' | 'os' | 'source_id'>, hasCollector = false): Platform {
	switch (d.kind) {
		case 'cli':
			return 'console';
		case 'agent':
			return 'agent';
		case 'sensor':
			return 'sensor';
		case 'mobile_app':
			return 'ios';
		case 'desktop_app':
			if (d.os === 'macos' || hasCollector || isCollector(d)) return 'mac';
			if (d.os === 'windows') return 'windows';
			if (d.os === 'linux') return 'linux';
			return 'desktop';
	}
}

/** "Mac", "computer": what a sentence calls the machine. */
export function machineWord(p: Platform): string {
	if (p === 'mac') return 'Mac';
	if (p === 'ios') return 'phone';
	return 'computer';
}

/**
 * The first half of a device's line in the list, and of its page's lede:
 * "The Mac app and its collector", "The iOS app", "An AI agent, added from
 * Studio MacBook". No closing period, so a caller can go on.
 */
export function kindSentence(d: Device, devices: Device[]): string {
	const collector = collectorOf(d, devices);
	const platform = platformOf(d, !!collector);
	switch (platform) {
		case 'console':
			return 'The command line on your server, and a browser through it';
		case 'agent': {
			const from = d.installed_by ? devices.find((x) => x.id === d.installed_by) : undefined;
			return from ? `An AI agent, added from ${from.label}` : 'An AI agent';
		}
		case 'sensor':
			return 'A sensor';
		case 'ios':
			return 'The iOS app';
		case 'mac':
			if (isCollector(d)) return 'A Mac collector whose app is no longer paired';
			return collector ? 'The Mac app and its collector' : 'The Mac app';
		case 'windows':
			return 'The Windows app';
		case 'linux':
			return 'The Linux app';
		case 'desktop':
			return 'The desktop app';
	}
}

/** What a device is called on screen. The console's row is labeled for the
 *  server's own bookkeeping, so it reads as what it is. */
export function deviceName(d: Pick<Device, 'kind' | 'label'>): string {
	return d.kind === 'cli' ? "Your server's console" : d.label;
}

/** "November 12", or "November 12, 2025" in another year. */
export function pairedOn(iso: string, now = new Date()): string {
	const d = new Date(iso);
	return d.toLocaleDateString('en-US', {
		month: 'long',
		day: 'numeric',
		year: d.getFullYear() !== now.getFullYear() ? 'numeric' : undefined,
	});
}

/**
 * "3 minutes ago", "2 hours ago", "yesterday", "4 days ago", "October 30".
 * Full words, for a sentence that starts "Last reached".
 */
export function reachedAgo(iso: string, now = new Date()): string {
	const then = new Date(iso);
	const mins = Math.floor((now.getTime() - then.getTime()) / 60_000);
	if (mins < 1) return 'just now';
	if (mins < 60) return `${mins} minute${mins === 1 ? '' : 's'} ago`;
	const hours = Math.floor(mins / 60);
	const day = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
	const days = Math.round((day(now) - day(then)) / 86_400_000);
	if (days === 0) return `${hours} hour${hours === 1 ? '' : 's'} ago`;
	if (days === 1) return 'yesterday';
	if (days < 7) return `${days} days ago`;
	return then.toLocaleDateString('en-US', {
		month: 'long',
		day: 'numeric',
		year: then.getFullYear() !== now.getFullYear() ? 'numeric' : undefined,
	});
}

/**
 * The server only knows when a device last spoke to it. Five minutes covers
 * every poll interval a paired app runs at, so a device inside that window is
 * talking to the server as you read this. The device you are holding made
 * this request, so it is connected.
 */
const CONNECTED_MS = 5 * 60 * 1000;

/** "Connected now.", "Last reached yesterday.", "Never reached your server." */
export function reachLine(d: Pick<Device, 'is_current' | 'last_seen_at'>, now = new Date()): string {
	if (d.is_current) return 'Connected now.';
	if (!d.last_seen_at) return 'Never reached your server.';
	if (now.getTime() - new Date(d.last_seen_at).getTime() < CONNECTED_MS) return 'Connected now.';
	return `Last reached ${reachedAgo(d.last_seen_at, now)}.`;
}

export function kindIcon(k: DeviceKind): string {
	switch (k) {
		case 'mobile_app':
			return 'mynaui:mobile';
		case 'desktop_app':
			return 'mynaui:desktop';
		case 'sensor':
			return 'mynaui:chip';
		case 'cli':
			return 'mynaui:terminal';
		case 'agent':
			return 'mynaui:sparkles';
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
	/** The same, for a shell that shows the drag tile on System Settings. */
	dragFix?: string;
	/** Which pane `open` goes to, for its button: "Open Full Disk Access". */
	pane?: string;
	open?: () => Promise<PaneOpened>;
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
const DRAG_FIX = `System Settings opens with ${COLLECTOR_LIST_NAME} ready to drag into the list. macOS asks for Touch ID or your password to add it.`;

export const PERMISSION_COPY: Record<string, PermissionCopy> = {
	full_disk_access: {
		label: 'Full Disk Access',
		costs: "your Mac can't read Messages or Safari history",
		fix: FULL_DISK_ACCESS_FIX,
		dragFix: DRAG_FIX,
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
		dragFix: DRAG_FIX,
		pane: 'Full Disk Access',
		open: openFullDiskAccess
	},
	accessibility: {
		label: 'Accessibility',
		costs: 'your Mac records app events without window titles',
		fix: `Turn on ${COLLECTOR_LIST_NAME} in the list.`,
		dragFix: DRAG_FIX,
		pane: 'Accessibility',
		open: openAccessibilitySettings
	},
	// Call recording. No button yet: these panes need their own bridge
	// commands, which come with the call-recording settings.
	system_audio: {
		label: 'System Audio Recording',
		costs: "your Mac records only your side of calls",
		fix: `Turn on ${COLLECTOR_LIST_NAME} under Screen & System Audio Recording.`,
		pane: 'Screen & System Audio Recording'
	},
	microphone: {
		label: 'Microphone',
		costs: "your Mac records only the other side of calls",
		fix: `Turn on ${COLLECTOR_LIST_NAME} under Microphone.`,
		pane: 'Microphone'
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
	// Safari history comes with Full Disk Access, whose words already name it,
	// so it is said once.
	if (names.has('full_disk_access')) names.delete('safari_library');
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

/** One permission as a collector last reported it, for a plain row. */
export type ReportedPermission = { key: string; label: string; costs: string; granted: boolean | null };

/**
 * Every permission a Mac's collector last reported, in the order a person
 * reads them: the three Setup asks about (Safari only once it adds
 * something), then call recording's two when the collector mentioned them.
 * Pass the collector's row.
 */
export function reportedPermissions(device: Pick<Device, 'permissions'> | null): ReportedPermission[] {
	const rows: ReportedPermission[] = macPermissionsFromDevice(device).map((p) => ({
		key: p.key,
		label: p.label,
		costs: p.costs,
		granted: p.granted,
	}));
	const reported = device?.permissions as Record<string, unknown> | null | undefined;
	const denied = new Set(device?.permissions?.denied ?? []);
	for (const key of ['system_audio', 'microphone'] as const) {
		const v = reported?.[key];
		if (typeof v !== 'boolean' && !denied.has(key)) continue;
		rows.push({ key, label: PERMISSION_COPY[key].label, costs: PERMISSION_COPY[key].costs, granted: denied.has(key) ? false : (v as boolean) });
	}
	return rows;
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
 * the Mac's permissions. `null` when there is none. An agent added from the
 * same Mac carries the same `installed_by`, so only a collector counts.
 */
export function collectorOf<D extends Pick<Device, 'id' | 'installed_by' | 'kind' | 'source_id'>>(
	app: Pick<Device, 'id'> | null,
	devices: D[]
): D | null {
	if (!app) return null;
	return devices.find((d) => d.installed_by === app.id && isCollector(d)) ?? null;
}

/**
 * Where a device row's page lives.
 *
 * `this` rather than the row id for the device making the request, because on a
 * Mac that page reads the local collector over IPC, which knows things the
 * server never receives. A stable word also means a link can point at "this
 * machine" without knowing an id. A Mac's collector folds into its app's row,
 * so the list never links a collector on its own unless its app is gone.
 */
export function deviceHref(device: Pick<Device, 'id' | 'is_current'>): string {
	return device.is_current ? '/virtues/devices/this' : `/virtues/devices/${device.id}`;
}

/**
 * The Sources page that is this device's page, for a device that sends data
 * and is not the one you're using: an iPhone (`/sources/ios`), or a Mac app
 * or its collector (`/sources/mac`). Device pages live in Sources (owner,
 * 2026-11-20); Settings keeps This device and the devices that send nothing.
 */
export function sourcePageOf<D extends Pick<Device, 'id' | 'kind' | 'source_id' | 'installed_by' | 'is_current'>>(
	d: D,
	devices: D[]
): string | null {
	if (d.is_current) return null;
	if (d.kind === 'mobile_app' && d.source_id === 'ios') return '/sources/ios';
	if (isCollector(d) || collectorOf(d, devices)) return '/sources/mac';
	return null;
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
 * Confirm, revoke, and report. The 409 below is the part worth never getting
 * wrong.
 *
 * 409 is the box refusing to strand you: revoking your last active device
 * leaves nothing that can reach it, and the only way back in is physical. The
 * message says how to get past it rather than pretending the click merely
 * failed.
 *
 * Returns true when a device other than the current one was revoked, so the
 * caller can refresh or navigate. Revoking the current device never returns —
 * it hands off to pairing.
 */
export async function revokeDeviceFlow(
	device: Device,
	opts: {
		/** "Disconnect" for an agent. */
		verb?: 'Unpair' | 'Disconnect';
		/** Rows that go with it: a Mac's collector. */
		with?: Device[];
	} = {}
): Promise<boolean> {
	const verb = opts.verb ?? 'Unpair';
	const companions = opts.with ?? [];
	const ok = await confirmAction({
		title: device.is_current ? `${verb} this device?` : `${verb} ${device.label}?`,
		body: device.is_current
			? `${device.label} is the device you're using. You'll be signed out at once.`
			: 'It stops reaching your server at once.',
		confirmLabel: verb,
		danger: true
	});
	if (!ok) return false;

	const lastDevice = () =>
		toast.error("You can't unpair your only paired device", {
			description: 'Pair another device first, then unpair this one.'
		});
	const failed = (what: string, detail: string) =>
		toast.error(`Your server couldn't ${verb.toLowerCase()} ${what}`, { description: detail });

	/** DELETE one row; true when it went. */
	const revoke = async (d: Device, what: string): Promise<boolean> => {
		const resp = await fetch(`/api/devices/${d.id}`, { method: 'DELETE' });
		if (resp.status === 409) {
			lastDevice();
			return false;
		}
		if (!resp.ok) {
			const data = await resp.json().catch(() => ({}));
			failed(what, data.error ?? `HTTP ${resp.status}`);
			return false;
		}
		return true;
	};

	try {
		if (device.is_current) {
			// The device you're on goes last: once it is revoked, this window can
			// no longer ask for anything. So check first that it isn't the last
			// device, which the server refuses, rather than revoke its
			// collector and then be refused.
			if (companions.length) {
				const listed = await fetch('/api/devices').then((r) => r.json() as Promise<DevicesResponse>);
				const gone = new Set([device.id, ...companions.map((c) => c.id)]);
				if (!listed.devices.some((d) => !gone.has(d.id))) {
					lastDevice();
					return false;
				}
			}
			for (const c of companions) if (!(await revoke(c, c.label))) return false;
			if (!(await revoke(device, 'it'))) return false;
			toast.success(verb === 'Disconnect' ? `${device.label} is disconnected` : `${device.label} is unpaired`);
			returnToPairing();
			return false;
		}
		if (!(await revoke(device, 'it'))) return false;
		for (const c of companions) if (!(await revoke(c, c.label))) return false;
		toast.success(verb === 'Disconnect' ? `${device.label} is disconnected` : `${device.label} is unpaired`);
		return true;
	} catch (e) {
		failed('it', e instanceof Error ? e.message : 'Network error');
		return false;
	}
}
