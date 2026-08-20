<!--
	BookmarkDetailView.svelte

	One saved thing, and what the box has made of it.

	The generic record view (/record/…) can render this row, and did until now,
	but it reads as a database row: `Enrichment Attempts: 0` sits beside the
	description, and `likely_queries` — the field most worth reading — arrives
	as pretty-printed JSON. This view exists to put the two things a person came
	for first: the artifact, and their own note.

	The ordering is the argument. Their words, then the thing, then what we made
	of it, then plumbing. Machine text never sits above user text.
-->

<script lang="ts">
	import { Page } from '$lib';
	import Icon from '$lib/components/Icon.svelte';
	import type { Tab } from '$lib/tabs/types';
	import {
		getBookmark,
		updateBookmarkNote,
		type BookmarkDetailApi,
	} from '$lib/bookmarks/api';

	let { tab }: { tab: Tab } = $props();

	const id = $derived(tab.route.match(/^\/bookmark\/(.+)$/)?.[1] ?? '');

	let bookmark = $state<BookmarkDetailApi | null>(null);
	let loading = $state(true);
	let error = $state<string | null>(null);

	// The note editor is deliberately uncontrolled once loaded: rebinding it to
	// the server value on every save would fight anyone still typing.
	let draft = $state('');
	let savingNote = $state(false);
	let noteError = $state<string | null>(null);
	let savedAt = $state<number | null>(null);

	$effect(() => {
		const bookmarkId = id;
		if (!bookmarkId) {
			error = 'Malformed bookmark link.';
			loading = false;
			return;
		}
		loading = true;
		error = null;
		getBookmark(bookmarkId)
			.then((b) => {
				bookmark = b;
				draft = b.note ?? '';
			})
			.catch((e) => (error = e instanceof Error ? e.message : String(e)))
			.finally(() => (loading = false));
	});

	const dirty = $derived((bookmark?.note ?? '') !== draft.trim());

	async function saveNote() {
		if (!bookmark || savingNote || !dirty) return;
		savingNote = true;
		noteError = null;
		try {
			bookmark = await updateBookmarkNote(bookmark.id, draft.trim() || null);
			draft = bookmark.note ?? '';
			savedAt = Date.now();
		} catch (e) {
			noteError = e instanceof Error ? e.message : 'Could not save that note';
		} finally {
			savingNote = false;
		}
	}

	function hostOf(raw: string): string {
		try {
			return new URL(raw).hostname.replace(/^www\./, '');
		} catch {
			return raw;
		}
	}

	/** Internal asset routes have no host worth reading — see the url contract. */
	const origin = $derived(
		!bookmark
			? ''
			: bookmark.url.startsWith('/drive/')
				? (bookmark.source_platform ?? 'Saved image')
				: hostOf(bookmark.url)
	);

	const isInternal = $derived(bookmark?.url.startsWith('/drive/') ?? false);

	const heading = $derived(
		bookmark?.title?.trim() || (isInternal ? 'Saved image' : origin) || 'Bookmark'
	);

	const tags = $derived<string[]>(
		Array.isArray(bookmark?.tags) ? (bookmark.tags as string[]) : []
	);

	/** Said only when it changes what you are looking at. */
	const stateNote = $derived.by(() => {
		switch (bookmark?.state) {
			case 'held':
				return 'Waiting to be read — the pass that reads images is not built yet.';
			case 'queued':
				return 'Not read yet. The next sweep will pick it up.';
			case 'failed':
				return "This page could not be read, so there is nothing below but what the source gave us.";
			case 'skipped':
				return 'Deliberately not read — this address is not one the box fetches.';
			default:
				return null;
		}
	});

	function when(iso: string | null): string {
		if (!iso) return '—';
		const d = new Date(iso);
		if (Number.isNaN(d.getTime()) || d.getTime() <= 0) return '—';
		return d.toLocaleDateString('en-US', {
			month: 'long',
			day: 'numeric',
			year: 'numeric',
		});
	}
</script>

