<script lang="ts" generics="T extends string | number">
	/**
	 * A choice among named options (design.md, "Primitives"). A field-edged
	 * 34px control that opens a menu of `MenuItem` rows on `lib/floating`, so
	 * it looks and moves like every other menu in the app: the floating
	 * surface, `--motion-enter` in and the float-out exit. A native
	 * `<select>` draws the operating system's menu instead, which ignores
	 * the theme, the type and the motion.
	 *
	 * Keyboard: Enter, Space or an arrow opens it on the chosen option; the
	 * arrows, Home and End move; a letter jumps to the next option starting
	 * with it; Enter or Space chooses; Escape or Tab closes and focus
	 * returns to the control.
	 *
	 * Options may carry a `group`: consecutive options with one group get
	 * one heading.
	 *
	 * `variant="bare"` drops the box and takes its font from the text around
	 * it, for a select set inside a sentence or an underlined form (Setup).
	 * The caller draws its edge on `.v-select`.
	 */
	import { Popover } from '$lib/floating';
	import MenuItem from '$lib/components/MenuItem.svelte';
	import Icon from '$lib/components/Icon.svelte';

	type Option = { value: T; label: string; description?: string; group?: string };

	let {
		id,
		value,
		options,
		label,
		describedby,
		placeholder = 'Choose…',
		display,
		disabled = false,
		variant = 'field',
		block = false,
		onchange
	}: {
		id?: string;
		value: T;
		options: readonly Option[];
		/** For screen readers when no `<label for>` names it. */
		label?: string;
		/** The id of a caption that explains the choice. */
		describedby?: string;
		/** Shown when `value` is none of the options. */
		placeholder?: string;
		/** Text on the control in place of the chosen option's label, when
		 *  the control says it differently (Setup's time zone). */
		display?: string;
		disabled?: boolean;
		variant?: 'field' | 'bare';
		/** Fill the container's width, as a column of selects lines up. */
		block?: boolean;
		onchange: (next: T) => void;
	} = $props();

	let open = $state(false);
	let cursor = $state(-1);
	let button: HTMLButtonElement | undefined = $state();
	let menu: HTMLDivElement | undefined = $state();
	let width = $state(0);

	const chosen = $derived(options.findIndex((o) => o.value === value));

	/** Put the keyboard on the control, for a caller handing focus back. */
	export function focus() {
		button?.focus();
	}
	const uid = `select-${Math.random().toString(36).slice(2, 9)}`;

	function show() {
		if (disabled) return;
		width = button?.offsetWidth ?? 0;
		cursor = chosen >= 0 ? chosen : 0;
		open = true;
		requestAnimationFrame(() => {
			menu?.focus();
			menu?.querySelector(`#${uid}-${cursor}`)?.scrollIntoView({ block: 'nearest' });
		});
	}

	function hide() {
		open = false;
		button?.focus();
	}

	function choose(i: number) {
		const option = options[i];
		if (!option) return;
		hide();
		if (option.value !== value) onchange(option.value);
	}

	function move(to: number) {
		cursor = Math.max(0, Math.min(options.length - 1, to));
		menu?.querySelector(`#${uid}-${cursor}`)?.scrollIntoView({ block: 'nearest' });
	}

	function onButtonKey(e: KeyboardEvent) {
		if (['ArrowDown', 'ArrowUp', 'Enter', ' '].includes(e.key)) {
			e.preventDefault();
			show();
		}
	}

	function onMenuKey(e: KeyboardEvent) {
		if (e.key === 'ArrowDown') move(cursor + 1);
		else if (e.key === 'ArrowUp') move(cursor - 1);
		else if (e.key === 'Home') move(0);
		else if (e.key === 'End') move(options.length - 1);
		else if (e.key === 'Enter' || e.key === ' ') choose(cursor);
		else if (e.key === 'Tab') {
			open = false;
			return;
		} else if (e.key.length === 1 && /\S/.test(e.key)) {
			const k = e.key.toLowerCase();
			const order = [...options.keys()].map((n) => (cursor + 1 + n) % options.length);
			const hit = order.find((n) => options[n].label.toLowerCase().startsWith(k));
			if (hit !== undefined) move(hit);
		} else return;
		e.preventDefault();
	}
