<script lang="ts">
	import "iconify-icon";

	let { isOpen = false, onToggle } = $props<{
		isOpen: boolean;
		onToggle: () => void;
	}>();
</script>

<button
	class="observability-toggle"
	class:active={isOpen}
	onclick={onToggle}
	aria-label={isOpen
		? "Hide observability panel"
		: "Show observability panel"}
	aria-pressed={isOpen}
	title={isOpen ? "Hide trace" : "Show trace"}
>
	<iconify-icon icon="ri:pulse-line" width="14" height="14"></iconify-icon>
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
		width: 30px;
		height: 30px;
		background: var(--color-surface-elevated);
		border: 1px solid
			color-mix(in srgb, var(--color-border) 60%, transparent);
		border-radius: 9999px;
		cursor: pointer;
		color: var(--color-foreground-muted);
		transition: all 0.15s ease;
	}

	.observability-toggle iconify-icon {
		color: var(--color-foreground-muted);
	}

	.observability-toggle:hover {
		background: var(--color-surface);
		border-color: color-mix(in srgb, var(--color-border) 80%, transparent);
		color: var(--color-foreground);
	}

	.observability-toggle.active {
		background: color-mix(
			in srgb,
			var(--color-primary-subtle) 80%,
			transparent
		);
		border-color: color-mix(in srgb, var(--color-primary) 70%, transparent);
		color: var(--color-primary);

		iconify-icon {
			color: var(--color-primary);
		}
	}
</style>
