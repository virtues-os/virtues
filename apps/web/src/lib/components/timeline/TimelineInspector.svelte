<!--
	TimelineInspector.svelte - the day as one column, in the prototype's two tiers
	(dayback/src/main.js:1600-1637, dayback/index.html:912-953): a section for
	where you were (a stay, a drive, a signal gap, In Bed), titled with a
	duration only, and under it the moments that happened there, each with its
	time in the gutter. A section header pins while its rows scroll past.

	A click parks the playhead and takes the map there (`onpick`). The inspector
	follows the playhead (main.js:1607-1615): the latest-begun section holding
	it is where you are, the latest-begun row holding it is what's happening.
-->
<script lang="ts">
	import { untrack } from 'svelte';
	import type { InspectorPick as Pick, InspectorRow, InspectorSection, SectionKind } from '$lib/timeline/inspector';
	import { lineAt, lineTime, rowLines, spkIdx } from '$lib/timeline/transcript';

	let {
		sections,
		zone,
		playT,
		focus = 'row',
		armed = false,
		onpick,
		onseek,
	}: {
		sections: InspectorSection[];
		zone: string;
		playT: number;
		/** What the user last pointed at: 'sec' after a section click keeps the
		 *  inspector on that section; anything else hands focus back to the moment. */
		focus?: 'row' | 'sec';
		/** The user has moved the playhead in this day: until then nothing opens itself. */
		armed?: boolean;
		onpick: (p: Pick) => void;
		/** A transcript line was picked: park the playhead at its moment. */
		onseek: (t: number) => void;
	} = $props();

	let inspector = $state<HTMLElement | null>(null);
	let scroller = $state<HTMLElement | null>(null);
	export function width(): number {
		return inspector?.offsetWidth ?? 0;
	}
	/** Open a conversation row's transcript, as a click on the row does: a
	 *  moment picked on the map opens its row here too. */
	export function expand(s: number) {
		const row = sections.flatMap((x) => x.rows).find((r) => r.s === s);
		if (row?.convs.length) open = s;
	}

	// One hue per kind, the same as the map's and the scrubber's (lib/timeline/colours.ts).
	const DOT: Record<SectionKind | InspectorRow['kind'], string> = {
		place: 'var(--c-place)',
		transit: 'var(--c-move)',
		gap: 'var(--c-gap)',
		sleep: 'var(--c-rest-ink)',
		conversation: 'var(--c-voice)',
		walk: 'var(--c-move)',
	};

	const time = (ms: number) =>
		new Date(ms).toLocaleTimeString('en-US', { timeZone: zone, hour: 'numeric', minute: '2-digit' });
	const holds = (x: { s: number; e: number }) => x.s <= playT && playT < x.e;
	// Sections and rows are in start order, so the last one holding the
	// playhead is the latest begun; the highlight never climbs back up.
	const curSection = $derived(sections.findLastIndex(holds));
	const curRow = $derived(
		sections
			.flatMap((s) => s.rows)
			.sort((a, b) => a.s - b.s)
			.findLast(holds) ?? null,
	);

	/** The one conversation row whose transcript is open, by its start
	 *  (main.js:1548, 1634-1636); a fresh day starts with none open. */
	let open = $state<number | null>(null);
	/** The live section's detail (In Bed's source). A new live section opens
	 *  itself once the day has been touched; a second click folds it
	 *  (main.js:1610, 1637). */
	let secOpen = $state(false);
	let lastSec = -2;
	$effect(() => {
		void sections;
		untrack(() => {
			open = null;
			secOpen = false;
			lastSec = -2;
		});
	});
	$effect(() => {
		const c = curSection;
		untrack(() => {
			if (c !== lastSec) {
				lastSec = c;
				secOpen = armed;
			}
		});
	});

	function pickSection(i: number, sec: InspectorSection) {
		if (i === curSection) secOpen = !secOpen;
		open = null;
		onpick({ kind: sec.kind, s: sec.s, e: sec.e });
	}
	function pickRow(row: InspectorRow) {
		if (row.convs.length) open = open === row.s ? null : row.s;
		onpick({ kind: row.kind, s: row.s, e: row.e });
	}

	// The open conversation (main.js:1616-1622): how many voices, the names
	// mentioned (never "present"), and its lines as turns, the one spoken at
	// the playhead lit - an estimate, as the recorder keeps no per-line times.
	const openRow = $derived(sections.flatMap((s) => s.rows).find((r) => r.s === open) ?? null);
	const openLines = $derived(openRow ? rowLines(openRow.convs) : []);
	const openSpeakers = $derived(openRow ? Math.max(0, ...openRow.convs.map((c) => c.speaker_count)) : 0);
	const openPeople = $derived(openRow ? [...new Set(openRow.convs.flatMap((c) => c.people))] : []);
	const liveLine = $derived(openRow && openLines.length ? lineAt(openRow.s, openRow.e, openLines.length, playT) : -1);

	// One scroll surface, kept on the finest live thing (main.js:1627-1633):
	// the pointed-at section, else the spoken line, else the live row, else
	// the live section. It glides only when that thing is out of view, and
	// once per target.
	let glide = -1;
	$effect(() => {
		const box = scroller;
		const sec = curSection;
		const row = curRow;
		void liveLine;
		if (!box) return;
		const top = box.getBoundingClientRect().top;
		const at = (n: HTMLElement) => n.getBoundingClientRect().top - top + box.scrollTop;
		const sT = box.scrollTop;
		const h = box.clientHeight;
		const group = sec >= 0 ? box.querySelectorAll<HTMLElement>('.group')[sec] : undefined;
		const lineEl = box.querySelector<HTMLElement>('.line.cur');
		const rowEl = row ? box.querySelector<HTMLElement>('.row.cur') : null;
		const sectionView = (g: HTMLElement) => {
			const t = at(g);
			// A section counts as seen while any of it is in the box: its pinned header shows.
			return { seen: t <= sT + h - 40 && t + g.offsetHeight >= sT + 40, want: t - h * 0.28 };
		};
		let view: { seen: boolean; want: number } | null = null;
		if (focus === 'sec' && group) view = sectionView(group);
		else if (lineEl) {
			const t = at(lineEl);
			view = { seen: t >= sT + 24 && t <= sT + h - 48, want: t - h * 0.45 };
		} else if (rowEl) {
			const t = at(rowEl);
			view = { seen: t >= sT && t <= sT + h - 40, want: t - h * 0.28 };
		} else if (group) view = sectionView(group);
		if (!view || view.seen) {
			glide = -1;
			return;
		}
		const want = Math.max(0, Math.round(view.want));
		if (want !== glide) {
			glide = want;
			box.scrollTo({ top: want, behavior: 'smooth' });
		}
	});

	// A pinned header is one pressed against the top of the box, poking 1 px
	// out of it: only then does it take a hairline (main.js:1605).
	$effect(() => {
		const box = scroller;
		void sections;
		if (!box || typeof IntersectionObserver === 'undefined') return;
		const io = new IntersectionObserver(
			(es) => {
				for (const e of es) {
					const pinned = e.intersectionRatio < 1 && e.boundingClientRect.top <= (e.rootBounds?.top ?? 0) + 1;
					(e.target as HTMLElement).classList.toggle('stuck', pinned);
				}
			},
			{ root: box, threshold: [1] },
		);
		box.querySelectorAll('.sec').forEach((n) => io.observe(n));
		return () => io.disconnect();
	});
