<script lang="ts">
	/**
	 * NotebookGraphBand — the entities referenced across a notebook's members,
	 * sitting above the member list as a *filter*, not as decoration.
	 *
	 * Clicking a node filters the list below to the members that reference it;
	 * clicking it again clears. That is the whole justification for the band
	 * occupying the top of the page: a graph nobody can act on is trivial at
	 * seven items and a hairball at two hundred.
	 *
	 * Layout is a deterministic ellipse rather than a force simulation — with a
	 * dozen nodes it reads more clearly, it never jitters between loads, and it
	 * costs nothing.
	 */
	import Icon from '$lib/components/Icon.svelte';
	import type { NotebookGraphNode, NotebookGraphEdge } from '$lib/api/client';

	interface Props {
		nodes: NotebookGraphNode[];
		edges: NotebookGraphEdge[];
		selected: string | null;
		onSelect: (url: string | null) => void;
	}

	let { nodes, edges, selected, onSelect }: Props = $props();

	let open = $state(true);

	/** Beyond this the ellipse stops being readable; the overflow is stated, never silent. */
	const MAX_NODES = 12;

	const shown = $derived(nodes.slice(0, MAX_NODES));
	const hiddenCount = $derived(Math.max(0, nodes.length - MAX_NODES));

	const W = 620;
	const H = 190;

	type Placed = NotebookGraphNode & { cx: number; cy: number; r: number };

	const placed = $derived.by<Placed[]>(() => {
		const n = shown.length;
		if (n === 0) return [];
		const max = Math.max(...shown.map((s) => s.item_urls.length));
		// A lone node sits centred; otherwise spread around an ellipse.
		if (n === 1) {
			return [{ ...shown[0], cx: W / 2, cy: H / 2 - 8, r: 15 }];
		}
		const rx = W / 2 - 96;
		const ry = H / 2 - 44;
		return shown.map((node, i) => {
			const angle = -Math.PI / 2 + (i * 2 * Math.PI) / n;
			const weight = max > 1 ? node.item_urls.length / max : 1;
			return {
				...node,
				cx: W / 2 + rx * Math.cos(angle),
				cy: H / 2 - 8 + ry * Math.sin(angle),
				r: 9 + weight * 7
			};
		});
	});

	const byUrl = $derived(new Map(placed.map((p) => [p.url, p])));

	// Only edges whose endpoints both survived the MAX_NODES cut can be drawn.
	const drawn = $derived(
		edges
			.map((e) => ({ e, a: byUrl.get(e.source), b: byUrl.get(e.target) }))
			.filter((x): x is { e: NotebookGraphEdge; a: Placed; b: Placed } => !!x.a && !!x.b)
	);

	function iconFor(entityType: string): string {
		const map: Record<string, string> = {
			person: 'ri:user-line',
			place: 'ri:map-pin-line',
			org: 'ri:building-line',
			thing: 'ri:shapes-line'
		};
		return map[entityType] ?? 'ri:circle-line';
	}

	function pick(url: string) {
		onSelect(selected === url ? null : url);
	}
</script>

