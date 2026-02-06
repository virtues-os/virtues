<script lang="ts">
	import type { WikiPage } from "$lib/wiki";
	import Icon from "$lib/components/Icon.svelte";

	interface Props {
		page: WikiPage;
	}

	let { page }: Props = $props();

	// Mock history entries - in a real app these would come from the backend
	interface HistoryEntry {
		id: string;
		timestamp: Date;
		author: "human" | "ai";
		action: "created" | "edited" | "ai_enhanced" | "citations_added";
		summary?: string;
	}

	// Generate mock history based on the page data
	const historyEntries = $derived.by(() => {
		const entries: HistoryEntry[] = [];

		// Add creation entry
		entries.push({
			id: "1",
			timestamp: page.createdAt,
			author: "human",
			action: "created",
			summary: "Page created",
		});

		// If there are citations, add an entry for when they were added
		if (page.citations.length > 0) {
			entries.push({
				id: "2",
				timestamp: new Date(
					page.createdAt.getTime() +
						(page.updatedAt.getTime() - page.createdAt.getTime()) *
							0.5,
				),
				author: "ai",
				action: "citations_added",
				summary: `Added ${page.citations.length} citation${page.citations.length > 1 ? "s" : ""}`,
			});
		}

		// Add the most recent edit
		if (page.updatedAt.getTime() !== page.createdAt.getTime()) {
			entries.push({
				id: "3",
				timestamp: page.updatedAt,
				author: page.lastEditedBy,
				action: page.lastEditedBy === "ai" ? "ai_enhanced" : "edited",
				summary:
					page.lastEditedBy === "ai"
						? "AI-assisted improvements"
						: "Content updated",
			});
		}

		// Sort by timestamp descending (most recent first)
		return entries.sort(
			(a, b) => b.timestamp.getTime() - a.timestamp.getTime(),
		);
	});

	function formatTimestamp(date: Date): string {
		const now = new Date();
		const diffMs = now.getTime() - date.getTime();
		const diffDays = Math.floor(diffMs / (1000 * 60 * 60 * 24));

		if (diffDays === 0) {
			return "Today";
		} else if (diffDays === 1) {
			return "Yesterday";
		} else if (diffDays < 7) {
			return `${diffDays} days ago`;
		} else {
			return date.toLocaleDateString("en-US", {
				month: "short",
				day: "numeric",
				year:
					date.getFullYear() !== now.getFullYear()
						? "numeric"
						: undefined,
			});
		}
	}

	function formatTime(date: Date): string {
		return date.toLocaleTimeString("en-US", {
			hour: "numeric",
			minute: "2-digit",
		});
	}

	function getActionIcon(action: HistoryEntry["action"]): string {
		switch (action) {
			case "created":
				return "ri:add-circle-line";
			case "edited":
				return "ri:edit-line";
			case "ai_enhanced":
				return "ri:magic-line";
			case "citations_added":
				return "ri:double-quotes-l";
			default:
				return "ri:history-line";
		}
	}

	function getActionLabel(action: HistoryEntry["action"]): string {
		switch (action) {
			case "created":
				return "Created";
			case "edited":
				return "Edited";
			case "ai_enhanced":
				return "AI Enhanced";
			case "citations_added":
				return "Citations Added";
			default:
				return "Updated";
		}
	}
</script>

