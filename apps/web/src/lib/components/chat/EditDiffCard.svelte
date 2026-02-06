<script lang="ts">
	/**
	 * EditDiffCard
	 *
	 * Displays edit_page tool results with an expandable diff view.
	 * Shows Accept/Reject buttons for pending edits.
	 */
	import { slide } from "svelte/transition";
	import Icon from "$lib/components/Icon.svelte";

	interface Props {
		/** Status of the edit */
		status: "pending" | "accepted" | "rejected" | "failed";
		/** Edit ID for tracking */
		editId?: string;
		/** Page ID being edited */
		pageId?: string;
		/** Text that was searched for (original text) */
		find: string;
		/** Replacement text (new text) */
		replace: string;
		/** Whether this is a full document replacement */
		isFullReplace?: boolean;
		/** Callback when user views the page */
		onViewPage?: () => void;
		/** Callback when user accepts the edit */
		onAccept?: () => void;
		/** Callback when user rejects the edit */
		onReject?: () => void;
	}

	let {
		status,
		editId,
		pageId,
		find,
		replace,
		isFullReplace = false,
		onViewPage,
		onAccept,
		onReject,
	}: Props = $props();

	let expanded = $state(false);

	// Build diff lines from find (deletion) and replace (addition)
	const parsedDiff = $derived(() => {
		const lines: {
			type: "context" | "addition" | "deletion";
			text: string;
		}[] = [];

		// If it's a full replace, just show the new content as additions
		if (isFullReplace || find === "") {
			if (replace) {
				lines.push({ type: "addition", text: replace });
			}
			return lines;
		}

		// Show find text as deletion (what was removed)
		if (find) {
			lines.push({ type: "deletion", text: find });
		}

		// Show replace text as addition (what was added)
		if (replace && replace !== find) {
			lines.push({ type: "addition", text: replace });
		}

		return lines;
	});

	const statusConfig = $derived(
		{
			pending: {
				icon: "ri:time-line",
				label: "Edit pending",
				color: "warning",
			},
			accepted: {
				icon: "ri:check-line",
				label: "Edit accepted",
				color: "success",
			},
			rejected: {
				icon: "ri:close-line",
				label: "Edit rejected",
				color: "error",
			},
			failed: {
				icon: "ri:error-warning-line",
				label: "Edit failed",
				color: "error",
			},
		}[status] ?? {
			icon: "ri:question-line",
			label: "Unknown",
			color: "muted",
		},
	);

	function toggleExpanded() {
		expanded = !expanded;
	}

	function handleViewPage(e: Event) {
		e.stopPropagation();
		onViewPage?.();
	}

	function handleAccept(e: Event) {
		e.stopPropagation();
		onAccept?.();
	}

	function handleReject(e: Event) {
		e.stopPropagation();
		onReject?.();
	}
</script>

