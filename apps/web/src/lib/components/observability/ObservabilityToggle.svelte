<script lang="ts">
	import 'iconify-icon';

	let {
		isOpen = false,
		onToggle
	} = $props<{
		isOpen: boolean;
		onToggle: () => void;
	}>();
</script>

<button
	class="observability-toggle"
	class:active={isOpen}
	onclick={onToggle}
	aria-label={isOpen ? 'Hide observability panel' : 'Show observability panel'}
	aria-pressed={isOpen}
	title={isOpen ? 'Hide trace' : 'Show trace'}
>
	<iconify-icon icon="ri:pulse-line" width="18" height="18"></iconify-icon>
</button>

<style>
	.observability-toggle {
		position: absolute;
		top: 16px;
		right: 16px;
		z-index: 10;
		display: flex;
		align-items: center;
		justify-content: center;
		width: 36px;
		height: 36px;
		background: var(--color-surface-elevated, #fafafa);
		border: 1px solid var(--color-border, #e5e5e5);
		border-radius: 8px;
		cursor: pointer;
		color: var(--color-foreground-muted, #737373);
		transition: all 0.15s ease;
	}

	.observability-toggle:hover {
		background: var(--color-surface, #ffffff);
		border-color: var(--color-foreground-muted, #a3a3a3);
		color: var(--color-foreground, #171717);
	}

	.observability-toggle.active {
		background: var(--color-primary-subtle, #eff6ff);
		border-color: var(--color-primary, #3b82f6);
		color: var(--color-primary, #3b82f6);
	}
</style>
