<!--
	TimelineRail.svelte - the day as one column, in the prototype's two tiers
	(dayback/src/main.js:1600-1637, dayback/index.html:912-953): a section for
	where you were (a stay, a drive, a signal gap, In Bed), titled with a
	duration only, and under it the moments that happened there, each with its
	time in the gutter. A section header pins while its rows scroll past.

	A click takes the map there (`onpick`). Until the Timeline has a playhead,
	the last thing picked wears the prototype's "live" look.
-->
<script lang="ts">
	import type { RailPick as Pick, RailRow, RailSection, SectionKind } from '$lib/timeline/rail';

	let {
		sections,
		zone,
		picked = null,
		onpick,
	}: { sections: RailSection[]; zone: string; picked?: Pick | null; onpick: (p: Pick) => void } = $props();

	let rail = $state<HTMLElement | null>(null);
	export function width(): number {
		return rail?.offsetWidth ?? 0;
	}

	// One hue per kind, the same as the map's and the scrubber's (lib/timeline/colours.ts).
	const DOT: Record<SectionKind | RailRow['kind'], string> = {
		place: 'var(--c-place)',
		transit: 'var(--c-move)',
		gap: 'var(--c-gap)',
		sleep: 'var(--c-rest-ink)',
		conversation: 'var(--c-voice)',
		walk: 'var(--c-move)',
	};

	const time = (ms: number) =>
		new Date(ms).toLocaleTimeString('en-US', { timeZone: zone, hour: 'numeric', minute: '2-digit' });
	const isPicked = (x: { s: number; e: number }, kind: Pick['kind']) =>
		picked !== null && picked.kind === kind && picked.s === x.s && picked.e === x.e;
	// A section is live while the pick falls inside it: a row picked inside a
	// stay lights the stay too, and the highlight never climbs back up the list.
	const holds = (sec: RailSection) => picked !== null && sec.s <= picked.s && picked.s < sec.e;
</script>

{#if sections.length}
	<section class="rail" bind:this={rail} aria-label="The day, stay by stay">
		<div class="scroll">
			{#each sections as sec (sec.kind + sec.s)}
				<div class="group" class:bare={!sec.rows.length} class:cur={holds(sec)}>
					<button class="sec" class:cur={isPicked(sec, sec.kind)} onclick={() => onpick({ kind: sec.kind, s: sec.s, e: sec.e })}>
						<span class="hd">
							<i class="dot" style="background: {DOT[sec.kind]}"></i>
							<b>{sec.title}</b>
							<span class="dur">{sec.dur}</span>
						</span>
						{#each sec.notes as line (line)}
							<span class="note">{line}</span>
						{/each}
					</button>
					{#each sec.rows as row (row.kind + row.s)}
						<button class="row" class:cur={isPicked(row, row.kind)} onclick={() => onpick({ kind: row.kind, s: row.s, e: row.e })}>
							<span class="t">{time(row.s)}</span>
							<span class="hd">
								<i class="dot" style="background: {DOT[row.kind]}"></i>
								<b>{row.title}</b>
								<span class="dur">{row.dur}</span>
							</span>
						</button>
					{/each}
				</div>
			{/each}
		</div>
	</section>
{/if}

<style>
	.rail {
		position: absolute;
		top: 16px;
		right: 16px;
		width: min(384px, 42%);
		max-height: calc(100% - 32px);
		display: flex;
		flex-direction: column;
		overflow: hidden;
		border-radius: 12px;
		/* The prototype's tile, as the view's other tiles (dayback/index.html:599-601). */
		background: var(--c-tile);
		border: 1px solid color-mix(in srgb, var(--color-foreground) 7%, transparent);
		/* design-ok: the Timeline follows the Dayback prototype's look (owner's call, 2026-09-30) */
		box-shadow: var(--tile-shadow);
	}
	.scroll {
		overflow-y: auto;
		padding-bottom: 8px;
		background: inherit;
	}
	.group {
		position: relative;
		background: inherit;
	}
	/* Where you are: a soft edge down the whole stay (index.html:915). */
	.group.cur::before {
		content: '';
		position: absolute;
		left: 0;
		top: 8px;
		bottom: 8px;
		width: 3px;
		/* design-ok: the Dayback prototype's 2 px mark (owner's call, 2026-09-30) */
		border-radius: 2px;
		background: var(--c-place);
		opacity: 0.3;
	}
	/* The prototype's 2 px marks below (edge-strip ends, a baseline nudge) sit
	   off the page grammar's scale; each carries design-ok for that reason. */
	button {
		display: block;
		width: 100%;
		border: 0;
		background: transparent;
		font: inherit;
		text-align: left;
		color: inherit;
		cursor: pointer;
	}
	/* The section header pins while its rows scroll past, like Contacts. */
	.sec {
		position: sticky;
		top: 0;
		z-index: 2;
		background: var(--c-tile);
		padding: 16px 16px 8px;
	}
	.bare .sec {
		padding-bottom: 12px;
	}
	.bare.cur .sec {
		background: color-mix(in srgb, var(--c-place) 7%, var(--c-tile));
	}
	.row {
		position: relative;
		display: grid;
		grid-template-columns: 56px 1fr;
		gap: 8px;
		padding: 8px 16px 8px 32px;
	}
	.row:hover {
		background: color-mix(in srgb, var(--color-foreground) 4%, transparent);
	}
	/* The live row: tinted, with the place-blue edge (index.html:928-929). */
	.row.cur {
		background: color-mix(in srgb, var(--c-place) 7%, transparent);
	}
	.row.cur::before {
		content: '';
		position: absolute;
		left: 0;
		top: 8px;
		bottom: 8px;
		width: 3px;
		/* design-ok: the Dayback prototype's 2 px mark (owner's call, 2026-09-30) */
		border-radius: 2px;
		background: var(--c-place);
	}
	.hd {
		display: flex;
		align-items: baseline;
		gap: 8px;
		min-width: 0;
	}
	.dot {
		flex: 0 0 8px;
		width: 8px;
		height: 8px;
		border-radius: 50%;
		position: relative;
		top: -1px;
	}
	.hd b {
		flex: 1 1 auto;
		min-width: 0;
		font-family: var(--font-sans);
		font-size: 13px;
		font-weight: 500;
		line-height: 1.25;
		color: var(--color-foreground);
	}
	.row.cur .hd b {
		font-weight: 600;
	}
	.sec .hd b {
		font-size: 14px;
		font-weight: 700;
	}
	.sec:hover .hd b {
		color: var(--c-place);
	}
	/* Nothing filed here (a drive, a gap): a quieter line. */
	.bare .sec .hd b {
		font-weight: 500;
		color: var(--color-foreground-muted);
	}
	.dur,
	.t {
		font-family: var(--font-sans);
		font-size: 11px;
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
	}
	.t {
		text-align: right;
		/* design-ok: the Dayback prototype's 2 px mark (owner's call, 2026-09-30) */
		padding-top: 2px;
	}
	.note {
		display: block;
		/* design-ok: the Dayback prototype's 2 px mark (owner's call, 2026-09-30) */
		margin: 2px 0 0 16px;
		font-family: var(--font-sans);
		font-size: 12px;
		line-height: 1.3;
		color: var(--color-foreground-muted);
	}
</style>
