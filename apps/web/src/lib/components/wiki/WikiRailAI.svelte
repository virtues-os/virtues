<script lang="ts">
	import "iconify-icon";

	interface Props {
		pageTitle: string;
		pageSlug: string;
	}

	let { pageTitle, pageSlug }: Props = $props();

	// Mock AI suggestions
	const suggestions = [
		{
			id: "1",
			type: "enhance",
			title: "Expand narrative section",
			description:
				"Add more detail about the emotional journey during this period",
		},
		{
			id: "2",
			type: "cite",
			title: "Add citations",
			description:
				"Found 3 potential sources in your data that could support claims",
		},
		{
			id: "3",
			type: "link",
			title: "Link related pages",
			description:
				"This page could connect to 2 other pages in your wiki",
		},
	];

	function getSuggestionIcon(type: string): string {
		switch (type) {
			case "enhance":
				return "ri:quill-pen-line";
			case "cite":
				return "ri:double-quotes-l";
			case "link":
				return "ri:links-line";
			default:
				return "ri:lightbulb-line";
		}
	}
</script>

<div class="rail-ai">
	<div class="ai-header">
		<iconify-icon icon="ri:magic-line" width="14" height="14"
		></iconify-icon>
		<span class="ai-title">AI Assistant</span>
	</div>

	<div class="ai-content">
		<!-- Quick actions -->
		<div class="ai-actions">
			<button class="ai-action">
				<iconify-icon icon="ri:quill-pen-line" width="16" height="16"
				></iconify-icon>
				<span>Enhance</span>
			</button>
			<button class="ai-action">
				<iconify-icon icon="ri:search-line" width="16" height="16"
				></iconify-icon>
				<span>Find citations</span>
			</button>
			<button class="ai-action">
				<iconify-icon icon="ri:translate-2" width="16" height="16"
				></iconify-icon>
				<span>Improve clarity</span>
			</button>
		</div>

		<!-- Suggestions -->
		<div class="ai-suggestions">
			<div class="suggestions-header">
				<span>Suggestions</span>
			</div>
			<ul class="suggestions-list">
				{#each suggestions as suggestion}
					<li class="suggestion-item">
						<div class="suggestion-icon">
							<iconify-icon
								icon={getSuggestionIcon(suggestion.type)}
								width="14"
								height="14"
							></iconify-icon>
						</div>
						<div class="suggestion-content">
							<span class="suggestion-title"
								>{suggestion.title}</span
							>
							<span class="suggestion-description"
								>{suggestion.description}</span
							>
						</div>
					</li>
				{/each}
			</ul>
		</div>
	</div>

	<div class="ai-footer">
		<div class="ai-input-wrapper">
			<input
				type="text"
				class="ai-input"
				placeholder="Ask about this page..."
			/>
			<button class="ai-send" aria-label="Send message">
				<iconify-icon icon="ri:send-plane-line" width="16" height="16"
				></iconify-icon>
			</button>
		</div>
	</div>
</div>

<style>
	.rail-ai {
		display: flex;
		flex-direction: column;
		height: 100%;
	}

	.ai-header {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.75rem 1rem;
		border-bottom: 1px solid var(--color-border-subtle);
		color: var(--color-foreground-muted);
	}

	.ai-title {
		font-size: 0.6875rem;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}

	.ai-content {
		flex: 1;
		overflow-y: auto;
		padding: 1rem;
	}

	/* Quick actions */
	.ai-actions {
		display: flex;
		flex-wrap: wrap;
		gap: 0.5rem;
		margin-bottom: 1.5rem;
	}

	.ai-action {
		display: flex;
		align-items: center;
		gap: 0.375rem;
		padding: 0.375rem 0.625rem;
		font-size: 0.75rem;
		color: var(--color-foreground-muted);
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 0.375rem;
		cursor: pointer;
		transition: all 0.15s ease;
	}

	.ai-action:hover {
		background: var(--color-surface-elevated);
		border-color: var(--color-border-strong);
		color: var(--color-foreground);
	}

	/* Suggestions */
	.ai-suggestions {
		/* Container */
	}

	.suggestions-header {
		font-size: 0.6875rem;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--color-foreground-subtle);
		margin-bottom: 0.75rem;
	}

	.suggestions-list {
		list-style: none;
		margin: 0;
		padding: 0;
	}

	.suggestion-item {
		display: flex;
		gap: 0.75rem;
		padding: 0.75rem;
		margin: 0 -0.75rem;
		border-radius: 0.375rem;
		cursor: pointer;
		transition: background 0.15s ease;
	}

	.suggestion-item:hover {
		background: var(--color-surface);
	}

	.suggestion-icon {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 1.75rem;
		height: 1.75rem;
		border-radius: 0.375rem;
		background: color-mix(in srgb, var(--color-primary) 10%, transparent);
		color: var(--color-primary);
		flex-shrink: 0;
	}

	.suggestion-content {
		flex: 1;
		min-width: 0;
	}

	.suggestion-title {
		display: block;
		font-size: 0.8125rem;
		font-weight: 500;
		color: var(--color-foreground);
	}

	.suggestion-description {
		display: block;
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
		margin-top: 0.125rem;
	}

	/* Footer with input */
	.ai-footer {
		padding: 0.75rem 1rem;
		border-top: 1px solid var(--color-border-subtle);
	}

	.ai-input-wrapper {
		display: flex;
		gap: 0.5rem;
	}

	.ai-input {
		flex: 1;
		padding: 0.5rem 0.75rem;
		font-size: 0.8125rem;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 0.375rem;
		color: var(--color-foreground);
	}

	.ai-input::placeholder {
		color: var(--color-foreground-subtle);
	}

	.ai-input:focus {
		outline: none;
		border-color: var(--color-primary);
	}

	.ai-send {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 2rem;
		height: 2rem;
		background: var(--color-primary);
		border: none;
		border-radius: 0.375rem;
		color: white;
		cursor: pointer;
		transition: opacity 0.15s ease;
	}

	.ai-send:hover {
		opacity: 0.9;
	}
</style>

