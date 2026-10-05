/**
 * colours.ts - the Timeline's paint, all of it from the theme.
 *
 * The pane's grammar (agents/build/design-grammar.md §5): one accent, meaning
 * *now* or *pressable*, and nothing else is coloured. A stay, a drive, a
 * night, a conversation and a step are told apart by form and weight - a
 * filled ribbon, a hollow one, a band, a waveform, bars - never by a hue, which
 * would assert a kinship the record does not have. So every lane is ink at a
 * weight, and the accent is the playhead, the stretch of path it is on, the
 * pin, and what was picked.
 */

/** Ink at `pct` percent on the page: the weights the Timeline draws in. */
const ink = (pct: number) => `color-mix(in srgb, var(--color-foreground) ${pct}%, transparent)`;

/** The Timeline's paint as CSS custom properties, set once on its root. */
const PAINT: Record<string, string> = {
	/** A stay: the heaviest thing on a lane, as being somewhere is the day's frame. */
	"--c-place": ink(78),
	/** Moving: a drive's ribbon and row, at a lighter weight than a stay. */
	"--c-move": ink(46),
	/** The Voice lane's waveform and a conversation's row. */
	"--c-voice": ink(62),
	/** In Bed: the night band, and its ink. */
	"--c-rest": ink(22),
	"--c-rest-ink": ink(58),
	/** Steps. */
	"--c-body": ink(52),
	"--c-calendar": ink(58),
	/** A signal gap: the faintest weight, for "nothing was recorded". */
	"--c-gap": ink(26),
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
	/** The day's path. */
	track: string;
	/** The stretch of path at the playhead, and a picked drive. */
	now: string;
}

export function mapInk(el: Element = document.documentElement): MapInk {
	const css = getComputedStyle(el);
	const read = (name: string, fallback: string) => css.getPropertyValue(name).trim() || fallback;
	// design-ok: last-resort fallbacks for a theme that failed to load, never the paint itself
	return { track: read("--color-foreground", "#333"), now: read("--color-primary", "#335") };
}
