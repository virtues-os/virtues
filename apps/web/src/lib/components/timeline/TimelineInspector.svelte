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
	import type { InspectorPick as Pick, InspectorRow, InspectorSection } from '$lib/timeline/inspector';
	import { lineAt, lineTime, rowLines } from '$lib/timeline/transcript';
	import TextAction from '$lib/components/TextAction.svelte';
	import { getNearbyPlaces, type NearbyPlace } from '$lib/wiki/api';

	let {
		sections,
		zone,
		playT,
		focus = 'row',
		armed = false,
		onpick,
		onseek,
		onname,
		onmerge,
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
		/** Name an unnamed place; resolves false when the server refused. */
		onname: (placeId: string, name: string) => Promise<boolean>;
		/** Fold an unnamed place into a named one near it. */
		onmerge: (placeId: string, into: NearbyPlace) => Promise<boolean>;
	} = $props();

	// Naming a place, one at a time. The named places within 150 m come first,
	// as "Same as…": a stay's centre drifts, and the place it drifted to is
	// usually the one already named next door.
	let naming = $state<string | null>(null);
	let draft = $state('');
	let nearby = $state<NearbyPlace[]>([]);
	let saving = $state(false);
	let nameError = $state<string | null>(null);

	function startNaming(placeId: string) {
		naming = placeId;
		draft = '';
		nearby = [];
		nameError = null;
		getNearbyPlaces(placeId)
			.then((rows) => {
				if (naming === placeId) nearby = rows;
			})
			.catch(() => {});
	}
	function stopNaming() {
		naming = null;
		nameError = null;
	}
	async function finish(done: Promise<boolean>) {
		saving = true;
		nameError = null;
		const ok = await done.catch(() => false);
		saving = false;
		if (ok) stopNaming();
		else nameError = "Your server couldn't save that. Try again.";
	}
	function saveName(placeId: string) {
		const name = draft.trim();
		if (name && !saving) void finish(onname(placeId, name));
	}
	function sameAs(placeId: string, into: NearbyPlace) {
		if (!saving) void finish(onmerge(placeId, into));
	}

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
			{#each sections as sec, i (sec.kind + sec.s)}
				<div class="group" class:bare={!sec.rows.length} class:cur={i === curSection}>
					<button class="sec" onclick={() => pickSection(i, sec)}>
						<span class="hd">
							<i class="dot {sec.kind}"></i>
							<b>{sec.title}</b>
							<span class="dur">{sec.dur}</span>
							{#if sec.atag}<em class="tag">· {sec.atag}</em>{/if}
						</span>
						{#each sec.notes as line (line)}
							<span class="note">{line}</span>
						{/each}
						{#if secOpen && i === curSection && curRow === null && sec.kind === 'sleep'}
							<span class="note">From Health on your iPhone</span>
						{/if}
					</button>
					{#if sec.kind === 'place' && !sec.named && sec.placeId && !sec.placeId.startsWith('visit:')}
						{@const placeId = sec.placeId}
						{#if naming === placeId}
							<form class="name-form" onsubmit={(e) => (e.preventDefault(), saveName(placeId))}>
								{#if nearby.length}
									<p class="same">
										Same as
										{#each nearby as p, k (p.id)}{#if k > 0}, {/if}<TextAction inline disabled={saving} onclick={() => sameAs(placeId, p)}>{p.name}</TextAction>{/each}?
									</p>
								{/if}
								<!-- svelte-ignore a11y_autofocus -->
								<input
									class="name-input"
									bind:value={draft}
									placeholder="Name this place"
									aria-label="Name this place"
									autofocus
									onkeydown={(e) => e.key === 'Escape' && stopNaming()}
								/>
								<span class="name-actions">
									<TextAction type="submit" disabled={!draft.trim()} loading={saving}>Save the name</TextAction>
									<TextAction quiet onclick={stopNaming}>Cancel</TextAction>
								</span>
								{#if nameError}<p class="name-error">{nameError}</p>{/if}
							</form>
						{:else}
							<span class="name-door" class:shown={i === curSection}>
								<TextAction onclick={() => startNaming(placeId)}>Name this place</TextAction>
							</span>
						{/if}
					{/if}
					{#each sec.rows as row (row.kind + row.s)}
						<div class="row" class:cur={row === curRow}>
							<button class="row-hd" onclick={() => pickRow(row)}>
								<span class="t">{time(row.s)}</span>
								<span class="hd">
									<i class="dot {row.kind}"></i>
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
													{#if q.spk}<span class="spk">{q.spk}</span>{' '}{/if}{q.txt}
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
	/* The day as one column on the right, the page's surface in a hairline
	   like every card over the map. A pane too narrow for the date card beside
	   it starts the inspector under the date card (TimelineView sets
	   --inspector-top). */
	.inspector {
		position: absolute;
		top: var(--inspector-top, 16px);
		bottom: 16px;
		z-index: 10;
		right: 16px;
		width: min(384px, 42%);
		display: flex;
		flex-direction: column;
		overflow: hidden;
		border-radius: var(--tile-radius);
		background: var(--color-surface);
		border: 1px solid var(--color-border);
	}
	/* Narrow pane: a sheet across the bottom third rather than a column. */
	:global(.timeline.narrow) .inspector {
		top: auto;
		left: 16px;
		width: auto;
		height: 34%;
	}
	.scroll {
		overflow-y: auto;
		padding: 4px 0 8px;
		background: inherit;
	}
	.group {
		position: relative;
		background: inherit;
	}
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
		   pinned from at rest. */
		top: -1px;
		z-index: 2;
		background: var(--color-surface);
		padding: 16px 16px 8px;
	}
	/* A hairline only while rows are passing beneath it. */
	.sec:global(.stuck) {
		border-bottom: 1px solid var(--color-border);
	}
	.bare .sec {
		padding-bottom: 12px;
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
	.sec:hover,
	.row:hover {
		background: var(--hover-bg);
	}
	/* Where the playhead is: an edge in the accent, which means now. */
	.group.cur::before,
	.row.cur::before {
		content: '';
		position: absolute;
		left: 0;
		top: 8px;
		bottom: 8px;
		width: 3px;
		border-radius: 999px;
		background: var(--c-sel);
		z-index: 3;
	}
	.group.cur::before {
		opacity: 0.35;
	}
	.hd {
		display: flex;
		align-items: baseline;
		gap: 8px;
		min-width: 0;
	}
	/* What a stretch was, by form: a stay is filled ink, a drive a hollow
	   ring, a gap a dashed one, a night half-filled, a conversation a small
	   dot. No hue (design-grammar §5). */
	.dot {
		flex: 0 0 8px;
		width: 8px;
		height: 8px;
		border-radius: 50%;
		position: relative;
		top: -1px;
		box-sizing: border-box;
	}
	.dot.place {
		background: var(--c-place);
	}
	.dot.transit,
	.dot.walk {
		border: 1.5px solid var(--c-move);
	}
	.dot.gap {
		border: 1.5px dashed var(--c-gap);
	}
	.dot.sleep {
		border: 1.5px solid var(--c-rest-ink);
		background: linear-gradient(90deg, var(--c-rest-ink) 50%, transparent 50%);
	}
	.dot.conversation {
		flex-basis: 6px;
		width: 6px;
		height: 6px;
		background: var(--c-voice);
	}
	/* Two voices: a section names where you were in the serif, a row says
	   what happened in the sans; durations and times are the caption. */
	.hd b {
		flex: 1 1 auto;
		min-width: 0;
		font-family: var(--font-sans);
		font-size: 14px;
		font-weight: 400;
		line-height: 1.3;
		color: var(--color-foreground);
	}
	/* A conversation's title is the start of its summary: two lines at most,
	   and the open transcript holds the rest. */
	.row .hd b {
		display: -webkit-box;
		-webkit-line-clamp: 2;
		line-clamp: 2;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}
	.sec .hd b {
		font-family: var(--font-serif-ui);
		font-size: 18px;
		line-height: 1.2;
	}
	/* Nothing filed here (a drive, a gap): a quieter line. */
	.bare .sec .hd b {
		color: var(--color-foreground-muted);
	}
	.row.cur .hd b {
		color: var(--color-foreground);
	}
	.dur,
	.t {
		font-family: var(--font-sans);
		font-size: 13px;
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
	}
	.t {
		text-align: right;
	}
	.note {
		display: block;
		margin: 4px 0 0 16px;
		font-family: var(--font-sans);
		font-size: 13px;
		line-height: 1.3;
		color: var(--color-foreground-muted);
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
	/* A quiet stay's audio note, after its duration. */
	.tag {
		flex: 0 0 auto;
		font-style: normal;
		font-family: var(--font-sans);
		font-size: 13px;
		color: var(--color-foreground-subtle);
		white-space: nowrap;
	}
	/* The open conversation, under its row and out of the time gutter: the
	   voices, the names mentioned, then the lines. */
	.open {
		display: block;
		margin-top: 4px;
	}
	.sub,
	.people {
		display: block;
		font-family: var(--font-sans);
		font-size: 13px;
		color: var(--color-foreground-muted);
	}
	.people {
		margin-top: 4px;
	}
	.lines {
		margin-top: 8px;
		padding-top: 8px;
		border-top: 1px solid var(--color-border);
	}
	.line {
		padding: 4px 0;
		font-family: var(--font-sans);
		font-size: 13px;
		line-height: 1.45;
		color: var(--color-foreground-muted);
	}
	.line:hover {
		color: var(--color-foreground);
	}
	/* The line being spoken at the playhead. */
	.line.cur {
		color: var(--color-foreground);
	}
	/* Naming a place: a quiet verb under the stay at the playhead (and under
	   any stay you point at), then a field where the name goes. */
	.name-door {
		display: none;
		padding: 0 16px 8px 32px;
		font-size: 13px;
	}
	.name-door.shown,
	.group:hover .name-door,
	.name-door:focus-within {
		display: block;
	}
	.name-form {
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 0 16px 12px 32px;
	}
	.same {
		margin: 0;
		font-size: 13px;
		color: var(--color-foreground-muted);
	}
	.name-input {
		width: 100%;
		padding: 8px 12px;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		background: var(--color-background);
		color: var(--color-foreground);
		font-family: var(--font-sans);
		font-size: 14px;
	}
	.name-input:focus {
		outline: none;
		border-color: var(--color-primary);
	}
	.name-actions {
		display: flex;
		gap: 16px;
		font-size: 14px;
	}
	.name-error {
		margin: 0;
		font-size: 13px;
		color: var(--color-foreground);
	}
	.spk {
		font-weight: 600;
		color: var(--color-foreground);
	}
</style>
