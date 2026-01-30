<script lang="ts">
	/**
	 * WorkspaceDropdown - Dropdown menu for switching workspaces
	 *
	 * Shows all spaces with checkmark for current, keyboard shortcuts,
	 * and actions for new space / space settings.
	 */
	import Icon from "$lib/components/Icon.svelte";
	import Modal from "$lib/components/Modal.svelte";
	import { spaceStore } from "$lib/stores/space.svelte";
	import { onMount, onDestroy } from "svelte";

	interface Props {
		open: boolean;
		anchor: { x: number; y: number };
		onClose: () => void;
		onSpaceSettings: () => void;
		onRename?: () => void;
		onChangeIcon?: () => void;
		onChangeColor?: () => void;
	}

	let { open, anchor, onClose, onSpaceSettings, onRename, onChangeIcon, onChangeColor }: Props = $props();

	// Whether the active workspace is a system workspace (hides edit actions)
	const isSystemWorkspace = $derived(
		spaceStore.spaces.find(s => s.id === spaceStore.activeSpaceId)?.is_system ?? false
	);

	const activeAccentColor = $derived(
		spaceStore.spaces.find(s => s.id === spaceStore.activeSpaceId)?.accent_color ?? null
	);

	// New workspace modal state
	let showNewModal = $state(false);
	let newWorkspaceName = $state("");
	let isCreating = $state(false);
	let inputEl: HTMLInputElement | null = $state(null);

	let menuRef = $state<HTMLElement | null>(null);

	// Position the menu below the anchor, but keep it in viewport
	const menuStyle = $derived.by(() => {
		if (!open) return "";

		// Start below the click point
		let top = anchor.y + 8;
		let left = anchor.x;

		// Ensure menu doesn't go off-screen (we'll adjust after render if needed)
		return `top: ${top}px; left: ${left}px;`;
	});

	function handleBackdropClick(e: MouseEvent) {
		if (e.target === e.currentTarget) {
			onClose();
		}
	}

	function handleKeydown(e: KeyboardEvent) {
		if (!open) return;
		if (e.key === "Escape") {
			e.preventDefault();
			onClose();
		}
	}

	function handleSpaceClick(spaceId: string) {
		spaceStore.switchSpace(spaceId, true);
		onClose();
	}

	function handleNewSpaceClick() {
		newWorkspaceName = "";
		showNewModal = true;
	}

	function handleSettingsClick() {
		onSpaceSettings();
		onClose();
	}

	function handleRenameClick() {
		onRename?.();
		onClose();
	}

	function handleChangeIconClick() {
		onChangeIcon?.();
		onClose();
	}

	function handleChangeColorClick() {
		onChangeColor?.();
		onClose();
	}

	async function handleCreateWorkspace() {
		if (!newWorkspaceName.trim() || isCreating) return;

		isCreating = true;
		try {
			await spaceStore.createSpace(newWorkspaceName.trim());
			showNewModal = false;
			newWorkspaceName = "";
			onClose();
		} catch (error) {
			console.error("Failed to create workspace:", error);
		} finally {
			isCreating = false;
		}
	}

	function handleInputKeydown(e: KeyboardEvent) {
		if (e.key === "Enter") {
			e.preventDefault();
			handleCreateWorkspace();
		}
		if (e.key === "Escape") {
			e.preventDefault();
			showNewModal = false;
		}
	}

	// Focus input when modal opens
	$effect(() => {
		if (showNewModal && inputEl) {
			inputEl.focus();
		}
	});

	// Get keyboard shortcut for a space (1-9)
	function getShortcut(index: number): string | null {
		if (index < 9) {
			return `⌘${index + 1}`;
		}
		return null;
	}

	// Check if icon is an emoji (not an icon name like "ri:...")
	function isEmoji(val: string | null | undefined): boolean {
		if (!val) return false;
		return !val.includes(":");
	}

	onMount(() => {
		window.addEventListener("keydown", handleKeydown);
	});

	onDestroy(() => {
		if (typeof window !== "undefined") {
			window.removeEventListener("keydown", handleKeydown);
		}
	});
</script>

