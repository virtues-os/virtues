<!--
	NarrativeIdentitySection.svelte

	The wiki's standing answer to "who is this person?" — user-authored prose,
	set as an essay. Read view renders markdown in the serif register; the
	edit view is a plain textarea, saved whole. Deliberately one document, not
	a form: identity is written, not configured.
-->

<script lang="ts">
	import { onMount } from 'svelte';
	import Markdown from '$lib/components/Markdown.svelte';
	import {
		getNarrativeIdentity,
		updateNarrativeIdentity,
	} from '$lib/wiki/api';

	let loading = $state(true);
	let content = $state('');

	let updatedAt = $state<string | null>(null);
	let editing = $state(false);
	let draft = $state('');
	/**
	 * ~2k tokens, matching the server's ceiling (build_narrative_identity).
	 *
	 * Shown rather than enforced. This document is in every conversation, so
	 * length costs precision — a longer identity gives the assistant more
	 * surface to find a spurious connection to a routine question. But deciding
	 * what has stopped being true about yourself is a value judgment, so the
	 * page reports the overage and the person prunes. Nothing here truncates.
	 */
	const BUDGET_CHARS = 8000;
	const overBudget = $derived(draft.length > BUDGET_CHARS);
	let saving = $state(false);
	let error = $state<string | null>(null);

	onMount(async () => {
		try {
			const identity = await getNarrativeIdentity();
			if (identity) {
				content = identity.content;
				updatedAt = identity.content ? identity.updated_at : null;
			}
		} finally {
			loading = false;
		}
	});

	function startEditing() {
		draft = content;
		editing = true;
		error = null;
	}

	async function save() {
		saving = true;
		error = null;
		const updated = await updateNarrativeIdentity(draft);
		saving = false;
		if (!updated) {
			error = 'Could not save. Your writing is still here — try again.';
			return;
		}
		content = updated.content;
		updatedAt = updated.updated_at;
		editing = false;
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
	{#if loading}
		<p class="quiet">Loading…</p>
	{:else if editing}
		<!-- svelte-ignore a11y_autofocus -->
		<textarea
			class="editor"
			bind:value={draft}
			placeholder="Who is this a record of? Write it the way a careful biographer would."
			autofocus
		></textarea>
		{#if error}
			<p class="error">{error}</p>
		{/if}
		<div class="actions">
			<button class="btn" onclick={() => (editing = false)} disabled={saving}>
				Cancel
			</button>
			{#if overBudget}
				<span class="over-budget">
					{draft.length.toLocaleString()} / {BUDGET_CHARS.toLocaleString()} characters —
					past this the assistant only reads the beginning. Worth pruning.
				</span>
			{/if}
			<button class="btn primary" onclick={save} disabled={saving}>
				{saving ? 'Saving…' : 'Save'}
			</button>
		</div>
	{:else if content}
		<article class="essay">
			<Markdown {content} />
		</article>
		<footer class="colophon">
			{#if updatedLabel}
				<span>Last revised {updatedLabel}</span>
			{/if}
			<button class="btn" onclick={startEditing}>Edit</button>
		</footer>
	{:else}
		<div class="empty">
			<p class="empty-lead">Nothing written yet.</p>
			<p class="empty-body">
				This page is the standing answer to <em>who is this a record of?</em> —
				in your words, not inferred from your data. The rest of the wiki
				accumulates on its own; this one is yours to write.
			</p>
			<button class="btn primary" onclick={startEditing}>Start writing</button>
		</div>
	{/if}
</div>

<style>
	.identity {
		display: flex;
		flex-direction: column;
		gap: 1.25rem;
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
		padding-top: 0.75rem;
		font-size: 0.6875rem;
		letter-spacing: 0.04em;
		color: var(--color-foreground-subtle);
	}

	.colophon .btn {
		margin-left: auto;
	}

	.editor {
		width: 100%;
		min-height: 20rem;
		resize: vertical;
		font-family: var(--font-serif, Georgia, serif);
		font-size: 1rem;
		line-height: 1.6;
		color: var(--color-foreground);
		background: var(--color-surface-elevated);
		border: 1px solid var(--color-border);
		border-radius: 8px;
		padding: 1rem 1.125rem;
	}

	.editor:focus {
		outline: 2px solid var(--color-primary);
		outline-offset: -1px;
	}

	.actions {
		display: flex;
		justify-content: flex-end;
		gap: 0.5rem;
	}

	.btn {
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
		color: var(--color-primary-foreground, #fff);
	}

	.error {
		font-size: 0.8125rem;
		color: var(--color-danger, #b3261e);
		margin: 0;
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

	.over-budget {
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
		max-width: 34ch;
		line-height: 1.4;
	}
</style>