<section class="band">
	<button
		class="band-head"
		class:closed={!open}
		type="button"
		aria-expanded={open}
		onclick={() => (open = !open)}
	>
		<Icon icon="ri:arrow-down-s-line" width="15" class="chev" />
		<span class="band-title">Referenced across these items</span>
		<span class="band-count font-mono">
			{nodes.length}
			{nodes.length === 1 ? 'entity' : 'entities'}
		</span>
	</button>

	{#if open}
		<div class="band-body">
			<svg class="graph" viewBox="0 0 {W} {H}" role="group" aria-label="Entities referenced across this notebook. Select one to filter the list.">
				<g class="edges">
					{#each drawn as d (d.e.source + d.e.target)}
						<line
							x1={d.a.cx}
							y1={d.a.cy}
							x2={d.b.cx}
							y2={d.b.cy}
							class:dim={!!selected && selected !== d.e.source && selected !== d.e.target}
						/>
					{/each}
				</g>
				<g class="nodes">
					{#each placed as p (p.url)}
						<g
							class="node"
							class:sel={selected === p.url}
							class:dim={!!selected && selected !== p.url}
							role="button"
							tabindex="0"
							aria-pressed={selected === p.url}
							aria-label="{p.name}, {p.item_urls.length} {p.item_urls.length === 1 ? 'item' : 'items'}"
							onclick={() => pick(p.url)}
							onkeydown={(e) => {
								if (e.key === 'Enter' || e.key === ' ') {
									e.preventDefault();
									pick(p.url);
								}
							}}
						>
							<circle cx={p.cx} cy={p.cy} r={p.r} />
							<text x={p.cx} y={p.cy + p.r + 13}>{p.name}</text>
						</g>
					{/each}
				</g>
			</svg>

			<p class="band-hint">
				{#if selected}
					Filtered to <strong>{byUrl.get(selected)?.name ?? 'one entity'}</strong>.
					<button class="link-btn" type="button" onclick={() => onSelect(null)}>Clear</button>
				{:else}
					Select an entity to filter the list below. Built from what you filed and what your
					pages link to — nothing is inferred from document text.
				{/if}
				{#if hiddenCount > 0}
					<span class="overflow">Showing the {MAX_NODES} most-referenced; {hiddenCount} more not drawn.</span>
				{/if}
			</p>
		</div>
	{/if}
</section>

<style>
	.band {
		border: 1px solid var(--color-border);
		border-radius: 10px;
		background: var(--color-surface-elevated);
		overflow: hidden;
	}
	.band-head {
		display: flex;
		align-items: center;
		gap: 9px;
		width: 100%;
		text-align: left;
		padding: 0.55rem 0.8rem;
		border: none;
		background: transparent;
		font: inherit;
		color: var(--color-foreground-muted);
		cursor: pointer;
	}
	.band-head:hover { color: var(--color-foreground); }
	.band-head:focus-visible { outline: 2px solid var(--room-accent, var(--color-primary)); outline-offset: -2px; }
	.band-head :global(.chev) { transition: transform 0.16s ease; flex-shrink: 0; }
	.band-head.closed :global(.chev) { transform: rotate(-90deg); }
	.band-title { flex: 1; font-size: 0.8125rem; }
	.band-count {
		font-size: 10px;
		letter-spacing: 0.04em;
		color: var(--color-foreground-subtle);
	}

	.band-body { padding: 0 1rem 0.9rem; }
	.graph { width: 100%; height: auto; display: block; }

	.edges line {
		stroke: var(--color-border-strong, var(--color-border));
		stroke-width: 1.2;
		transition: opacity 0.14s ease;
	}
	.edges line.dim { opacity: 0.3; }

	.node { cursor: pointer; transition: opacity 0.14s ease; }
	.node.dim { opacity: 0.42; }
	.node circle {
		fill: color-mix(in srgb, var(--room-accent, var(--color-primary)) 24%, transparent);
		stroke: var(--room-accent, var(--color-primary));
		stroke-width: 1.2;
		transition: fill 0.14s ease;
	}
	.node:hover circle { fill: color-mix(in srgb, var(--room-accent, var(--color-primary)) 44%, transparent); }
	.node.sel circle {
		fill: var(--room-accent, var(--color-primary));
		stroke: var(--room-accent, var(--color-primary));
	}
	.node text {
		fill: var(--color-foreground-subtle);
		font-family: var(--font-mono);
		font-size: 9.5px;
		text-anchor: middle;
	}
	.node.sel text { fill: var(--color-foreground); }
	.node:focus-visible { outline: none; }
	.node:focus-visible circle { stroke-width: 3; }

	.band-hint {
		margin: 0.2rem 0 0;
		font-size: 0.75rem;
		line-height: 1.55;
		color: var(--color-foreground-subtle);
	}
	.band-hint strong { color: var(--color-foreground); font-weight: 600; }
	.link-btn {
		border: none;
		background: none;
		padding: 0;
		font: inherit;
		font-size: inherit;
		color: var(--room-accent, var(--color-primary));
		text-decoration: underline;
		cursor: pointer;
	}
	.overflow { display: block; margin-top: 2px; opacity: 0.85; }
</style>
