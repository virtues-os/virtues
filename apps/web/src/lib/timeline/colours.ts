/**
 * colours.ts - the Timeline's colours: the Dayback prototype's overlay, one
 * hue per stream (dayback/index.html:17, the "Apple" preset's side of it,
 * dayback/src/main.js:1662). The Timeline follows the prototype's look
 * rather than the pane's one-accent grammar, by the owner's call
 * (2026-09-30); the map, the scrubber and the rail all read these, so a
 * stream is the same colour everywhere.
 */

export const COLOURS = {
	/** At a place; also the path and the pin, which are location (main.js:1755). */
	place: "#4C86D6",
	/** Driving, flying: a drive's lit stretch and its section. */
	move: "#E6A04E",
	/** A conversation's lit stretch on the map. */
	talk: "#4E6A8A",
	/** The Voice lane and a conversation's row. */
	voice: "#9174BE",
	/** In Bed: the night band, and its ink. */
	rest: "#7683b5",
	restInk: "#565fa2",
	/** Body: steps. */
	body: "#57B06E",
	calendar: "#D07B6E",
	finance: "#4E9AA6",
	/** A signal gap. Grey means "we don't know", and nothing else uses it. */
	gap: "#9aa1ad",
	/** The ground of every tile over the map. */
	tile: "#fefefe",
} as const;

/** The tiles' one soft shadow (dayback/index.html:599-601). */
const TILE_SHADOW = "0 10px 34px -10px rgba(0, 0, 0, 0.2), 0 2px 8px -3px rgba(0, 0, 0, 0.1)";

/** The colours as CSS variables for the Timeline's root element. */
export const colourVars = [
	...Object.entries(COLOURS).map(([k, v]) => `--c-${k.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`)}: ${v}`),
	`--tile-shadow: ${TILE_SHADOW}`,
].join("; ");
