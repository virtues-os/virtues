<!--
	Rows with a start, an optional end and a label, laid along one time axis.
	A span is a bar and an instant is a dot; overlapping ones take the next
	lane down. Labels sit inside a bar wide enough to hold them, and the
	readout above names whatever the pointer is on.
-->
<script lang="ts">
	import { assignLanes, formatTime, parseTime, rowRef, type Row } from "./show";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";

	interface Props {
		rows: Row[];
	}
	let { rows }: Props = $props();

	const LANE = 20;
	const AXIS = 22;
	const MAX_LANES = 8;

	let width = $state(0);
	let hover = $state<number | null>(null);

	const items = $derived(
		rows
			.map((r) => {
				const start = parseTime(r.start);
				const end = parseTime(r.end);
				if (!start) return null;
				return {
					row: r,
					label: typeof r.label === "string" ? r.label : String(r.label ?? ""),
					start: start.getTime(),
					end: end && end > start ? end.getTime() : start.getTime(),
					instant: !end || end <= start,
				};
			})
			.filter((i): i is NonNullable<typeof i> => i !== null),
	);

	const lo = $derived(Math.min(...items.map((i) => i.start)));
	const hi = $derived(Math.max(...items.map((i) => i.end)));
	const span = $derived(Math.max(hi - lo, 60_000));
	const x = (t: number) => ((t - lo) / span) * Math.max(0, width - 8) + 4;

	// Lanes in pixels, so an instant's dot claims room too.
	const lanes = $derived(
		width > 0
			? assignLanes(
					items.map((i) => ({ start: x(i.start), end: Math.max(x(i.end), x(i.start) + 8) })),
					2,
				).map((l) => Math.min(l, MAX_LANES - 1))
			: items.map(() => 0),
	);
	const laneCount = $derived(Math.max(1, ...lanes.map((l) => l + 1)));
	const height = $derived(laneCount * LANE + AXIS);

	const DAY = 86_400_000;
	const ticks = $derived.by(() => {
		const count = Math.max(2, Math.floor(width / 90));
		const step = span / (count - 1);
		return Array.from({ length: count }, (_, i) => lo + i * step);
	});
	const tickLabel = (t: number) => {
		const d = new Date(t);
		return span > 2 * DAY ? formatTime(d, { withDate: true, withTime: false }) : formatTime(d);
	};

	const readout = $derived.by(() => {
		if (hover === null) return null;
		const i = items[hover];
		const from = new Date(i.start);
		const sameDay = new Date(i.end).toDateString() === from.toDateString();
		const when = i.instant
			? formatTime(from, { withDate: span > DAY })
			: `${formatTime(from, { withDate: span > DAY })} - ${formatTime(new Date(i.end), { withDate: !sameDay })}`;
		return { label: i.label, when };
	});

	function open(i: number) {
		const ref = rowRef(items[i].row);
		if (ref) windowShellStore.openRouteBeside(ref, items[i].label);
	}
</script>

<div class="timeline" bind:clientWidth={width}>
	<div class="readout" aria-live="polite">
		{#if readout}
			<span class="label">{readout.label}</span><span>{readout.when}</span>
		{:else}
			<span>{items.length} {items.length === 1 ? "item" : "items"}</span>
		{/if}
	</div>
	{#if width > 0 && items.length}
		<svg {width} {height} role="img" aria-label="Timeline of {items.length} items">
			{#each ticks as t}
				<line class="grid" x1={x(t)} x2={x(t)} y1="0" y2={height - AXIS + 4} />
			{/each}
			{#each ticks as t, ti}
				<text
					class="tick"
					x={x(t)}
					y={height - 6}
					text-anchor={ti === 0 ? "start" : ti === ticks.length - 1 ? "end" : "middle"}>{tickLabel(t)}</text
				>
			{/each}
			{#each items as item, i}
				{@const top = lanes[i] * LANE + 3}
				{@const linked = !!rowRef(item.row)}
				<g
					class="item"
					class:dim={hover !== null && hover !== i}
					class:linked
					{...linked ? { role: "button", tabindex: 0, "aria-label": item.label } : {}}
					onpointerenter={() => (hover = i)}
					onpointerleave={() => (hover = null)}
					onfocus={() => (hover = i)}
					onblur={() => (hover = null)}
					onclick={() => open(i)}
					onkeydown={(e) => e.key === "Enter" && open(i)}
				>
					{#if item.instant}
						<circle cx={x(item.start)} cy={top + 7} r="4" />
					{:else}
						{@const w = Math.max(3, x(item.end) - x(item.start))}
						<rect x={x(item.start)} y={top} width={w} height={LANE - 6} rx="3" />
						{#if w > item.label.length * 6 + 12}
							<text class="in" x={x(item.start) + 6} y={top + 10}>{item.label}</text>
						{/if}
					{/if}
				</g>
			{/each}
		</svg>
	{/if}
</div>

<style>
	.timeline {
		width: 100%;
	}
	.readout {
		display: flex;
		flex-wrap: wrap;
		gap: 0.25rem 0.75rem;
		min-height: 1.25rem;
		margin-bottom: 0.375rem;
		font-family: var(--font-sans);
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
	}
	.readout .label {
		color: var(--color-foreground);
	}
	svg {
		display: block;
		overflow: visible;
	}
	.grid {
		stroke: var(--color-border-subtle);
	}
	.tick {
		font-family: var(--font-mono);
		font-size: 9.5px;
		fill: var(--color-foreground-muted);
	}
	.item rect,
	.item circle {
		fill: var(--color-primary);
		transition: opacity 120ms;
	}
	.item circle {
		stroke: var(--color-background);
		stroke-width: 2;
	}
	.item.dim rect,
	.item.dim circle {
		opacity: 0.45;
	}
	.item.linked {
		cursor: pointer;
	}
	.item:focus-visible {
		outline: none;
	}
	.item:focus-visible rect,
	.item:focus-visible circle {
		stroke: var(--color-border-focus);
		stroke-width: 2;
	}
	.in {
		font-family: var(--font-mono);
		font-size: 9.5px;
		fill: var(--color-background);
		pointer-events: none;
	}
</style>
