<!--
	BookmarkDetailView.svelte

	One saved thing, and everything about it.

	Two columns: the saved thing on the left, laid out by the same rule as its
	card (lib/bookmarks/shape.ts), and a side column on the right that moves
	below it when the pane is narrow. The side column runs: your note, the one
	way out to the original, mentions, what the source filed it under, then
	how your server indexed it, folded. The index is your server's notes for
	search, so it stays one click away.

	A post looks like a post: its author, the text with its line breaks, then
	its picture, then when it was posted.
-->

<script lang="ts">
	import { Button, Page, Textarea } from '$lib';
	import Chip from '$lib/components/Chip.svelte';
	import LinkPreviewCard from '$lib/components/bookmarks/LinkPreviewCard.svelte';
	import Icon from '$lib/components/Icon.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { openWebLink } from '$lib/utils/webLinks';
	import type { Tab } from '$lib/tabs/types';
	import { getBookmark, updateBookmarkNote, type BookmarkDetailApi } from '$lib/bookmarks/api';
	import { arrivedBy, crumbOf, dayLabel, openLabel, shapeOf } from '$lib/bookmarks/shape';

	let { tab }: { tab: Tab } = $props();

	const id = $derived(tab.route.match(/^\/bookmark\/(.+)$/)?.[1] ?? '');

	let bookmark = $state<BookmarkDetailApi | null>(null);
	let loading = $state(true);
	let error = $state<string | null>(null);

	let editing = $state(false);
	let draft = $state('');
	let savingNote = $state(false);
	let noteError = $state<string | null>(null);
	let indexOpen = $state(false);
	let failed = $state<Record<string, boolean>>({});
	const shown = (key: string, src: string | null | undefined) => (src && !failed[key] ? src : null);
	const hide = (key: string) => () => (failed = { ...failed, [key]: true });

	$effect(() => {
		const bookmarkId = id;
		if (!bookmarkId) {
			error = "That link doesn't name a bookmark. Go back to Bookmarks to find it.";
			loading = false;
			return;
		}
		loading = true;
		error = null;
		getBookmark(bookmarkId)
			.then((b) => {
				bookmark = b;
				// The tab and the crumb name the bookmark by its headline.
				windowShellStore.updateTab(tab.id, { label: crumbOf(b) });
			})
			.catch((e) => (error = e instanceof Error ? e.message : String(e)))
			.finally(() => (loading = false));
	});

	const shape = $derived(bookmark ? shapeOf(bookmark) : null);
	const picture = $derived(shown('picture', shape?.picture));
	const tags = $derived<string[]>(Array.isArray(bookmark?.tags) ? (bookmark.tags as string[]) : []);
	const open = $derived(bookmark ? openLabel(bookmark) : null);

	function startEditing() {
		draft = bookmark?.note ?? '';
		noteError = null;
		editing = true;
	}

	/** Saving it blank deletes the note. */
	async function saveNote() {
		if (!bookmark || savingNote) return;
		savingNote = true;
		noteError = null;
		try {
			bookmark = await updateBookmarkNote(bookmark.id, draft.trim() || null);
			editing = false;
		} catch (e) {
			noteError = e instanceof Error ? e.message : "Your server couldn't save that note. Try again.";
		} finally {
			savingNote = false;
		}
	}

	/** A plain click opens an in-app page in a tab; a modified click is the browser's. */
	function openRoute(e: MouseEvent, route: string) {
		if (e.metaKey || e.ctrlKey || e.shiftKey || e.button !== 0) return;
		e.preventDefault();
		windowShellStore.openTabFromRoute(route);
	}

	/** Where a click on a web link goes: the Browser pane in the app, a new tab in a browser. */
	function openOriginal() {
		if (bookmark) void openWebLink(bookmark.url);
	}

	function longDate(iso: string | null): string {
		if (!iso) return '';
		const d = new Date(iso);
		if (Number.isNaN(d.getTime()) || d.getTime() <= 0) return '';
		return d.toLocaleDateString('en-US', { month: 'long', day: 'numeric', year: 'numeric' });
	}

	/** "today", "yesterday" or "Nov 21", to sit mid-sentence. */
	function savedDay(iso: string): string {
		const d = dayLabel(iso);
		return d === 'Today' || d === 'Yesterday' ? d.toLowerCase() : d;
	}

	/** The model's name as a person reads it: no provider prefix. */
	const modelName = (m: string | null) => m?.split('/').pop() ?? null;
</script>

