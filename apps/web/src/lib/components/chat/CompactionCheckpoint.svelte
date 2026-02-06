<script lang="ts">
	/**
	 * CompactionCheckpoint
	 *
	 * Displays a conversation checkpoint/compaction in the chat.
	 * Shows summary version, message count, and expandable summary content.
	 */
	import Icon from '$lib/components/Icon.svelte';

	interface Props {
		version: number;
		messagesSummarized: number;
		summary: string;
		timestamp: string;
	}

	let { version, messagesSummarized, summary, timestamp }: Props = $props();
	let expanded = $state(false);

	// Format timestamp for display
	const formattedTime = $derived(() => {
		try {
			const date = new Date(timestamp);
			return date.toLocaleString(undefined, {
				month: 'short',
				day: 'numeric',
				hour: 'numeric',
				minute: '2-digit'
			});
		} catch {
			return '';
		}
	});

	// Extract first 2 lines of summary for preview
	const summaryPreview = $derived(() => {
		const lines = summary.split('\n').filter(line => line.trim());
		// Find first content lines (skip XML tags)
		const contentLines = lines.filter(line =>
			!line.startsWith('<') && !line.startsWith('</') && line.trim().length > 0
		);
		return contentLines.slice(0, 2).join('\n');
	});

	function toggleExpanded() {
		expanded = !expanded;
	}
</script>

<div class="checkpoint-card">
	<button class="checkpoint-header" onclick={toggleExpanded} type="button">
		<div class="header-left">
			<Icon icon="ri:bookmark-3-line" width="16" />
			<span class="checkpoint-title">Conversation checkpoint</span>
			<span class="checkpoint-meta">v{version}</span>
		</div>
		<div class="header-right">
			<span class="checkpoint-stats">
				{messagesSummarized} messages summarized
				{#if formattedTime()}
					<span class="separator">•</span>
					{formattedTime()}
				{/if}
			</span>
			<Icon
				icon={expanded ? 'ri:arrow-up-s-line' : 'ri:arrow-down-s-line'}
				width="18"
			/>
		</div>
	</button>

	<div class="checkpoint-content" class:expanded>
		{#if expanded}
			<pre class="summary-full">{summary}</pre>
		{:else}
			<div class="summary-preview">
				{summaryPreview()}
				{#if summary.split('\n').length > 2}
					<span class="show-more">Show more...</span>
				{/if}
			</div>
		{/if}
	</div>
</div>

<style>
	.checkpoint-card {
		border: 1px solid var(--color-border);
		border-radius: 0.5rem;
		margin: 0.75rem 0;
		background: var(--color-surface-elevated);
		overflow: hidden;
	}

	.checkpoint-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		width: 100%;
		padding: 0.625rem 0.875rem;
		background: transparent;
		border: none;
		cursor: pointer;
		font-size: 0.8125rem;
		color: var(--color-foreground);
		transition: background 0.15s ease;
	}

	.checkpoint-header:hover {
		background: var(--color-surface-hover);
	}

	.header-left {
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}

	.checkpoint-title {
		font-weight: 500;
	}

	.checkpoint-meta {
		font-size: 0.6875rem;
		font-weight: 600;
		padding: 0.125rem 0.375rem;
		background: var(--color-primary-subtle);
		color: var(--color-primary);
		border-radius: 0.25rem;
		text-transform: uppercase;
		letter-spacing: 0.025em;
	}

	.header-right {
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}

	.checkpoint-stats {
		font-size: 0.75rem;
		color: var(--color-foreground-muted);
	}

	.separator {
		margin: 0 0.25rem;
		opacity: 0.5;
	}

	.checkpoint-content {
		border-top: 1px solid var(--color-border);
		padding: 0.75rem 0.875rem;
		font-size: 0.8125rem;
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}

	.summary-preview {
		white-space: pre-wrap;
		display: -webkit-box;
		-webkit-line-clamp: 3;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}

	.show-more {
		display: inline;
		color: var(--color-primary);
		font-weight: 500;
		margin-left: 0.25rem;
	}

	.summary-full {
		white-space: pre-wrap;
		font-family: inherit;
		font-size: inherit;
		margin: 0;
		color: var(--color-foreground);
		background: transparent;
	}

	.checkpoint-content.expanded {
		max-height: 400px;
		overflow-y: auto;
	}
</style>
