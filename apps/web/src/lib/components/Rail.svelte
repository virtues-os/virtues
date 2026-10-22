<!--
	Rail.svelte

	An index down a gutter beside a long column: one mark per item, the
	item being read inked. Chat puts the owner's turns on it, in the left
	gutter (ChatView); a page puts its headings on it, in the right gutter,
	since its left holds the block grip (PageContent). One component, so the
	two read and move alike.

	Three states, and the middle one is the whole point:

	  resting   short tacks, barely inked. The rail is furniture until
	            someone reaches for it.
	  reached   the pointer enters the gutter and every mark lengthens; the
	            ones nearest the pointer lengthen most, so the rail leans
	            toward the hand. That taper is what makes a 2px target
	            hittable — the mark under the cursor is the widest thing
	            there before the cursor arrives.
	  reading   the mark for the item currently on screen stays inked at all
	            times, hover or not. It is a position indicator first and a
	            control second.

	An item's `level` (a heading's) shortens its mark 3px a level, so a page
	shows its shape at rest. The card names the item over its `preview`: a
	chat turn over the opening of the answer it got, because a turn alone is
	often "and then?".
-->

<script lang="ts" module>
	export interface RailItem {
		/** The key, stable across a re-render. */
		id: string;
		/** What the item says, one line: a turn, a heading. */
		label: string;
		/** A line under it on the card, if there is one. */
		preview?: string;
		/** 1 for a top-level item; each level down draws a shorter mark. */
		level?: number;
	}

	/**
	 * An element's distance from the top of its scroller's content, by its
	 * id. Measured off rects rather than `offsetTop`: the element's offset
	 * parent is the column, not the scroller, so `offsetTop` is only
	 * accidentally the same number and stops being it the moment anything
	 * sits above the column (a room's cover plate does).
	 */
	export function topOfElement(scroller: HTMLElement, id: string): number | null {
		const el = scroller.querySelector<HTMLElement>(`#${CSS.escape(id)}`);
		if (!el) return null;
		return el.getBoundingClientRect().top - scroller.getBoundingClientRect().top + scroller.scrollTop;
	}
</script>

