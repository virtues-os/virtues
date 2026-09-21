<!--
	ConversationRail.svelte

	The transcript's spine, in the left gutter: one mark per thing the owner
	said. A long chat is a column of replies with the owner's own turns buried
	in it, and the only way back to "the one where I asked about the margins"
	was to scroll and read. The rail makes the turns the index — they are the
	only lines in a chat anyone navigates by.

	Three states, and the middle one is the whole point:

	  resting   short tacks, barely inked. The rail is furniture until
	            someone reaches for it.
	  reached   the pointer enters the gutter and every mark lengthens; the
	            ones nearest the pointer lengthen most, so the rail leans
	            toward the hand. That taper is what makes a 2px target
	            hittable — the mark under the cursor is the widest thing
	            there before the cursor arrives.
	  reading   the mark for the turn currently on screen stays inked at all
	            times, hover or not. It is a position indicator first and a
	            control second.

	The card names the turn (what the owner said) over the opening of the
	answer it got, because a turn alone is often "and then?" — the reply is
	what makes the entry recognizable.
-->

<script lang="ts">
	import { browser } from "$app/environment";

	export interface RailTurn {
		/** The message id — the key, and stable across a re-render. */
		id: string;
		/** The element id on the turn's row in the transcript. */
		anchor: string;
		/** What the owner said, one line. */
		label: string;
		/** The opening of the answer it got, if it has one yet. */
		preview: string;
	}

	interface Props {
		turns: RailTurn[];
		/** The transcript's scroller — the thing the rail spies on and drives. */
		scrollContainer?: HTMLElement | null;
	}

	let { turns, scrollContainer = null }: Props = $props();

	let activeIndex = $state(0);
	let hoveredIndex = $state<number | null>(null);
	let railEl: HTMLElement | null = $state(null);
	let marksEl: HTMLElement | null = $state(null);
	let cardTop = $state(0);
	let cardHeight = $state(0);
	let railHeight = $state(0);

	/** How far below the top of the view the reading line sits. A turn becomes
	 *  "the one being read" once its first line crosses it. A fraction of the
	 *  window alone put the line a third of the way down a tall pane, which
	 *  made the SECOND turn active on a chat scrolled to the very top. */
	const READ_LINE = (h: number) => Math.min(h * 0.28, 120);

	/** A turn's distance from the top of the scrollable content. Measured off
	 *  rects rather than `offsetTop`: the anchors' offset parent is the
	 *  messages column, not the scroller, so `offsetTop` is only accidentally
	 *  the same number and stops being it the moment anything sits above the
	 *  column (the room's cover plate does). */
	function topOf(scroller: HTMLElement, base: number, anchor: string): number | null {
		const el = scroller.querySelector<HTMLElement>(`#${CSS.escape(anchor)}`);
		if (!el) return null;
		return el.getBoundingClientRect().top - base + scroller.scrollTop;
	}

	// ── scroll-spy ─────────────────────────────────────────────────────────
	// A band-shaped IntersectionObserver was the obvious build and it is wrong
	// twice: it never fires for the last turn of a short chat, which cannot
	// reach the band, and it fires twice for a turn taller than the window.
	// One line, and a binary search down it — a long chat is hundreds of
	// anchors and this runs on every frame of a scroll.
	$effect(() => {
		const scroller = scrollContainer;
		if (!browser || !scroller || turns.length === 0) return;

		let frame = 0;
		const measure = () => {
			frame = 0;
			const base = scroller.getBoundingClientRect().top;
			const line = scroller.scrollTop + READ_LINE(scroller.clientHeight);
			let lo = 0;
			let hi = turns.length - 1;
			let found = 0;
			while (lo <= hi) {
				const mid = (lo + hi) >> 1;
				const top = topOf(scroller, base, turns[mid].anchor);
				if (top === null) break;
				if (top <= line) {
					found = mid;
					lo = mid + 1;
				} else {
					hi = mid - 1;
				}
			}
			// At the very bottom the last turn is the one being read even if its
			// first line never crosses the line — a short final exchange sits
			// entirely below it.
			if (scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < 8) {
				found = turns.length - 1;
			}
			activeIndex = found;
		};

		const onScroll = () => {
			if (frame) return;
			frame = requestAnimationFrame(measure);
		};

		measure();
		scroller.addEventListener("scroll", onScroll, { passive: true });
		const ro = new ResizeObserver(onScroll);
		ro.observe(scroller);
		return () => {
			if (frame) cancelAnimationFrame(frame);
			scroller.removeEventListener("scroll", onScroll);
			ro.disconnect();
		};
	});

	// Keep the active mark in view when the rail is taller than its gutter —
	// but never yank it out from under a pointer that is reading it.
	$effect(() => {
		const i = activeIndex;
		if (!browser || hoveredIndex !== null || !marksEl) return;
		const mark = marksEl.children[i] as HTMLElement | undefined;
		mark?.scrollIntoView({ block: "nearest" });
	});

	/** How much a mark leans toward the pointer: 1 under it, 0 three away. */
	function nearness(i: number): number {
		if (hoveredIndex === null) return 0;
		return Math.max(0, 1 - Math.abs(i - hoveredIndex) / 3);
	}

	function enter(i: number, event: MouseEvent | FocusEvent) {
		hoveredIndex = i;
		const row = event.currentTarget as HTMLElement;
		const rail = railEl;
		if (!rail) return;
		const r = row.getBoundingClientRect();
		const b = rail.getBoundingClientRect();
		cardTop = r.top - b.top + r.height / 2;
	}

	/** The card centers on its mark, then gives way at the rail's ends rather
	 *  than hanging off the top of the window. */
	const clampedCardTop = $derived(
		Math.min(
			Math.max(cardTop, cardHeight / 2),
			Math.max(cardHeight / 2, railHeight - cardHeight / 2),
		),
	);

	function go(turn: RailTurn) {
		const scroller = scrollContainer;
		if (!scroller) return;
		const top = topOf(scroller, scroller.getBoundingClientRect().top, turn.anchor);
		if (top === null) return;
		scroller.scrollTo({
			// A hair above the turn, so it lands under the top edge rather than
			// flush against it.
			top: Math.max(0, top - 24),
			behavior: "smooth",
		});
	}
