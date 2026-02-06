<script lang="ts">
	/**
	 * TableToolbar - Floating toolbar for table manipulation
	 *
	 * Shows above tables with controls for adding/removing rows and columns.
	 * Pattern follows SelectionToolbar - "dumb display" component
	 * that receives state from the table-toolbar plugin.
	 *
	 * Uses the floating UI system for smart positioning.
	 */

	import Icon from '$lib/components/Icon.svelte';
	import { fade } from 'svelte/transition';
	import { FloatingContent, useClickOutside, useEscapeKey } from '$lib/floating';
	import type { VirtualAnchor } from '$lib/floating';

	type TableCommand =
		| 'addRowBefore'
		| 'addRowAfter'
		| 'addColumnBefore'
		| 'addColumnAfter'
		| 'deleteRow'
		| 'deleteColumn'
		| 'deleteTable';

	interface Props {
		/** Position for absolute positioning */
		position: { x: number; y: number };
		/** Called when a command button is clicked */
		onCommand: (command: TableCommand) => void;
		/** Called when toolbar should close */
		onClose: () => void;
	}

	let { position, onCommand, onClose }: Props = $props();

	let toolbarEl: HTMLDivElement | null = $state(null);

	// Convert position to virtual anchor for Floating UI
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

	interface CommandButton {
		command: TableCommand;
		icon: string;
		label: string;
		group: 'add' | 'delete';
	}

	const buttons: CommandButton[] = [
		{ command: 'addRowBefore', icon: 'ri:insert-row-top', label: 'Add row above', group: 'add' },
		{ command: 'addRowAfter', icon: 'ri:insert-row-bottom', label: 'Add row below', group: 'add' },
		{ command: 'addColumnBefore', icon: 'ri:insert-column-left', label: 'Add column left', group: 'add' },
		{ command: 'addColumnAfter', icon: 'ri:insert-column-right', label: 'Add column right', group: 'add' },
		{ command: 'deleteRow', icon: 'ri:delete-row', label: 'Delete row', group: 'delete' },
		{ command: 'deleteColumn', icon: 'ri:delete-column', label: 'Delete column', group: 'delete' },
		{ command: 'deleteTable', icon: 'ri:delete-bin-line', label: 'Delete table', group: 'delete' },
	];

	const addButtons = buttons.filter((b) => b.group === 'add');
	const deleteButtons = buttons.filter((b) => b.group === 'delete');

	function handleButtonClick(e: MouseEvent, command: TableCommand) {
		e.preventDefault();
		e.stopPropagation();
		onCommand(command);
	}

	// Prevent mousedown from stealing focus from editor
	function handleMouseDown(e: MouseEvent) {
		e.preventDefault();
	}
</script>

<FloatingContent
	anchor={virtualAnchor}
	options={{ placement: 'top', offset: 8, flip: true, shift: true, padding: 8 }}
	class="table-toolbar-container"
>
	<div
		bind:this={toolbarEl}
		class="table-toolbar"
		transition:fade={{ duration: 100 }}
		onmousedown={handleMouseDown}
		role="toolbar"
		aria-label="Table controls"
		tabindex="-1"
	>
		<!-- Add buttons -->
		{#each addButtons as btn}
			<button
				type="button"
				class="toolbar-btn add"
				onclick={(e) => handleButtonClick(e, btn.command)}
				title={btn.label}
			>
				<Icon icon={btn.icon} width="15" />
			</button>
		{/each}

		<div class="divider"></div>

		<!-- Delete buttons -->
		{#each deleteButtons as btn}
			<button
				type="button"
				class="toolbar-btn delete"
				class:danger={btn.command === 'deleteTable'}
				onclick={(e) => handleButtonClick(e, btn.command)}
				title={btn.label}
			>
				<Icon icon={btn.icon} width="15" />
			</button>
		{/each}
	</div>
</FloatingContent>

<style>
	/* FloatingContent wrapper styles */
	:global(.table-toolbar-container) {
		--z-floating: 101;
		padding: 0;
		background: transparent;
		border: none;
		box-shadow: none;
	}

	.table-toolbar {
		display: flex;
		align-items: center;
		gap: 2px;
		padding: 4px;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 8px;
		box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
	}

	.divider {
		width: 1px;
		height: 20px;
		background: var(--color-border);
		margin: 0 4px;
	}

	.toolbar-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 28px;
		height: 28px;
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

	.toolbar-btn.add:hover {
		background: var(--color-success-subtle);
		color: var(--color-success);
	}

	.toolbar-btn.delete:hover {
		background: var(--color-error-subtle);
		color: var(--color-error);
	}

	.toolbar-btn.danger {
		color: var(--color-foreground-muted);
	}

	.toolbar-btn.danger:hover {
		background: var(--color-error);
		color: white;
	}
</style>