<script lang="ts" generics="T extends RailItem">
	import { browser } from "$app/environment";

	interface Props {
		items: T[];
		/** The column's scroller: what the rail spies on and drives. */
		scroller?: HTMLElement | null;
		/** Where an item starts, from the top of the scroller's content; null when it is not drawn. */
		topOf: (item: T, scroller: HTMLElement) => number | null;
		/** Go to an item. Without one, the scroller goes to it, a hair above its top. */
		go?: (item: T) => void;
		/** Which gutter: the rail sits against that edge and its card opens toward the column. */
		side?: "left" | "right";
		/** What the rail indexes, for assistive technology ("Turns in this chat"). */
		label: string;
	}

	let { items, scroller = null, topOf, go: goTo, side = "left", label }: Props = $props();

	let activeIndex = $state(0);
	let hoveredIndex = $state<number | null>(null);
	let railEl: HTMLElement | null = $state(null);
	let marksEl: HTMLElement | null = $state(null);
	let cardTop = $state(0);
	let cardHeight = $state(0);
	let railHeight = $state(0);

	/** How far below the top of the view the reading line sits. An item becomes
	 *  "the one being read" once its first line crosses it. A fraction of the
	 *  window alone put the line a third of the way down a tall pane, which
	 *  made the SECOND turn active on a chat scrolled to the very top. */
	const READ_LINE = (h: number) => Math.min(h * 0.28, 120);

	// ── scroll-spy ─────────────────────────────────────────────────────────
	// A band-shaped IntersectionObserver was the obvious build and it is wrong
	// twice: it never fires for the last item of a short column, which cannot
	// reach the band, and it fires twice for an item taller than the window.
	// One line, and a binary search down it — a long chat is hundreds of
	// items and this runs on every frame of a scroll.
	$effect(() => {
		const el = scroller;
		if (!browser || !el || items.length === 0) return;

		let frame = 0;
		const measure = () => {
			frame = 0;
			const line = el.scrollTop + READ_LINE(el.clientHeight);
			let lo = 0;
			let hi = items.length - 1;
			let found = 0;
			while (lo <= hi) {
				const mid = (lo + hi) >> 1;
				const top = topOf(items[mid], el);
				if (top === null) break;
				if (top <= line) {
					found = mid;
					lo = mid + 1;
				} else {
					hi = mid - 1;
				}
			}
			// At the very bottom the last item is the one being read even if its
			// first line never crosses the line — a short final exchange sits
			// entirely below it.
			if (el.scrollHeight - el.scrollTop - el.clientHeight < 8) {
				found = items.length - 1;
			}
			activeIndex = found;
		};

		const onScroll = () => {
			if (frame) return;
			frame = requestAnimationFrame(measure);
		};

		measure();
		el.addEventListener("scroll", onScroll, { passive: true });
		const ro = new ResizeObserver(onScroll);
		ro.observe(el);
		return () => {
			if (frame) cancelAnimationFrame(frame);
			el.removeEventListener("scroll", onScroll);
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

	function go(item: T) {
		if (goTo) return goTo(item);
		const el = scroller;
		if (!el) return;
		const top = topOf(item, el);
		if (top === null) return;
		el.scrollTo({
			// A hair above the item, so it lands under the top edge rather than
			// flush against it.
			top: Math.max(0, top - 24),
			behavior: "smooth",
		});
	}
</script>

{#if items.length > 1}
	<nav
		class="rail"
		class:right={side === "right"}
		bind:this={railEl}
		bind:clientHeight={railHeight}
		aria-label={label}
		onmouseleave={() => (hoveredIndex = null)}
	>
		<div class="marks" bind:this={marksEl}>
			{#each items as item, i (item.id)}
				<button
					type="button"
					class="row"
					class:active={i === activeIndex}
					class:hovered={i === hoveredIndex}
					style:--near={nearness(i)}
					style:--cut="{((item.level ?? 1) - 1) * 3}px"
					aria-current={i === activeIndex ? "true" : undefined}
					title={item.label}
					onmouseenter={(e) => enter(i, e)}
					onfocus={(e) => enter(i, e)}
					onblur={() => (hoveredIndex = null)}
					onclick={() => go(item)}
				>
					<span class="mark"></span>
				</button>
			{/each}
		</div>

		{#if hoveredIndex !== null}
			{@const item = items[hoveredIndex]}
			<div
				class="card"
				bind:clientHeight={cardHeight}
				style:top="{clampedCardTop}px"
			>
				<p class="said">{item.label}</p>
				{#if item.preview}
					<p class="answered">{item.preview}</p>
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
		/* Against the pane's own edge, not the column's. Trailing the column
		   meant the rail moved every time the pane resized and sat right up
		   against the words on a narrow one; pinned here it is a fixture of
		   the window, in the same place every time the hand goes looking for
		   it. Its host hides it when the pane is too narrow to have a gutter
		   at all. */
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

	.rail.right {
		left: auto;
		right: 1.5rem;
		align-items: flex-end;
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

	.right .marks {
		align-items: flex-end;
		padding: 0.5rem 0.5rem 0.5rem 0.75rem;
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
		   mark directly under it adds the rest. A deeper level is `--cut`
		   shorter in every state. */
		width: calc(10px - var(--cut, 0px) + var(--near, 0) * 12px);
		background: color-mix(in srgb, var(--color-foreground) 16%, transparent);
		transition:
			width 0.24s cubic-bezier(0.22, 1, 0.36, 1),
			background 0.2s ease;
	}

	.row.hovered .mark {
		width: calc(26px - var(--cut, 0px));
		background: var(--color-foreground);
	}

	/* Where the reading eye is. Inked at rest — this is the rail's one
	   persistent statement, and it has to survive the hover taper. */
	.row.active .mark {
		background: color-mix(in srgb, var(--color-foreground) 70%, transparent);
		width: calc(20px - var(--cut, 0px) + var(--near, 0) * 6px);
	}

	.row.active.hovered .mark {
		width: calc(26px - var(--cut, 0px));
		background: var(--color-foreground);
	}

	@media (prefers-reduced-motion: reduce) {
		.mark {
			transition: background 0.2s ease;
		}
	}

	/* A layer floating over the page, drawn as every other one is. */
	.card {
		position: absolute;
		left: 100%;
		transform: translateY(-50%);
		width: 17rem;
		box-sizing: border-box;
		padding: 0.625rem 0.75rem;
		border: 1px solid var(--color-border);
		border-radius: var(--card-radius);
		background: var(--color-surface);
		box-shadow: var(--shadow-float);
		pointer-events: none;
		font-family: var(--font-sans, system-ui, sans-serif);
		animation: card-in 0.16s cubic-bezier(0.22, 1, 0.36, 1);
	}

	.right .card {
		left: auto;
		right: 100%;
		animation-name: card-in-left;
	}

	@keyframes card-in {
		from {
			opacity: 0;
			transform: translateY(-50%) translateX(-4px);
		}
	}

	@keyframes card-in-left {
		from {
			opacity: 0;
			transform: translateY(-50%) translateX(4px);
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.card {
			animation: none;
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