<Page title={heading} description={origin} maxWidth="prose">
	{#if loading}
		<p class="state">Opening…</p>
	{:else if error}
		<p class="state error" role="alert">
			<Icon icon="ri:error-warning-line" width="16" />
			{error}
		</p>
	{:else if bookmark}
		{#if bookmark.deleted_at_source}
			<p class="banner">
				Removed from {origin} on {when(bookmark.deleted_at_source)}. It is kept
				here because your note is yours.
			</p>
		{/if}

		<!-- THE NOTE COMES FIRST. It is the only text on this page a person
		     wrote, and the only field they can change. -->
		<section class="note-block">
			<label class="note-label" for="note">Your note</label>
			<textarea
				id="note"
				class="note-input"
				bind:value={draft}
				rows="3"
				placeholder="Why you kept this — a reason, a todo, the bit worth coming back to."
				disabled={savingNote}
				onblur={saveNote}
			></textarea>
			<div class="note-foot">
				{#if noteError}
					<span class="note-error" role="alert">{noteError}</span>
				{:else if savingNote}
					<span>Saving…</span>
				{:else if dirty}
					<span>Unsaved</span>
				{:else if savedAt}
					<span>Saved</span>
				{/if}
				{#if dirty && !savingNote}
					<button class="note-save" onclick={saveNote}>Save note</button>
				{/if}
			</div>
		</section>

		{#if bookmark.thumbnail_url}
			<!-- Same guard as the Wall's tiles: og:image links rot, and a dead one
			     should cost nothing rather than leave a broken glyph and a gap
			     where the artifact was meant to be. -->
			<img
				class="hero"
				src={bookmark.thumbnail_url}
				alt=""
				onerror={(e) => ((e.currentTarget as HTMLImageElement).hidden = true)}
			/>
		{/if}

		{#if !isInternal}
			<a class="original" href={bookmark.url} target="_blank" rel="noopener noreferrer">
				<Icon icon="ri:external-link-line" width="14" />
				<span>Open on {origin}</span>
			</a>
		{/if}

		{#if stateNote}
			<p class="unread">{stateNote}</p>
		{/if}

		{#if bookmark.description}
			<p class="lede">{bookmark.description}</p>
		{/if}

		{#if tags.length}
			<section class="block">
				<h2>Filed under</h2>
				<ul class="chips">
					{#each tags as t (t)}<li class="chip user">{t}</li>{/each}
				</ul>
			</section>
		{/if}

		{#if bookmark.extraction}
			{@const ex = bookmark.extraction}
			<!-- Everything below this line was written by a model, and is
			     separated from the user's words above it on purpose. -->
			<section class="block machine">
				<h2>What the box made of it</h2>

				{#if ex.subject?.length}
					<div class="row">
						<span class="key">About</span>
						<ul class="chips">
							{#each ex.subject as s (s)}<li class="chip">{s}</li>{/each}
						</ul>
					</div>
				{/if}
				{#if ex.entities?.length}
					<div class="row">
						<span class="key">Mentions</span>
						<ul class="chips">
							{#each ex.entities as e (e)}<li class="chip">{e}</li>{/each}
						</ul>
					</div>
				{/if}
				{#if ex.style}
					<div class="row"><span class="key">Style</span><p class="val">{ex.style}</p></div>
				{/if}
				{#if ex.likely_queries?.length}
					<div class="row">
						<span class="key">Findable by</span>
						<ul class="queries">
							{#each ex.likely_queries as q (q)}<li>“{q}”</li>{/each}
						</ul>
					</div>
				{/if}
			</section>
		{/if}

		<footer class="meta">
			<span>Saved {when(bookmark.timestamp)}</span>
			{#if bookmark.source_platform}<span>from {bookmark.source_platform}</span>{/if}
			{#if bookmark.medium}<span>{bookmark.medium}</span>{/if}
			{#if bookmark.enrichment_model}<span>read by {bookmark.enrichment_model}</span>{/if}
		</footer>
	{/if}
</Page>

<style>
	@reference "../../../../app.css";

	.state {
		font-size: 0.875rem;
		color: var(--color-foreground-muted);
	}
	.state.error {
		display: flex;
		align-items: center;
		gap: 0.4rem;
		color: var(--color-error, #dc2626);
	}

	.banner {
		margin: 0 0 1.5rem;
		padding: 0.6rem 0.8rem;
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
		background: var(--color-surface-elevated);
		border-left: 2px solid var(--color-border-strong, var(--color-border));
		border-radius: 0 0.25rem 0.25rem 0;
	}

	/* The note: the one editable thing, and the first thing. */
	.note-block {
		margin: 0 0 2rem;
	}
	.note-label {
		display: block;
		margin-bottom: 0.4rem;
		font-size: 0.6875rem;
		letter-spacing: 0.04em;
		text-transform: uppercase;
		color: var(--color-foreground-subtle);
	}
	.note-input {
		width: 100%;
		padding: 0.7rem 0.8rem;
		font-family: var(--font-serif);
		font-size: 1.0625rem;
		line-height: 1.5;
		color: var(--color-foreground);
		background: var(--color-surface-elevated);
		border: 1px solid var(--color-border-subtle, var(--color-border));
		border-radius: 0.375rem;
		resize: vertical;
	}
	.note-input:focus {
		outline: none;
		border-color: var(--color-foreground-muted);
	}
	.note-input::placeholder {
		font-family: var(--font-sans);
		font-size: 0.8125rem;
		color: var(--color-foreground-subtle);
	}
	.note-foot {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		min-height: 1.5rem;
		margin-top: 0.4rem;
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
	}
	.note-error {
		color: var(--color-error, #dc2626);
	}
	.note-save {
		padding: 0.2rem 0.6rem;
		font: inherit;
		font-size: 0.75rem;
		color: var(--color-foreground);
		background: var(--color-surface-elevated);
		border: 1px solid var(--color-border);
		border-radius: 0.25rem;
		cursor: pointer;
	}

	.hero {
		display: block;
		width: 100%;
		max-height: 26rem;
		object-fit: cover;
		border-radius: 0.375rem;
		background: var(--color-surface);
	}

	.original {
		display: inline-flex;
		align-items: center;
		gap: 0.35rem;
		margin-top: 1rem;
		font-size: 0.8125rem;
		color: var(--color-primary);
		text-decoration: none;
	}
	.original:hover {
		text-decoration: underline;
	}

	.unread {
		margin: 1.25rem 0 0;
		font-size: 0.8125rem;
		font-style: italic;
		color: var(--color-foreground-subtle);
	}

	.lede {
		margin: 1.5rem 0 0;
		font-family: var(--font-serif);
		font-size: 1.125rem;
		line-height: 1.6;
		color: var(--color-foreground);
	}

	.block {
		margin-top: 2rem;
	}
	.block h2 {
		margin: 0 0 0.75rem;
		font-size: 0.6875rem;
		font-weight: 400;
		letter-spacing: 0.04em;
		text-transform: uppercase;
		color: var(--color-foreground-subtle);
	}
	/* A hairline is the whole separation between their words and ours. */
	.block.machine {
		padding-top: 1.5rem;
		border-top: 1px solid var(--color-border-subtle, var(--color-border));
	}

	.row {
		display: grid;
		grid-template-columns: 7rem 1fr;
		gap: 0.75rem;
		margin-bottom: 0.75rem;
	}
	.key {
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
		padding-top: 0.15rem;
	}
	.val {
		margin: 0;
		font-size: 0.875rem;
		color: var(--color-foreground);
	}

	.chips {
		display: flex;
		flex-wrap: wrap;
		gap: 0.35rem;
		margin: 0;
		padding: 0;
		list-style: none;
	}
	.chip {
		padding: 0.15rem 0.5rem;
		font-size: 0.75rem;
		color: var(--color-foreground-muted);
		background: var(--color-surface-elevated);
		border: 1px solid var(--color-border-subtle, var(--color-border));
		border-radius: 0.25rem;
	}
	/* The user's own containers read as theirs, not as machine output. */
	.chip.user {
		color: var(--color-foreground);
		border-color: var(--color-border);
	}

	.queries {
		margin: 0;
		padding: 0;
		list-style: none;
		font-family: var(--font-serif);
		font-size: 0.9375rem;
		line-height: 1.6;
		color: var(--color-foreground-muted);
	}

	.meta {
		display: flex;
		flex-wrap: wrap;
		gap: 1rem;
		margin-top: 2.5rem;
		padding-top: 1rem;
		border-top: 1px solid var(--color-border-subtle, var(--color-border));
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
	}
</style>
