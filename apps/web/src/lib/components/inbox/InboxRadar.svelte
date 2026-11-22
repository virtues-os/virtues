<script lang="ts">
	/**
	 * The Inbox as a radar. You are in the middle. A thing's distance from
	 * you says how soon it matters, and its slice says which thread it
	 * belongs to, with one wedge at the bottom for what is in no thread.
	 * The slices, the thread names and the bands over groups of threads are
	 * DialFrame's, which the graph shares. Pointing at a mark names it at
	 * once. The geometry is in lib/inbox/radar: it is drawn in a fixed box,
	 * which FitBox scales to the pane.
	 */
	import { MARK_SIZE as SIZE, RINGS, type InboxItem } from '$lib/inbox/items';
	import {
		BOX_H,
		BOX_W,
		CX,
		CY,
		LOOSE,
		RING_R,
		layout,
		placeLabels,
		polar,
		slots,
		type RadarThread,
		type WantedLabel,
	} from '$lib/inbox/radar';
	import DialFrame from './DialFrame.svelte';
	import FitBox from './FitBox.svelte';
	import InboxDot from './InboxDot.svelte';

	let {
		items,
		threads,
		now,
		picked = null,
		pickedThread = null,
		atRest = false,
		faded,
		onPick,
		onThread,
		onRest,
	}: {
		items: InboxItem[];
		threads: RadarThread[];
		now: Date;
		/** The key of the thing you picked. */
		picked?: string | null;
		/** The thread you picked, or `LOOSE`. */
		pickedThread?: string | null;
		/** Nothing is picked, so the center is. */
		atRest?: boolean;
		/** A thing the filters leave out is drawn faint, so the shape stays. */
		faded?: (it: InboxItem) => boolean;
		onPick: (key: string) => void;
		onThread: (id: string) => void;
		onRest: () => void;
	} = $props();

	const OUTER = RING_R[RING_R.length - 1] + 6;
	const clip = (s: string) => (s.length > 30 ? `${s.slice(0, 28).trimEnd()}…` : s);

	const lay = $derived(layout(threads));
	const spots = $derived(
		slots(
			items.map((it) => ({ key: it.key, lane: it.threadId ?? LOOSE, ring: it.ring, size: SIZE[it.kind] })),
			lay,
		),
	);
	const marks = $derived(items.map((it) => ({ x: spots[it.key].x, y: spots[it.key].y, size: SIZE[it.kind] })));
	const want = (it: InboxItem, pinned: boolean): WantedLabel => ({
		key: it.key,
		text: clip(it.title),
		x: spots[it.key].x,
		y: spots[it.key].y,
		size: SIZE[it.kind],
		ang: spots[it.key].ang,
		pinned,
	});
	// The nearest things in ink are named, and so is the one you picked. A
	// faint thing has no name, so the names stay with what the filters keep.
	// When the filters leave only a few in ink, every one of them is named.
	const FEW = 12;
	const inked = $derived(items.filter((it) => !faded?.(it)));
	const labels = $derived(
		placeLabels(
			items
				.filter((it) => it.key === picked || (!faded?.(it) && (inked.length <= FEW || it.ring <= 1)))
				.map((it) => want(it, it.key === picked)),
			marks,
			lay,
		),
	);
	const labeled = $derived(items.filter((it) => labels[it.key]));

	/** The mark under the pointer. It is named over whatever is there, so the other names stay put. */
	let hovered = $state<string | null>(null);
	const hover = $derived.by(() => {
		const it = hovered ? items.find((s) => s.key === hovered) : undefined;
		if (!it || labels[it.key]) return null;
		const spot = placeLabels([want(it, true)], marks, lay)[it.key];
		return spot ? { it, spot } : null;
	});

	const activeThread = $derived(pickedThread ?? (picked ? (items.find((it) => it.key === picked)?.threadId ?? LOOSE) : null));
	const clock = $derived(now.toLocaleString('en-US', { weekday: 'short', hour: 'numeric', minute: '2-digit' }).replace(/\s?[AP]M$/, ''));
</script>

