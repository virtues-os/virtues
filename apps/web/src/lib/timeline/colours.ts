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
	/** The white ring around the pin, a count chip and the live bubble. */
	ring: "#ffffff",
	/** A bubble's ring (index.html:105). */
	mark: "#8a94a6",
	/** A count chip's ground (index.html:110). */
	knot: "#5b6472",
	/** A bubble's leader line (index.html:108). */
	leader: "#9aa1ad",
	/** Speaker tints in a transcript: teal, plum, olive, terracotta (index.html:18). */
	spk0: "#2f7d70",
	spk1: "#9c5f86",
	spk2: "#7d7a3f",
	spk3: "#a56a4a",
	/** The selection blue: an active step bar leans toward it (index.html:1363). */
	sel: "#0A84FF",
} as const;

/** The prototype's shadows (dayback/index.html:104-129, 599-601): the tiles'
 *  one soft shadow, and the marks' own on the map. */
const SHADOWS = {
	tile: "0 10px 34px -10px rgba(0, 0, 0, 0.2), 0 2px 8px -3px rgba(0, 0, 0, 0.1)",
	pin: "0 0 0 1px rgba(0, 0, 0, 0.06), 0 1px 6px rgba(0, 0, 0, 0.4)",
	mark: "0 1px 3px rgba(0, 0, 0, 0.28)",
	markCur: "0 0 0 1px rgba(0, 0, 0, 0.06), 0 2px 7px rgba(35, 131, 243, 0.5)",
	label: "0 5px 18px -6px rgba(0, 0, 0, 0.3)",
	labelCur: "0 6px 22px -6px rgba(0, 0, 0, 0.34)",
	knot: "0 0 0 1px rgba(0, 0, 0, 0.06), 0 2px 8px rgba(0, 0, 0, 0.32)",
	card: "0 10px 30px -8px rgba(0, 0, 0, 0.38), 0 0 0 1px rgba(0, 0, 0, 0.05)",
	cardTail: "3px 3px 6px -3px rgba(0, 0, 0, 0.25)",
	/** The scrubber's hover peek (index.html:908). */
	peek: "0 10px 30px rgba(0, 0, 0, 0.16)",
	/** The raised tab of a segmented toggle (index.html:968). */
	toggle: "0 1px 2.5px rgba(0, 0, 0, 0.12)",
};

const kebab = (k: string) => k.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`);

/** The colours and shadows as CSS variables for the Timeline's root element. */
export const colourVars = [
	...Object.entries(COLOURS).map(([k, v]) => `--c-${kebab(k)}: ${v}`),
	...Object.entries(SHADOWS).map(([k, v]) => `--${kebab(k)}-shadow: ${v}`),
].join("; ");
