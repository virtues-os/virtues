<!--
	Trivet.svelte

	THE ROOM AT REST — the figure the mark's three points imply, with none of
	them drawn.

	The interview and Getting Started show no thinking block, and while a turn
	works they breathe `ThinkingMark` at depth 3 (see `Composing`). Between
	turns the margin has been empty since the vendored character came out, and
	a mark belongs there: someone is in the room whether or not they are
	speaking.

	It cannot be dots. `ThinkingMark` made the dot COUNT a claim — one a point,
	three a triangle, four a tetrahedron, five a five-cell — so a second set of
	three dots standing in the same margin at rest would make that number mean
	both "reasoning here" and "idle", and a signal that means two things means
	neither.

	So: the trivet. Working shows the VERTICES; resting shows the FORM they add
	up to. Same figure, same geometry to the unit — apex (12,5), base (4.5,18)
	and (19.5,18), the app icon's equilateral on the 24-box, on the same padded
	viewBox `ThinkingMark` uses so the two occupy one square and the swap moves
	nothing. Opposite register: points assert, an outline just stands there.

	WHAT IT IS NOT, each struck in the grammar before this was written:
	- Not a per-turn glyph. One in the room, under the last turn, never a run
	  of them — "per-step marks read as stray punctuation" (§3).
	- Not a rule. No hairline is literally drawn in this app; "rule" names the
	  turn, not an `<hr>` (§2).
	- Not animated. Only the deck's now-marker pulses, and it stays that way
	  (§ motion). This is furniture; it holds still.
	- Not announced. The working state owns the live region, because working
	  is the thing worth telling someone who cannot see the screen. "A room
	  exists" is not news.
-->

<script lang="ts">
	interface Props {
		/** Rendered size in px. 24 matches what `Composing` gives the mark. */
		size?: number;
	}
	let { size = 24 }: Props = $props();
</script>

<svg class="trivet" width={size} height={size} viewBox="-3 -3 30 30" aria-hidden="true">
	<polygon points="12,5 4.5,18 19.5,18" />
</svg>

<style>
	.trivet {
		display: block;
		/* The same box `Composing` gives the mark — 24px of figure over 0.5rem
		   of air — so the swap between resting and working moves nothing. No
		   negative margin to pull the figure onto the column edge, for the same
		   reason: `Composing` has none either, and a nudge on one pose and not
		   the other is a jump every time a turn starts. */
		padding-bottom: 0.5rem;
		box-sizing: content-box;
		/* FILLED, not stroked. An outline is a different material from the mark
		   — the ∴ is three dots of ink, and a hollow triangle at this size
		   reads as an icon (the warning glyph is the one everybody has seen).
		   A solid tint is the same ink at paper weight, which is what a
		   printer's device on a page actually is. It must lose to every word
		   on the page; it is furniture, not a signal. */
		fill: var(--color-foreground);
		opacity: 0.1;
	}
</style>
