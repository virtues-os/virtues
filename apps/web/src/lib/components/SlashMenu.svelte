<script lang="ts">
	/**
	 * SlashMenu - Command palette for inserting blocks
	 *
	 * Triggered by typing "/" in the CodeMirror editor.
	 * Shows available commands filtered by query.
	 *
	 * Pattern follows RefPicker - "dumb display" component
	 * that receives state from the slash-commands plugin.
	 *
	 * Uses the floating UI system for smart positioning.
	 */

	import Icon from '$lib/components/Icon.svelte';
	import { onMount, tick } from 'svelte';
	import { fade } from 'svelte/transition';
	import type { SlashCommand } from '$lib/codemirror/extensions/slash-commands';
	import { FloatingContent, useClickOutside } from '$lib/floating';
	import type { VirtualAnchor } from '$lib/floating';

	interface Props {
		/** Available commands (pre-filtered by plugin) */
		commands: SlashCommand[];
		/** Position for absolute positioning */
		position: { x: number; y: number };
		/** Called when a command is selected */
		onSelect: (command: SlashCommand) => void;
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

	// One flat list — the order in slash-commands.ts is the order shown.
	const flatCommands = $derived(commands);

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			e.preventDefault();
			e.stopPropagation();
			onClose();
		} else if (e.key === 'ArrowDown') {
			e.preventDefault();
			e.stopPropagation();
			selectedIndex = Math.min(selectedIndex + 1, flatCommands.length - 1);
			scrollToSelected();
		} else if (e.key === 'ArrowUp') {
			e.preventDefault();
			e.stopPropagation();
			selectedIndex = Math.max(selectedIndex - 1, 0);
			scrollToSelected();
		} else if (e.key === 'Enter' || e.key === 'Tab') {
			e.preventDefault();
			e.stopPropagation();
			const cmd = flatCommands[selectedIndex];
			if (cmd) {
				onSelect(cmd);
			}
		}
	}

	async function scrollToSelected() {
		await tick(); // Wait for DOM to update
		const selected = menuEl?.querySelector('.command-item.selected');
		selected?.scrollIntoView({ block: 'nearest' });
	}

	function handleItemClick(cmd: SlashCommand) {
		onSelect(cmd);
	}

	onMount(() => {
		// Focus trap - capture keyboard events (uses capture for CodeMirror integration)
		document.addEventListener('keydown', handleKeydown, true);
		return () => {
			document.removeEventListener('keydown', handleKeydown, true);
		};
	});
</script>

<FloatingContent
	anchor={virtualAnchor}
	options={{ placement: 'bottom-start', offset: 4, flip: true, shift: true, padding: 8 }}
	class="slash-menu-container"
>
	<div bind:this={menuEl} class="slash-menu" transition:fade={{ duration: 100 }}>
		<div class="commands">
			{#if flatCommands.length === 0}
				<div class="empty">No matching commands</div>
			{:else}
				{#each flatCommands as cmd, index}
					<button
						class="command-item"
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
