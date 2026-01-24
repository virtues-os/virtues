<script lang="ts">
	import "iconify-icon";
	import { workspaceStore } from "$lib/stores/workspace.svelte";
	import type { WorkspaceSummary } from "$lib/api/client";
	import Modal from "$lib/components/Modal.svelte";

	// Modal state
	let showNewModal = $state(false);
	let newWorkspaceName = $state("");
	let isCreating = $state(false);
	let inputEl: HTMLInputElement | null = $state(null);

	// Track swipe
	let touchStartX = 0;
	let swiping = false;

	function handlePrev() {
		workspaceStore.navigateWorkspace("prev");
	}

	function handleNext() {
		workspaceStore.navigateWorkspace("next");
	}

	function handleDotClick(workspace: WorkspaceSummary) {
		workspaceStore.switchWorkspace(workspace.id);
	}

	function handleTouchStart(e: TouchEvent) {
		touchStartX = e.touches[0].clientX;
		swiping = true;
	}

	function handleTouchEnd(e: TouchEvent) {
		if (!swiping) return;
		swiping = false;

		const touchEndX = e.changedTouches[0].clientX;
		const diff = touchEndX - touchStartX;
		const threshold = 50;

		if (diff > threshold) {
			handlePrev();
		} else if (diff < -threshold) {
			handleNext();
		}
	}

	// Current workspace index for dot highlighting
	const currentIndex = $derived(
		workspaceStore.workspaces.findIndex(
			(w) => w.id === workspaceStore.activeWorkspaceId
		)
	);

	// Show max 5 dots, centered around current
	const visibleDots = $derived.by(() => {
		const total = workspaceStore.workspaces.length;
		if (total <= 5) return workspaceStore.workspaces;

		const start = Math.max(0, Math.min(currentIndex - 2, total - 5));
		return workspaceStore.workspaces.slice(start, start + 5);
	});

	function openNewModal() {
		newWorkspaceName = "";
		showNewModal = true;
	}

	function closeNewModal() {
		showNewModal = false;
		newWorkspaceName = "";
	}

	async function handleCreateWorkspace() {
		if (!newWorkspaceName.trim() || isCreating) return;
		
		isCreating = true;
		try {
			await workspaceStore.createWorkspace(newWorkspaceName.trim());
			closeNewModal();
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
	}

	// Focus input when modal opens
	$effect(() => {
		if (showNewModal && inputEl) {
			inputEl.focus();
		}
	});
</script>

<div
	class="workspace-switcher"
	ontouchstart={handleTouchStart}
	ontouchend={handleTouchEnd}
	role="navigation"
	aria-label="Workspace navigation"
>
	<button
		class="nav-btn"
		onclick={handlePrev}
		aria-label="Previous workspace"
		disabled={workspaceStore.workspaces.length <= 1}
	>
		<iconify-icon icon="ri:arrow-left-s-line" width="16"></iconify-icon>
	</button>

	<div class="dots">
		{#each visibleDots as workspace}
			<button
				class="dot"
				class:active={workspace.id === workspaceStore.activeWorkspaceId}
				class:system={workspace.is_system}
				onclick={() => handleDotClick(workspace)}
				title={workspace.name}
				aria-label={workspace.name}
				aria-current={workspace.id === workspaceStore.activeWorkspaceId
					? "true"
					: undefined}
			>
				{#if workspace.is_system}
					<div class="dot-inner system-dot"></div>
				{:else if workspace.accent_color}
					<div
						class="dot-inner"
						style="background: {workspace.accent_color}"
					></div>
				{:else}
					<div class="dot-inner"></div>
				{/if}
			</button>
		{/each}
		<button
			class="add-dot"
			onclick={openNewModal}
			title="New workspace"
			aria-label="Create new workspace"
		>
			<iconify-icon icon="ri:add-line" width="10"></iconify-icon>
		</button>
	</div>

	<button
		class="nav-btn"
		onclick={handleNext}
		aria-label="Next workspace"
		disabled={workspaceStore.workspaces.length <= 1}
	>
		<iconify-icon icon="ri:arrow-right-s-line" width="16"></iconify-icon>
	</button>
</div>

<Modal open={showNewModal} onClose={closeNewModal} title="New Workspace" width="sm">
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
				placeholder="My Workspace"
				autocomplete="off"
			/>
		</div>
	{/snippet}

	{#snippet footer()}
		<button class="modal-btn modal-btn-secondary" onclick={closeNewModal}>
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

	:root {
		--ease-premium: cubic-bezier(0.2, 0, 0, 1);
	}

	.workspace-switcher {
		display: flex;
		align-items: center;
		justify-content: space-between;
		width: 100%;
		padding: 8px 0;
		user-select: none;
	}

	.nav-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 24px;
		height: 24px;
		border-radius: 4px;
		background: transparent;
		color: var(--color-foreground-subtle);
		border: none;
		cursor: pointer;
		transition: all 150ms var(--ease-premium);
	}

	.nav-btn:hover:not(:disabled) {
		background: color-mix(in srgb, var(--color-foreground) 8%, transparent);
		color: var(--color-foreground);
	}

	.nav-btn:active:not(:disabled) {
		transform: scale(0.9);
	}

	.nav-btn:disabled {
		opacity: 0.3;
		cursor: not-allowed;
	}

	.dots {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 8px;
		flex: 1;
	}

	.dot {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 16px;
		height: 16px;
		border-radius: 50%;
		background: transparent;
		border: none;
		cursor: pointer;
		transition: all 150ms var(--ease-premium);
	}

	.dot:hover {
		transform: scale(1.2);
	}

	.dot-inner {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--color-foreground-subtle);
		transition: all 150ms var(--ease-premium);
	}

	.dot.active .dot-inner {
		width: 6px;
		height: 6px;
		background: var(--primary);
	}

	.dot.system .system-dot {
		/* Triangle shape for system workspace */
		width: 0;
		height: 0;
		border-radius: 0;
		border-left: 4px solid transparent;
		border-right: 4px solid transparent;
		border-bottom: 7px solid var(--color-foreground-subtle);
		background: transparent;
	}

	.dot.system.active .system-dot {
		border-bottom-color: var(--primary);
	}

	.add-dot {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 16px;
		height: 16px;
		border-radius: 50%;
		background: transparent;
		color: var(--color-foreground-subtle);
		border: none;
		cursor: pointer;
		opacity: 0.5;
		transition: all 150ms var(--ease-premium);
	}

	.add-dot:hover {
		opacity: 1;
		color: var(--primary);
		transform: scale(1.2);
	}

	.form-group {
		display: flex;
		flex-direction: column;
	}
</style>
