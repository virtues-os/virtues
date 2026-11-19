<script lang="ts" generics="T extends string">
	/**
	 * A segmented control: two to five choices side by side, one picked. For a
	 * setting whose few options are worth seeing at once (text size, glass,
	 * units), in place of a menu that hides them.
	 *
	 * Arrow keys move the choice, as in a radio group. An option may carry a
	 * `style` so it shows what it picks (a font, a size).
	 */
	let {
		value,
		options,
		label,
		id,
		disabled = false,
		onchange,
	}: {
		value: T;
		options: readonly { value: T; label: string; style?: string }[];
		/** The group's accessible name. */
		label: string;
		id?: string;
		disabled?: boolean;
		onchange: (value: T) => void;
	} = $props();

	function onkeydown(e: KeyboardEvent) {
		const step = e.key === 'ArrowRight' || e.key === 'ArrowDown' ? 1 : e.key === 'ArrowLeft' || e.key === 'ArrowUp' ? -1 : 0;
		if (!step) return;
		e.preventDefault();
		const i = options.findIndex((o) => o.value === value);
		const next = options[(i + step + options.length) % options.length];
		onchange(next.value);
		const group = e.currentTarget as HTMLElement;
		requestAnimationFrame(() => group.querySelector<HTMLElement>('[aria-checked="true"]')?.focus());
	}
</script>

<div class="segmented" role="radiogroup" tabindex="-1" aria-label={label} {id} {onkeydown}>
	{#each options as option (option.value)}
		{@const picked = option.value === value}
		<button
			type="button"
			role="radio"
			aria-checked={picked}
			tabindex={picked ? 0 : -1}
			class:picked
			{disabled}
			style={option.style}
			onclick={() => !picked && onchange(option.value)}
		>
			{option.label}
		</button>
	{/each}
</div>

<style>
	.segmented {
		display: inline-flex;
		padding: 2px;
		border-radius: calc(var(--radius-md) + 2px);
		background: var(--color-surface-elevated);
	}

	button {
		height: 30px;
		padding: 0 12px;
		border: 1px solid transparent;
		border-radius: var(--radius-md);
		background: none;
		font: inherit;
		font-size: 13px;
		color: var(--color-foreground-muted);
		cursor: pointer;
		transition: background var(--motion-hover-out) var(--ease-premium);
	}

	button:hover:not(.picked):not(:disabled) {
		color: var(--color-foreground);
	}

	button.picked {
		border-color: var(--color-border);
		background: var(--color-surface);
		color: var(--color-foreground);
		cursor: default;
	}

	button:disabled {
		cursor: default;
		opacity: 0.6;
	}

	@media (prefers-reduced-motion: reduce) {
		button {
			transition: none;
		}
	}
</style>
