<script lang="ts">
	/**
	 * SlashMenu - Command palette for inserting blocks
	 *
	 * Triggered by typing "/" in the CodeMirror editor.
	 * Shows available commands filtered by query.
	 *
	 * Pattern follows EntityPicker - "dumb display" component
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
		/** Current query (text after /) */
		query: string;
		/** Available commands (pre-filtered by plugin) */
		commands: SlashCommand[];
		/** Position for absolute positioning */
		position: { x: number; y: number };
		/** Called when a command is selected */
		onSelect: (command: SlashCommand) => void;
		/** Called when menu should close */
		onClose: () => void;
	}

	let { query, commands, position, onSelect, onClose }: Props = $props();

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

	// Group commands by group
	const groupedCommands = $derived.by(() => {
		const groups: Record<string, SlashCommand[]> = {};
		for (const cmd of commands) {
			const group = cmd.group;
			if (!groups[group]) groups[group] = [];
			groups[group].push(cmd);
		}
		return groups;
	});

	// Flat list for keyboard navigation
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
	<!-- Header -->
	{#if query}
		<div class="menu-header">
			<span class="query-label">/{query}</span>
		</div>
	{/if}

	<!-- Commands -->
	<div class="commands">
		{#if flatCommands.length === 0}
			<div class="empty">No matching commands</div>
		{:else}
			{#each Object.entries(groupedCommands) as [group, cmds]}
				<div class="command-group">
					<div class="group-header">{group}</div>
					{#each cmds as cmd}
						{@const globalIndex = flatCommands.indexOf(cmd)}
						<button
							class="command-item"
							class:selected={globalIndex === selectedIndex}
							onclick={() => handleItemClick(cmd)}
							onmouseenter={() => (selectedIndex = globalIndex)}
							type="button"
						>
							<div class="command-icon">
								<Icon icon={cmd.icon} width="16" />
							</div>
							<span class="command-label">{cmd.label}</span>
						</button>
					{/each}
				</div>
			{/each}
		{/if}
	</div>

		<!-- Footer -->
		<div class="footer">
			<span class="hint"><kbd>↑↓</kbd> navigate</span>
			<span class="hint"><kbd>↵</kbd> select</span>
			<span class="hint"><kbd>esc</kbd> close</span>
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

	.slash-menu {
		width: 280px;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 8px;
		box-shadow: 0 4px 20px rgba(0, 0, 0, 0.15);
		z-index: 101;
		overflow: hidden;
	}

	.menu-header {
		padding: 8px 12px;
		border-bottom: 1px solid var(--color-border);
		background: var(--color-surface-elevated);
	}

	.query-label {
		font-size: 12px;
		font-family: var(--font-mono);
		color: var(--color-foreground-muted);
	}

	.commands {
		max-height: 320px;
		overflow-y: auto;
	}

	.empty {
		padding: 20px;
		text-align: center;
		color: var(--color-foreground-muted);
		font-size: 13px;
	}

	.command-group {
		border-bottom: 1px solid var(--color-border);
	}

	.command-group:last-child {
		border-bottom: none;
	}

	.group-header {
		padding: 6px 12px;
		font-size: 11px;
		font-weight: 500;
		color: var(--color-foreground-muted);
		background: var(--color-surface-elevated);
		text-transform: uppercase;
		letter-spacing: 0.5px;
	}

	.command-item {
		display: flex;
		align-items: center;
		gap: 8px;
		width: 100%;
		padding: 5px 10px;
		border: none;
		background: none;
		text-align: left;
		cursor: pointer;
		color: var(--color-foreground);
	}

	.command-item.selected {
		background: var(--color-primary-subtle);
	}

	.command-icon {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 24px;
		height: 24px;
		background: var(--color-surface-elevated);
		border: 1px solid var(--color-border);
		border-radius: 5px;
		color: var(--color-foreground-muted);
		flex-shrink: 0;
	}

	.command-item.selected .command-icon {
		border-color: var(--color-primary);
		color: var(--color-primary);
	}

	.command-label {
		font-size: 13px;
		font-weight: 500;
	}

	.footer {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 8px 12px;
		border-top: 1px solid var(--color-border);
		background: var(--color-surface-elevated);
	}

	.hint {
		font-size: 11px;
		color: var(--color-foreground-subtle);
	}

	.hint kbd {
		display: inline-block;
		padding: 1px 4px;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 3px;
		font-family: inherit;
		font-size: 10px;
	}
</style>
