<!--
	NarrativeIdentitySection.svelte

	The wiki's standing answer to "who is this person?" — the "In your own
	words" DOCUMENT, presented in the wiki's own register (mast h1 like the
	overview). Read-only by design: the document is edited on its page — the
	editor, history, marginalia — never through a side textarea. The textarea
	this replaced wrote to the retired abridged copy, which the assistant read
	while the person edited something else entirely. The chapters live in
	their own room (/wiki/chapters): structure, not part of this prose.
-->

<script lang="ts">
	import { onMount } from 'svelte';
	import Markdown from '$lib/components/Markdown.svelte';
	import Icon from '$lib/components/Icon.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { getNarrativeIdentity } from '$lib/wiki/api';

	let loading = $state(true);
	let content = $state('');
	let pageId = $state('');
	let updatedAt = $state<string | null>(null);

	onMount(async () => {
		try {
			const identity = await getNarrativeIdentity();
			if (identity) {
				content = identity.content;
				pageId = identity.page_id;
				updatedAt = identity.content ? identity.updated_at : null;
			}
		} finally {
			loading = false;
		}
	});

	function editDocument() {
		if (pageId) windowShellStore.openRouteBeside(`/page/${pageId}`);
	}

	function openInterview() {
		windowShellStore.openRouteBeside('/chat/chat_narrative_interview');
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
		<h1>Narrative identity</h1>
		<p class="standfirst">
			The standing answer to who this is a record of — told in the interview,
			never inferred, and yours to correct.
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
				form. When you say "write it up" there, it lands here and on its own
				page, in your words.
			</p>
			<button class="btn primary" onclick={openInterview}>Open the interview</button>
		</div>
	{/if}
</div>

<style>
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
