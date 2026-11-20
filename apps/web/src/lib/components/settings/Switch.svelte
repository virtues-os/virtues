<script lang="ts">
	/**
	 * The on/off switch settings use: a track that fills with the accent, the
	 * color that means "now" (design.md), with a knob that slides across.
	 * `label` names it for screen readers when the row's words are not its
	 * label.
	 */
	let {
		checked,
		label,
		disabled = false,
		onchange,
	}: {
		checked: boolean;
		label: string;
		disabled?: boolean;
		onchange: (next: boolean) => void;
	} = $props();
</script>

<button
	type="button"
	class="switch"
	class:on={checked}
	role="switch"
	aria-checked={checked}
	aria-label={label}
	{disabled}
	onclick={() => onchange(!checked)}
></button>

<style>
	.switch {
		width: 44px;
		height: 26px;
		border-radius: var(--radius-full);
		background: var(--color-border);
		position: relative;
		flex: none;
		border: 0;
		cursor: pointer;
		transition: background var(--motion-state) var(--ease-premium);
		padding: 0;
	}

	.switch.on {
		background: var(--color-primary);
	}

	.switch::after {
		content: '';
		position: absolute;
		top: 3px;
		left: 3px;
		width: 20px;
		height: 20px;
		border-radius: 50%;
		background: #fff;
		box-shadow: 0 1px 2px rgba(0, 0, 0, 0.18);
		transition: transform var(--motion-state) var(--ease-premium);
	}

	.switch.on::after {
		transform: translateX(18px);
	}

	.switch:disabled {
		cursor: default;
		opacity: 0.6;
	}

	@media (prefers-reduced-motion: reduce) {
		.switch,
		.switch::after {
			transition: none;
		}
	}
</style>
