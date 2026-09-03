<!--
	ChaptersSection.svelte

	The chapters of the life — the person's own gapless partition of it into
	named eras, authored in the narrative interview and never inferred
	(wiki_chapters, migration 0015). Its own wiki room, deliberately separate
	from the narrative-identity document: chapters are STRUCTURE (the record's
	coordinate system), the document is prose. Each chapter is an entity with
	its own article page; clicking a row opens it beside.
-->

<script lang="ts">
	import { onMount } from 'svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { getChapters, type ChapterApi } from '$lib/wiki/api';

	let loading = $state(true);
	let chapters = $state<ChapterApi[]>([]);

	onMount(async () => {
		try {
			chapters = await getChapters();
		} finally {
			loading = false;
		}
	});

	/** A chapter whose page hasn't been seeded yet — acknowledged, not a
	 *  dead click. Cleared after a beat. */
	let missingNote = $state<string | null>(null);
	let missingTimer: ReturnType<typeof setTimeout> | undefined;

	async function openChapter(ch: ChapterApi) {
		// The chapter's article page, resolved on demand — seeded by the
		// interview's finisher.
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

	function openInterview() {
		windowShellStore.openRouteBeside('/chat/chat_narrative_interview');
	}

	function yearOf(date: string): string {
		return date.slice(0, 4);
	}

	function spanOf(ch: ChapterApi): string {
		return `${yearOf(ch.started_at)} – ${ch.ended_at ? yearOf(ch.ended_at) : 'now'}`;
	}
</script>

<div class="chapters-room">
	<header class="mast">
		<h1>Chapters</h1>
		<p class="standfirst">
			Your life, divided the way you divided it — named in the interview, never
			inferred. Every day the record holds falls inside exactly one of these.
		</p>
	</header>

	{#if loading}
		<p class="quiet">Loading…</p>
	{:else if chapters.length}
		<ol class="chapters">
			{#each chapters as ch (ch.id)}
				<li>
					<button class="chapter" class:unnamed={!ch.title} onclick={() => openChapter(ch)}>
						<div class="chapter-line">
							<span class="chapter-title">
								{ch.title ?? 'An unnamed stretch'}
							</span>
							<span class="chapter-years">{spanOf(ch)}</span>
						</div>
						{#if ch.summary}
							<p class="chapter-note">{ch.summary}</p>
						{/if}
						{#if ch.changepoint}
							<p class="chapter-note changepoint">Ended when: {ch.changepoint}</p>
						{/if}
						{#if missingNote === ch.id}
							<p class="chapter-note changepoint">No page yet — it is written when the interview is written up.</p>
						{/if}
					</button>
				</li>
			{/each}
		</ol>
	{:else}
		<div class="empty">
			<p class="empty-lead">No chapters yet.</p>
			<p class="empty-body">
				They are written when the interview is written up — rough names and
				rough years, in your words. Each one becomes a page you can keep
				writing in.
			</p>
			<button class="btn primary" onclick={openInterview}>Open the interview</button>
		</div>
	{/if}
</div>

<style>
	.chapters-room {
		display: flex;
		flex-direction: column;
	}

	/* Mirrors the wiki overview's mast so the rooms read as siblings. */
	.mast {
		margin-bottom: 2rem;
	}

	.mast h1 {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 2rem;
		font-weight: 500;
		letter-spacing: -0.01em;
		color: var(--color-foreground);
		margin: 0 0 0.625rem;
	}

	.standfirst {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 1.0625rem;
		line-height: 1.5;
		color: var(--color-foreground-muted);
		margin: 0;
		max-width: 40rem;
	}

	.quiet {
		font-size: 0.875rem;
		color: var(--color-foreground-subtle);
		margin: 0;
	}

	/* A numbered contents page for a life. */
	.chapters {
		list-style: none;
		margin: 0;
		padding: 0;
		counter-reset: chapter;
		border-top: 1px solid var(--color-border);
	}

	.chapters li {
		counter-increment: chapter;
	}

	.chapters li + li {
		border-top: 1px solid color-mix(in srgb, var(--color-border) 55%, transparent);
	}

	.chapter {
		display: block;
		width: 100%;
		text-align: left;
		background: none;
		border: none;
		padding: 0.875rem 0.5rem;
		cursor: pointer;
		border-radius: 6px;
	}

	.chapter:hover {
		background: var(--color-surface-hover);
	}

	.chapter-line {
		display: flex;
		align-items: baseline;
		gap: 0.75rem;
	}

	.chapter-line::before {
		content: counter(chapter, upper-roman) '.';
		font-family: var(--font-serif, Georgia, serif);
		font-size: 0.8125rem;
		color: var(--color-foreground-subtle);
		min-width: 1.75rem;
	}

	.chapter-title {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 1.125rem;
		color: var(--color-foreground);
	}

	.unnamed .chapter-title {
		color: var(--color-foreground-subtle);
	}

	.chapter-years {
		margin-left: auto;
		font-size: 0.8125rem;
		font-variant-numeric: tabular-nums;
		color: var(--color-foreground-subtle);
		white-space: nowrap;
	}

	.chapter-note {
		margin: 0.25rem 0 0 2.5rem;
		font-size: 0.875rem;
		line-height: 1.5;
		color: var(--color-foreground-muted);
		max-width: 38rem;
	}

	.chapter-note.changepoint {
		color: var(--color-foreground-subtle);
	}

	.btn {
		display: inline-flex;
		align-items: center;
		gap: 0.375rem;
		font: inherit;
		font-size: 0.8125rem;
		padding: 0.375rem 0.875rem;
		border-radius: 6px;
		border: 1px solid var(--color-border);
		background: var(--color-surface-elevated);
		color: var(--color-foreground);
		cursor: pointer;
	}

	.btn:hover {
		background: var(--color-surface-hover);
	}

	.btn.primary {
		background: var(--color-primary);
		border-color: var(--color-primary);
		/* --color-background, not a hardcoded white: on dark themes the
		   primary is light and white-on-light would vanish. (There is no
		   --color-primary-foreground token; it silently fell back.) */
		color: var(--color-background);
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

	.empty-body {
		font-size: 0.9375rem;
		line-height: 1.55;
		color: var(--color-foreground-muted);
		margin: 0 0 1.25rem;
	}
</style>
