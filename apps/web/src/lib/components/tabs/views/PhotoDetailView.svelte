<!--
	PhotoDetailView.svelte

	One photo favorite: the picture, and beside it your note, when and where it
	was taken, and what is in it. The side column moves below the picture when
	the pane is narrow. How your server indexed it stays one click away, as on
	a bookmark's page.
-->

<script lang="ts">
	import { Button, Page, Textarea } from '$lib';
	import Chip from '$lib/components/Chip.svelte';
	import Icon from '$lib/components/Icon.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import type { Tab } from '$lib/tabs/types';
	import { getLocalDateSlug } from '$lib/utils/dateUtils';
	import { getPhoto, updatePhotoNote, type PhotoDetailApi } from '$lib/photos/api';
	import { durationOf, pictureSrc, takenOn, titleOf } from '$lib/photos/shape';

	let { tab }: { tab: Tab } = $props();

	const id = $derived(tab.route.match(/^\/photo\/(.+)$/)?.[1] ?? '');

	let photo = $state<PhotoDetailApi | null>(null);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let pictureFailed = $state(false);

	let editing = $state(false);
	let draft = $state('');
	let savingNote = $state(false);
	let noteError = $state<string | null>(null);
	let indexOpen = $state(false);

	$effect(() => {
		const photoId = id;
		if (!photoId) {
			error = "That link doesn't name a photo. Go back to Photo favorites to find it.";
			loading = false;
			return;
		}
		loading = true;
		error = null;
		pictureFailed = false;
		getPhoto(photoId)
			.then((p) => {
				if (photoId !== id) return;
				photo = p;
				windowShellStore.updateTab(tab.id, { label: titleOf(p) });
			})
			.catch((e) => {
				if (photoId === id) error = e instanceof Error ? e.message : String(e);
			})
			.finally(() => {
				if (photoId === id) loading = false;
			});
	});

	const src = $derived(photo && !pictureFailed ? pictureSrc(photo) : null);
	const length = $derived(photo ? durationOf(photo) : null);
	const day = $derived(photo ? `/day/day_${getLocalDateSlug(new Date(photo.occurred_at))}` : '');
	const time = $derived(
		photo ? new Date(photo.occurred_at).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' }) : ''
	);

	function startEditing() {
		draft = photo?.note ?? '';
		noteError = null;
		editing = true;
	}

	/** Saving it blank deletes the note. */
	async function saveNote() {
		if (!photo || savingNote) return;
		savingNote = true;
		noteError = null;
		try {
			photo = await updatePhotoNote(photo.id, draft.trim() || null);
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

	/** The model's name as a person reads it: no provider prefix. */
	const modelName = (m: string | null) => m?.split('/').pop() ?? null;
</script>

<Page title={photo ? titleOf(photo) : 'Photo'} maxWidth="full">
	{#if loading}
		<p class="state">Opening…</p>
	{:else if error}
		<p class="state error" role="alert">
			<Icon icon="mynaui:danger-circle" width="16" />
			{error}
		</p>
	{:else if photo}
		<div class="layout">
			<div class="thing">
				{#if src}
					<div class="frame">
						<img class="picture" {src} alt={photo.description ?? titleOf(photo)} onerror={() => (pictureFailed = true)} />
						{#if length}<span class="duration">{length}</span>{/if}
					</div>
				{:else if photo.deleted_at_source}
					<p class="state">
						This photo is no longer a favorite on your iPhone, so its picture was removed. Your note stays here.
					</p>
				{:else}
					<p class="state">Your server couldn't open this picture. Reload to try again.</p>
				{/if}
			</div>

			<aside class="side">
				<section>
					<h2 class="label">Your note</h2>
					{#if editing}
						<Textarea bind:value={draft} placeholder="What do you want to remember?" disabled={savingNote} />
						{#if noteError}<p class="note-error" role="alert">{noteError}</p>{/if}
						<div class="note-actions">
							<Button variant="secondary" onclick={saveNote} loading={savingNote}>Save note</Button>
							<Button variant="ghost" onclick={() => (editing = false)} disabled={savingNote}>Cancel</Button>
						</div>
					{:else if photo.note}
						<p class="note">{photo.note}</p>
						<button type="button" class="quiet" onclick={startEditing}>Edit note</button>
					{:else}
						<Button variant="secondary" onclick={startEditing}>Add a note</Button>
					{/if}
				</section>

				<section>
					<h2 class="label">Taken</h2>
					<p class="fact">{takenOn(photo.occurred_at)}, {time}</p>
					<div class="chips">
						<Chip href={day} onclick={(e) => openRoute(e, day)}>Open the day</Chip>
						{#if photo.place}
							{@const route = `/place/${photo.place.id}`}
							<Chip href={route} onclick={(e) => openRoute(e, route)}>{photo.place.name}</Chip>
						{/if}
					</div>
				</section>

				{#if photo.description}
					<section>
						<h2 class="label">In the photo</h2>
						<p class="words">{photo.description}</p>
					</section>
				{:else if photo.state === 'waiting' && src}
					<section>
						<h2 class="label">In the photo</h2>
						<p class="fact">Your server hasn't described this photo yet.</p>
					</section>
				{/if}

				{#if photo.mentions.length}
					<section>
						<h2 class="label">Written in it</h2>
						<div class="chips">
							{#each photo.mentions as m, i (i)}
								{#if m.route}
									<Chip href={m.route} onclick={(e) => openRoute(e, m.route!)}>{m.name}</Chip>
								{:else}
									<Chip>{m.name}</Chip>
								{/if}
							{/each}
						</div>
					</section>
				{/if}

				{#if photo.state === 'described' && photo.extraction}
					{@const ex = photo.extraction}
					<section>
						<button type="button" class="quiet fold" aria-expanded={indexOpen} onclick={() => (indexOpen = !indexOpen)}>
							How Virtues indexed this
							<Icon icon={indexOpen ? 'mynaui:chevron-up' : 'mynaui:chevron-down'} width="14" />
						</button>
						{#if indexOpen}
							<dl class="index">
								{#if ex.subject?.length}<dt>In it</dt><dd>{ex.subject.join(', ')}</dd>{/if}
								{#if ex.setting}<dt>Setting</dt><dd>{ex.setting}</dd>{/if}
								{#if ex.colors?.length}<dt>Colors</dt><dd>{ex.colors.join(', ')}</dd>{/if}
								{#if ex.visible_text}<dt>Words in it</dt><dd>{ex.visible_text}</dd>{/if}
								{#if ex.likely_queries?.length}
									<dt>Search finds it with</dt>
									<dd>{ex.likely_queries.join(' · ')}</dd>
								{/if}
								{#if photo.enriched_at}
									<dt>Described</dt>
									<dd>
										{takenOn(photo.enriched_at)}{#if photo.enrichment_model}, by {modelName(photo.enrichment_model)}{/if}
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
		margin: 0;
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
		gap: 32px;
		align-items: flex-start;
	}

	.thing {
		flex: 999 1 420px;
		min-width: 0;
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

	.side :global(textarea) {
		align-self: stretch;
	}

	.frame {
		position: relative;
		width: fit-content;
		max-width: 100%;
	}

	.picture {
		display: block;
		max-width: 100%;
		max-height: 80vh;
		border-radius: var(--radius-xl);
		background: var(--fill-bg);
	}

	/* On the picture, so it is light ink on a dark chip in both themes. */
	.duration {
		position: absolute;
		right: 12px;
		bottom: 12px;
		padding: 4px 8px;
		border-radius: var(--radius-sm);
		background: rgb(0 0 0 / 0.75); /* design-ok: a chip on a photograph, the same in both themes */
		color: #fff; /* design-ok: ink on the dark chip above */
		font-size: var(--text-small);
		font-variant-numeric: tabular-nums;
	}

	.label {
		margin: 0;
		font-size: var(--text-caption);
		font-weight: 400;
		color: var(--color-foreground-subtle);
	}

	.fact {
		margin: 0;
		font-size: var(--text-label);
	}

	.words {
		margin: 0;
		font-family: var(--font-serif);
		font-size: var(--text-body);
		line-height: 1.5;
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

	@container (max-width: 720px) {
		.side {
			max-width: none;
		}
	}
</style>
