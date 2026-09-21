<!--
	Setup — getting started's one mention outside its room, on the rail.

	It stands at the foot of the rail's spacer, above Developer, from step 1 to
	graduation, and opens the room. It is the standing reminder that setup is
	unfinished; the app is not closed off while it is, so this is the only
	thing saying so.

	It was a card in the panel column, under whichever room's panel was open.
	That put a global thing in a room's column — it rode along inside Wiki's
	and Settings' panels like a footer — and it vanished with the panel when
	the sidebar collapsed, which is exactly the state a new owner doing a
	setup step in Sources or Settings may be in. The rail is the spine that
	never moves and never hides, so the reminder lives here.

	It is NOT a room. A rail tile swaps the panel and never opens a tab; this
	one opens the getting-started chat, so it takes the rail's other precedent
	— the ∴ mark, the one object on the rail with a job of its own — and has
	no occupied or selected state. To keep it from reading as a seventh room
	its tile is not a glyph but the folio, set in the same serif as the mark,
	so the head and the foot of the rail rhyme.

	The folio, not a bar: a bar dramatizes a number nobody asked for, and a
	fraction is the printed page's own answer to "where am I in this". The
	card used to name the next step under its title — "Introductions" is the
	answer you were going to have to find anyway — and a 72px rail cannot
	carry a name. So the name moves to the hover card, where the whole list
	fits, and the tile's tooltip carries it for anyone who never rests.

	"Setup" is the label because "Getting started" wraps at this width and a
	rail label is one word. The room keeps its name; this is the door.
-->
<script lang="ts">
	import HoverCard from "./HoverCard.svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { gettingStarted } from "$lib/stores/gettingStarted.svelte";
	import { GETTING_STARTED_CHAT_ID } from "$lib/components/chat/getting-started/getting-started";

	const steps = $derived(gettingStarted.steps);
	const settled = $derived(steps.filter((s) => s.status !== "open").length);
	const next = $derived(steps.find((s) => s.status === "open")?.title ?? null);

	function go() {
		closeCard();
		windowShellStore.openTabFromRoute(`/chat/${GETTING_STARTED_CHAT_ID}`, {
			label: "Getting started",
			focusExisting: true,
		});
	}

	// Same delay-and-grace as the Home panel's project card: the card is the
	// tile opened, not a tooltip that bites.
	const OPEN_AFTER_MS = 320;
	const CLOSE_AFTER_MS = 180;

	let anchor = $state<HTMLElement | null>(null);
	let cardOpen = $state(false);
	let openTimer: ReturnType<typeof setTimeout> | null = null;
	let closeTimer: ReturnType<typeof setTimeout> | null = null;

	function clearTimers() {
		if (openTimer) clearTimeout(openTimer);
		if (closeTimer) clearTimeout(closeTimer);
		openTimer = null;
		closeTimer = null;
	}

	function armCard() {
		clearTimers();
		if (cardOpen) return;
		openTimer = setTimeout(() => {
			cardOpen = true;
		}, OPEN_AFTER_MS);
	}

	function disarmCard() {
		if (openTimer) clearTimeout(openTimer);
		openTimer = null;
		if (!cardOpen) return;
		if (closeTimer) clearTimeout(closeTimer);
		closeTimer = setTimeout(closeCard, CLOSE_AFTER_MS);
	}

	function holdCard() {
		if (closeTimer) clearTimeout(closeTimer);
		closeTimer = null;
	}

	function closeCard() {
		clearTimers();
		cardOpen = false;
	}

	function onKeydown(e: KeyboardEvent) {
		if (e.key === "Escape" && cardOpen) closeCard();
	}
</script>

<svelte:window onkeydown={onKeydown} />

<button
	type="button"
	class="setup"
	bind:this={anchor}
	onclick={go}
	onmouseenter={armCard}
	onmouseleave={disarmCard}
	aria-label={`Setup — ${settled} of ${steps.length} done${next ? `, next: ${next}` : ""}`}
	title={next ? `Getting started · next: ${next}` : "Getting started"}
