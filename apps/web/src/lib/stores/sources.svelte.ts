/**
 * The Sources room's data, loaded once and shared by its three sections.
 *
 * The point of this module is the word *connection*. A source is connected in
 * one of two ways and the backend models them separately: OAuth and API-key
 * sources mint a `credentials` row, while device sources (iOS, Mac) pair into
 * `app_device` and never touch that table. The old Sources page listed
 * credentials only, so a paired iPhone — one of the two headline tiles on it —
 * could never appear there, and lived in Settings → Devices instead.
 *
 * `Connection` is the join those two shapes were missing. Everything above this
 * module works in connections and stops caring which table it came from.
 */
import {
	listCredentials,
	listSourceCatalog,
	listDevices,
	type Credential,
	type SourceCatalogItem
} from '$lib/api/client';
import { PERMISSION_COPY } from '$lib/devices/shared';

/** A device as `GET /api/devices` returns it. */
export interface DeviceRow {
	id: string;
	kind: string;
	/** Catalog source it ingests as; null for view-only devices. */
	source_id: string | null;
	label: string;
	paired_at: string;
	last_seen_at: string | null;
	permissions: Record<string, unknown> | null;
	is_current: boolean;
}

/** One live connection to a source, whichever table it lives in. */
export interface Connection {
	kind: 'credential' | 'device';
	id: string;
	sourceId: string;
	name: string;
	/** Raw status. Devices are only ever listed while active. */
	status: string;
	/** Display status — the init-sync lifecycle for healthy credentials. */
	statusLabel: string;
	statusReason: string | null;
	lastSeenAt: string | null;
	/** Fan-out size. Devices don't report one (the list endpoint has no count). */
	appletCount: number | null;
	/** True when the provider has locked us out and only the user can fix it. */
	broken: boolean;
	/**
	 * Devices only: capabilities the collector's most recent report lists as
	 * denied (`full_disk_access`, …); always empty for credentials. This is
	 * the health.json signal, and it is what lets a run's bare "Permission
	 * denied" be pinned to a named permission (see sources/run-errors.ts).
	 *
	 * Deliberately NOT blanked when the report is stale, unlike `broken` and
	 * `statusReason`: those assert the device's state NOW, which a frozen
	 * snapshot cannot support — but explaining why a past run failed is a
	 * claim about roughly when the report was written, and run-error
	 * classification additionally requires the error itself to look like a
	 * permission failure before it trusts this list.
	 */
	denied: string[];
	/** Where clicking it goes, or null when the row has no detail surface. */
	route: string | null;
	/** Devices only: this is the device you're reading this on. */
	isCurrent: boolean;
}

export function isBrokenStatus(status: string): boolean {
	return status === 'reauth_required' || status === 'error';
}

function credentialStatusLabel(c: Credential): string {
	if (c.status === 'active') return c.sync_state ?? 'active';
	if (c.status === 'reauth_required') return 'sign in again';
	return c.status;
}

function fromCredential(c: Credential): Connection {
	return {
		kind: 'credential',
		id: c.id,
		sourceId: c.provider,
		name: c.name,
		status: c.status,
		statusLabel: credentialStatusLabel(c),
		statusReason: c.status_reason ?? null,
		lastSeenAt: c.last_seen_at,
		appletCount: c.applet_count,
		broken: isBrokenStatus(c.status),
		denied: [],
		route: `/sources/${c.id}`,
		isCurrent: false
	};
}

/** Human "3 minutes ago" for a permission record's own timestamp. */
function agoLabel(iso: string): string {
	const then = Date.parse(iso);
	if (Number.isNaN(then)) return 'an unknown time ago';
	const mins = Math.max(0, Math.round((Date.now() - then) / 60000));
	if (mins < 60) return `${mins} minute${mins === 1 ? '' : 's'} ago`;
	const hours = Math.round(mins / 60);
	if (hours < 48) return `${hours} hour${hours === 1 ? '' : 's'} ago`;
	return `${Math.round(hours / 24)} days ago`;
}

