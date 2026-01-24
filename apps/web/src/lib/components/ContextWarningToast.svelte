<script lang="ts">
	import { fly } from 'svelte/transition';

	interface Props {
		usagePercentage: number;
		oncompact: () => void;
		ondismiss: () => void;
	}

	let { usagePercentage, oncompact, ondismiss }: Props = $props();
</script>

<div class="toast" transition:fly={{ y: -20, duration: 200 }}>
	<div class="content">
		<iconify-icon icon="ri:alert-line" class="icon warning" width="20"></iconify-icon>
		<div class="text">
			<span class="title">Context filling up ({usagePercentage.toFixed(0)}%)</span>
			<span class="subtitle">Older messages will be summarized at 85%</span>
		</div>
	</div>
	<div class="actions">
		<button type="button" class="btn compact" onclick={oncompact}> Compact Now </button>
		<button type="button" class="btn dismiss" onclick={ondismiss} aria-label="Dismiss">
			<iconify-icon icon="ri:close-line" width="16"></iconify-icon>
		</button>
	</div>
</div>

<style>
	.toast {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		padding: 0.75rem 1rem;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-left: 3px solid #f97316;
		border-radius: var(--radius-lg);
		box-shadow: var(--shadow-lg);
		max-width: 420px;
	}

	.content {
		display: flex;
		align-items: center;
		gap: 0.75rem;
	}

	:global(.icon.warning) {
		color: #f97316;
		font-size: 1.25rem;
		flex-shrink: 0;
	}

	.text {
		display: flex;
		flex-direction: column;
		gap: 0.125rem;
	}

	.title {
		font-size: 0.875rem;
		font-weight: 500;
		color: var(--color-text);
	}

	.subtitle {
		font-size: 0.75rem;
		color: var(--color-text-secondary);
	}

	.actions {
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}

	.btn {
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 0.375rem 0.75rem;
		border: none;
		border-radius: var(--radius-md);
		cursor: pointer;
		font-size: 0.8125rem;
		font-weight: 500;
		transition: all 0.15s ease;
	}

	.btn.compact {
		background: var(--color-primary);
		color: white;
	}

	.btn.compact:hover {
		background: var(--color-primary-hover);
	}

	.btn.dismiss {
		background: transparent;
		color: var(--color-text-secondary);
		padding: 0.375rem;
	}

	.btn.dismiss:hover {
		background: var(--color-surface-hover);
		color: var(--color-text);
	}
</style>
