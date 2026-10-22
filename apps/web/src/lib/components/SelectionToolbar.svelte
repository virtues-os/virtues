<script lang="ts">
	/**
	 * SelectionToolbar - Floating formatting toolbar for text selection
	 *
	 * Shows above selected text with formatting buttons.
	 * Pattern follows SlashMenu - "dumb display" component
	 * that receives state from the block editor (`lib/document/toolbar.ts`).
	 *
	 * Uses the floating UI system for smart positioning.
	 */

	import IconButton from '$lib/components/IconButton.svelte';
	import { fade } from 'svelte/transition';
	import { FloatingContent, useClickOutside, useEscapeKey } from '$lib/floating';
	import type { VirtualAnchor } from '$lib/floating';
	import { TOOLBAR_OFFSET } from '$lib/document/toolbar';
	import { shortcuts } from '$lib/shortcuts/registry.svelte';

	/** On touch every button is 44pt (design-grammar §6): this bar is how a phone formats. */
	const touch = typeof window !== 'undefined' && !!window.matchMedia?.('(pointer: coarse)').matches;

	type MarkType = 'strong' | 'em' | 'underline' | 'code' | 'strikethrough' | 'link';

	interface Props {
		/** Position for absolute positioning */
		position: { x: number; y: number };
		/** Mark states (which marks are active) */
		activeMarks: {
			strong: boolean;
			em: boolean;
			underline: boolean;
			code: boolean;
			strikethrough: boolean;
			link: boolean;
		};
		/** Called when a format button is clicked */
		onFormat: (mark: MarkType) => void;
		/** Called when "Ask AI" is clicked */
		onAskAi?: () => void;
		/** Whether the mark buttons show; text that holds no marks (a code block) offers Ask AI alone. */
		marks?: boolean;
		/** Called when toolbar should close */
		onClose: () => void;
	}

	let { position, activeMarks, onFormat, onAskAi, marks = true, onClose }: Props = $props();

	let toolbarEl: HTMLDivElement | null = $state(null);

	// Convert position to virtual anchor for Floating UI
	// Include height so the arrow can point to the selection
	const virtualAnchor = $derived<VirtualAnchor>({
		x: position.x,
		y: position.y,
		width: 0,
		height: 0
	});

	// Use hooks for dismiss behavior (wrap callbacks to capture current values)
	useClickOutside(
		() => [toolbarEl],
		() => onClose(),
		() => true
	);
	useEscapeKey(() => onClose(), () => true);

	interface FormatButton {
		mark: MarkType;
		icon: string;
		label: string;
		/** Its key, as a chord (`mod+b`): shown with the keyboard's own names, ⌘ on a Mac and Ctrl elsewhere. */
		shortcut: string;
	}

	const buttons: FormatButton[] = [
		{ mark: 'strong', icon: 'ri:bold', label: 'Bold', shortcut: 'mod+b' },
		{ mark: 'em', icon: 'ri:italic', label: 'Italic', shortcut: 'mod+i' },
		{ mark: 'underline', icon: 'ri:underline', label: 'Underline', shortcut: 'mod+u' },
		{ mark: 'code', icon: 'ri:code-line', label: 'Code', shortcut: 'mod+e' },
		{ mark: 'strikethrough', icon: 'ri:strikethrough', label: 'Strikethrough', shortcut: 'mod+shift+s' },
		{ mark: 'link', icon: 'ri:link', label: 'Link', shortcut: '' },
	];

	function handleButtonClick(e: MouseEvent, mark: MarkType) {
		e.preventDefault();
		e.stopPropagation();
		onFormat(mark);
	}

	// Prevent mousedown from stealing focus from editor
	function handleMouseDown(e: MouseEvent) {
		e.preventDefault();
	}
</script>

<FloatingContent
	anchor={virtualAnchor}
	options={{ placement: 'top', offset: TOOLBAR_OFFSET, flip: true, shift: true, padding: 8 }}
	class="selection-toolbar-container"
>
	<div
		bind:this={toolbarEl}
		class="selection-toolbar"
		transition:fade={{ duration: 100 }}
		onmousedown={handleMouseDown}
		role="toolbar"
		aria-label="Text formatting"
		tabindex="-1"
	>
		{#if onAskAi}
			<IconButton
				icon="ri:sparkling-2-line"
				label="Ask AI"
				size={touch ? "touch" : "md"}
				onclick={(e) => {
					e.preventDefault();
					e.stopPropagation();
					onAskAi?.();
				}}
			/>
			{#if marks}<span class="toolbar-sep"></span>{/if}
		{/if}
		{#each marks ? buttons : [] as btn}
			<!-- Its state is said, not only drawn: a screen reader on a phone reads this bar. -->
			<IconButton
				icon={btn.icon}
				label={btn.label}
				shortcut={btn.shortcut ? shortcuts.format(btn.shortcut) : undefined}
				size={touch ? "touch" : "md"}
				pressed={!!activeMarks[btn.mark]}
				onclick={(e) => handleButtonClick(e, btn.mark)}
			/>
		{/each}
	</div>
</FloatingContent>

<style>
	/* FloatingContent wrapper styles */
	:global(.selection-toolbar-container) {
		--z-floating: 102;
		padding: 0;
		background: transparent;
		border: none;
		box-shadow: none;
	}

	.selection-toolbar {
		display: flex;
		align-items: center;
		gap: 4px;
		padding: 4px;
		background: var(--color-surface);
		border: 1px solid var(--color-border-subtle, var(--color-border));
		border-radius: var(--card-radius);
		box-shadow: var(--shadow-float);
	}

	/* Arrow pointing down (default when placement is top) */
	:global(.selection-toolbar-container[data-placement^='top']) .selection-toolbar::after {
		content: '';
		position: absolute;
		bottom: -6px;
		left: 50%;
		transform: translateX(-50%);
		border-left: 6px solid transparent;
		border-right: 6px solid transparent;
		border-top: 6px solid var(--color-border);
	}

	:global(.selection-toolbar-container[data-placement^='top']) .selection-toolbar::before {
		content: '';
		position: absolute;
		bottom: -5px;
		left: 50%;
		transform: translateX(-50%);
		border-left: 5px solid transparent;
		border-right: 5px solid transparent;
		border-top: 5px solid var(--color-surface);
		z-index: 1;
	}

	/* Arrow pointing up (when placement is bottom - flipped) */
	:global(.selection-toolbar-container[data-placement^='bottom']) .selection-toolbar::after {
		content: '';
		position: absolute;
		top: -6px;
		left: 50%;
		transform: translateX(-50%);
		border-left: 6px solid transparent;
		border-right: 6px solid transparent;
		border-bottom: 6px solid var(--color-border);
	}

	:global(.selection-toolbar-container[data-placement^='bottom']) .selection-toolbar::before {
		content: '';
		position: absolute;
		top: -5px;
		left: 50%;
		transform: translateX(-50%);
		border-left: 5px solid transparent;
		border-right: 5px solid transparent;
		border-bottom: 5px solid var(--color-surface);
		z-index: 1;
	}

	.toolbar-sep {
		width: 1px;
		align-self: stretch;
		margin: 4px 0;
		background: var(--color-border-subtle, var(--color-border));
	}
</style>