function fromDevice(d: DeviceRow): Connection {
	// A denied macOS permission is the device equivalent of a dead credential:
	// the pairing is fine, the data isn't coming. Surfaced as broken so it lands
	// in the same attention list rather than looking merely quiet.
	// Union of the collector's own `denied[]` and boolean fields set to false,
	// same as deniedPermissions() in devices/shared.ts and for the same reason:
	// early builds write one without the other.
	const deniedSet = new Set((d.permissions?.denied as string[] | undefined) ?? []);
	for (const key of Object.keys(PERMISSION_COPY)) {
		if (d.permissions?.[key] === false) deniedSet.add(key);
	}
	const denied = [...deniedSet];

	// ...but only if the report is CURRENT. The device writes `stale` itself
	// (the collector's record is older than it promises to refresh), and this
	// read used to ignore it and present a frozen snapshot as live fact. On
	// 2026-08-05 that showed "Access denied: accessibility" — from a record
	// written six days earlier, naming the one permission that WAS granted,
	// while the permission actually denied went unmentioned the whole time.
	//
	// A stale record supports no claim in either direction. Say when we last
	// heard instead of inventing a state, and don't file it as broken: we have
	// not observed a fault, only a silence.
	// Two different staleness questions, and only one of them was being asked.
	//
	// `permissions.stale` is the COLLECTOR's claim about its own record. It says
	// nothing about whether this device row is still the live one — and every
	// reinstall mints a NEW `app_device` row, leaving the old one frozen with
	// whatever it last reported, `stale: false` included. This box carries three
	// rows for one laptop; without a liveness bound, one laptop with one problem
	// raises three permission alarms, two of them describing builds that no
	// longer exist.
	//
	// So bound on `last_seen_at` too: "broken" must mean currently collecting and
	// currently blocked. The window matches `degraded_collectors` in
	// box_status.rs — the collector reports every 5 minutes, so 30 covers six
	// missed cycles. A device that is merely switched off reads as unreported
	// rather than as a standing complaint.
	const LIVE_WINDOW_MS = 30 * 60 * 1000;
	const lastSeenMs = d.last_seen_at ? Date.parse(d.last_seen_at) : NaN;
	const reportingNow = !isNaN(lastSeenMs) && Date.now() - lastSeenMs < LIVE_WINDOW_MS;
	const stale = d.permissions?.stale === true || !reportingNow;
	const checkedAt = d.permissions?.checked_at as string | undefined;
	if (stale) {
		return {
			kind: 'device',
			id: d.id,
			sourceId: d.source_id ?? '',
			name: d.label,
			status: 'unreported',
			statusLabel: 'not reported',
			statusReason: checkedAt
				? `Last reported ${agoLabel(checkedAt)}; what it can read now is unknown.`
				: 'This device has never reported what it can read.',
			lastSeenAt: d.last_seen_at,
			appletCount: null,
			broken: false,
			denied,
			route: null,
			isCurrent: d.is_current
		};
	}

	// The collector reports raw capability names; the reason line speaks the
	// System Settings names, because that is where the reader is being sent.
	const deniedLabels = denied.map((name) => PERMISSION_COPY[name]?.label ?? name);
	return {
		kind: 'device',
		id: d.id,
		sourceId: d.source_id ?? '',
		name: d.label,
		status: denied.length > 0 ? 'permission_denied' : 'active',
		statusLabel: denied.length > 0 ? 'permission needed' : 'paired',
		statusReason:
			denied.length > 0
				? `${deniedLabels.join(' and ')} ${denied.length === 1 ? 'is' : 'are'} off — grant it in System Settings → Privacy & Security on that machine.`
				: null,
		lastSeenAt: d.last_seen_at,
		appletCount: null,
		broken: denied.length > 0,
		denied,
		// Devices have no per-connection page in this room yet; the current one
		// has "This Mac" / "This device", the rest live under Settings → Devices.
		route: null,
		isCurrent: d.is_current
	};
}

class SourcesStore {
	catalog = $state<SourceCatalogItem[]>([]);
	connections = $state<Connection[]>([]);
	loading = $state(true);
	error = $state<string | null>(null);

	/** Catalog tile by source id. */
	get catalogById(): Map<string, SourceCatalogItem> {
		return new Map(this.catalog.map((s) => [s.id, s]));
	}

	sourceLabel(sourceId: string): string {
		return this.catalogById.get(sourceId)?.name ?? sourceId;
	}

	/** Connections grouped under the source they belong to. */
	get bySource(): Map<string, Connection[]> {
		const m = new Map<string, Connection[]>();
		for (const c of this.connections) {
			if (!c.sourceId) continue;
			const list = m.get(c.sourceId);
			if (list) list.push(c);
			else m.set(c.sourceId, [c]);
		}
		return m;
	}

	/** Everything the user has to act on. */
	get broken(): Connection[] {
		return this.connections.filter((c) => c.broken);
	}

	async load(): Promise<void> {
		this.loading = true;
		this.error = null;
		try {
			const [creds, catalog, devices] = await Promise.all([
				listCredentials(),
				listSourceCatalog(),
				listDevices<{ devices: DeviceRow[] }>()
			]);
			this.catalog = catalog;
			this.connections = [
				// `pending` credentials are a transient pre-pairing state the server
				// hard-deletes on cancel; they should never reach a list.
				...creds.filter((c) => c.status !== 'pending' && c.status !== 'revoked').map(fromCredential),
				// A device that only views — it collects nothing — belongs in
				// Settings → Devices, not under a source here. `__device__` is the
				// server's sentinel for exactly that (pair.rs `resolve_source_id`),
				// and it is truthy, so filtering on presence alone let the Tauri
				// desktop shell surface here as an orphaned connection claiming its
				// source was uninstalled.
				...(devices.devices ?? [])
					.filter((d) => d.source_id && d.source_id !== '__device__')
					.map(fromDevice)
			];
		} catch (e) {
			this.error = e instanceof Error ? e.message : String(e);
		} finally {
			this.loading = false;
		}
	}
}

export const sourcesStore = new SourcesStore();
