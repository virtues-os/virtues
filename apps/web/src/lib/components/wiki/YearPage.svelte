<script lang="ts">
	/**
	 * A year.
	 *
	 * Not an index of days — the days index already exists and this page would
	 * be a worse copy of it. A year is a subject with an article, in the sense
	 * a wikipedia gives "2026", and the page is arranged so the prose is the
	 * page and everything else is apparatus around it.
	 *
	 * THREE STATES, and the box decides which (`state`), because what the page
	 * may OFFER depends on it:
	 *
	 *   before_record — no day was ever recorded. Their own words and the
	 *                   chapter dateline, and NO offer to draft: an article
	 *                   invented over an empty record would look exactly like
	 *                   a year the box knew something about.
	 *   thin          — days on record, nothing written yet. Say how thin,
	 *                   plainly and early, and show the days.
	 *   dense         — the article, then the rest.
	 *
	 * The page is readable with no AI in any of them: the dateline, the counts
	 * and the days are all record, not prose.
	 */
	import { onMount } from 'svelte';
	import Markdown from '$lib/components/Markdown.svelte';
	import TextAction from '$lib/components/TextAction.svelte';
	import { getYear, updateYear, writeYearArticle, type YearApi } from '$lib/wiki/api';

	interface Props {
		year: number;
	}
	let { year }: Props = $props();

	let page = $state<YearApi | null>(null);
	let loading = $state(true);
	let writing = $state(false);
	let failed = $state<string | null>(null);
	let editingTitle = $state(false);
	let titleDraft = $state('');
	let editingSummary = $state(false);
	let summaryDraft = $state('');

	onMount(load);

	async function load() {
		loading = true;
		page = await getYear(year);
		loading = false;
	}

	const narratedDays = $derived(page ? page.days.filter((d) => d.narrated) : []);
	const first = $derived(page?.days[0]?.date ?? null);
	const last = $derived(page ? (page.days[page.days.length - 1]?.date ?? null) : null);

	function when(iso: string): string {
		return new Date(iso + 'T12:00:00').toLocaleDateString('en-US', {
			month: 'long',
			day: 'numeric'
		});
	}

	async function saveTitle() {
		editingTitle = false;
		const t = titleDraft.trim();
		if (!page || t === (page.title ?? '')) return;
		try {
			await updateYear(year, { title: t });
			await load();
		} catch (e) {
			failed = e instanceof Error ? e.message : 'Could not save that';
		}
	}

	async function saveSummary() {
		editingSummary = false;
		const s = summaryDraft.trim();
		if (!page || s === (page.summary ?? '')) return;
		try {
			await updateYear(year, { summary: s });
			await load();
		} catch (e) {
			failed = e instanceof Error ? e.message : 'Could not save that';
		}
	}

	async function write() {
		writing = true;
		failed = null;
		try {
			await writeYearArticle(year);
			await load();
		} catch (e) {
			failed = e instanceof Error ? e.message : 'Could not write that';
		} finally {
			writing = false;
		}
	}
</script>

