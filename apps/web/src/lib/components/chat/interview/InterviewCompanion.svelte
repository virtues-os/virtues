<!--
	InterviewCompanion.svelte

	The room's ONE activity indicator, below the last turn — these rooms show
	no ThinkingBlock, because the model's reasoning about the person must
	never surface as chrome in a room built on their own account.

	AT REST IT HOLDS ITS OWN MARK: the `sleep` state, which in this fork is
	the virtues ∴ — three dots breathing in a slow ripple (see bot/states.ts).
	A face sitting in the margin pulls the eye to itself and away from the
	page (Adam, 2026-09-14), so the resident's face only comes out while the
	model is composing, or for a blip when someone puts a pointer on it.
	Engine vendored from bloub (MIT) — see lib/bloub/README.md.
-->

<script lang="ts">
	import Bloub from "$lib/bloub/Bloub.svelte";
	import { EXPRESSIONS } from "$lib/bloub/bot/expressions";
	import { DEFAULT_SHAPE, SHAPES } from "$lib/bloub/bot/skins";
	import { getRandomThinkingLabel } from "$lib/utils/thinkingLabels";

	interface Props {
		/** The chat's status; "submitted" and "streaming" read as thinking. */
		status: string;
	}
	let { status }: Props = $props();

	const thinking = $derived(status === "submitted" || status === "streaming");

	/** The resident is out: composing, or roused by a pointer for a moment. */
	let visiting = $state(false);
	let visitTimer: ReturnType<typeof setTimeout> | undefined;
	const out = $derived(thinking || visiting);

	// A poke morphs the eyes to one random expression — and, roughly one poke
	// in five, the body to one random shape, so the circle stays the norm.
	// Both hold while the pointer stays and settle back to resting one second
	// after it leaves. Re-entering re-rolls.
	let expression = $state<string | null>(null);
	let shape = $state<string | null>(null);
	let hoverTimer: ReturnType<typeof setTimeout> | undefined;
	function poke() {
		visiting = true;
		clearTimeout(visitTimer);
		const others = EXPRESSIONS.filter((e) => e.id !== "neutre" && e.id !== expression);
		expression = others[Math.floor(Math.random() * others.length)].id;
		const shapes = SHAPES.filter((s) => s.id !== DEFAULT_SHAPE && s.id !== shape);
		shape =
			Math.random() < 0.2 ? shapes[Math.floor(Math.random() * shapes.length)].id : null;
		clearTimeout(hoverTimer);
	}
	function settle() {
		clearTimeout(hoverTimer);
		clearTimeout(visitTimer);
		// A blip, not a stay: the mark comes back on its own.
		visitTimer = setTimeout(() => (visiting = false), 1400);
		hoverTimer = setTimeout(() => {
			expression = null;
			shape = null;
		}, 1000);
	}

	// The doze timer went with the resting state it used to reach after 90
	// seconds: resting IS the mark now, and a turn arriving shows the face by
	// way of `thinking`. Nothing is left to wake, so the `activity` prop that
	// fed it went too.

	// While the bot thinks, one rotating gerund rides beside it — the bot's
	// three-dot morph is already the ellipsis, so the word comes bare.
	let word = $state("");
	$effect(() => {
		if (!thinking) return;
		word = getRandomThinkingLabel();
		const rotate = setInterval(() => {
			word = getRandomThinkingLabel();
		}, 4000);
		return () => clearInterval(rotate);
	});
</script>

<div class="flex justify-start">
	<div
		class="interview-companion"
		role="presentation"
		onmouseenter={poke}
		onmouseleave={settle}
	>
		<Bloub
			size={54}
			state={thinking ? "thinking" : out ? "idle" : "sleep"}
			shape={shape ?? DEFAULT_SHAPE}
			expression={expression ?? "neutre"}
			ink="var(--color-foreground)"
			paper="var(--color-background)"
		/>
		{#if thinking}
			<span class="companion-word">{word}</span>
		{/if}
	</div>
</div>

<style>
	.interview-companion {
		display: flex;
		align-items: center;
		gap: 0.375rem;
		padding: 0 0 0.5rem;
		/* The bloub viewBox is ±158 around a body of radius 100, so the SVG
		   carries built-in whitespace on every side; pull it back so the shape
		   sits on the column's left margin rather than a gutter in from it, and
		   its top toward the conversation's tail. Keep the px in step with the
		   size= prop.

		   The LEFT is measured from the resting mark, not the ball: the ∴ is
		   what stands here almost always, and its outer dot reaches only 43.5
		   viewBox units (SLEEP_BASE 0.30 + SLEEP_DOT_R 0.1347, in body radii),
		   leaving 114.5 units of air to its left. Aligning the ball's edge
		   instead — 58 units — left the mark visibly indented from the prose.
		   When he wakes, the ball now overhangs the column slightly, which is
		   the usual optical treatment for a round shape at a text edge.

		   MEASURED, not judged by eye: at size=54 a viewBox unit is 54/316 px,
		   the resting foot's center stands 30 units left of the SVG's center
		   and its ink 13.47 further, so this lands the mark's left edge on the
		   column to within 0.01px. An optical nudge of +3px was tried and read
		   as a visible indent — at this size the dot is 4.6px across, so three
		   of them is most of a dot. */
		margin-left: calc(54px * -114.5 / 316);
		margin-top: calc(54px * -58 / 316);
	}

	.companion-word {
		font-size: 0.8125rem;
		color: var(--color-foreground);
		opacity: 0.5;
	}
</style>
