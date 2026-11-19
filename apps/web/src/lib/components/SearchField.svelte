<!--
	SearchField.svelte

	The one search box for lists: a glass icon, the drawn caret the page
	editor and the composer use, a clear button that shows only while there is
	text, and Esc to clear.

	The caret is the same bar as `lib/document/caret.css`, measured here
	instead of by ProseMirror: a hidden span set in the input's font holds the
	text before the cursor, and its width is where the bar goes. `type="text"`
	on purpose. `type="search"` brings the browser's own clear button, which
	ignores the app's type and color.
-->
<script lang="ts">
	import { onDestroy, type Snippet } from 'svelte';
	import type { HTMLInputAttributes } from 'svelte/elements';
	import Icon from '$lib/components/Icon.svelte';
	import '$lib/document/caret.css';

	let {
		value = $bindable(''),
		placeholder = 'Search',
		label,
		input = $bindable<HTMLInputElement | null>(null),
		size = 'md',
		onkeydown,
		trailing,
		...rest
	}: Omit<HTMLInputAttributes, 'size' | 'value' | 'type' | 'onkeydown'> & {
		value?: string;
		placeholder?: string;
		/** The field's accessible name. Defaults to the placeholder. */
		label?: string;
		/** The input element, for a consumer that moves focus into it. */
		input?: HTMLInputElement | null;
		/** `md` for a list's toolbar, `lg` for a page whose field is the page. */
		size?: 'md' | 'lg';
		/** Runs before the field's own keys. Call preventDefault to keep Esc. */
		onkeydown?: (e: KeyboardEvent) => void;
		/** Shown at the field's end in place of the clear button, such as a
		    key hint. */
		trailing?: Snippet;
	} = $props();

	/** Matches `caret.ts`: solid while moving, breathing after this long still. */
	const BLINK_DELAY_MS = 500;
	const CARET_HEIGHT_RATIO = 1.25;

	let mirror = $state<HTMLSpanElement | null>(null);
	let focused = $state(false);
	let composing = $state(false);
	let inputKind = $state<'typing' | 'pointer' | 'move'>('move');
	let arriving = $state(true);
	let blinking = $state(false);
	let x = $state(0);
	let y = $state(0);
	let height = $state(0);
	let collapsed = $state(true);
	let blinkTimer: ReturnType<typeof setTimeout> | null = null;
	let frame = 0;

	function place() {
		if (!input || !mirror) return;
		const start = input.selectionStart ?? value.length;
		collapsed = start === (input.selectionEnd ?? start);
		mirror.textContent = input.value.slice(0, start);
		const style = getComputedStyle(input);
		const fontSize = parseFloat(style.fontSize) || 14;
		height = fontSize * CARET_HEIGHT_RATIO;
		x = input.offsetLeft + parseFloat(style.paddingLeft) + mirror.offsetWidth - input.scrollLeft;
		y = input.offsetTop + (input.offsetHeight - height) / 2;
		restartBlink();
	}

	function schedule() {
		cancelAnimationFrame(frame);
		frame = requestAnimationFrame(place);
	}

	function restartBlink() {
		blinking = false;
		if (blinkTimer) clearTimeout(blinkTimer);
		blinkTimer = setTimeout(() => (blinking = true), BLINK_DELAY_MS);
	}

	function onKeydown(e: KeyboardEvent) {
		onkeydown?.(e);
		if (e.defaultPrevented) {
			schedule();
			return;
		}
		if (e.key === 'Escape' && value) {
			e.preventDefault();
			e.stopPropagation();
			value = '';
			schedule();
			return;
		}
		const types = (e.key.length === 1 && !e.metaKey && !e.ctrlKey) || e.key === 'Backspace' || e.key === 'Delete';
		inputKind = types ? 'typing' : 'move';
		arriving = false;
		schedule();
	}

	function onFocus() {
		focused = true;
		arriving = true;
		schedule();
	}

	function clear() {
		value = '';
		input?.focus();
		arriving = true;
		schedule();
	}

	$effect(() => {
		void value;
		schedule();
	});

	onDestroy(() => {
		cancelAnimationFrame(frame);
		if (blinkTimer) clearTimeout(blinkTimer);
	});

	const showBar = $derived(focused && collapsed && !composing);
</script>

