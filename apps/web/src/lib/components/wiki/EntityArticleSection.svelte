<!--
	EntityArticleSection.svelte

	The wikipedia-style article at the top of an entity page, rendered in the
	same linked-prose register as the day narration (Markdown, quiet refs).

	Articles are OPT-IN. Nothing is written until someone asks, and nothing is
	maintained until they say so — two decisions, two switches.
	So the empty state is an OFFER, not a warning: the record below is the
	product, and prose is an addition to it. An earlier version promised "one
	will be written once the record holds enough", which is now simply untrue —
	nobody is waiting on a threshold, they are waiting on you.
-->

<script lang="ts">
	import Markdown from '$lib/components/Markdown.svelte';
	import TextAction from '$lib/components/TextAction.svelte';
	import { writeArticle, setArticleMaintenance, getArticle } from '$lib/wiki/api';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';

	interface Props {
		// `| null` because these come straight off the wire now, where an
		// absent column is null rather than undefined. Coercing at each of the
		// three call sites was three chances to forget.
		article?: string | null;
		articleUpdatedAt?: Date | null;
		/** The entity's name, for the offer line. */
		name: string;
		/** Subject coordinates, so this can write and maintain its own article. */
		subjectType?: 'person' | 'place' | 'organization';
		subjectId?: string;
		/** Is the record keeping this article up to date? */
		maintained?: boolean;
		/** Re-fetch the entity after a write. */
		onChanged?: () => void;
	}

	let {
		article,
		articleUpdatedAt,
		name,
		subjectType,
		subjectId,
		maintained: maintainedProp = false,
		onChanged
	}: Props = $props();

	let writing = $state(false);
	let failed = $state<string | null>(null);
	let maintained = $state(maintainedProp);
	$effect(() => {
		maintained = maintainedProp;
	});

	const canWrite = $derived(Boolean(subjectType && subjectId));

	async function write() {
		if (!subjectType || !subjectId) return;
		writing = true;
		failed = null;
		try {
			await writeArticle(subjectType, subjectId);
			onChanged?.();
		} catch (e) {
			failed = e instanceof Error ? e.message : 'Could not write the article';
		} finally {
			writing = false;
		}
	}

	// `auto`, not `always`, when switching back on: `always` exists for an
	// article the person wants revisited on every pass, and a two-state control
	// has no way to say which of the two "on" means. The queue treats them
	// identically today, so the only thing lost by choosing `auto` is a
	// distinction this button was never able to make.
	async function toggleMaintenance() {
		if (!subjectType || !subjectId) return;
		const next = !maintained;
		maintained = next;
		try {
			await setArticleMaintenance(subjectType, subjectId, next ? 'auto' : 'never');
		} catch (e) {
			maintained = !next;
			failed = e instanceof Error ? e.message : "Your server couldn't change that. Try again.";
		}
	}

	// An article IS a page — Edit opens the page editor. Editing does NOT take
	// the record's pen away: both keep writing the one document, and what
	// protects the person's words is that the server refuses any machine edit
	// that would lose them. The one-pen rule this comment used to describe was
	// overruled — see agents/record/article-resolution.md.
	async function openInEditor() {
		if (!subjectType || !subjectId) return;
		try {
			const a = await getArticle(subjectType, subjectId);
			if (a?.page_id) windowShellStore.openTabFromRoute(`/page/${a.page_id}`);
		} catch (e) {
			failed = e instanceof Error ? e.message : 'Could not open the article';
		}
	}

	const revisedLabel = $derived(
		articleUpdatedAt
			? articleUpdatedAt.toLocaleDateString('en-US', {
					month: 'long',
					day: 'numeric',
					year: 'numeric',
				})
			: null
	);
</script>

{#if article}
	<div class="article">
		<div class="article-prose">
			<Markdown content={article} refVariant="quiet" />
		</div>
		<p class="colophon">
			<!-- "Not kept" means the person switched maintenance off. It no
			     longer means "you edited it once": editing an article does not
			     take the record's pen away, so this line must not imply it. -->
			{maintained
				? 'The record wrote this and keeps it current'
				: 'The record wrote this and no longer updates it. New evidence arrives as notes'}{revisedLabel
				? ` · revised ${revisedLabel}`
				: ''}
			{#if canWrite}
				<span class="colophon-sep">·</span>
				<TextAction
					inline
					onclick={toggleMaintenance}
					title={maintained
						? 'The record revises this article as new evidence arrives, leaving anything you wrote untouched. Turning it off stops that.'
						: 'Let the record keep this updated. It edits around your own sentences rather than over them.'}
				>
					{maintained ? 'Keeping this updated' : 'Keep this updated'}
				</TextAction>
				<span class="colophon-sep">·</span>
				<TextAction
					inline
					onclick={openInEditor}
					title={maintained
						? 'Edit freely. Your sentences stay yours, and the record edits around them.'
						: 'Open in the editor.'}
				>Edit</TextAction>
			{/if}
		</p>
		{#if maintained && canWrite}
			<p class="regime-hint">
				Anything you write here stays as you wrote it - the record edits around your
				sentences, and every change it makes is listed in History, where you can put any
				version back.
			</p>
		{/if}
	</div>
{:else}
	<p class="stub">
		{#if canWrite}
			No article yet.
			<TextAction inline loading={writing} loadingLabel="Writing…" onclick={write}>
				Write the article
			</TextAction>
		{:else}
			No article yet about {name}.
		{/if}
	</p>
{/if}

{#if failed}
	<p class="article-error">{failed}</p>
{/if}

<style>
	.article-prose {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 1.0313rem;
		line-height: 1.65;
		color: var(--color-foreground);
	}

	.colophon-sep {
		margin: 0 0.25rem;
		opacity: 0.5;
	}

	.article-error {
		margin: 0.5rem 0 0;
		font-size: 0.75rem;
		color: var(--color-danger, #b00);
	}

	.colophon {
		margin: 0.75rem 0 0;
		font-size: 0.6875rem;
		letter-spacing: 0.04em;
		color: var(--color-foreground-subtle);
	}

	/* The one-pen rule, said once, quietly, before the edit — not as a modal
	   after it. */
	.regime-hint {
		margin: 0.25rem 0 0;
		font-size: 0.6875rem;
		color: var(--color-foreground-subtle);
		opacity: 0.85;
	}

	.stub {
		margin: 0;
		font-family: var(--font-serif, Georgia, serif);
		font-size: 0.9375rem;
		color: var(--color-foreground-subtle);
	}
</style>
