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
		route: `/sources/${c.id}`,
		isCurrent: false
	};
}

function fromDevice(d: DeviceRow): Connection {
	// A denied macOS permission is the device equivalent of a dead credential:
	// the pairing is fine, the data isn't coming. Surfaced as broken so it lands
	// in the same attention list rather than looking merely quiet.
	const denied = (d.permissions?.denied as string[] | undefined) ?? [];
	return {
		kind: 'device',
		id: d.id,
		sourceId: d.source_id ?? '',
		name: d.label,
		status: denied.length > 0 ? 'permission_denied' : 'active',
		statusLabel: denied.length > 0 ? 'permission needed' : 'paired',
		statusReason:
			denied.length > 0 ? `Access denied: ${denied.join(', ')}. Grant it on the device.` : null,
		lastSeenAt: d.last_seen_at,
		appletCount: null,
		broken: denied.length > 0,
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
