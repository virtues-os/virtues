<script lang="ts">
	/**
	 * The Inbox as a graph: what is linked to what. It is drawn in the
	 * radar's frame (DialFrame), so a thread's slice and name are where the
	 * radar has them. Inside, you are in the middle, each thread has a hub on
	 * its slice, and each thing hangs off its thread, or off you when it has
	 * none. A line joins everything else: an insight to the threads and the
	 * task it is about, a task to what else it is about, a thread to the
	 * thing it sits under. Choose a node and its lines are drawn in the
	 * accent, each with what it means. Any node can be dragged to untangle a
	 * knot. The layout is in lib/inbox/graph.
	 */
	import { TextAction } from '$lib';
	import { buildGraph, hubId, type ExtraLink } from '$lib/inbox/graph';
	import type { InboxItem } from '$lib/inbox/items';
	import { fitLabels, markBox, type Wanted } from '$lib/inbox/labels';
	import { BOX_H, BOX_W, CX, LOOSE, layout, type RadarThread } from '$lib/inbox/radar';
	import DialFrame from './DialFrame.svelte';
	import FitBox from './FitBox.svelte';
	import InboxDot from './InboxDot.svelte';

	let {
		items,
		threads,
		picked = null,
		pickedThread = null,
		atRest = false,
		faded,
		extra,
		onPick,
		onThread,
		onRest,
	}: {
		items: InboxItem[];
		threads: RadarThread[];
		/** The key of the thing you picked. */
		picked?: string | null;
		/** The thread you picked, or `LOOSE`. */
		pickedThread?: string | null;
		/** Nothing is picked, so the center is. */
		atRest?: boolean;
		/** A thing the Show filter leaves out is drawn faint, so the shape stays. */
		faded?: (it: InboxItem) => boolean;
		/** A task's links beyond its own thread: what else it is about, or a home that is not an active thread. */
		extra?: (it: InboxItem) => ExtraLink[];
		onPick: (key: string) => void;
		onThread: (id: string) => void;
		onRest: () => void;
	} = $props();

	const clip = (s: string) => (s.length > 30 ? `${s.slice(0, 28).trimEnd()}…` : s);

	const lay = $derived(layout(threads));
	const graph = $derived(buildGraph({ items, threads, lay, extra }));

	/** Where you dragged a node to. It stays there until you put everything back. */
	let moved = $state<Record<string, { x: number; y: number }>>({});
	const nodes = $derived(graph.nodes.map((n) => (moved[n.id] ? { ...n, ...moved[n.id] } : n)));
	const byId = $derived(new Map(nodes.map((n) => [n.id, n])));
	const anyMoved = $derived(nodes.some((n) => moved[n.id]));

	const activeThread = $derived(pickedThread ?? (picked ? (items.find((it) => it.key === picked)?.threadId ?? LOOSE) : null));
	/** The node whose lines are lit: the thing you picked, or the hub of the thread you picked. */
	const lit = $derived(picked ?? (pickedThread && pickedThread !== LOOSE ? hubId(pickedThread) : null));
	const lines = $derived(
		graph.links.flatMap((l) => {
			const a = byId.get(l.from);
			const b = byId.get(l.to);
			return a && b ? [{ ...l, a, b, hot: lit !== null && (l.from === lit || l.to === lit) }] : [];
		}),
	);

	/** The mark under the pointer. */
	let hovered = $state<string | null>(null);
	/**
	 * The names drawn inside the frame: every subject, the thing you picked
	 * and the one under the pointer. Threads are named on the rim, and a
	 * thing or a hub names itself in a tip.
	 */
	const names = $derived.by(() => {
		const list: (Wanted & { tone: '' | 'on' | 'pointed' })[] = [];
		for (const n of nodes) {
			const named = n.kind === 'subject' || (n.kind === 'item' && (n.id === picked || n.id === hovered));
			if (!named) continue;
			// A name goes outward first, away from the middle.
			list.push({
				key: n.id,
				text: clip(n.label),
				x: n.x,
				y: n.y,
				size: n.size,
				right: n.x >= CX,
				pinned: true,
				tone: n.id === picked ? 'on' : n.kind === 'item' ? 'pointed' : '',
			});
		}
		return { list, spots: fitLabels(list, nodes.map(markBox), { w: BOX_W, h: BOX_H }) };
	});

	// ── dragging a node ────────────────────────────────────────────────────
	let field = $state<HTMLElement | null>(null);
	let drag = $state<{ id: string; fromX: number; fromY: number; x: number; y: number; moving: boolean } | null>(null);
	/** A drag ends in a click on whatever is under the pointer. That click is not a choice. */
	let justDragged = false;

	function down(e: PointerEvent) {
		if (e.button !== 0 || !(e.target instanceof Element)) return;
		const el = e.target.closest<HTMLElement>('[data-node], [data-key]');
		const id = el?.dataset.node ?? el?.dataset.key;
		const n = id ? byId.get(id) : undefined;
		// You stay in the middle.
		if (!n || n.kind === 'you') return;
		justDragged = false;
		drag = { id: n.id, fromX: e.clientX, fromY: e.clientY, x: n.x, y: n.y, moving: false };
	}

	function move(e: PointerEvent) {
		if (!drag || !field) return;
		const dx = e.clientX - drag.fromX;
		const dy = e.clientY - drag.fromY;
		// A few pixels of travel is still a click.
		if (!drag.moving && Math.hypot(dx, dy) < 4) return;
		if (!drag.moving) {
			drag.moving = true;
			field.setPointerCapture(e.pointerId);
		}
		// The drawing is scaled to fit its pane, so the pointer's travel is scaled back.
		const scale = field.getBoundingClientRect().width / BOX_W || 1;
		const clamp = (v: number, max: number) => Math.min(max - 8, Math.max(8, v));
		moved = { ...moved, [drag.id]: { x: clamp(drag.x + dx / scale, BOX_W), y: clamp(drag.y + dy / scale, BOX_H) } };
	}

	function up() {
		justDragged = !!drag?.moving;
		drag = null;
	}

	function swallow(e: MouseEvent) {
		if (!justDragged) return;
		justDragged = false;
		e.stopPropagation();
		e.preventDefault();
	}