<div class="rail-history">
	<div class="history-header">
		<Icon icon="ri:history-line" width="14" height="14"
		/>
		<span class="history-title">Edit History</span>
		<span class="history-count">{historyEntries.length}</span>
	</div>

	{#if historyEntries.length === 0}
		<div class="history-empty">
			<Icon icon="ri:history-line" width="24" height="24"
			/>
			<p>No edit history available</p>
		</div>
	{:else}
		<div class="history-timeline">
			{#each historyEntries as entry, index}
				<div class="history-entry" class:is-first={index === 0}>
					<div class="entry-timeline">
						<div
							class="entry-icon"
							class:is-ai={entry.author === "ai"}
						>
							<Icon
								icon={getActionIcon(entry.action)}
								width="12"
								height="12"
							/>
						</div>
						{#if index < historyEntries.length - 1}
							<div class="entry-line"></div>
						{/if}
					</div>
					<div class="entry-content">
						<div class="entry-header">
							<span class="entry-action"
								>{getActionLabel(entry.action)}</span
							>
							<span class="entry-time"
								>{formatTime(entry.timestamp)}</span
							>
						</div>
						<div class="entry-meta">
							<span class="entry-date"
								>{formatTimestamp(entry.timestamp)}</span
							>
							{#if entry.author === "ai"}
								<span class="entry-author entry-author--ai"
									>AI</span
								>
							{/if}
						</div>
						{#if entry.summary}
							<p class="entry-summary">{entry.summary}</p>
						{/if}
					</div>
				</div>
			{/each}
		</div>
	{/if}

	<div class="history-footer">
		<button class="history-action" disabled>
			<Icon icon="ri:time-line" width="14" height="14"
			/>
			<span>View Full History</span>
		</button>
	</div>
</div>

<style>
	.rail-history {
		display: flex;
		flex-direction: column;
		height: 100%;
	}

	.history-header {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.75rem 1rem;
		border-bottom: 1px solid var(--color-border-subtle);
		color: var(--color-foreground-muted);
	}

	.history-title {
		font-size: 0.6875rem;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		flex: 1;
	}

	.history-count {
		font-size: 0.6875rem;
		color: var(--color-foreground-subtle);
		background: var(--color-surface);
		padding: 0.125rem 0.375rem;
		border-radius: 9999px;
	}

	.history-empty {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 0.5rem;
		padding: 2rem 1rem;
		color: var(--color-foreground-subtle);
		text-align: center;
		flex: 1;
	}

	.history-empty p {
		font-size: 0.8125rem;
		margin: 0;
	}

	.history-timeline {
		flex: 1;
		overflow-y: auto;
		padding: 0.75rem 1rem;
	}

	.history-entry {
		display: flex;
		gap: 0.75rem;
		position: relative;
	}

	.entry-timeline {
		display: flex;
		flex-direction: column;
		align-items: center;
		flex-shrink: 0;
	}

	.entry-icon {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 1.5rem;
		height: 1.5rem;
		border-radius: 50%;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		color: var(--color-foreground-muted);
	}

	.entry-icon.is-ai {
		background: color-mix(in srgb, var(--color-primary) 15%, transparent);
		border-color: color-mix(in srgb, var(--color-primary) 30%, transparent);
		color: var(--color-primary);
	}

	.entry-line {
		width: 1px;
		flex: 1;
		min-height: 1rem;
		background: var(--color-border-subtle);
		margin: 0.25rem 0;
	}

	.entry-content {
		flex: 1;
		min-width: 0;
		padding-bottom: 1rem;
	}

	.history-entry:last-child .entry-content {
		padding-bottom: 0;
	}

	.entry-header {
		display: flex;
		align-items: baseline;
		gap: 0.5rem;
	}

	.entry-action {
		font-size: 0.8125rem;
		font-weight: 500;
		color: var(--color-foreground);
	}

	.entry-time {
		font-size: 0.6875rem;
		color: var(--color-foreground-subtle);
	}

	.entry-meta {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		margin-top: 0.125rem;
	}

	.entry-date {
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
	}

	.entry-author {
		font-size: 0.625rem;
		font-weight: 500;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		padding: 0.125rem 0.375rem;
		border-radius: 0.25rem;
	}

	.entry-author--ai {
		background: color-mix(in srgb, var(--color-primary) 15%, transparent);
		color: var(--color-primary);
	}

	.entry-summary {
		font-size: 0.75rem;
		color: var(--color-foreground-muted);
		margin: 0.375rem 0 0 0;
		line-height: 1.4;
	}

	.history-footer {
		padding: 0.75rem 1rem;
		border-top: 1px solid var(--color-border-subtle);
	}

	.history-action {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 0.5rem;
		width: 100%;
		padding: 0.5rem 0.75rem;
		font-size: 0.75rem;
		color: var(--color-foreground-muted);
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 0.375rem;
		cursor: pointer;
		transition: all 0.15s ease;
	}

	.history-action:hover:not(:disabled) {
		background: var(--color-surface-elevated);
		color: var(--color-foreground);
	}

	.history-action:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}
</style>
