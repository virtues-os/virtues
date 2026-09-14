<!--
	InterviewCompanion.svelte

	The room's ONE activity indicator, below the last turn — these rooms show
	no ThinkingBlock, because the model's reasoning about the person must
	never surface as chrome in a room built on their own account.

	AT REST IT IS THE MARK: ∴, three dots, therefore. A face sitting in the
	margin pulls the eye to itself and away from the page (Adam, 2026-09-14),
	so the resident only comes out while the model is composing, or for a
	blip when someone puts a pointer on it. Engine vendored from bloub (MIT)
	— see lib/bloub/README.md.
-->

<script lang="ts">
	import Bloub from "$lib/bloub/Bloub.svelte";
	import { EXPRESSIONS } from "$lib/bloub/bot/expressions";
	import { DEFAULT_SHAPE, SHAPES } from "$lib/bloub/bot/skins";
	import { getRandomThinkingLabel } from "$lib/utils/thinkingLabels";

	interface Props {
		/** The chat's status; "submitted" and "streaming" read as thinking. */
		status: string;
		/** Anything that changes here rouses the bot — the message count is
		 *  what ChatView passes, so a new turn wakes it. */
		activity: unknown;
	}
	let { status, activity }: Props = $props();

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
		rouse();
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

	// After a quiet stretch the bot dozes off instead of blinking at an empty
	// room forever. Anything happening — a hover, a send, the model speaking —
	// rouses it and re-arms the timer.
	const DOZE_MS = 90_000;
	let asleep = $state(false);
	let sleepTimer: ReturnType<typeof setTimeout> | undefined;
	function rouse() {
		asleep = false;
		clearTimeout(sleepTimer);
		sleepTimer = setTimeout(() => (asleep = true), DOZE_MS);
	}

	// The activity feed: a new message or a status change wakes it and
	// re-arms the doze timer.
	$effect(() => {
		void activity;
		void status;
		rouse();
		return () => clearTimeout(sleepTimer);
	});

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
		{#if out}
			<Bloub
				size={54}
				state={thinking ? "thinking" : asleep ? "sleep" : "idle"}
				shape={shape ?? DEFAULT_SHAPE}
				expression={expression ?? "neutre"}
				ink="var(--color-foreground)"
				paper="var(--color-background)"
			/>
		{:else}
			<span class="mark" aria-hidden="true">∴</span>
		{/if}
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
		   carries (58/316)·size of built-in whitespace per side; pull the ball's
		   edge back onto the column's left margin, and its top toward the
		   conversation's tail. Keep the px in step with the size= prop. */
		margin-left: calc(54px * -58 / 316);
		margin-top: calc(54px * -58 / 316);
	}

	/* The mark stands in the ball's place, so nothing shifts when the
	   resident comes and goes. */
	.mark {
		width: 54px;
		height: 54px;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		font-family: var(--font-serif, Georgia, serif);
		font-size: 1.5rem;
		line-height: 1;
		color: var(--color-foreground);
		opacity: 0.35;
		transition: opacity 0.2s ease;
	}
	.interview-companion:hover .mark {
		opacity: 0.7;
	}

	.companion-word {
		font-size: 0.8125rem;
		color: var(--color-foreground);
		opacity: 0.5;
	}
</style>