{#if loading}
	<p class="quiet">Loading {year}…</p>
{:else if !page}
	<p class="quiet">There is no page for {year}.</p>
{:else}
	<article class="year">
		<header>
			<p class="eyebrow">Year</p>
			{#if editingTitle}
				<!-- svelte-ignore a11y_autofocus -->
				<input
					class="title-input"
					bind:value={titleDraft}
					autofocus
					onblur={saveTitle}
					onkeydown={(e) => e.key === 'Enter' && saveTitle()}
					placeholder="What do you call this year?"
				/>
			{:else}
				<h1>
					{page.title ?? page.year}
					{#if page.title}<span class="numeral">{page.year}</span>{/if}
				</h1>
				<TextAction
					onclick={() => {
						titleDraft = page?.title ?? '';
						editingTitle = true;
					}}>{page.title ? 'Rename' : 'Name this year'}</TextAction
				>
			{/if}

			{#if page.chapters.length}
				<p class="dateline">{page.chapters.join(' · ')}</p>
			{/if}
		</header>

		<!-- Theirs, above anything the record says. -->
		{#if editingSummary}
			<textarea
				class="summary-input"
				bind:value={summaryDraft}
				rows="3"
				onblur={saveSummary}
				placeholder="What was this year, in your words?"
			></textarea>
		{:else if page.summary}
			<blockquote class="summary">{page.summary}</blockquote>
			<TextAction
				onclick={() => {
					summaryDraft = page?.summary ?? '';
					editingSummary = true;
				}}>Edit</TextAction
			>
		{/if}

		{#if page.state === 'before_record'}
			<section class="state">
				<p>
					Nothing was recorded in {page.year}. That is most of a life for most
					people, and the part only you can write.
				</p>
				{#if !page.summary}
					<TextAction
						onclick={() => {
							summaryDraft = '';
							editingSummary = true;
						}}>What do you remember of {page.year}?</TextAction
					>
				{/if}
			</section>
		{:else if page.state === 'thin'}
			<section class="state">
				<p class="quiet">
					The record for {page.year} runs from {first ? when(first) : '—'} to {last
						? when(last)
						: '—'} — {page.days_recorded}
					{page.days_recorded === 1 ? 'day' : 'days'}, {page.days_narrated} written up.
				</p>
				{#if page.days_narrated > 0 && !page.has_article}
					<TextAction loading={writing} loadingLabel="Writing…" onclick={write}>
						Write the article
					</TextAction>
				{/if}
			</section>
		{/if}

		{#if page.article}
			<div class="prose">
				<Markdown content={page.article} refVariant="quiet" />
			</div>
		{/if}

		{#if failed}
			<p class="failed">{failed}</p>
		{/if}

		{#if narratedDays.length}
			<section class="days">
				<h2>The days</h2>
				<ul>
					{#each narratedDays as d}
						<li>
							<a href="/day/day_{d.date}">{when(d.date)}</a>
							{#if d.lede}<span class="lede">{d.lede}</span>{/if}
						</li>
					{/each}
				</ul>
			</section>
		{/if}
	</article>
{/if}

<style>
	.year {
		max-width: 44rem;
		margin: 0 auto;
		padding: 2rem 1.5rem 4rem;
	}

	.eyebrow {
		font-size: 0.6875rem;
		letter-spacing: 0.12em;
		text-transform: uppercase;
		color: var(--color-foreground-subtle);
		margin: 0 0 0.25rem;
	}

	h1 {
		font-family: var(--font-serif);
		font-size: 2rem;
		font-weight: 400;
		margin: 0;
		line-height: 1.15;
	}

	.numeral {
		font-size: 1.25rem;
		color: var(--color-foreground-subtle);
		margin-left: 0.5rem;
	}

	.dateline {
		margin: 0.35rem 0 0;
		color: var(--color-foreground-subtle);
		font-size: 0.875rem;
	}

	.summary {
		font-family: var(--font-serif);
		font-size: 1.125rem;
		line-height: 1.6;
		margin: 1.5rem 0 0;
		padding-left: 1rem;
		border-left: 2px solid var(--color-border);
	}

	.title-input,
	.summary-input {
		width: 100%;
		font-family: var(--font-serif);
		background: none;
		border: 0;
		border-bottom: 1px solid var(--color-border);
		color: var(--color-foreground);
		padding: 0.25rem 0;
	}

	.title-input {
		font-size: 2rem;
	}

	.summary-input {
		font-size: 1.125rem;
		line-height: 1.6;
		margin-top: 1.5rem;
		resize: vertical;
	}

	.state {
		margin-top: 1.75rem;
	}

	.state p {
		margin: 0 0 0.5rem;
		line-height: 1.6;
	}

	.prose {
		margin-top: 2rem;
		font-family: var(--font-serif);
		line-height: 1.7;
	}

	.days {
		margin-top: 3rem;
		border-top: 1px solid var(--color-border);
		padding-top: 1.25rem;
	}

	.days h2 {
		font-family: var(--font-serif);
		font-size: 1.125rem;
		font-weight: 400;
		margin: 0 0 0.75rem;
	}

	.days ul {
		list-style: none;
		margin: 0;
		padding: 0;
	}

	.days li {
		padding: 0.4rem 0;
		border-bottom: 1px solid var(--color-border-subtle, var(--color-border));
		display: flex;
		gap: 0.75rem;
		align-items: baseline;
	}

	.days a {
		color: var(--color-foreground);
		text-decoration: none;
		white-space: nowrap;
		font-variant-numeric: tabular-nums;
	}

	.days a:hover {
		text-decoration: underline;
	}

	.lede {
		color: var(--color-foreground-subtle);
		font-size: 0.875rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.quiet {
		color: var(--color-foreground-subtle);
	}

	.failed {
		color: var(--color-danger, #b00);
		font-size: 0.875rem;
	}

</style>