<FitBox w={BOX_W} h={BOX_H}>
	<DialFrame {lay} active={activeThread} {onThread} />

	{#each RING_R as r, i (r)}
		<div class="ring" style:left="{CX - r}px" style:top="{CY - r}px" style:width="{r * 2}px" style:height="{r * 2}px"></div>
	{/each}
	{#each lay.bounds as b (b.at)}
		<div
			class="spoke"
			class:inner={!b.group}
			style:left="{CX}px"
			style:top="{CY}px"
			style:height="{OUTER - 40}px"
			style:transform="rotate({b.at + 180}deg) translateY(40px)"
		></div>
	{/each}
	{#each RING_R as r, i (r)}
		{@const p = polar(lay.scaleAt, r)}
		<span class="ring-name" style:left="{p.x}px" style:top="{p.y}px">{RINGS[i]}</span>
	{/each}

	<button
		type="button"
		class="you"
		class:on={atRest}
		style:left="{CX - 38}px"
		style:top="{CY - 38}px"
		onclick={onRest}
		aria-label="You, {clock}. Show what is closest."
	>
		<span class="who">You</span>
		<span class="when">{clock}</span>
	</button>

	<svg class="over" width={BOX_W} height={BOX_H} viewBox="0 0 {BOX_W} {BOX_H}" aria-hidden="true">
		{#each labeled as it (it.key)}
			{@const lead = labels[it.key]?.lead}
			{#if lead}<line x1={lead.x1} y1={lead.y1} x2={lead.x2} y2={lead.y2} />{/if}
		{/each}
		{#if hover?.spot.lead}
			{@const lead = hover.spot.lead}
			<line x1={lead.x1} y1={lead.y1} x2={lead.x2} y2={lead.y2} />
		{/if}
	</svg>
	{#each labeled as it (it.key)}
		{@const l = labels[it.key]}
		{#if l}
			<span
				class="label"
				class:on={it.key === picked}
				class:leftward={!l.right}
				style:left="{l.anchor}px"
				style:top="{l.y}px"
				aria-hidden="true"
			>
				{clip(it.title)}
			</span>
		{/if}
	{/each}
	{#if hover}
		<span class="label pointed" class:leftward={!hover.spot.right} style:left="{hover.spot.anchor}px" style:top="{hover.spot.y}px" aria-hidden="true">
			{clip(hover.it.title)}
		</span>
	{/if}

	{#each items as it (it.key)}
		<InboxDot
			item={it}
			x={spots[it.key].x}
			y={spots[it.key].y}
			picked={it.key === picked}
			faded={!!faded?.(it)}
			tip={false}
			{onPick}
			onHover={(key) => (hovered = key)}
		/>
	{/each}
</FitBox>

<style>
	.ring {
		position: absolute;
		box-sizing: border-box;
		border: 1px solid var(--color-border);
		border-radius: 50%;
	}
	/* A line between two slices. Between two threads under the same thing it is fainter. */
	.spoke {
		position: absolute;
		width: 1px;
		background: var(--color-border);
		transform-origin: 0 0;
	}
	.spoke.inner {
		background: var(--color-border-subtle);
	}
	.ring-name {
		position: absolute;
		transform: translate(-50%, -50%);
		padding: 0 4px;
		background: var(--color-page);
		font-size: var(--text-caption);
		line-height: 1;
		white-space: nowrap;
		color: var(--color-foreground-muted);
	}

	.you {
		position: absolute;
		z-index: 2;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		width: 76px;
		height: 76px;
		padding: 0;
		border: 1px solid var(--color-border);
		border-radius: 50%;
		background: var(--color-page);
		font: inherit;
		cursor: pointer;
		transition: border-color var(--motion-state) var(--ease-premium);
	}
	.you:hover,
	.you.on {
		border-color: var(--color-primary);
	}
	.who {
		font-family: var(--font-serif);
		font-size: var(--text-lead);
		line-height: 20px;
		color: var(--color-foreground);
	}
	.when {
		font-size: var(--text-caption);
		line-height: 16px;
		color: var(--color-foreground-muted);
	}

	.label {
		position: absolute;
		z-index: 2;
		padding: 0 4px;
		border-radius: var(--radius-sm);
		background: color-mix(in srgb, var(--color-page) 88%, transparent);
		font-family: var(--font-serif-ui);
		font-size: var(--text-label);
		line-height: 20px;
		white-space: nowrap;
		color: var(--color-foreground);
		pointer-events: none;
		transition:
			left var(--motion-layout) var(--ease-premium),
			top var(--motion-layout) var(--ease-premium);
	}
	/* A name on the left of its mark ends at its anchor. */
	.label.leftward {
		transform: translateX(-100%);
	}
	.label.on {
		z-index: 4;
		color: var(--color-primary);
	}
	/* The name of the mark under the pointer: on solid paper, over the others. */
	.label.pointed {
		z-index: 5;
		background: var(--color-page);
		outline: 1px solid var(--color-border);
		transition: none;
	}
	.over {
		position: absolute;
		inset: 0;
		z-index: 1;
		pointer-events: none;
	}
	.over line {
		stroke: var(--color-border-field);
		stroke-width: 1;
	}
	@media (prefers-reduced-motion: reduce) {
		.you,
		.label {
			transition: none;
		}
	}
</style>
