<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";

	interface ContextUsage {
		percentage: number;
		tokens: number;
		window: number;
		status: "healthy" | "warning" | "critical";
	}

	interface Props {
		contextUsage: ContextUsage | undefined;
		onClick?: () => void;
		alwaysVisible?: boolean;
		showThreshold?: number;
	}

	let { contextUsage, onClick, alwaysVisible = false, showThreshold = 50 }: Props = $props();

	const isVisible = $derived(
		alwaysVisible ||
		(contextUsage && contextUsage.percentage >= showThreshold)
	);

	const statusIcon = $derived.by(() => {
		if (!contextUsage) return "ri:database-2-line";
		switch (contextUsage.status) {
			case "critical": return "ri:error-warning-fill";
			case "warning": return "ri:alert-fill";
			default: return "ri:database-2-line";
		}
	});
</script>

{#if isVisible && contextUsage}
	<button
		type="button"
		class="context-indicator"
		class:warning={contextUsage.status === "warning"}
		class:critical={contextUsage.status === "critical"}
		onclick={onClick}
		title="Context usage: {contextUsage.percentage}% ({contextUsage.tokens.toLocaleString()} / {contextUsage.window.toLocaleString()} tokens)"
	>
		<Icon icon={statusIcon} width="14" />
		<span class="percentage">{contextUsage.percentage}%</span>
	</button>
{/if}

<style>
	@reference "../../../app.css";

	.context-indicator {
		display: flex;
		align-items: center;
		gap: 4px;
		padding: 4px 8px;
		font-size: 12px;
		font-weight: 500;
		color: var(--color-foreground-muted);
		background: var(--color-surface-elevated);
		border: 1px solid var(--color-border);
		border-radius: 6px;
		cursor: pointer;
		transition: all 150ms ease;
	}

	.context-indicator:hover {
		background: var(--color-surface-overlay);
		color: var(--color-foreground);
	}

	.context-indicator.warning {
		color: var(--color-warning);
		border-color: var(--color-warning);
		background: var(--color-warning-subtle);
	}

	.context-indicator.critical {
		color: var(--color-error);
		border-color: var(--color-error);
		background: var(--color-error-subtle);
	}

	.percentage {
		font-variant-numeric: tabular-nums;
	}
</style>
