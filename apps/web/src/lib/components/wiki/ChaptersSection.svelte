<!--
	ChaptersSection.svelte

	The chapters of the life: the person's own gapless partition of it into
	named eras, written in the narrative interview and never inferred
	(wiki_chapters, migration 0015). Its own wiki room, separate from the
	narrative-identity document, because chapters are the structure the rest
	of the record hangs on and the document is prose. Each chapter has its own
	article page, which its title opens beside.

	A ribbon across the top draws each chapter as wide as its years, so the
	shape of the life reads before any of the words. Below it, each chapter
	has its number, its title, the person's sentence about it, what ended it,
	and its years and length. The owner's words are shown with a capital
	first letter, since they are often typed in lowercase; what is stored is
	never rewritten. A sentence that only repeats the title, or a "How it
	ended" that repeats the sentence, is left out.
-->

<script lang="ts">
	import { onMount } from 'svelte';
	import IconButton from '$lib/components/IconButton.svelte';
	import { contextMenu } from '$lib/stores/contextMenu.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { getChapters, listYears, updateChapter, deleteChapter, type ChapterApi } from '$lib/wiki/api';

	let loading = $state(true);
	let chapters = $state<ChapterApi[]>([]);
	/** The first year your server has a recorded day in, once known. */
	let firstRecordedYear = $state<number | null>(null);

	async function load() {
		try {
			chapters = await getChapters();
		} finally {
			loading = false;
		}
	}

	onMount(() => {
		void load();
		void listYears()
			.then((ys) => {
				const recorded = ys.filter((y) => y.days_recorded > 0).map((y) => y.year);
				firstRecordedYear = recorded.length ? Math.min(...recorded) : null;
			})
			.catch(() => {
				// absent-ok: without the years, no chapter is marked as before the record.
			});
	});

	/** A chapter whose page hasn't been made yet: said so for a moment rather than a dead click. */
	let missingNote = $state<string | null>(null);
	let missingTimer: ReturnType<typeof setTimeout> | undefined;

	let editing = $state<string | null>(null);
	let renameDraft = $state('');
	let chapterError = $state<string | null>(null);
	let errorText = $state<string | null>(null);

	/** A name someone regrets, on a partition of their own life, can always be changed. */
	async function saveTitle(ch: ChapterApi) {
		const title = renameDraft.trim();
		editing = null;
		if (!title || title === (ch.title ?? '')) return;
		try {
			await updateChapter(ch.id, { title });
			await load();
		} catch (e) {
			chapterError = ch.id;
			errorText = e instanceof Error ? e.message : "Your server couldn't save that. Try again.";
		}
	}

	/**
	 * Removing a name keeps the years. The partition has no gaps, so the time
	 * becomes an unnamed stretch: the years someone would rather not name are
	 * still part of the shape.
	 */
	async function removeName(ch: ChapterApi) {
		if (!confirm(`Remove the name "${shown(ch.title)}"? The years stay, as an unnamed stretch.`)) return;
		try {
			await deleteChapter(ch.id);
			await load();
		} catch (e) {
			chapterError = ch.id;
			errorText = e instanceof Error ? e.message : "Your server couldn't do that. Try again.";
		}
	}

	function startRename(ch: ChapterApi) {
		renameDraft = ch.title ?? '';
		editing = ch.id;
	}

	function openMenu(e: MouseEvent, ch: ChapterApi) {
		const el = e.currentTarget as HTMLElement;
		const r = el.getBoundingClientRect();
		contextMenu.show(
			{ x: r.right, y: r.bottom },
			[
				{ id: 'rename', label: ch.title ? 'Rename' : 'Name it', icon: 'mynaui:edit', action: () => startRename(ch) },
				...(ch.title
					? [{ id: 'remove-name', label: 'Remove name', icon: 'mynaui:x', action: () => void removeName(ch) }]
					: []),
			],
			{ anchor: { x: r.left, y: r.top, width: r.width, height: r.height }, placement: 'bottom-end' },
		);
	}

	async function openChapter(ch: ChapterApi) {
		// The chapter's article page, made by the interview's finisher.
		const res = await fetch(`/api/wiki/articles/chapter/${ch.id}`);
		const article = res.ok ? await res.json() : null;
		if (article?.page_id) {
			windowShellStore.openRouteBeside(`/page/${article.page_id}`);
			return;
		}
		missingNote = ch.id;
		clearTimeout(missingTimer);
		missingTimer = setTimeout(() => (missingNote = null), 2600);
	}

	// --- Words ---

	/** The owner's words with a capital first letter, for display only. */
	function shown(s: string | null | undefined): string {
		const t = (s ?? '').trim();
		return t ? t.charAt(0).toUpperCase() + t.slice(1) : '';
	}

	/** Words compared without case, spacing or punctuation. */
	function norm(s: string | null | undefined): string {
		return (s ?? '').toLowerCase().replace(/[^\p{L}\p{N}]+/gu, ' ').trim();
	}

	/** True when one says nothing the other doesn't: equal, or one inside the other. */
	function repeats(a: string | null | undefined, b: string | null | undefined): boolean {
		const x = norm(a);
		const y = norm(b);
		return !!x && !!y && (x === y || x.includes(y) || y.includes(x));
	}

	function summaryOf(ch: ChapterApi): string | null {
		return ch.summary && !repeats(ch.summary, ch.title) ? shown(ch.summary) : null;
	}

	function endedOf(ch: ChapterApi): string | null {
		if (!ch.changepoint || repeats(ch.changepoint, ch.title)) return null;
		if (ch.summary && repeats(ch.changepoint, ch.summary)) return null;
		return shown(ch.changepoint);
	}

	// --- Time ---

	const YEAR_MS = 365.25 * 86_400_000;
	const nowMs = Date.now();

	function startMs(ch: ChapterApi): number {
		return new Date(ch.started_at).getTime();
	}

	function endMs(ch: ChapterApi): number {
		return ch.ended_at ? new Date(ch.ended_at).getTime() : nowMs;
	}

	function spanOf(ch: ChapterApi): string {
		return `${ch.started_at.slice(0, 4)} - ${ch.ended_at ? ch.ended_at.slice(0, 4) : 'now'}`;
	}

	function lengthOf(ch: ChapterApi): string {
		const years = (endMs(ch) - startMs(ch)) / YEAR_MS;
		if (years < 1) {
			const months = Math.max(1, Math.round(years * 12));
			return `${months} ${months === 1 ? 'month' : 'months'}`;
		}
		const n = Math.round(years);
		return `${n} ${n === 1 ? 'year' : 'years'}`;
	}

	/** Ended before the first year your server recorded, so known only from what was said. */
	function beforeRecord(ch: ChapterApi): boolean {
		return firstRecordedYear != null && !!ch.ended_at && Number(ch.ended_at.slice(0, 4)) <= firstRecordedYear - 1;
	}

	// --- The ribbon ---

	/** The painting's hues, in an order that keeps neighbors apart. */
	const HUES = ['yellow', 'emerald', 'indigo', 'rose', 'violet', 'orange', 'cyan', 'pink', 'purple'];

	const first = $derived(chapters.length ? startMs(chapters[0]) : 0);
	const last = $derived(chapters.length ? endMs(chapters[chapters.length - 1]) : 0);
	const total = $derived(Math.max(1, last - first));

	const ribbon = $derived(
		chapters.map((ch, i) => ({
			id: ch.id,
			label: ch.title ? shown(ch.title) : '',
			title: `${ch.title ? shown(ch.title) : 'An unnamed stretch'}, ${spanOf(ch)}`,
			share: (endMs(ch) - startMs(ch)) / total,
			hue: HUES[i % HUES.length],
			named: !!ch.title,
			ch,
		})),
	);

	/** Years under the ribbon: the first, each decade inside it, and now, none crowding another. */
	const ticks = $derived.by(() => {
		if (!chapters.length) return [];
		const at = (ms: number) => (ms - first) / total;
		const startYear = new Date(first).getFullYear();
		const endYear = new Date(last).getFullYear();
		const out: { label: string; at: number }[] = [{ label: String(startYear), at: 0 }];
		for (let y = Math.ceil((startYear + 1) / 10) * 10; y < endYear; y += 10) {
			out.push({ label: String(y), at: at(new Date(y, 0, 1).getTime()) });
		}
		const lastCh = chapters[chapters.length - 1];
		out.push({ label: lastCh.ended_at ? String(endYear) : 'Now', at: 1 });
		const MIN_GAP = 0.08;
		return out.filter((t, i) => (i === 0 || t.at - out[i - 1].at >= MIN_GAP) && (t.at === 1 || 1 - t.at >= MIN_GAP));
	});

	const ROMAN: [number, string][] = [
		[10, 'X'],
		[9, 'IX'],
		[5, 'V'],
		[4, 'IV'],
		[1, 'I'],
	];

	function roman(n: number): string {
		let out = '';
		for (const [v, s] of ROMAN) {
			while (n >= v) {
				out += s;
				n -= v;
			}
		}
		return out;
	}
