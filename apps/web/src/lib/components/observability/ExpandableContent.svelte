<script lang="ts">
	import "iconify-icon";

	let {
		content,
		title = "",
		maxLines = 5,
		maxHeight = 0,
		preformatted = true,
		defaultExpanded = false,
	} = $props<{
		content: string;
		title?: string;
		maxLines?: number;
		maxHeight?: number;
		preformatted?: boolean;
		defaultExpanded?: boolean;
	}>();

	let expanded = $state(defaultExpanded);

	// Check if content exceeds max lines
	const lines = $derived(content.split("\n"));
	const needsTruncation = $derived(lines.length > maxLines);
	const displayContent = $derived(
		expanded || !needsTruncation
			? content
			: lines.slice(0, maxLines).join("\n") + "...",
	);
</script>

<div class="expandable-content" class:has-title={!!title}>
	{#if title}
		<button
			class="title-header"
			class:expanded
			onclick={() => (expanded = !expanded)}
		>
			<iconify-icon
				icon={expanded
					? "ri:arrow-down-s-line"
					: "ri:arrow-right-s-line"}
				width="16"
				height="16"
			></iconify-icon>
			<span class="title-text">{title}</span>
			{#if !expanded}
				<span class="line-count">{lines.length} lines</span>
			{/if}
		</button>
	{/if}

	{#if !title || expanded}
		<div
			class="content-wrapper"
			class:collapsible={!!title}
			style={maxHeight && !expanded ? `max-height: ${maxHeight}px` : ""}
		>
			{#if preformatted}
				<pre class="content-pre">{displayContent}</pre>
			{:else}
				<div class="content-text">{displayContent}</div>
			{/if}
		</div>

		{#if !title && needsTruncation}
			<button
				class="expand-button"
				onclick={() => (expanded = !expanded)}
			>
				<iconify-icon
					icon={expanded
						? "ri:arrow-up-s-line"
						: "ri:arrow-down-s-line"}
					width="14"
					height="14"
				></iconify-icon>
				<span
					>{expanded
						? "Show less"
						: `Show ${lines.length - maxLines} more lines`}</span
				>
			</button>
		{/if}
	{/if}
</div>

<style>
	.expandable-content {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	.expandable-content.has-title {
		background: var(--color-surface-elevated);
		border-radius: 6px;
		overflow: hidden;
	}

	.title-header {
		display: flex;
		align-items: center;
		gap: 6px;
		width: 100%;
		padding: 10px 12px;
		background: none;
		border: none;
		cursor: pointer;
		text-align: left;
		color: var(--color-foreground);
		transition: background 0.15s;
	}

	.title-header:hover {
		background: color-mix(in srgb, var(--color-foreground) 3%, transparent);
	}

	.title-text {
		flex: 1;
		font-size: 0.8125rem;
		font-weight: 500;
	}

	.line-count {
		font-size: 0.6875rem;
		color: var(--color-foreground-subtle);
	}

	.content-wrapper {
		overflow: hidden;
	}

	.content-wrapper.collapsible {
		padding: 0 12px 12px 12px;
	}

	.content-pre {
		margin: 0;
		padding: 8px;
		background: var(--color-surface-elevated);
		border-radius: 6px;
		font-family: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
		font-size: 0.75rem;
		line-height: 1.5;
		white-space: pre-wrap;
		word-break: break-word;
		overflow-x: auto;
		color: var(--color-foreground);
	}

	.has-title .content-pre {
		background: var(--color-background);
	}

	.content-text {
		font-size: 0.8125rem;
		line-height: 1.5;
		color: var(--color-foreground);
	}

	.expand-button {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		padding: 4px 8px;
		background: transparent;
		border: none;
		border-radius: 4px;
		font-size: 0.6875rem;
		color: var(--color-primary);
		cursor: pointer;
		transition: background 0.15s;
		align-self: flex-start;
	}

	.expand-button:hover {
		background: var(--color-primary-subtle);
	}
</style>
