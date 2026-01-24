<script lang="ts">
	import type { Citation, CitationContext } from "$lib/types/Citation";
	import "iconify-icon";

	let { context, onCitationClick } = $props<{
		context: CitationContext | undefined;
		onCitationClick?: (citation: Citation) => void;
	}>();

	let expanded = $state(false);

	// Only show if we have citations
	$effect(() => {
		// Auto-collapse when context changes (new message)
		expanded = false;
	});

	function toggleExpanded() {
		expanded = !expanded;
	}

	function handleCitationClick(citation: Citation) {
		if (onCitationClick) {
			onCitationClick(citation);
		}
	}

	function handleKeyDown(e: KeyboardEvent, citation: Citation) {
		if (e.key === "Enter" || e.key === " ") {
			e.preventDefault();
			handleCitationClick(citation);
		}
	}

	// Format source type for grouping display
	function formatSourceType(type: string): string {
		switch (type) {
			case "ontology":
				return "Personal Data";

			case "web_search":
				return "Web";
			case "narratives":
				return "Narratives";
			case "location":
				return "Location";
			default:
				return "Data";
		}
	}
</script>

{#if context && context.citations.length > 0}
	<div class="sources-footer" class:expanded>
		<button
			class="sources-header"
			onclick={toggleExpanded}
			aria-expanded={expanded}
		>
			<div class="header-left">
				<iconify-icon
					icon={expanded
						? "ri:arrow-down-s-line"
						: "ri:arrow-right-s-line"}
					width="16"
					height="16"
				></iconify-icon>
				<span class="header-title">Sources</span>
				<span class="citation-count">({context.citations.length})</span>
			</div>

			<!-- Preview of citation pills when collapsed -->
			{#if !expanded}
				<div class="pills-preview">
					{#each context.citations.slice(0, 4) as citation}
						<span class="preview-pill {citation.color}">
							<iconify-icon
								icon={citation.icon}
								width="12"
								height="12"
							></iconify-icon>
						</span>
					{/each}
					{#if context.citations.length > 4}
						<span class="more-count"
							>+{context.citations.length - 4}</span
						>
					{/if}
				</div>
			{/if}
		</button>

		{#if expanded}
			<ul class="sources-list">
				{#each context.citations as citation (citation.id)}
					<li class="source-item">
						<button
							class="source-button"
							onclick={() => handleCitationClick(citation)}
							onkeydown={(e) => handleKeyDown(e, citation)}
						>
							<div class="source-icon {citation.color}">
								<iconify-icon
									icon={citation.icon}
									width="16"
									height="16"
								></iconify-icon>
							</div>
							<div class="source-content">
								<div class="source-header">
									<span class="source-label"
										>{citation.label}</span
									>
									<span class="source-type"
										>{formatSourceType(
											citation.source_type,
										)}</span
									>
								</div>
								<span class="source-preview"
									>{citation.preview}</span
								>
							</div>
							<iconify-icon
								icon="ri:arrow-right-s-line"
								width="16"
								height="16"
								class="arrow-icon"
							></iconify-icon>
						</button>
					</li>
				{/each}
			</ul>
		{/if}
	</div>
{/if}

<style>
	.sources-footer {
		margin-top: 16px;
		border: 1px solid var(--color-border);
		border-radius: 8px;
		overflow: hidden;
		background: var(--color-surface);
	}

	.sources-header {
		width: 100%;
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 10px 12px;
		background: var(--color-surface-elevated);
		border: none;
		cursor: pointer;
		transition: background 0.15s;
		font-family: inherit;
	}

	.sources-header:hover {
		background: var(--color-border-subtle);
	}

	.header-left {
		display: flex;
		align-items: center;
		gap: 6px;
		color: var(--color-foreground-muted);
	}

	.header-title {
		font-size: 0.8125rem;
		font-weight: 500;
		color: var(--color-foreground);
	}

	.citation-count {
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
	}

	.pills-preview {
		display: flex;
		align-items: center;
		gap: 4px;
	}

	.preview-pill {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 24px;
		height: 24px;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 50%;
	}

	.more-count {
		font-size: 0.6875rem;
		color: var(--color-foreground-subtle);
		padding-left: 4px;
	}

	.sources-list {
		list-style: none;
		margin: 0;
		padding: 0;
		border-top: 1px solid var(--color-border);
	}

	.source-item {
		border-bottom: 1px solid var(--color-border);
	}

	.source-item:last-child {
		border-bottom: none;
	}

	.source-button {
		width: 100%;
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 12px;
		background: transparent;
		border: none;
		cursor: pointer;
		text-align: left;
		transition: background 0.15s;
		font-family: inherit;
	}

	.source-button:hover {
		background: var(--color-surface-elevated);
	}

	.source-icon {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 32px;
		height: 32px;
		background: var(--color-surface-elevated);
		border-radius: 8px;
		flex-shrink: 0;
	}

	.source-content {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.source-header {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.source-label {
		font-size: 0.875rem;
		font-weight: 500;
		color: var(--color-foreground);
	}

	.source-type {
		font-size: 0.6875rem;
		color: var(--color-foreground-subtle);
		text-transform: uppercase;
		letter-spacing: 0.025em;
	}

	.source-preview {
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.arrow-icon {
		color: var(--color-foreground-subtle);
		flex-shrink: 0;
	}

	/* Animation for expansion */
	.sources-footer.expanded .sources-list {
		animation: slide-down 0.2s ease-out;
	}

	@keyframes slide-down {
		from {
			opacity: 0;
			transform: translateY(-8px);
		}
		to {
			opacity: 1;
			transform: translateY(0);
		}
	}
</style>
