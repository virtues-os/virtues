/**
 * Automatic updates - the nightly pass (`virtues auto-update`, api/updates.rs).
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

export async function getAutoUpdate(): Promise<AutoUpdateStatus> {
	const res = await fetch('/api/system/update/auto');
	if (!res.ok) throw new Error(`Failed to get automatic update status: ${res.statusText}`);
	return res.json();
}

export async function setAutoUpdate(enabled: boolean): Promise<AutoUpdateStatus> {
	const res = await fetch('/api/system/update/auto', {
		method: 'PUT',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ enabled })
	});
	if (!res.ok) {
		// The server's own reason, e.g. a state root it can't write.
		const body = await res.json().catch(() => null);
		throw new Error(body?.error ?? res.statusText);
	}
	return res.json();
}
