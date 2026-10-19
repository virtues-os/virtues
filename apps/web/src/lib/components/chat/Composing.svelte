<!--
	Composing.svelte

	The activity indicator for the two rooms that show no thinking block — the
	narrative interview and Getting Started, where "the machine's workings are
	not the subject" (ChatView). The mark, breathing. Nothing about tools,
	nothing about models, no word.

	It replaces a vendored character engine (`lib/bloub`, 15 files) whose
	resting state was the ∴ mark and whose thinking state was an animated
	ellipsis — with a rotating random gerund beside it. The ellipsis was doing
	all the work; the gerund was a second indicator of the same one bit, and
	the one it added was which verb came up.

	Removing the character also took the mark out of the margin, and for a
	while three plain dots stood there instead — the house idiom, but a second
	three-dot idiom beside the one that is actually ours. `ThinkingMark` at
	depth 3 is the same three dots breathing, in the shape of the ∴.

	DEPTH 3 IS THE WHOLE POINT, and it is pinned here rather than passed in.
	The mark reports how far a turn went by growing a fourth and fifth dot;
	at 3 it never grows one, so it can say "something is happening" without
	saying anything about what — which is the contract these rooms hold. If a
	room ever wants to report depth it has the thinking block for that.

	THE ANNOUNCEMENT IS THE POINT, not the mark. The wrapper it replaced was
	`role="presentation"` over an SVG animation, so that random gerund was the
	only thing a screen reader could perceive about the box working at all.
	Here the mark is decoration (aria-hidden, inside ThinkingMark) and a live
	region carries the fact. Reduced motion holds the mark still, and nothing
	is lost, because the live region is what actually reports.
-->

<script lang="ts">
	import ThinkingMark from "$lib/components/ThinkingMark.svelte";

	interface Props {
		/** What a screen reader hears. Steady, not a rotating word. */
		label?: string;
	}
	let { label = "Composing a reply" }: Props = $props();
</script>

<div class="composing" role="status" aria-live="polite">
	<!-- 24px puts the dots at 4.8px across — the same weight as the three
	     plain dots this replaces, so the room's texture does not change. The
	     chat status line runs the mark smaller, at 16, because there it sits
	     beside 14px text rather than standing on its own. -->
	<ThinkingMark depth={3} size={24} />
	<span class="sr-only">{label}</span>
</div>

<style>
	.composing {
		display: flex;
		align-items: center;
		padding: 0 0 0.5rem;
	}

	.sr-only {
		position: absolute;
		width: 1px;
		height: 1px;
		padding: 0;
		margin: -1px;
		overflow: hidden;
		clip: rect(0, 0, 0, 0);
		white-space: nowrap;
		border: 0;
	}
</style>