<div class="edit-diff-card" class:expanded>
	<!-- Header -->
	<!-- svelte-ignore a11y_click_events_have_key_events -->
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="card-header" onclick={toggleExpanded}>
		<div class="header-left">
			<Icon icon={statusConfig.icon} width="16" />
			<span class="status-label">{statusConfig.label}</span>
		</div>
		<div class="header-right">
			{#if onViewPage}
				<button
					class="view-btn"
					onclick={handleViewPage}
					type="button"
					title="Open page"
				>
					<Icon icon="ri:external-link-line" width="16" />
				</button>
			{/if}
			<Icon
				icon={expanded ? "ri:arrow-up-s-line" : "ri:arrow-down-s-line"}
				width="18"
			/>
		</div>
	</div>

	<!-- Expanded content -->
	{#if expanded}
		<div class="card-content" transition:slide={{ duration: 200 }}>
			<div class="diff-view">
				{#each parsedDiff() as line}
					<div class="diff-line {line.type}">
						<span class="diff-marker">
							{#if line.type === "deletion"}-{:else if line.type === "addition"}+{:else}&nbsp;{/if}
						</span>
						<pre class="diff-text">{line.text}</pre>
					</div>
				{/each}
			</div>

			<!-- Accept/Reject buttons for pending edits -->
			{#if status === "pending" && (onAccept || onReject)}
				<div class="action-buttons">
					{#if onAccept}
						<button
							class="action-btn accept"
							onclick={handleAccept}
							type="button"
						>
							<Icon icon="ri:check-line" width="14" />
							Accept
						</button>
					{/if}
					{#if onReject}
						<button
							class="action-btn reject"
							onclick={handleReject}
							type="button"
						>
							<Icon icon="ri:close-line" width="14" />
							Reject
						</button>
					{/if}
				</div>
			{/if}
		</div>
	{/if}
</div>

<style>
	.edit-diff-card {
		margin: 0.5rem 0;
		border: 1px solid var(--color-border);
		border-radius: 0.5rem;
		background: var(--color-surface);
		overflow: hidden;
		font-size: 0.8125rem;
	}

	/* Header */
	.card-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		width: 100%;
		padding: 0.625rem 0.875rem;
		background: var(--color-surface);
		border: none;
		cursor: pointer;
		color: var(--color-text);
		transition: background 0.15s ease;
	}

	.card-header:hover {
		background: var(--color-surface-hover);
	}

	.header-left {
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}

	.status-label {
		font-weight: 500;
	}

	.header-right {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		color: var(--color-foreground-muted);
	}

	.view-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 0.25rem;
		background: transparent;
		border: none;
		color: var(--color-foreground-muted);
		cursor: pointer;
		border-radius: 0.25rem;
		transition: all 0.15s ease;
	}

	.view-btn:hover {
		background: var(--color-surface-hover);
		color: var(--color-text);
	}

	/* Content */
	.card-content {
		border-top: 1px solid var(--color-border);
		background: var(--color-surface);
	}

	/* Diff view */
	.diff-view {
		font-family: var(--font-mono, ui-monospace, monospace);
		font-size: 0.75rem;
		line-height: 1.5;
		max-height: 20rem;
		overflow-y: auto;
	}

	.diff-line {
		display: flex;
		padding: 0.25rem 0.5rem;
		border-left: 2px solid transparent;
	}

	.diff-line.deletion {
		background: rgba(var(--color-error-rgb, 239, 68, 68), 0.08);
		border-left-color: var(--color-error);
		color: var(--color-text);
	}

	.diff-line.addition {
		background: rgba(var(--color-success-rgb, 34, 197, 94), 0.08);
		border-left-color: var(--color-success);
		color: var(--color-text);
	}

	.diff-line.context {
		color: var(--color-foreground-muted);
	}

	.diff-marker {
		flex-shrink: 0;
		width: 1.25rem;
		font-weight: 600;
		user-select: none;
		color: inherit;
	}

	.diff-line.deletion .diff-marker {
		color: var(--color-error);
	}

	.diff-line.addition .diff-marker {
		color: var(--color-success);
	}

	.diff-text {
		flex: 1;
		margin: 0;
		white-space: pre-wrap;
		word-break: break-word;
		font-family: inherit;
		font-size: inherit;
	}

	/* Action buttons */
	.action-buttons {
		display: flex;
		gap: 0.5rem;
		padding: 0.75rem;
		border-top: 1px solid var(--color-border);
		background: var(--color-surface);
	}

	.action-btn {
		display: flex;
		align-items: center;
		gap: 0.25rem;
		padding: 0.375rem 0.75rem;
		border-radius: 0.375rem;
		font-size: 0.75rem;
		font-weight: 500;
		cursor: pointer;
		transition: all 0.15s ease;
		border: 1px solid transparent;
	}

	.action-btn.accept {
		background: color-mix(in srgb, var(--color-success) 15%, transparent);
		color: var(--color-success);
		border-color: color-mix(in srgb, var(--color-success) 30%, transparent);
	}

	.action-btn.accept:hover {
		background: var(--color-success);
		color: white;
	}

	.action-btn.reject {
		background: color-mix(in srgb, var(--color-error) 15%, transparent);
		color: var(--color-error);
		border-color: color-mix(in srgb, var(--color-error) 30%, transparent);
	}

	.action-btn.reject:hover {
		background: var(--color-error);
		color: white;
	}
</style>