</script>

<Popover bind:open placement="bottom-start" offset={4} onClose={() => button?.focus()}>
	{#snippet trigger()}
		<button
			bind:this={button}
			{id}
			type="button"
			class="v-select"
			class:bare={variant === 'bare'}
			class:block
			class:open
			{disabled}
			aria-haspopup="listbox"
			aria-expanded={open}
			aria-label={label}
			aria-describedby={describedby}
			onclick={() => (open ? hide() : show())}
			onkeydown={onButtonKey}
		>
			<span class="v-select-value" class:placeholder={!display && chosen < 0}>
				{display ?? (chosen >= 0 ? options[chosen].label : placeholder)}
			</span>
			<Icon icon="mynaui:chevron-down" width="14" />
		</button>
	{/snippet}
	{#snippet children()}
		<div
			bind:this={menu}
			class="v-select-menu"
			role="listbox"
			aria-label={label}
			aria-activedescendant={cursor >= 0 ? `${uid}-${cursor}` : undefined}
			tabindex="-1"
			style:min-width="{width}px"
			onkeydown={onMenuKey}
		>
			{#each options as option, i (option.value)}
				{#if option.group && option.group !== options[i - 1]?.group}
					<div class="v-select-group" role="presentation">{option.group}</div>
				{/if}
				<MenuItem
					id="{uid}-{i}"
					role="option"
					checked={i === chosen}
					focused={i === cursor}
					label={option.label}
					description={option.description}
					onclick={() => choose(i)}
					onmouseenter={() => (cursor = i)}
				/>
			{/each}
		</div>
	{/snippet}
</Popover>

<style>
	/* A control (design.md, "Controls"): 34px tall, on the field edge that
	   holds 3:1 against the page. */
	.v-select {
		display: inline-flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
		height: var(--control-h);
		max-width: 100%;
		padding: 0 8px 0 12px;
		border: 1px solid var(--color-border-field);
		border-radius: var(--radius-md);
		background: var(--color-surface);
		color: var(--color-foreground);
		font: inherit;
		font-size: var(--text-label);
		text-align: left;
		cursor: pointer;
		transition: background-color var(--motion-hover-out) var(--ease-premium);
	}

	.v-select:hover:not(:disabled),
	.v-select.open {
		background: color-mix(in srgb, var(--wash-ink) 4%, var(--color-surface));
		transition-duration: var(--motion-hover-in);
	}

	/* Popover wraps the control in an inline box; a block select fills it. */
	:global(.popover-trigger:has(> .v-select.block)) {
		display: block;
	}

	.v-select.block {
		width: 100%;
	}

	.v-select.bare {
		height: auto;
		padding: 0 0 0 0;
		border: 0;
		border-radius: 0;
		background: transparent;
		font: inherit;
		color: inherit;
	}

	.v-select:disabled {
		color: var(--color-foreground-disabled);
		cursor: default;
	}

	.v-select-value {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.v-select-value.placeholder {
		color: var(--color-foreground-subtle);
	}

	.v-select :global(svg) {
		flex-shrink: 0;
		color: var(--color-foreground-muted);
		transition: transform var(--motion-state) var(--ease-premium);
	}

	.v-select.open :global(svg) {
		transform: rotate(180deg);
	}

	/* FloatingContent draws the surface (fill, hairline, radius and
	   --shadow-float), the one every floating menu shares. */
	.v-select-menu {
		display: flex;
		flex-direction: column;
		max-height: min(360px, 60vh);
		overflow-y: auto;
		padding: 4px;
		outline: none;
	}

	.v-select-group {
		padding: 8px 8px 4px;
		font-size: var(--text-caption);
		color: var(--color-foreground-subtle);
	}

	.v-select-group:not(:first-child) {
		margin-top: 4px;
		border-top: 1px solid var(--color-border-subtle);
	}

	@media (pointer: coarse) {
		.v-select {
			height: var(--control-touch-h);
			font-size: 16px;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.v-select,
		.v-select :global(svg) {
			transition: none;
		}
	}
</style>
