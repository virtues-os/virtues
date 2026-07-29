<!--
	EntitySummarySection.svelte

	The wikipedia-style article at the top of an entity page. Machine-written
	by the entity_summary applet, rendered in the same linked-prose register
	as the day narration (CitedMarkdown, quiet refs). While the record is too
	thin for an article, a single honest stub line stands in — never an empty
	section scaffold.
-->

<script lang="ts">
	import CitedMarkdown from '$lib/components/CitedMarkdown.svelte';

	interface Props {
		summary?: string;
		summarizedAt?: Date;
		/** The entity's name, for the stub line. */
		name: string;
	}

	let { summary, summarizedAt, name }: Props = $props();

	const revisedLabel = $derived(
		summarizedAt
			? summarizedAt.toLocaleDateString('en-US', {
					month: 'long',
					day: 'numeric',
					year: 'numeric',
				})
			: null
	);
</script>

{#if summary}
	<div class="summary">
		<div class="summary-prose">
			<CitedMarkdown content={summary} refVariant="quiet" />
		</div>
		{#if revisedLabel}
			<p class="colophon">Written from the record · revised {revisedLabel}</p>
		{/if}
	</div>
{:else}
	<p class="stub">
		No article yet — one will be written once the record holds enough about
		{name}.
	</p>
{/if}

<style>
	.summary-prose {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 1.0313rem;
		line-height: 1.65;
		color: var(--color-foreground);
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
