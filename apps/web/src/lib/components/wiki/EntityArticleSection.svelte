<!--
	EntityArticleSection.svelte

	The wikipedia-style article at the top of an entity page, rendered in the
	same linked-prose register as the day narration (Markdown, quiet refs).

	Articles are OPT-IN (migration 0081). Nothing is written until someone asks,
	and nothing is maintained until they say so — two decisions, two switches.
	So the empty state is an OFFER, not a warning: the record below is the
	product, and prose is an addition to it. An earlier version promised "one
	will be written once the record holds enough", which is now simply untrue —
	nobody is waiting on a threshold, they are waiting on you.
-->

<script lang="ts">
	import Markdown from '$lib/components/Markdown.svelte';
	import { writeArticle, setArticleAutoUpdate } from '$lib/wiki/api';

	interface Props {
		article?: string;
		articleUpdatedAt?: Date;
		/** The entity's name, for the offer line. */
		name: string;
		/** Subject coordinates, so this can write and maintain its own article. */
		subjectType?: 'person' | 'place' | 'organization';
		subjectId?: string;
		/** Is the record keeping this article up to date? */
		autoUpdate?: boolean;
		/** Re-fetch the entity after a write. */
		onChanged?: () => void;
	}

	let {
		article,
		articleUpdatedAt,
		name,
		subjectType,
		subjectId,
		autoUpdate = false,
		onChanged
	}: Props = $props();

	let writing = $state(false);
	let failed = $state<string | null>(null);
	let maintained = $state(autoUpdate);
	$effect(() => {
		maintained = autoUpdate;
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

	async function toggleMaintenance() {
		if (!subjectType || !subjectId) return;
		const next = !maintained;
		maintained = next;
		try {
			await setArticleAutoUpdate(subjectType, subjectId, next);
		} catch (e) {
			maintained = !next;
			failed = e instanceof Error ? e.message : 'Could not change that';
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
			Written from the record{revisedLabel ? ` · revised ${revisedLabel}` : ''}
			{#if canWrite}
				<span class="colophon-sep">·</span>
				<button type="button" class="linkish" onclick={toggleMaintenance}>
					{maintained ? 'Keeping this updated' : 'Keep this updated'}
				</button>
			{/if}
		</p>
	</div>
{:else}
	<p class="stub">
		{#if canWrite}
			No article yet.
			<button type="button" class="linkish" disabled={writing} onclick={write}>
				{writing ? 'Writing…' : `Write the article`}
			</button>
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

	/* A verb in running text, not a button that competes with the prose. The
	   offer should read as a sentence the page is saying, since most entities
	   will never have an article and a row of grey buttons on 573 pages is a
	   chore list. */
	.linkish {
		background: none;
		border: none;
		padding: 0;
		font: inherit;
		color: var(--color-accent, currentColor);
		text-decoration: underline;
		text-underline-offset: 2px;
		cursor: pointer;
	}

	.linkish:disabled {
		opacity: 0.6;
		cursor: default;
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

	.stub {
		margin: 0;
		font-family: var(--font-serif, Georgia, serif);
		font-style: italic;
		font-size: 0.9375rem;
		color: var(--color-foreground-subtle);
	}
</style>