</script>

{#if turns.length > 1}
	<nav
		class="rail"
		bind:this={railEl}
		bind:clientHeight={railHeight}
		aria-label="Turns in this chat"
		onmouseleave={() => (hoveredIndex = null)}
	>
		<div class="marks" bind:this={marksEl}>
			{#each turns as turn, i (turn.id)}
				<button
					type="button"
					class="row"
					class:active={i === activeIndex}
					class:hovered={i === hoveredIndex}
					style:--near={nearness(i)}
					aria-current={i === activeIndex ? "true" : undefined}
					title={turn.label}
					onmouseenter={(e) => enter(i, e)}
					onfocus={(e) => enter(i, e)}
					onblur={() => (hoveredIndex = null)}
					onclick={() => go(turn)}
				>
					<span class="mark"></span>
				</button>
			{/each}
		</div>

		{#if hoveredIndex !== null}
			{@const turn = turns[hoveredIndex]}
			<div
				class="card"
				bind:clientHeight={cardHeight}
				style:top="{clampedCardTop}px"
			>
				<p class="said">{turn.label}</p>
				{#if turn.preview}
					<p class="answered">{turn.preview}</p>
				{/if}
			</div>
		{/if}
	</nav>
{/if}

<style>
	.rail {
		position: absolute;
		top: 50%;
		transform: translateY(-50%);
		/* Against the pane's own left edge, not the column's. Trailing the
		   column meant the rail moved every time the pane resized and sat
		   right up against the words on a narrow one; pinned here it is a
		   fixture of the window, in the same place every time the hand goes
		   looking for it. ChatView hides it when the pane is too narrow to
		   have a gutter at all. */
		left: 1.5rem;
		z-index: 5;
		max-height: 64%;
		/* A COLUMN flex box, and the marks shrink inside it. Laid out as a row
		   with `max-height: 100%` on the marks, a hundred-turn chat drew a
		   1400px rail through a 565px box and out the bottom of the pane: a
		   percentage height resolves against a parent's HEIGHT, and a parent
		   whose only constraint is `max-height` has none to give. Shrinking
		   along the main axis needs no percentage and no measurement. */
		display: flex;
		flex-direction: column;
		justify-content: center;
		align-items: flex-start;
	}

	.marks {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		flex: 0 1 auto;
		min-height: 0;
		overflow-y: auto;
		scrollbar-width: none;
		/* The gutter is the target, not the 2px line in it. */
		padding: 0.5rem 0.75rem 0.5rem 0.5rem;
	}

	.marks::-webkit-scrollbar {
		display: none;
	}

	.row {
		all: unset;
		display: flex;
		align-items: center;
		cursor: pointer;
		/* Every mark keeps a full row of pointer height even at rest, so the
		   rail is one continuous target from the first turn to the last —
		   including past the rail's own capacity, where the rows must hold
		   their height and let the rail scroll rather than squeeze a hundred
		   turns into a hatch pattern nobody can hit. */
		height: 10px;
		flex: 0 0 auto;
		box-sizing: border-box;
	}

	.mark {
		display: block;
		height: 2px;
		/* A pill, said the way the grammar says one. The browser clamps a
		   radius to half the shorter side, so on a 2px bar this renders
		   exactly as the 1px it replaces. */
		border-radius: 100px;
		/* 10px at rest; the pointer's nearness adds up to 12 more, and the
		   mark directly under it adds the rest. */
		width: calc(10px + var(--near, 0) * 12px);
		background: color-mix(in srgb, var(--color-foreground) 16%, transparent);
		transition:
			width 0.24s cubic-bezier(0.22, 1, 0.36, 1),
			background 0.2s ease;
	}

	.row.hovered .mark {
		width: 26px;
		background: var(--color-foreground);
	}

	/* Where the reading eye is. Inked at rest — this is the rail's one
	   persistent statement, and it has to survive the hover taper. */
	.row.active .mark {
		background: color-mix(in srgb, var(--color-foreground) 70%, transparent);
		width: calc(20px + var(--near, 0) * 6px);
	}

	.row.active.hovered .mark {
		width: 26px;
		background: var(--color-foreground);
	}

	@media (prefers-reduced-motion: reduce) {
		.mark {
			transition: background 0.2s ease;
		}
	}

	.card {
		position: absolute;
		left: 100%;
		transform: translateY(-50%);
		width: 17rem;
		box-sizing: border-box;
		padding: 0.625rem 0.75rem;
		border: 1px solid var(--color-border);
		border-radius: 12px;
		background: var(--color-surface-elevated);
		pointer-events: none;
		font-family: var(--font-sans, system-ui, sans-serif);
		animation: card-in 0.16s cubic-bezier(0.22, 1, 0.36, 1);
	}

	@keyframes card-in {
		from {
			opacity: 0;
			transform: translateY(-50%) translateX(-4px);
		}
	}

	.card p {
		margin: 0;
		/* Two lines each: enough to recognize a turn, not enough to read it
		   here instead of going there. */
		display: -webkit-box;
		-webkit-box-orient: vertical;
		-webkit-line-clamp: 2;
		line-clamp: 2;
		overflow: hidden;
	}

	.said {
		font-size: 0.8125rem;
		line-height: 1.4;
		color: var(--color-foreground);
	}

	.answered {
		margin-top: 0.3rem;
		font-size: 0.75rem;
		line-height: 1.45;
		color: var(--color-foreground-subtle);
	}
</style>
