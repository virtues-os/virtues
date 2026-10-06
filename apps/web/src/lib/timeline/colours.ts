/**
 * colours.ts - the Timeline's paint.
 *
 * The pane's grammar (agents/build/design-grammar.md §5) gives a page one
 * accent. The Timeline keeps that accent for *now* and *pressable* (the
 * playhead, the lit stretch of path, the pin, a pick, a link) and departs from
 * it for the streams: each stream has its own hue, the same in the scrubber,
 * the inspector and on the map, so a blue stay in a lane is the blue dot in
 * the list and the blue line on the map. The hues are the Dayback prototype's
 * (dayback/index.html:17, main.js:1662), kept on both sides of the theme as
 * the prototype keeps them. Marks that aren't a stream stay ink at a weight.
 */

/** Ink at `pct` percent on the page: the weights the Timeline draws in. */
const ink = (pct: number) => `color-mix(in srgb, var(--color-foreground) ${pct}%, transparent)`;

/** The Timeline's paint as CSS custom properties, set once on its root. */
const PAINT: Record<string, string> = {
	/** At a place: a stay's ribbon and row, and the day's path and pin, which
	 *  are location too. */
	"--c-place": "#4C86D6",
	/** Moving: a drive's ribbon and row. */
	"--c-move": "#E6A04E",
	/** The Voice lane's waveform and a conversation's row. */
	"--c-voice": "#9174BE",
	/** In Bed: the night band, and its ink. */
	"--c-rest": "#7683b5",
	"--c-rest-ink": "#565fa2",
	/** Steps. */
	"--c-body": "#57B06E",
	"--c-calendar": "#D07B6E",
	/** A signal gap. Grey means "nothing was recorded", and no stream uses it. */
	"--c-gap": "#9aa1ad",
	/** What floats over the map: the page's own surface. */
	"--c-tile": "var(--color-surface)",
	/** The ring around the pin and a count chip. */
	"--c-ring": "var(--color-background)",
	/** A bubble's ring, its count chip and its leader line. */
	"--c-mark": ink(46),
	"--c-knot": ink(70),
	"--c-leader": ink(36),
	/** Now: the playhead, the lit stretch, the pin, a pick. */
	"--c-sel": "var(--color-primary)",
};

/** The paint as a `style` string for the Timeline's root element. */
export function colourVars(): string {
	return Object.entries(PAINT)
		.map(([k, v]) => `${k}: ${v}`)
		.join("; ");
}

/** The map's own paint. MapLibre takes colours, not CSS variables, so these
 *  are read off the theme when the map is built and again when it changes. */
export interface MapInk {
	/** The day's path: location, so a stay's hue. */
	track: string;
	/** A drive's stretch of the path: moving's hue. */
	move: string;
	/** A label drawn on the map, and the halo that keeps it legible. */
	label: string;
	halo: string;
	/** The stretch of path at the playhead, and a picked drive. */
	now: string;
}

export function mapInk(el: Element = document.documentElement): MapInk {
	const css = getComputedStyle(el);
	const read = (name: string, fallback: string) => css.getPropertyValue(name).trim() || fallback;
	// design-ok: last-resort fallbacks for a theme that failed to load, never the paint itself
	return {
		track: PAINT["--c-place"],
		move: PAINT["--c-move"],
		now: read("--color-primary", "#335"),
		label: read("--color-foreground", "#333"),
		halo: read("--color-background", "#fff"),
	};
}