</script>

{#if sections.length}
	<section class="inspector" bind:this={inspector} aria-label="The day, stay by stay">
		<div class="scroll" bind:this={scroller}>
			<!-- The day's standout leads the inspector once the Timeline can score
			     what was unusual (the prototype's "What stood out" card,
			     main.js:1615-1617). Until then its place is held, quietly, and
			     says so, never with a guess. -->
			<div class="standout" aria-disabled="true">
				<span class="standout-eye">What stood out today</span>
				<span class="standout-soon">Coming soon</span>
			</div>
			{#each sections as sec, i (sec.kind + sec.s)}
				<div class="group" class:bare={!sec.rows.length} class:cur={i === curSection}>
					<button class="sec" onclick={() => pickSection(i, sec)}>
						<span class="hd">
							<i class="dot" style="background: {DOT[sec.kind]}"></i>
							<b>{sec.title}</b>
							<span class="dur">{sec.dur}</span>
							{#if sec.atag}<em class="tag">· {sec.atag}</em>{/if}
						</span>
						{#each sec.notes as line (line)}
							<span class="note">{line}</span>
						{/each}
						{#if secOpen && i === curSection && curRow === null && sec.kind === 'sleep'}
							<span class="note">Source · {sec.src === 'healthkit' ? 'iPhone (Health, In Bed)' : 'quiet hours, no conversation'}</span>
						{/if}
					</button>
					{#each sec.rows as row (row.kind + row.s)}
						<div class="row" class:cur={row === curRow}>
							<button class="row-hd" onclick={() => pickRow(row)}>
								<span class="t">{time(row.s)}</span>
								<span class="hd">
									<i class="dot" style="background: {DOT[row.kind]}"></i>
									<b>{row.title}</b>
									<span class="dur">{row.dur}</span>
									{#if row.convs.length}<span class="chev" class:open={row.s === open} aria-hidden="true">›</span>{/if}
								</span>
							</button>
							{#if row.s === open && openRow}
								<div class="open">
									<span class="sub">{openSpeakers} speaker{openSpeakers !== 1 ? 's' : ''}</span>
									{#if openPeople.length}<span class="people">Mentioned {openPeople.join(', ')}</span>{/if}
									{#if openLines.length}
										<div class="lines">
											{#each openLines as q, li (li)}
												<button class="line" class:cur={li === liveLine} onclick={() => onseek(lineTime(row.s, row.e, li, openLines.length))}>
													{#if q.spk}<span class="spk" style="color: var(--c-spk{spkIdx(q.spk)})">{q.spk}</span>{' '}{/if}{q.txt}
												</button>
											{/each}
										</div>
									{/if}
								</div>
							{/if}
						</div>
					{/each}
				</div>
			{/each}
		</div>
	</section>
{/if}

<style>
	.inspector {
		position: absolute;
		/* Full height on the right, above the map's markers; a pane too
		   narrow for the date card beside it starts the inspector under the top
		   cards instead (TimelineView sets --inspector-top). */
		top: var(--inspector-top, 16px);
		bottom: 16px;
		z-index: 10;
		right: 16px;
		width: min(384px, 42%);
		display: flex;
		flex-direction: column;
		overflow: hidden;
		/* The Timeline's one material (TimelineView's tile variables). */
		border-radius: var(--tile-radius);
		background: var(--tile-bg);
		border: var(--tile-border);
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
		/* Pinned, it pokes 1 px out of the box: that is how the observer tells
		   pinned from at rest (index.html:916). */
		top: -1px;
		z-index: 2;
		background: var(--c-tile);
		padding: 16px 16px 8px;
	}
	/* A hairline only while rows are passing beneath it (index.html:917). */
	.sec:global(.stuck) {
		/* design-ok: the Dayback prototype's pinned-header hairline (owner's call, 2026-09-30) */
		box-shadow: 0 1px 0 color-mix(in srgb, var(--color-foreground) 8%, transparent);
	}
	.bare .sec {
		padding-bottom: 12px;
	}
	.bare.cur .sec {
		background: color-mix(in srgb, var(--c-place) 7%, var(--c-tile));
	}
	.row {
		position: relative;
		padding: 8px 16px 8px 32px;
	}
	.row-hd {
		display: grid;
		grid-template-columns: 56px 1fr;
		gap: 8px;
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
	/* The standout's held place, at the head of the inspector: the prototype's
	   eyebrow (index.html:938-942), greyed, with a quiet line under it. */
	.standout {
		display: flex;
		flex-direction: column;
		gap: 4px;
		padding: 16px 16px 12px;
		border-bottom: 1px solid color-mix(in srgb, var(--color-foreground) 8%, transparent);
	}
	.standout-eye {
		font-family: var(--font-sans);
		/* design-ok: the Dayback prototype's standout eyebrow (owner's call, 2026-09-30) */
		font-size: 10px;
		font-weight: 600;
		letter-spacing: 0.11em;
		text-transform: uppercase;
		color: var(--color-foreground-subtle);
	}
	.standout-soon {
		font-family: var(--font-sans);
		font-size: 12px;
		color: var(--color-foreground-subtle);
	}
	/* A conversation row opens its transcript: the chevron says so, and
	   turns down while it is open. */
	.chev {
		flex: 0 0 auto;
		font-size: 14px;
		line-height: 1;
		color: var(--color-foreground-subtle);
		transition: transform 0.18s ease;
	}
	.chev.open {
		transform: rotate(90deg);
	}
	@media (prefers-reduced-motion: reduce) {
		.chev {
			transition: none;
		}
	}
	/* A quiet stay's audio note, after its duration (index.html:936). */
	.tag {
		flex: 0 0 auto;
		font-style: normal;
		font-family: var(--font-sans);
		font-size: 11px;
		font-weight: 500;
		color: var(--color-foreground-subtle);
		white-space: nowrap;
	}
	/* The open conversation, under its row and out of the time gutter
	   (index.html:944-953): the voices, the names mentioned, then the lines. */
	.open {
		display: block;
		/* design-ok: the Dayback prototype's 2 px mark (owner's call, 2026-09-30) */
		margin-top: 2px;
	}
	.sub,
	.people {
		display: block;
		font-family: var(--font-sans);
		font-size: 11px;
		color: var(--color-foreground-muted);
	}
	.people {
		/* design-ok: the Dayback prototype's line spacing (owner's call, 2026-09-30) */
		margin-top: 4px;
	}
	.lines {
		/* design-ok: the Dayback prototype's transcript spacing (owner's call, 2026-09-30) */
		margin-top: 9px;
		/* design-ok: the Dayback prototype's transcript spacing (owner's call, 2026-09-30) */
		padding-top: 7px;
		border-top: 1px solid color-mix(in srgb, var(--color-foreground) 8%, transparent);
	}
	.line {
		/* design-ok: the Dayback prototype's transcript line (owner's call, 2026-09-30) */
		padding: 2.5px 0;
		font-family: var(--font-sans);
		font-size: 12px;
		line-height: 1.42;
		color: var(--color-foreground-muted);
	}
	.line:hover {
		color: var(--color-foreground);
	}
	/* The line being spoken at the playhead. */
	.line.cur {
		color: var(--color-foreground);
		font-weight: 600;
	}
</style>