</script>

<FitBox w={BOX_W} h={BOX_H}>
	<DialFrame {lay} active={activeThread} {onThread} />

	<svg class="links" width={BOX_W} height={BOX_H} viewBox="0 0 {BOX_W} {BOX_H}" aria-hidden="true">
		{#each lines as l (l.key)}
			<line class={l.style} class:hot={l.hot} x1={l.a.x} y1={l.a.y} x2={l.b.x} y2={l.b.y} />
		{/each}
		{#each names.list as t (t.key)}
			{@const lead = names.spots[t.key]?.lead}
			{#if lead}<line class="lead" x1={lead.x1} y1={lead.y1} x2={lead.x2} y2={lead.y2} />{/if}
		{/each}
		{#each lines as l (l.key)}
			{#if l.hot && l.rel}
				<text class="rel" x={(l.a.x + l.b.x) / 2} y={(l.a.y + l.b.y) / 2 - 4} text-anchor="middle">{l.rel}</text>
			{/if}
		{/each}
	</svg>

	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div
		class="field"
		class:dragging={drag?.moving}
		bind:this={field}
		onpointerdown={down}
		onpointermove={move}
		onpointerup={up}
		onpointercancel={up}
		onclickcapture={swallow}
	>
		{#each nodes as n (n.id)}
			{#if n.kind === 'you'}
				<button type="button" class="node you" class:on={atRest} style:left="{n.x}px" style:top="{n.y}px" onclick={onRest} aria-label="You. Show what is closest.">
					You
				</button>
			{:else if n.kind === 'thing'}
				<span class="node thing" data-node={n.id} data-tip={n.label} style:left="{n.x}px" style:top="{n.y}px"></span>
			{:else if n.kind === 'hub'}
				<button
					type="button"
					class="node hub"
					class:on={activeThread === n.threadId}
					data-node={n.id}
					data-tip={n.label}
					aria-label={n.label}
					aria-pressed={activeThread === n.threadId}
					style:left="{n.x}px"
					style:top="{n.y}px"
					onclick={() => n.threadId && onThread(n.threadId)}
				></button>
			{:else if n.kind === 'subject'}
				<span class="node subject" data-node={n.id} style:left="{n.x}px" style:top="{n.y}px"></span>
			{:else if n.item}
				<InboxDot item={n.item} x={n.x} y={n.y} picked={n.id === picked} faded={!!faded?.(n.item)} tip={false} {onPick} onHover={(key) => (hovered = key)} />
			{/if}
		{/each}
	</div>

	{#each names.list as t (t.key)}
		{@const spot = names.spots[t.key]}
		{#if spot}
			<span class="name {t.tone}" class:leftward={!spot.right} style:left="{spot.anchor}px" style:top="{spot.y}px" aria-hidden="true">{t.text}</span>
		{/if}
	{/each}

	<p class="foot">
		{#if anyMoved}
			<TextAction onclick={() => (moved = {})}>Put everything back</TextAction>
		{:else}
			Drag anything to move it.
		{/if}
	</p>
</FitBox>

<style>
	.links {
		position: absolute;
		inset: 0;
		pointer-events: none;
	}
	.links line {
		stroke: var(--color-border);
		stroke-width: 1;
	}
	/* From you to something in no thread. */
	.links .loose {
		stroke-dasharray: 2 4;
	}
	/* A line that joins two branches. */
	.links .cross {
		stroke: var(--color-foreground-subtle);
	}
	.links .hot {
		stroke: var(--color-primary);
		stroke-width: 1.5;
	}
	.links .lead {
		stroke: var(--color-border-field);
	}
	/* What a lit line means, on the page's paper so the line does not run through it. */
	.rel {
		font-size: var(--text-caption);
		fill: var(--color-primary);
		stroke: var(--color-page);
		stroke-width: 4px;
		paint-order: stroke;
	}

	.field {
		position: absolute;
		inset: 0;
		touch-action: none;
	}
	/* A dragged node follows the pointer at once. */
	.field.dragging :global(.dot.placed) {
		transition: none;
	}
	.field.dragging {
		cursor: grabbing;
	}
	.node {
		position: absolute;
		box-sizing: border-box;
		padding: 0;
		transform: translate(-50%, -50%);
	}
	.you {
		width: 56px;
		height: 56px;
		border: 1px solid var(--color-border);
		border-radius: 50%;
		background: var(--color-page);
		font-family: var(--font-serif);
		font-size: var(--text-body);
		color: var(--color-foreground);
		cursor: pointer;
		transition: border-color var(--motion-state) var(--ease-premium);
	}
	.you:hover,
	.you.on {
		border-color: var(--color-primary);
	}
	/* A thread's hub: a ring, where the thread's things are tied on. */
	.hub {
		width: 14px;
		height: 14px;
		border: 2px solid var(--color-foreground);
		border-radius: 50%;
		background: var(--color-page);
		cursor: pointer;
		transition: border-color var(--motion-state) var(--ease-premium);
	}
	.hub:hover,
	.hub.on {
		border-color: var(--color-primary);
	}
	/* A thing that threads sit under. */
	.thing {
		width: 10px;
		height: 10px;
		border-radius: 50%;
		background: var(--color-foreground);
		cursor: grab;
	}
	/* A person, a place or a thread that is not active. */
	.subject {
		width: 10px;
		height: 10px;
		border-radius: var(--radius-sm);
		background: var(--color-foreground-muted);
		cursor: grab;
	}

	.name {
		position: absolute;
		z-index: 4;
		padding: 0 4px;
		border-radius: var(--radius-sm);
		background: color-mix(in srgb, var(--color-page) 88%, transparent);
		font-family: var(--font-serif-ui);
		font-size: var(--text-label);
		line-height: 20px;
		white-space: nowrap;
		color: var(--color-foreground);
		pointer-events: none;
	}
	.name.leftward {
		transform: translateX(-100%);
	}
	.name.on {
		color: var(--color-primary);
	}
	.name.pointed {
		background: var(--color-page);
		outline: 1px solid var(--color-border);
	}
	.foot {
		position: absolute;
		left: 0;
		bottom: 0;
		margin: 0;
		font-size: var(--text-caption);
		line-height: 16px;
		color: var(--color-foreground-muted);
	}

	@media (prefers-reduced-motion: reduce) {
		.you,
		.hub {
			transition: none;
		}
	}
</style>