</script>

<div class="chapters-room">
	<header class="mast">
		<h1>Chapters</h1>
		<p class="standfirst">
			Your life in the chapters you named. Every day in your history falls inside exactly one of them.
		</p>
	</header>

	{#if loading}
		<p class="quiet">Loading…</p>
	{:else if chapters.length}
		<div class="ribbon" aria-hidden="true">
			<div class="bands">
				{#each ribbon as seg (seg.id)}
					<button
						type="button"
						tabindex="-1"
						class="band"
						class:unnamed={!seg.named}
						style:flex-grow={seg.share}
						style:--hue="var(--cat-{seg.hue})"
						title={seg.title}
						onclick={() => openChapter(seg.ch)}
					>
						<span class="band-label">{seg.label}</span>
					</button>
				{/each}
			</div>
			<div class="ticks">
				{#each ticks as t (t.label)}
					<span class="tick" class:end={t.at === 1} style:left="{t.at * 100}%">{t.label}</span>
				{/each}
			</div>
		</div>

		<ol class="chapters">
			{#each chapters as ch, i (ch.id)}
				{@const summary = summaryOf(ch)}
				{@const ended = endedOf(ch)}
				<li class="chapter" class:unnamed={!ch.title}>
					<span class="numeral" aria-hidden="true">{roman(i + 1)}.</span>
					<div class="body">
						{#if editing === ch.id}
							<!-- svelte-ignore a11y_autofocus -->
							<input
								class="rename"
								bind:value={renameDraft}
								autofocus
								aria-label="Chapter name"
								placeholder="What do you call this stretch?"
								onblur={() => saveTitle(ch)}
								onkeydown={(e) => {
									if (e.key === 'Enter') saveTitle(ch);
									if (e.key === 'Escape') editing = null;
								}}
							/>
						{:else}
							<h2>
								<button type="button" class="title" onclick={() => openChapter(ch)}>
									{ch.title ? shown(ch.title) : 'An unnamed stretch'}
								</button>
							</h2>
						{/if}
						{#if summary}
							<p class="summary">{summary}</p>
						{/if}
						{#if ended}
							<p class="ended"><span class="ended-label">How it ended:</span> {ended}</p>
						{/if}
						{#if beforeRecord(ch)}
							<p class="note">Before your server started recording. Everything here comes from what you told it.</p>
						{/if}
						{#if missingNote === ch.id}
							<p class="note">No page yet. Finish the interview to make one.</p>
						{/if}
						{#if chapterError === ch.id && errorText}
							<p class="failed">{errorText}</p>
						{/if}
					</div>
					<div class="side">
						<span class="years">{spanOf(ch)}</span>
						<span class="length">{lengthOf(ch)}</span>
						<span class="menu">
							<IconButton
								icon="mynaui:dots"
								label="Options for {ch.title ? shown(ch.title) : 'this stretch'}"
								size="nested"
								haspopup="menu"
								onclick={(e) => openMenu(e, ch)}
							/>
						</span>
					</div>
				</li>
			{/each}
		</ol>
	{:else}
		<div class="empty">
			<p class="empty-lead">No chapters yet.</p>
		</div>
	{/if}
</div>

<style>
	.chapters-room {
		display: flex;
		flex-direction: column;
	}

	button {
		font: inherit;
		color: inherit;
		background: none;
		border: none;
		padding: 0;
		text-align: left;
		cursor: pointer;
	}

	/* Mirrors the wiki front page's mast so the rooms read as siblings. 400:
	   the serif ships one cut (agents/build/design.md, "Type"). */
	.mast {
		margin-bottom: 1.75rem;
	}

	.mast h1 {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 2.625rem;
		font-weight: 400;
		line-height: 1.1;
		letter-spacing: -0.01em;
		color: var(--color-foreground);
		margin: 0 0 0.75rem;
	}

	.standfirst {
		font-size: 1rem;
		line-height: 1.55;
		color: var(--color-foreground-muted);
		margin: 0;
		max-width: 40rem;
	}

	.quiet {
		font-size: 0.875rem;
		color: var(--color-foreground-subtle);
		margin: 0;
	}

	/* The ribbon: each chapter as wide as its years, in the painting's hues
	   washed toward the page so the words on it stay ink. */
	.ribbon {
		margin-bottom: 2rem;
	}

	.bands {
		display: flex;
		gap: 2px;
		height: 2.25rem;
		border-radius: var(--radius-md);
		overflow: hidden;
	}

	.band {
		flex-basis: 0;
		min-width: 4px;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 0 0.5rem;
		overflow: hidden;
		background: color-mix(in srgb, var(--hue) 28%, var(--color-background));
		transition: background-color var(--motion-hover-out) var(--ease-premium);
	}

	.band:hover {
		background: color-mix(in srgb, var(--hue) 42%, var(--color-background));
	}

	.band.unnamed {
		background: var(--color-border-subtle);
	}

	.band-label {
		font-size: 0.8125rem;
		color: var(--color-foreground);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.ticks {
		position: relative;
		height: 1.25rem;
		margin-top: 0.5rem;
	}

	.tick {
		position: absolute;
		top: 0;
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
	}

	.tick.end {
		transform: translateX(-100%);
	}

	/* A numbered contents page for a life. */
	.chapters {
		list-style: none;
		margin: 0;
		padding: 0;
		border-top: 1px solid var(--color-border);
	}

	.chapter {
		display: grid;
		grid-template-columns: 2.75rem minmax(0, 1fr) auto;
		gap: 0 1.25rem;
		padding: 1.5rem 0;
	}

	.chapter + .chapter {
		border-top: 1px solid var(--color-border);
	}

	.numeral {
		padding-top: 0.375rem;
		font-family: var(--font-serif, Georgia, serif);
		font-size: 1rem;
		color: var(--color-foreground-subtle);
	}

	.body {
		min-width: 0;
		max-width: 40rem;
	}

	.body h2 {
		margin: 0;
		font-weight: 400;
	}

	.title {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 1.75rem;
		line-height: 1.25;
		color: var(--color-foreground);
	}

	.title:hover {
		color: var(--color-foreground-muted);
	}

	.unnamed .title {
		color: var(--color-foreground-subtle);
	}

	.rename {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 1.75rem;
		line-height: 1.25;
		width: 100%;
		background: none;
		border: 0;
		border-bottom: 1px solid var(--color-border-strong);
		color: var(--color-foreground);
		padding: 0;
	}

	.summary {
		margin: 0.625rem 0 0;
		font-family: var(--font-serif, Georgia, serif);
		font-size: 1.1875rem;
		line-height: 1.5;
		color: var(--color-foreground);
	}

	.ended {
		margin: 0.75rem 0 0;
		font-size: 0.9375rem;
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}

	.ended-label {
		color: var(--color-foreground);
	}

	.note {
		margin: 0.75rem 0 0;
		font-size: 0.875rem;
		line-height: 1.5;
		color: var(--color-foreground-subtle);
	}

	.failed {
		margin: 0.5rem 0 0;
		font-size: 0.8125rem;
		color: var(--color-error);
	}

	.side {
		display: flex;
		flex-direction: column;
		align-items: flex-end;
		gap: 0.125rem;
		padding-top: 0.375rem;
		text-align: right;
	}

	.years {
		font-size: 0.9375rem;
		color: var(--color-foreground);
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
	}

	.length {
		font-size: 0.875rem;
		color: var(--color-foreground-subtle);
		white-space: nowrap;
	}

	/* The menu shows on the row under the pointer, and always while it has
	   focus, so a page of chapters is not a column of buttons. */
	.menu {
		margin-top: 0.5rem;
		opacity: 0;
		transition: opacity var(--motion-hover-out) var(--ease-premium);
	}

	.chapter:hover .menu,
	.menu:focus-within {
		opacity: 1;
	}

	@media (hover: none) {
		.menu {
			opacity: 1;
		}
	}

	.empty {
		max-width: 34rem;
	}

	.empty-lead {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 1.125rem;
		color: var(--color-foreground);
		margin: 0 0 0.5rem;
	}

	@media (prefers-reduced-motion: reduce) {
		.band,
		.menu {
			transition: none;
		}
	}

	@media (max-width: 640px) {
		.chapter {
			grid-template-columns: 2rem minmax(0, 1fr);
		}

		.side {
			grid-column: 2;
			flex-direction: row;
			align-items: baseline;
			gap: 0.75rem;
			padding-top: 0.75rem;
		}

		.menu {
			margin: 0 0 0 auto;
		}
	}
</style>