{#if open}
	<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
	<div class="dropdown-backdrop" onclick={handleBackdropClick}>
		<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
		<div
			bind:this={menuRef}
			class="dropdown-menu"
			style={menuStyle}
			role="menu"
			aria-label="Workspace menu"
			onclick={(e) => e.stopPropagation()}
		>
			<!-- Spaces list -->
			<div class="spaces-section">
				{#each spaceStore.spaces as space, index (space.id)}
					{@const isActive = space.id === spaceStore.activeSpaceId}
					{@const shortcut = getShortcut(index)}
					<button
						class="menu-item"
						class:active={isActive}
						onclick={() => handleSpaceClick(space.id)}
						role="menuitem"
					>
						<!-- Checkmark or spacer -->
						<span class="item-check">
							{#if isActive}
								<Icon icon="ri:check-line" width="14" />
							{/if}
						</span>

						<!-- Workspace icon -->
						<span class="item-icon">
							{#if space.is_system}
								<div class="virtues-logo">
									<div class="dot" style="left: 34%; top: 0%;"></div>
									<div class="dot" style="left: 0%; top: 67%;"></div>
									<div class="dot" style="left: 67%; top: 67%;"></div>
								</div>
							{:else if space.icon}
								{#if isEmoji(space.icon)}
									<span class="icon-emoji">{space.icon}</span>
								{:else}
									<Icon icon={space.icon} width="14" />
								{/if}
							{:else if space.accent_color}
								<div class="color-dot" style="background: {space.accent_color}"></div>
							{:else}
								<Icon icon="ri:folder-line" width="14" />
							{/if}
						</span>

						<!-- Workspace name -->
						<span class="item-label">{space.name}</span>

						<!-- Keyboard shortcut -->
						{#if shortcut}
							<span class="item-shortcut">{shortcut}</span>
						{/if}
					</button>
				{/each}
			</div>

			<!-- Divider -->
			<div class="divider"></div>

			<!-- Quick edit actions (only for non-system workspaces) -->
			{#if !isSystemWorkspace}
				<div class="actions-section">
					<button class="menu-item" onclick={handleRenameClick} role="menuitem">
						<span class="item-check"></span>
						<span class="item-icon">
							<Icon icon="ri:pencil-line" width="14" />
						</span>
						<span class="item-label">Rename</span>
					</button>

					<button class="menu-item" onclick={handleChangeIconClick} role="menuitem">
						<span class="item-check"></span>
						<span class="item-icon">
							<Icon icon="ri:emotion-line" width="14" />
						</span>
						<span class="item-label">Change Icon</span>
					</button>

					<button class="menu-item" onclick={handleChangeColorClick} role="menuitem">
						<span class="item-check"></span>
						<span class="item-icon">
							{#if activeAccentColor}
								<div class="color-dot" style="background: {activeAccentColor}"></div>
							{:else}
								<Icon icon="ri:palette-line" width="14" />
							{/if}
						</span>
						<span class="item-label">Change Color</span>
					</button>
				</div>

				<div class="divider"></div>
			{/if}

			<!-- Actions -->
			<div class="actions-section">
				<button class="menu-item" onclick={handleNewSpaceClick} role="menuitem">
					<span class="item-check"></span>
					<span class="item-icon">
						<Icon icon="ri:add-line" width="14" />
					</span>
					<span class="item-label">New Space</span>
				</button>

				<button class="menu-item" onclick={handleSettingsClick} role="menuitem">
					<span class="item-check"></span>
					<span class="item-icon">
						<Icon icon="ri:settings-3-line" width="14" />
					</span>
					<span class="item-label">Space Settings...</span>
				</button>
			</div>
		</div>
	</div>
{/if}

<!-- New Workspace Modal -->
<Modal open={showNewModal} onClose={() => (showNewModal = false)} title="New Space" width="sm">
	{#snippet children()}
		<div class="form-group">
			<label class="modal-label" for="workspace-name">Name</label>
			<input
				bind:this={inputEl}
				bind:value={newWorkspaceName}
				onkeydown={handleInputKeydown}
				id="workspace-name"
				type="text"
				class="modal-input"
				placeholder="My Space"
				autocomplete="off"
			/>
		</div>
	{/snippet}

	{#snippet footer()}
		<button class="modal-btn modal-btn-secondary" onclick={() => (showNewModal = false)}>
			Cancel
		</button>
		<button
			class="modal-btn modal-btn-primary"
			onclick={handleCreateWorkspace}
			disabled={!newWorkspaceName.trim() || isCreating}
		>
			{isCreating ? "Creating..." : "Create"}
		</button>
	{/snippet}
</Modal>

<style>
	@reference "../../../app.css";

	.dropdown-backdrop {
		position: fixed;
		inset: 0;
		z-index: 10000;
		background: transparent;
	}

	.dropdown-menu {
		position: fixed;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 8px;
		box-shadow: 0 4px 16px rgba(0, 0, 0, 0.16);
		padding: 4px;
		min-width: 200px;
		max-width: 280px;
		max-height: calc(100vh - 32px);
		overflow-y: auto;
		animation: menu-fade-in 100ms ease-out;
	}

	@keyframes menu-fade-in {
		from {
			opacity: 0;
			transform: scale(0.95);
		}
		to {
			opacity: 1;
			transform: scale(1);
		}
	}

	.spaces-section,
	.actions-section {
		display: flex;
		flex-direction: column;
	}

	.menu-item {
		display: flex;
		align-items: center;
		gap: 6px;
		width: 100%;
		padding: 6px 10px;
		border: none;
		border-radius: 6px;
		background: transparent;
		color: var(--color-foreground);
		font-size: 13px;
		text-align: left;
		cursor: pointer;
		transition: background-color 100ms ease;
	}

	.menu-item:hover {
		background: color-mix(in srgb, var(--color-foreground) 8%, transparent);
	}

	.menu-item.active {
		background: color-mix(in srgb, var(--color-primary) 12%, transparent);
	}

	.item-check {
		flex-shrink: 0;
		width: 14px;
		height: 14px;
		display: flex;
		align-items: center;
		justify-content: center;
		color: var(--color-primary);
	}

	.item-icon {
		flex-shrink: 0;
		width: 16px;
		height: 16px;
		display: flex;
		align-items: center;
		justify-content: center;
		color: var(--color-foreground-muted);
	}

	.item-label {
		flex: 1;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.item-shortcut {
		flex-shrink: 0;
		font-size: 11px;
		color: var(--color-foreground-muted);
		opacity: 0.7;
	}

	/* Virtues logo (mini version) */
	.virtues-logo {
		position: relative;
		width: 12px;
		height: 12px;
	}

	.virtues-logo .dot {
		position: absolute;
		width: 3px;
		height: 3px;
		border-radius: 50%;
		background: var(--color-foreground-muted);
	}

	.icon-emoji {
		font-size: 12px;
		line-height: 1;
	}

	.color-dot {
		width: 10px;
		height: 10px;
		border-radius: 50%;
	}

	.divider {
		height: 1px;
		background: var(--color-border);
		margin: 4px 8px;
	}

	.form-group {
		display: flex;
		flex-direction: column;
	}
</style>
