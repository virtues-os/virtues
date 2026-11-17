/**
 * The hand-drawn rule that runs on from a group's label (design.md, the
 * sidebar's group heads). Three strokes, each a little off level in its own
 * way, picked by the group's id so a group always draws the same one and
 * neighbours rarely match. Drawn in a `0 0 100 4` viewBox with
 * `preserveAspectRatio="none"` and a non-scaling stroke.
 */

// TODO(2026-10-28): migrate HomePanel's own copy of these strokes onto this module.
const RULE_STROKES = [
	'M0 2.3C14 1.6 27 2.7 41 2.2S71 1.5 85 2.1S96 2.4 100 1.9',
	'M0 1.8C11 2.5 26 1.7 44 2.3S69 2.6 83 1.9S95 1.6 100 2.2',
	'M0 2.1C17 1.5 31 2.4 50 2S78 1.6 91 2.4S98 2.1 100 2',
];

export function ruleStroke(id: string): string {
	let sum = 0;
	for (const ch of id) sum += ch.charCodeAt(0);
	return RULE_STROKES[sum % RULE_STROKES.length];
}
