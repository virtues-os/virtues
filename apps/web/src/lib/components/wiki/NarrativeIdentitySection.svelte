<!--
	The owner's own page.

	Titled with their NAME, because a person's page is. It used to be headed
	"Narrative identity", which is the name of the artifact rather than the
	name of the subject — and it left the owner as the only human in their own
	wiki without a page, while 573 other people had one.

	The body is the "In your own words" document: first person, theirs, and the
	only article here the editor may never touch. Read-only in this room by
	design — it is edited on its page, with the editor, history and marginalia,
	never through a side textarea.

	Around it is APPARATUS, drawn live from the record: their chapters, the
	years, the birth date the year partition starts from. None of it is
	injected into any prompt. What the assistant carries is the prose, byte for
	byte, and nothing else.
-->

<script lang="ts">
	import { onMount } from 'svelte';
	import Markdown from '$lib/components/Markdown.svelte';
	import Icon from '$lib/components/Icon.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { getMe, type MeApi } from '$lib/wiki/api';

	let loading = $state(true);
	let me = $state<MeApi | null>(null);

	const content = $derived(me?.article ?? '');
	const pageId = $derived(me?.page_id ?? '');
	const updatedAt = $derived(me?.article ? me.article_updated_at : null);

	onMount(async () => {
		try {
			me = await getMe();
		} finally {
			loading = false;
		}
	});

	function spanOf(c: { started_at: string; ended_at: string | null }): string {
		const y = (d: string | null) => (d ? d.slice(0, 4) : 'now');
		return `${y(c.started_at)} – ${y(c.ended_at)}`;
	}

	function editDocument() {
		if (pageId) windowShellStore.openRouteBeside(`/page/${pageId}`);
	}

	function openInterview() {
		windowShellStore.openRouteBeside('/chat/chat_getting_started');
	}

	const updatedLabel = $derived(
		updatedAt
			? new Date(updatedAt).toLocaleDateString('en-US', {
					month: 'long',
					day: 'numeric',
					year: 'numeric',
				})
			: null
	);
</script>

<div class="identity">
	<header class="mast">
		<h1>{me?.name ?? 'You'}</h1>
		<p class="standfirst">
			Your own page. Everything else here is written from the record; this is
			written by you, in your own words, and the record never edits it.
		</p>
	</header>

	{#if loading}
		<p class="quiet">Loading…</p>
	{:else if content}
		<article class="essay">
			<Markdown {content} />
		</article>
		<footer class="colophon">
			{#if updatedLabel}
				<span>Last revised {updatedLabel}</span>
			{/if}
			<button class="btn" onclick={editDocument} disabled={!pageId}>
				<Icon icon="ri:quill-pen-line" width="13" />
				Edit the page
			</button>
		</footer>
	{:else}
		<div class="empty">
			<p class="empty-lead">Nothing written yet.</p>
			<p class="empty-body">
				Your document is written from the interview — a conversation, not a
				form. When you close it there, it lands here and on its own page,
				in your words and in the first person.
			</p>
			<button class="btn primary" onclick={openInterview}>Open the interview</button>
		</div>
	{/if}

	{#if !loading && me}
		<!-- The apparatus: record, not prose. It is here so the page says
		     something true even before the document exists. -->
		<section class="apparatus">
			{#if me.chapters.length}
				<h2>Your chapters</h2>
				<ol class="chapters">
					{#each me.chapters as c (c.id)}
						<li>
							<span class="ch-title">{c.title ?? 'An unnamed stretch'}</span>
							<span class="ch-span">{spanOf(c)}</span>
						</li>
					{/each}
				</ol>
			{/if}

			{#if me.years.length}
				<h2>Your years</h2>
				<p class="years">
					{#each me.years as y, i (y)}<a href="/year/year_{y}">{y}</a>{#if i < me.years.length - 1}<span
								class="sep">·</span
							>{/if}{/each}
				</p>
			{/if}

			<p class="facts">
				{#if me.birth_date}
					Born {new Date(me.birth_date + 'T12:00:00').toLocaleDateString('en-US', {
						month: 'long',
						day: 'numeric',
						year: 'numeric'
					})}.
				{:else}
					<!-- Not a settings field: the year partition starts at the birth
					     date, so a life with none begins at its first record. -->
					No birth date yet — your years begin where the record does.
				{/if}
			</p>
		</section>
	{/if}
</div>

<style>
	.apparatus {
		margin-top: 2.5rem;
		border-top: 1px solid var(--color-border);
		padding-top: 1.25rem;
	}

	.apparatus h2 {
		font-family: var(--font-serif);
		font-size: 1rem;
		font-weight: 400;
		margin: 1rem 0 0.5rem;
	}

	.chapters {
		list-style: none;
		margin: 0;
		padding: 0;
	}

	.chapters li {
		display: flex;
		justify-content: space-between;
		gap: 1rem;
		padding: 0.3rem 0;
		border-bottom: 1px solid var(--color-border);
	}

	.ch-title {
		font-family: var(--font-serif);
	}

	.ch-span {
		font-size: 0.8125rem;
		color: var(--color-foreground-subtle);
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
	}

	.years a {
		color: var(--color-foreground);
		text-decoration: none;
		font-variant-numeric: tabular-nums;
	}

	.years a:hover {
		text-decoration: underline;
	}

	.sep {
		color: var(--color-foreground-subtle);
		margin: 0 0.4rem;
	}

	.facts {
		margin-top: 0.75rem;
		font-size: 0.875rem;
		color: var(--color-foreground-subtle);
	}

	.identity {
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

	/* Essay register: the serif carries it; Markdown supplies structure. */
	.essay {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 1.0625rem;
		line-height: 1.65;
		color: var(--color-foreground);
	}

	.colophon {
		display: flex;
		align-items: center;
		gap: 1rem;
		border-top: 1px solid var(--color-border);
		margin-top: 1.25rem;
		padding-top: 0.75rem;
		font-size: 0.6875rem;
		letter-spacing: 0.04em;
		color: var(--color-foreground-subtle);
	}

	.colophon .btn {
		margin-left: auto;
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

	.btn:hover:not(:disabled) {
		background: var(--color-surface-hover);
	}

	.btn:disabled {
		opacity: 0.6;
		cursor: default;
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
