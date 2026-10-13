<script lang="ts" generics="T extends MenuCommand">
	/**
	 * SlashMenu - Command palette for inserting blocks
	 *
	 * Triggered by typing "/" in a page editor: CodeMirror's `SlashCommand`s
	 * or the block editor's `TreeCommand`s, both `MenuCommand`s.
	 * Shows available commands filtered by query.
	 *
	 * Pattern follows RefPicker - "dumb display" component
	 * that receives state from the editor's trigger plugin.
	 *
	 * Uses the floating UI system for smart positioning.
	 */

	import Icon from '$lib/components/Icon.svelte';
	import { onMount, tick } from 'svelte';
	import { fade } from 'svelte/transition';
	import { menuKey, type MenuCommand } from '$lib/components/menuCommand';
	import { FloatingContent, useClickOutside } from '$lib/floating';
	import { focusOwner, pointFocusAt } from '$lib/components/contextMenu/activeDescendant';
	import type { VirtualAnchor } from '$lib/floating';
	import { inComposition } from '$lib/utils/ime';

	interface Props {
		/** Available commands (pre-filtered by plugin) */
		commands: T[];
		/** Position for absolute positioning */
		position: { x: number; y: number };
		/** Called when a command is selected */
		onSelect: (command: T) => void;
		/** Called when menu should close */
		onClose: () => void;
	}

	let { commands, position, onSelect, onClose }: Props = $props();

	let selectedIndex = $state(0);
	let menuEl: HTMLDivElement | null = $state(null);

	// Convert position to virtual anchor for Floating UI
	const virtualAnchor = $derived<VirtualAnchor>({
		x: position.x,
		y: position.y,
		width: 0,
		height: 0
	});

	// Use click-outside hook instead of backdrop (wrap callback to capture current value)
	useClickOutside(
		() => [menuEl],
		() => onClose(),
		() => true
	);

	// Reset selection when commands change
	$effect(() => {
		commands; // Track commands
		selectedIndex = 0;
	});

	// One flat list: the order the editor gives is the order shown.
	const flatCommands = $derived(commands);

	function handleKeydown(e: KeyboardEvent) {
		if (inComposition(e)) return;
		const action = menuKey(e.key, flatCommands.length);
		if (!action) return;
		e.preventDefault();
		e.stopPropagation();
		if (action === 'close') {
			onClose();
		} else if (action === 'next') {
			selectedIndex = Math.min(selectedIndex + 1, flatCommands.length - 1);
			scrollToSelected();
		} else if (action === 'previous') {
			selectedIndex = Math.max(selectedIndex - 1, 0);
			scrollToSelected();
		} else {
			const cmd = flatCommands[selectedIndex];
			if (cmd) onSelect(cmd);
		}
	}

	async function scrollToSelected() {
		await tick(); // Wait for DOM to update
		const selected = menuEl?.querySelector('.command-item.selected');
		selected?.scrollIntoView({ block: 'nearest' });
	}

	function handleItemClick(cmd: T) {
		onSelect(cmd);
	}

	// The editor keeps the focus while the menu is open: it points at the
	// list and at the command the arrows reach, so a screen reader reads it.
	const LIST_ID = 'slash-menu';
	const optionId = (index: number) => `${LIST_ID}-option-${index}`;
	const owner = focusOwner();
	const pointed = owner ? pointFocusAt(owner, LIST_ID) : null;
	$effect(() => {
		pointed?.highlight(flatCommands.length ? optionId(selectedIndex) : null);
	});

	onMount(() => {
		// Focus trap - capture keyboard events (uses capture for CodeMirror integration)
		document.addEventListener('keydown', handleKeydown, true);
		return () => {
			document.removeEventListener('keydown', handleKeydown, true);
			pointed?.release();
		};
	});
</script>

<FloatingContent
	anchor={virtualAnchor}
	options={{ placement: 'bottom-start', offset: 4, flip: true, shift: true, padding: 8 }}
	class="slash-menu-container"
>
	<div bind:this={menuEl} class="slash-menu" transition:fade={{ duration: 100 }}>
		<div class="commands" id={LIST_ID} role="listbox" aria-label="Commands">
			{#if flatCommands.length === 0}
				<div class="empty" role="presentation">No matching commands</div>
			{:else}
				{#each flatCommands as cmd, index}
					<button
						class="command-item"
						id={optionId(index)}
						role="option"
						aria-selected={index === selectedIndex}
						class:selected={index === selectedIndex}
						onclick={() => handleItemClick(cmd)}
						onmouseenter={() => (selectedIndex = index)}
						type="button"
					>
						<Icon icon={cmd.icon} width="17" />
						<span class="command-label">{cmd.label}</span>
					</button>
				{/each}
			{/if}
		</div>
	</div>
</FloatingContent>

<style>
	/* FloatingContent wrapper styles */
	:global(.slash-menu-container) {
		--z-floating: 101;
		padding: 0;
		background: transparent;
		border: none;
		box-shadow: none;
	}

	/* A short list of obvious choices needs no chrome around it: no query
	   echo (it is already on the line behind the menu), no group headers, no
	   keyboard-hint footer. What is left is the list. */
	.slash-menu {
		width: 232px;
		background: var(--color-surface);
		border: 1px solid var(--color-border-subtle, var(--color-border));
		border-radius: 12px;
		box-shadow: 0 10px 32px rgba(0, 0, 0, 0.1);
		z-index: var(--z-overlay);
		overflow: hidden;
	}

	.commands {
		max-height: 340px;
		overflow-y: auto;
		padding: 6px;
	}

	.empty {
		padding: 14px 10px;
		text-align: center;
		color: var(--color-foreground-muted);
		font-size: 13px;
	}

	.command-item {
		display: flex;
		align-items: center;
		gap: 11px;
		width: 100%;
		padding: 7px 10px;
		border: none;
		background: none;
		text-align: left;
		cursor: pointer;
		/* The icon inherits this, so the whole row lifts together on select. */
		color: var(--color-foreground-muted);
		border-radius: 8px;
		transition:
			color 0.12s ease,
			background-color 0.12s ease;
	}

	/* A thumb's 44pt, inside the row (design-grammar §6). */
	@media (pointer: coarse) {
		.command-item {
			min-height: 44px;
		}
	}

	/* One highlight, driven by keyboard AND hover (mouseenter moves the
	   selection), so the menu never shows two candidate rows at once. */
	.command-item.selected {
		background: var(--color-primary-subtle);
		color: var(--color-foreground);
	}

	.command-label {
		font-size: 13.5px;
		font-weight: 450;
		color: var(--color-foreground);
	}
</style>
