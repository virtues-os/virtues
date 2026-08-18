// Normalized, cached summaries for reference cards (Preview + Embed).
//
// The name/coords/relationship a card shows already live in the wiki tables —
// no lookup, just a GET. We normalize the per-type responses into one shape so
// RefCard renders every target the same way, and cache by (type,id) so repeat
// hovers / re-rendered embeds don't refetch.

const API = "/api";

export interface RefFact {
	label: string;
	value: string;
}

export interface RefSummary {
	type: string;
	name: string;
	subtitle?: string;
	facts: RefFact[];
	/** Person picture / place cover — shown as a round/rect avatar. */
	avatarUrl?: string;
	/** Place coordinates, if known — drives the (later) map, schematic for now. */
	coords?: { lat: number; lng: number };
	address?: string;
}

const cache = new Map<string, Promise<RefSummary | null>>();

function relativeTime(iso?: string | null): string {
	if (!iso) return "";
	const then = new Date(iso).getTime();
	if (Number.isNaN(then)) return "";
	const secs = Math.max(0, (Date.now() - then) / 1000);
	const day = 86400;
	if (secs < 3600) return "just now";
	if (secs < day) return `${Math.floor(secs / 3600)}h ago`;
	if (secs < day * 7) return `${Math.floor(secs / day)}d ago`;
	if (secs < day * 30) return `${Math.floor(secs / (day * 7))}w ago`;
	if (secs < day * 365) return `${Math.floor(secs / (day * 30))}mo ago`;
	return `${Math.floor(secs / (day * 365))}y ago`;
}

async function fetchByType(type: string, id: string): Promise<RefSummary | null> {
	switch (type) {
		case "person": {
			const r = await fetch(`${API}/wiki/person/${encodeURIComponent(id)}`);
			if (!r.ok) return null;
			const p = await r.json();
			const facts: RefFact[] = [];
			if (p.relationship_category) facts.push({ label: "Relationship", value: p.relationship_category });
			const seen = relativeTime(p.last_seen);
			if (seen) facts.push({ label: "Last seen", value: seen });
			if (p.ref_count) facts.push({ label: "Interactions", value: String(p.ref_count) });
			return {
				type,
				name: p.name || p.nickname || id,
				subtitle: p.relationship_category || undefined,
				facts,
				avatarUrl: p.picture || undefined,
			};
		}
		case "place": {
			const r = await fetch(`${API}/wiki/place/${encodeURIComponent(id)}`);
			if (!r.ok) return null;
			const p = await r.json();
			const facts: RefFact[] = [];
			if (p.category) facts.push({ label: "Type", value: p.category });
			if (p.ref_count) facts.push({ label: "Visits", value: String(p.ref_count) });
			const seen = relativeTime(p.last_seen);
			if (seen) facts.push({ label: "Last visit", value: seen });
			const coords =
				typeof p.latitude === "number" && typeof p.longitude === "number"
					? { lat: p.latitude, lng: p.longitude }
					: undefined;
			return {
				type,
				name: p.name || id,
				subtitle: p.category || undefined,
				facts,
				address: p.address || undefined,
				coords,
				avatarUrl: p.cover_image || undefined,
			};
		}
		case "org": {
			const r = await fetch(`${API}/wiki/organization/${encodeURIComponent(id)}`);
			if (!r.ok) return null;
			const o = await r.json();
			const facts: RefFact[] = [];
			if (o.role_title) facts.push({ label: "Role", value: o.role_title });
			if (o.relationship_type) facts.push({ label: "Relationship", value: o.relationship_type });
			const seen = relativeTime(o.last_seen);
			if (seen) facts.push({ label: "Last seen", value: seen });
			return {
				type,
				name: o.name || id,
				subtitle: o.role_title || o.organization_type || undefined,
				facts,
			};
		}
		default:
			return null;
	}
}

/** Cached, normalized summary for a ref target. Null if unknown/unavailable. */
export function getRefSummary(type: string, id: string): Promise<RefSummary | null> {
	const key = `${type}:${id}`;
	let hit = cache.get(key);
	if (!hit) {
		hit = fetchByType(type, id).catch(() => null);
		cache.set(key, hit);
	}
	return hit;
}
