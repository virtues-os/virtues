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

/**
 * A pin may carry an explicit `--cat-*` token key (the existing colour field).
 * Resolved to hex here so luminance stays computable — `textOnCloth` has to
 * measure the colour it is inverting against, which a `var()` can't be.
 */
const CAT_HEX: Record<string, string> = {
	purple: '#a855f7',
	indigo: '#6366f1',
	violet: '#8b5cf6',
	pink: '#ec4899',
	rose: '#f43f5e',
	orange: '#f97316',
	yellow: '#eab308',
	cyan: '#06b6d4',
	emerald: '#10b981',
};

/**
 * The cloth for a pinned thing: its chosen colour if it has one, otherwise
 * bookcloth derived from its url.
 *
 * Keyed on the URL, not on a species. Anything with a route can sit on the
 * Desk — a notebook, an applet, a PDF in Drive, a single day, a person, an
 * external link — so asking "what kind of thing is this?" would be both
 * fragile and beside the point. The url IS the identity.
 */
export function clothFor(pin: { url: string; color?: string | null }): string {
	if (pin.color && CAT_HEX[pin.color]) return CAT_HEX[pin.color];
	return pinColor(pin.url);
}
