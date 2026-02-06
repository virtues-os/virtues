<script lang="ts">
	/**
	 * SelectionToolbar - Floating formatting toolbar for text selection
	 *
	 * Shows above selected text with formatting buttons.
	 * Pattern follows SlashMenu - "dumb display" component
	 * that receives state from the selection-toolbar plugin.
	 *
	 * Uses the floating UI system for smart positioning.
	 */

	import Icon from '$lib/components/Icon.svelte';
	import { fade } from 'svelte/transition';
	import { FloatingContent, useClickOutside, useEscapeKey } from '$lib/floating';
	import type { VirtualAnchor } from '$lib/floating';

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
		/** Called when toolbar should close */
		onClose: () => void;
	}

	let { position, activeMarks, onFormat, onClose }: Props = $props();

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
		shortcut: string;
	}

	const buttons: FormatButton[] = [
		{ mark: 'strong', icon: 'ri:bold', label: 'Bold', shortcut: 'Cmd+B' },
		{ mark: 'em', icon: 'ri:italic', label: 'Italic', shortcut: 'Cmd+I' },
		{ mark: 'underline', icon: 'ri:underline', label: 'Underline', shortcut: 'Cmd+U' },
		{ mark: 'code', icon: 'ri:code-line', label: 'Code', shortcut: 'Cmd+E' },
		{ mark: 'strikethrough', icon: 'ri:strikethrough', label: 'Strikethrough', shortcut: 'Cmd+Shift+S' },
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
	options={{ placement: 'top', offset: 8, flip: true, shift: true, padding: 8 }}
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
		{#each buttons as btn}
			<button
				type="button"
				class="toolbar-btn"
				class:active={activeMarks[btn.mark]}
				onclick={(e) => handleButtonClick(e, btn.mark)}
				title={btn.shortcut ? `${btn.label} (${btn.shortcut})` : btn.label}
			>
				<Icon icon={btn.icon} width="16" />
			</button>
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
		gap: 2px;
		padding: 4px;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 8px;
		box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
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

	.toolbar-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 32px;
		height: 32px;
		border: none;
		background: transparent;
		border-radius: 6px;
		cursor: pointer;
		color: var(--color-foreground-muted);
		transition: all 0.1s ease;
	}

	.toolbar-btn:hover {
		background: var(--color-surface-elevated);
		color: var(--color-foreground);
	}

	.toolbar-btn.active {
		background: var(--color-primary-subtle);
		color: var(--color-primary);
	}

	.toolbar-btn:hover.active {
		background: var(--color-primary-subtle);
	}
</style>