>
	<span class="folio" aria-hidden="true">{settled}/{steps.length}</span>
	<span class="label">Setup</span>
</button>

{#if cardOpen && anchor}
	<HoverCard {anchor} onenter={holdCard} onleave={disarmCard}>
		<div class="card-head">
			<span class="card-name">Getting started</span>
			<span class="card-folio" aria-hidden="true">{settled} of {steps.length}</span>
		</div>
		<ol class="card-steps">
			{#each steps as step (step.id)}
				<li class="card-step" class:open={step.status === "open"}>
					<span class="card-mark" aria-hidden="true">
						{#if step.status === "done"}✓{:else if step.status === "skipped"}·{/if}
					</span>
					<span class="card-step-title">{step.title}</span>
				</li>
			{/each}
		</ol>
		<button type="button" class="card-go" onclick={go}>
			{next ? `Continue with ${next}` : "Open the room"}
		</button>
	</HoverCard>
{/if}

<style>
	/* The rail-item's box exactly — same padding, radius, hover, transitions —
	   so it sits in the column's rhythm. What differs is the tile: a serif
	   numeral where the rooms have a stroked glyph. */
	.setup {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 4px;
		width: 100%;
		padding: 8px 4px;
		border: none;
		border-radius: var(--sidebar-interactive-radius);
		background: none;
		cursor: pointer;
		color: var(--color-foreground-muted);
		transition:
			background var(--sidebar-transition-duration) ease,
			color var(--sidebar-transition-duration) ease;
	}

	.setup:hover {
		color: var(--color-foreground);
		background: var(--sidebar-hover-bg);
	}

	.setup:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: -2px;
	}

	/* The mark's face: serif, 21px, roman. Tabular so 3/7 and 10/12 sit on the
	   same axis, and a fixed 20px line so the tile is the height of a glyph. */
	.folio {
		font-family: var(--font-serif);
		font-size: 21px;
		font-weight: 400;
		line-height: 20px;
		font-variant-numeric: tabular-nums;
		letter-spacing: 0.01em;
		color: var(--color-foreground);
	}

	.label {
		font-size: 11px;
		font-weight: 500;
		line-height: 1.1;
		letter-spacing: 0.01em;
		white-space: nowrap;
	}

	/* The card: the Home panel's project card, with steps where it has rooms. */
	.card-head {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 8px;
		padding: 6px 8px 4px 8px;
	}

	.card-name {
		font-family: var(--font-serif-ui);
		font-size: 14px;
		font-weight: 400;
		letter-spacing: 0.02em;
		-webkit-text-stroke: 0.2px currentColor;
		line-height: 20px;
	}

	.card-folio {
		font-size: 11px;
		font-variant-numeric: tabular-nums;
		color: var(--color-foreground-disabled);
	}

	.card-steps {
		list-style: none;
		margin: 0;
		padding: 0 4px 4px;
	}

	.card-step {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 3px 4px;
		font-size: 12px;
		line-height: 18px;
		color: var(--color-foreground-subtle);
	}

	/* Open steps in full ink, settled ones stepped back: the list reads as
	   what is left, which is what you came to the card to learn. */
	.card-step.open {
		color: var(--color-foreground);
	}

	.card-mark {
		width: 12px;
		flex: none;
		text-align: center;
		font-variant-numeric: tabular-nums;
		color: var(--color-foreground-disabled);
	}

	.card-step-title {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.card-go {
		display: block;
		width: 100%;
		margin-top: 2px;
		padding: 6px 8px;
		border: none;
		border-radius: 6px;
		background: none;
		font: inherit;
		font-size: 12px;
		text-align: left;
		color: var(--color-primary);
		cursor: pointer;
	}

	.card-go:hover {
		background: var(--sidebar-hover-bg);
	}
</style>