<div class="search-field doc-caret-host" class:lg={size === 'lg'}>
	<Icon icon="mynaui:search" width={size === 'lg' ? '18' : '16'} class="search-field-icon" />
	<input
		{...rest}
		type="text"
		bind:this={input}
		bind:value
		{placeholder}
		aria-label={label ?? placeholder}
		autocomplete="off"
		spellcheck="false"
		class="search-field-input"
		onkeydown={onKeydown}
		onkeyup={schedule}
		onmousedown={() => {
			inputKind = 'pointer';
			schedule();
		}}
		onmouseup={schedule}
		onselect={schedule}
		onscroll={schedule}
		onfocus={onFocus}
		onblur={() => (focused = false)}
		oncompositionstart={() => (composing = true)}
		oncompositionend={() => {
			composing = false;
			schedule();
		}}
	/>
	<span class="search-field-mirror" bind:this={mirror} aria-hidden="true"></span>
	<div
		class="doc-caret"
		class:doc-caret--blinking={blinking}
		class:doc-caret--arriving={arriving}
		data-input={inputKind}
		hidden={!showBar}
		aria-hidden="true"
		style:height="{height}px"
		style:width="{height * 0.055 + 1.1}px"
		style:transform="translate({x}px, {y}px)"
		ontransitionend={() => (arriving = false)}
	></div>
	{#if trailing}
		<span class="search-field-trailing">{@render trailing()}</span>
	{:else if value}
		<button type="button" class="search-field-clear" onclick={clear} aria-label="Clear search">
			<Icon icon="mynaui:x" width="14" />
		</button>
	{/if}
</div>

<style>
	.search-field {
		position: relative;
		display: flex;
		align-items: center;
		flex: 1;
		min-width: 0;
		/* Fixed: a list toolbar swaps this row for selection controls, and
		   it must not change height when it does. */
		height: 34px;
		border: 1px solid var(--color-border-field);
		border-radius: var(--radius-lg);
		background: var(--color-surface);
		transition: border-color 0.25s ease;
	}
	.search-field:hover:not(:focus-within) {
		border-color: color-mix(in srgb, var(--color-border-field) 70%, var(--color-foreground));
	}
	.search-field:focus-within {
		border-color: var(--color-primary);
	}
	.search-field :global(.search-field-icon) {
		position: absolute;
		left: 0.75rem;
		color: var(--color-foreground-subtle);
		pointer-events: none;
	}
	.search-field-input {
		flex: 1;
		min-width: 0;
		height: 100%;
		padding: 0 2.25rem 0 2.25rem;
		border: 0;
		outline: 0;
		background: transparent;
		font: inherit;
		font-size: 0.875rem;
		color: var(--color-foreground);
		caret-color: transparent;
	}
	.search-field-input::placeholder {
		color: var(--color-foreground-subtle);
	}
	/* Set in the input's font so its width is the distance to the cursor. */
	.search-field-mirror {
		position: absolute;
		visibility: hidden;
		white-space: pre;
		font: inherit;
		font-size: 0.875rem;
		pointer-events: none;
	}
	.search-field-clear {
		position: absolute;
		right: 0.5rem;
		display: flex;
		align-items: center;
		justify-content: center;
		width: 1.5rem;
		height: 1.5rem;
		padding: 0;
		border: 0;
		border-radius: var(--radius-md);
		background: transparent;
		color: var(--color-foreground-subtle);
		cursor: pointer;
	}
	.search-field-clear:hover {
		background: color-mix(in srgb, var(--wash-ink) 8%, transparent);
		color: var(--color-foreground);
	}
	.search-field-trailing {
		position: absolute;
		right: 1rem;
		display: flex;
		align-items: center;
		pointer-events: none;
	}
	/* The large size: a page whose field is the point of the page. */
	.search-field.lg {
		height: 44px;
	}
	.search-field.lg :global(.search-field-icon) {
		left: 0.875rem;
	}
	.search-field.lg .search-field-input {
		padding: 0 4rem 0 2.75rem;
		font-size: 0.9375rem;
	}
	.search-field.lg .search-field-mirror {
		font-size: 0.9375rem;
	}
	@media (prefers-reduced-motion: reduce) {
		.search-field {
			transition: none;
		}
	}
</style>