<Page title={bookmark ? crumbOf(bookmark) : 'Bookmark'} maxWidth="medium">
	{#if loading}
		<p class="state">Opening…</p>
	{:else if error}
		<p class="state error" role="alert">
			<Icon icon="mynaui:danger-circle" width="16" />
			{error}
		</p>
	{:else if bookmark && shape}
		<div class="layout">
			<article class="thing">
				{#snippet pictureBlock()}
					{#if picture}
						<div class="frame">
							<img class="picture" src={picture} alt="" onerror={hide('picture')} />
							{#if shape?.duration}<span class="duration">{shape.duration}</span>{/if}
						</div>
					{/if}
				{/snippet}

				{#if shape.kind === 'post'}
					{#if shape.handle || shape.authorName}
						<div class="author">
							{#if shown('avatar', shape.authorPicture)}
								<img class="avatar" src={shape.authorPicture} alt="" onerror={hide('avatar')} />
							{/if}
							<span class="author-words">
								{#if shape.authorName}<span class="author-name">{shape.authorName}</span>{/if}
								{#if shape.handle}<span class="byline">{shape.handle}</span>{/if}
							</span>
						</div>
					{/if}
					{#if shape.pictureFirst}{@render pictureBlock()}{/if}
					{#if shape.headline}<p class="text">{shape.headline}</p>{/if}
					{#if !shape.pictureFirst}{@render pictureBlock()}{/if}
					{#if shape.preview}<LinkPreviewCard preview={shape.preview} href={shape.preview.url} />{/if}
					<p class="when">Posted {longDate(bookmark.timestamp)}</p>
				{:else if shape.kind === 'page'}
					{@render pictureBlock()}
					{#if shape.site}
						<p class="byline site">
							{#if shown('icon', shape.icon)}<img class="icon" src={shape.icon} alt="" onerror={hide('icon')} />{/if}
							{shape.site}
						</p>
					{/if}
					<h1 class="title">{shape.headline}</h1>
					{#if shape.summary}<p class="summary">{shape.summary}</p>{/if}
					{#if shape.facts.length}<p class="when">{shape.facts.join(' · ')}</p>{/if}
				{:else if shape.kind === 'picture'}
					{@render pictureBlock()}
				{:else}
					{#if shape.site}<p class="byline">{shape.site}</p>{/if}
					<p class="address">{shape.headline}</p>
				{/if}

				{#if bookmark.deleted_at_source}
					<p class="removed">
						{arrivedBy(bookmark)} no longer has this, since {longDate(bookmark.deleted_at_source)}. It stays
						here with your note.
					</p>
				{/if}
			</article>

			<aside class="side">
				<section>
					<h2 class="label">Your note</h2>
					{#if editing}
						<Textarea bind:value={draft} placeholder="What's this for?" disabled={savingNote} />
						{#if noteError}<p class="note-error" role="alert">{noteError}</p>{/if}
						<div class="note-actions">
							<Button variant="secondary" onclick={saveNote} loading={savingNote}>Save note</Button>
							<Button variant="ghost" onclick={() => (editing = false)} disabled={savingNote}>Cancel</Button>
						</div>
					{:else if bookmark.note}
						<p class="note">{bookmark.note}</p>
						<button type="button" class="quiet" onclick={startEditing}>Edit note</button>
					{:else}
						<Button variant="secondary" onclick={startEditing}>Add a note</Button>
					{/if}
				</section>

				<section class="out">
					{#if open}
						<Button onclick={openOriginal}>
							{open}
							<Icon icon="mynaui:arrow-up-right" width="14" />
						</Button>
					{/if}
					<p class="saved">Saved {savedDay(bookmark.saved_at)} from {arrivedBy(bookmark)}</p>
				</section>

				{#if bookmark.mentions?.length}
					<section>
						<h2 class="label">Mentions</h2>
						<div class="chips">
							{#each bookmark.mentions as m, i (i)}
								{#if m.route}
									<Chip href={m.route} onclick={(e) => openRoute(e, m.route!)}>{m.name}</Chip>
								{:else}
									<Chip>{m.name}</Chip>
								{/if}
							{/each}
						</div>
					</section>
				{/if}

				{#if tags.length}
					<section>
						<h2 class="label">Filed under</h2>
						<div class="chips">
							{#each tags as t, i (i)}<Chip>{t}</Chip>{/each}
						</div>
					</section>
				{/if}

				{#if bookmark.extraction}
					{@const ex = bookmark.extraction}
					<section>
						<button type="button" class="quiet fold" aria-expanded={indexOpen} onclick={() => (indexOpen = !indexOpen)}>
							How Virtues indexed this
							<Icon icon={indexOpen ? 'mynaui:chevron-up' : 'mynaui:chevron-down'} width="14" />
						</button>
						{#if indexOpen}
							<dl class="index">
								{#if ex.subject?.length}
									<dt>About</dt>
									<dd>{ex.subject.join(', ')}</dd>
								{/if}
								{#if ex.likely_queries?.length}
									<dt>Search finds it with</dt>
									<dd>{ex.likely_queries.join(' · ')}</dd>
								{/if}
								{#if bookmark.enriched_at || bookmark.enrichment_model}
									<dt>Read</dt>
									<dd>
										{longDate(bookmark.enriched_at)}{#if bookmark.enrichment_model}, by {modelName(bookmark.enrichment_model)}{/if}
									</dd>
								{/if}
							</dl>
						{/if}
					</section>
				{/if}
			</aside>
		</div>
	{/if}
</Page>

<style>
	.state {
		font-size: var(--text-label);
		color: var(--color-foreground-muted);
	}

	.state.error {
		display: flex;
		align-items: center;
		gap: 8px;
		color: var(--color-error);
	}

	.layout {
		container-type: inline-size;
		display: flex;
		flex-wrap: wrap;
		gap: 48px;
		align-items: flex-start;
	}

	.thing {
		flex: 999 1 420px;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 16px;
	}

	.side {
		flex: 1 1 260px;
		max-width: 320px;
		display: flex;
		flex-direction: column;
		gap: 24px;
	}

	.side section {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 8px;
	}

	.author {
		display: flex;
		align-items: center;
		gap: 12px;
	}

	.avatar {
		flex: none;
		width: 40px;
		height: 40px;
		border-radius: 50%;
		object-fit: cover;
	}

	.author-words {
		display: flex;
		flex-direction: column;
	}

	.author-name {
		font-size: var(--text-label);
	}

	.site {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.icon {
		width: 16px;
		height: 16px;
		border-radius: var(--radius-sm);
		object-fit: contain;
	}

	.frame {
		position: relative;
	}

	/* On the picture, so it is light ink on a dark chip in both themes. */
	.duration {
		position: absolute;
		right: 12px;
		bottom: 12px;
		padding: 2px 8px;
		border-radius: var(--radius-sm);
		background: rgb(0 0 0 / 0.75); /* design-ok: a chip on a photograph, the same in both themes */
		color: #fff; /* design-ok: ink on the dark chip above */
		font-size: var(--text-small);
		font-variant-numeric: tabular-nums;
	}

	.byline,
	.when,
	.saved {
		margin: 0;
		font-size: var(--text-small);
		color: var(--color-foreground-subtle);
	}

	.picture {
		display: block;
		width: 100%;
		max-height: 720px;
		object-fit: contain;
		border-radius: var(--radius-xl);
		background: var(--fill-bg);
	}

	.text {
		margin: 0;
		font-family: var(--font-serif);
		font-size: var(--text-lead);
		line-height: 1.5;
		white-space: pre-line;
		overflow-wrap: anywhere;
	}

	.title {
		margin: 0;
		font-family: var(--font-serif);
		font-weight: 400;
		font-size: var(--text-heading);
		line-height: 1.25;
	}

	.summary {
		margin: 0;
		font-family: var(--font-serif);
		font-size: var(--text-body);
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}

	.address {
		margin: 0;
		font-size: var(--text-body);
		overflow-wrap: anywhere;
	}

	.removed {
		margin: 0;
		font-size: var(--text-small);
		color: var(--color-foreground-muted);
	}

	.label {
		margin: 0;
		font-size: var(--text-caption);
		font-weight: 400;
		color: var(--color-foreground-subtle);
	}

	.note {
		margin: 0;
		align-self: stretch;
		padding: 12px;
		border-radius: var(--radius-md);
		background: var(--fill-bg);
		font-size: var(--text-label);
		line-height: 1.5;
		white-space: pre-line;
	}

	.note-actions {
		display: flex;
		gap: 8px;
	}

	.note-error {
		margin: 0;
		font-size: var(--text-small);
		color: var(--color-error);
	}

	.side :global(textarea) {
		align-self: stretch;
	}

	.side section.out {
		align-items: stretch;
	}

	.quiet {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 0;
		border: 0;
		background: none;
		font: inherit;
		font-size: var(--text-small);
		color: var(--color-foreground-muted);
		cursor: pointer;
	}

	.quiet:hover {
		color: var(--color-foreground);
	}

	.chips {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
	}

	.index {
		margin: 4px 0 0;
	}

	.index dt {
		font-size: var(--text-caption);
		color: var(--color-foreground-subtle);
		margin: 0 0 4px;
	}

	.index dd {
		margin: 0 0 12px;
		font-size: var(--text-small);
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}

	@container (max-width: 760px) {
		.side {
			max-width: none;
		}
	}
</style>
