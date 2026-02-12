<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";

	interface Props {
		linkCount: number;
		wordCount: number;
		charCount: number;
		saving: boolean;
		isTyping: boolean;
		hasSaved: boolean;
		isConnected: boolean;
		isSynced: boolean;
		saveError: boolean;
		connectionGracePeriod: boolean;
	}

	let {
		linkCount,
		wordCount,
		charCount,
		saving,
		isTyping,
		hasSaved,
		isConnected,
		isSynced,
		saveError,
		connectionGracePeriod,
	}: Props = $props();
</script>

<div class="page-status-bar">
	<div class="status-item" title="Outgoing links">
		<Icon icon="ri:links-line" width="12" />
		<span>{linkCount}</span>
	</div>
	<div class="status-divider"></div>
	<div class="status-item" title="Word count">
		<span>{wordCount.toLocaleString()} words</span>
	</div>
	<div class="status-divider"></div>
	<div class="status-item" title="Character count">
		<span>{charCount.toLocaleString()} chars</span>
	</div>
	<div class="status-spacer"></div>
	<!-- Unified save status -->
	<div
		class="status-item status-save"
		class:saving={saving || isTyping}
		class:saved={hasSaved ||
			(isConnected &&
				isSynced &&
				!saving &&
				!isTyping &&
				!saveError)}
		class:error={saveError}
		class:offline={!isConnected && !connectionGracePeriod}
	>
		{#if !isConnected && !connectionGracePeriod}
			<Icon icon="ri:wifi-off-line" width="12" />
			<span>Offline</span>
		{:else if saving || isTyping || connectionGracePeriod || !isSynced}
			{#if isTyping}
				<svg
					width="14"
					height="14"
					viewBox="0 0 16 16"
					fill="none"
				>
					<text
						x="1"
						y="12"
						font-family="var(--font-serif)"
						font-size="11"
						font-weight="500"
						fill="currentColor">Aa</text
					>
				</svg>
			{:else}
				<Icon icon="ri:cloud-line" width="12" />
			{/if}
			<span
				>{saving
					? "Saving"
					: isTyping
						? "Typing"
						: "Syncing"}</span
			>
		{:else if saveError}
			<Icon icon="ri:error-warning-line" width="12" />
			<span>Error</span>
		{:else}
			<Icon icon="ri:cloud-line" width="12" />
			<span>Saved</span>
		{/if}
	</div>
</div>

<style>
	.page-status-bar {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 6px 16px;
		background: var(--color-background);
		border-top: 1px solid var(--color-border);
		font-size: 11px;
		color: var(--color-foreground-muted);
		flex-shrink: 0;
	}

	.status-item {
		display: flex;
		align-items: center;
		gap: 4px;
	}

	.status-divider {
		width: 1px;
		height: 10px;
		background: var(--color-border);
	}

	.status-spacer {
		flex: 1;
	}

	.status-save {
		transition: color 0.2s ease;
	}

	.status-save.saving {
		color: var(--color-foreground-muted);
	}

	.status-save.saved {
		color: var(--color-success);
	}

	.status-save.error {
		color: var(--color-error);
	}

	.status-save.offline {
		color: var(--color-warning);
	}
</style>
