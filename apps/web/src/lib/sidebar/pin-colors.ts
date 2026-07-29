/**
 * Bookcloth colors — the identity palette for things on the Desk.
 *
 * A pinned thing (today: a notebook) carries one color, and the color follows
 * its NAME wherever the name appears: the dot on its sidebar spine, the dot in
 * the path mast, the filled tab pill in the window bar. It never colors a
 * container (a pane, a border, a background) — a pane can hold tabs from many
 * worlds, so painting it would claim the window for one thing when only one
 * tab is that thing.
 *
 * The palette is muted bookcloth, not category candy: these are the colors of
 * cloth bindings, chosen to sit on paper in the light themes and to stay
 * legible as fills in the dark ones. Deliberately small — identity colors only
 * work while they are few.
 *
 * Assignment is a stable hash of the id: no schema, no migration, and the same
 * notebook gets the same cloth on every device. When user-chosen colors arrive
 * they replace the hash, not the palette.
 */

export const BOOKCLOTH = [
	'#14283D', // constellation navy
	'#932725', // cinder
	'#1872A0', // cerulean
	'#B07514', // ochre
	'#2E6B43', // moss
	'#5B4A8A', // damson
] as const;

export function pinColor(id: string): string {
	let h = 0;
	for (let i = 0; i < id.length; i++) {
		h = (h * 31 + id.charCodeAt(i)) | 0;
	}
	return BOOKCLOTH[Math.abs(h) % BOOKCLOTH.length];
}

/** Text color that survives on a given cloth — ink on light, paper on dark. */
export function textOnCloth(hex: string): string {
	const r = parseInt(hex.slice(1, 3), 16);
	const g = parseInt(hex.slice(3, 5), 16);
	const b = parseInt(hex.slice(5, 7), 16);
	const luma = (0.299 * r + 0.587 * g + 0.114 * b) / 255;
	return luma > 0.55 ? '#17171A' : '#FFFFFF';
}

const NOTEBOOK_ROUTE = /^\/notebooks?\/([^/?#]+)/;

/**
 * The cloth for a tab route, or null when the route isn't a pinned thing.
 * Today only notebooks are pinnable; the desk's species list grows here.
 */
export function routeCloth(route: string | undefined | null): string | null {
	if (!route) return null;
	const m = route.match(NOTEBOOK_ROUTE);
	if (!m || m[1] === '') return null;
	return pinColor(m[1]);
}
