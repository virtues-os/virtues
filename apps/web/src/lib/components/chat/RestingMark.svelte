<!--
	RestingMark.svelte

	THE ROOM AT REST — the mark itself, holding still.

	The interview and Getting Started show no thinking block, and while a turn
	works they breathe `ThinkingMark` at depth 3 (see `Composing`). Between
	turns something belongs in the same margin: someone is in the room whether
	or not they are speaking. This is that, and it is the ∴ — not a second
	figure standing in for it.

	IT IS THE SAME MARK, NOT A DIFFERENT ONE. `ThinkingMark` starts and ends
	every loop at rotation zero with the extra dots retracted, so the last
	frame of working is already exactly these three circles at exactly these
	coordinates. Stopping is therefore the whole transition — no shape swaps,
	nothing to cross-fade, and the eye never has to learn a second glyph for
	the same occupant.

	MOTION IS WHAT DISTINGUISHES REST FROM WORK, NOT THE DOT COUNT. The count
	is a claim only while the figure is moving — three dots means "reasoning
	with what is loaded" against four that went out and five running passes.
	A still mark makes no claim at all; it is the logo, sitting there. But a
	preference for reduced motion holds the working mark still too, so weight
	carries the difference as well: `--color-foreground-subtle` here against
	`--color-foreground-muted` while a turn works. One step of ink, legible
	whether or not anything is allowed to move.

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

<!-- The app icon's geometry to the unit, and `ThinkingMark`'s own resting
     pose: equilateral, side 15, r 3, on the 24-unit box `virtues:logo` uses,
     on the viewBox padded to 30 that the deeper figures need. Same box here
     so the swap between resting and working moves nothing. -->
<svg class="resting-mark" width={size} height={size} viewBox="-3 -3 30 30" aria-hidden="true">
	<circle cx="12" cy="5" r="3" />
	<circle cx="4.5" cy="18" r="3" />
	<circle cx="19.5" cy="18" r="3" />
</svg>

<style>
	.resting-mark {
		display: block;
		/* The same box `Composing` gives the mark — 24px of figure over 0.5rem
		   of air — so the swap between resting and working moves nothing. No
		   negative margin to pull the figure onto the column edge, for the same
		   reason: `Composing` has none either, and a nudge on one pose and not
		   the other is a jump every time a turn starts. */
		padding-bottom: 0.5rem;
		box-sizing: content-box;
		/* A step lighter than the working mark's `--color-foreground-muted`.
		   It must lose to every word on the page; it is furniture, not a
		   signal. */
		color: var(--color-foreground-subtle);
	}

	.resting-mark circle {
		fill: currentColor;
	}
</style>
