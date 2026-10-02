/**
 * colours.ts - the Timeline's colours: the Dayback prototype's overlay, one
 * hue per stream (dayback/index.html:17, the "Apple" preset's side of it,
 * dayback/src/main.js:1662). The Timeline follows the prototype's look
 * rather than the pane's one-accent grammar, by the owner's call
 * (2026-09-30); the map, the scrubber and the inspector all read these, so a
 * stream is the same colour everywhere.
 */

export const COLOURS = {
	/** At a place; also the path and the pin, which are location (main.js:1755). */
	place: "#4C86D6",
	/** Moving: a drive's section, a walk's row, and the lit stretch of path. */
	move: "#E6A04E",
	/** A drive picked from the inspector: the move colour at full strength (Apple's
	 *  system orange), so the pick stands out from a drive merely passed. */
	movePicked: "#FF9500",
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

/** The dark side, for Virtues' dark themes. The prototype keeps its stream
 *  colours on both sides (dayback/src/main.js:1664) and lightens the speaker
 *  tints for dark tiles (dayback/index.html:603). Its only dark tile ground is
 *  its frosted mode's, Apple's dark system grey (main.js:1759); its solid tile
 *  stays white in the dark, under light text, so the Timeline takes the grey. */
const DARK: Partial<Record<keyof typeof COLOURS, string>> = {
	tile: "#1c1c1e",
	spk0: "#6bc0b0",
	spk1: "#d79fc0",
	spk2: "#c7c37a",
	spk3: "#dba07e",
};

const kebab = (k: string) => k.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`);

/** The colours and shadows as CSS variables for the Timeline's root element,
 *  on the light or the dark side. */
export function colourVars(dark: boolean): string {
	return [
		...Object.entries(COLOURS).map(([k, v]) => `--c-${kebab(k)}: ${(dark && DARK[k as keyof typeof COLOURS]) || v}`),
		...Object.entries(SHADOWS).map(([k, v]) => `--${kebab(k)}-shadow: ${v}`),
	].join("; ");
}
