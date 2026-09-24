/**
 * Automatic updates - the nightly pass (`virtues auto-update`, api/updates.rs).
 *
 * Rides the existing update endpoints: `GET /api/system/update` carries
 * `auto_update`, and `PUT /api/system/update/channel` takes `auto_update`
 * beside `channel`.
 */

export interface AutoUpdateInstall {
	at: string;
	from: string;
	/** Release slot, `<tag>-<sha7>`. */
	to: string;
	ok: boolean;
	error: string | null;
}

export interface AutoUpdateStatus {
	enabled: boolean;
	/** Local hour the nightly pass starts. */
	hour: number;
	checked_at: string | null;
	install: AutoUpdateInstall | null;
	/** Why the last pass stopped short, when it did. */
	problem: string | null;
}

/** Absent on a server that predates automatic updates. */
export function autoUpdateOf(status: object | null): AutoUpdateStatus | null {
	return (status as { auto_update?: AutoUpdateStatus } | null)?.auto_update ?? null;
}

export async function setAutoUpdate(enabled: boolean): Promise<AutoUpdateStatus> {
	const res = await fetch('/api/system/update/channel', {
		method: 'PUT',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ auto_update: enabled })
	});
	const body = await res.json().catch(() => null);
	if (!res.ok || !body?.auto_update) {
		// The server's own reason, e.g. a state root it can't write.
		throw new Error(body?.message ?? body?.error ?? res.statusText);
	}
	return body.auto_update;
}
