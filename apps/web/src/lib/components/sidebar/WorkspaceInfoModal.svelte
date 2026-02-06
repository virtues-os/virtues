<script lang="ts">
	/**
	 * WorkspaceInfoModal - Workspace settings panel
	 *
	 * Shows workspace metadata (items, dates, ID) and delete action.
	 * Rename, icon, and color changes are handled via the dropdown menu.
	 */
	import Modal from "$lib/components/Modal.svelte";
	import Icon from "$lib/components/Icon.svelte";
	import { spaceStore } from "$lib/stores/space.svelte";
	import type { SpaceSummary } from "$lib/api/client";

	interface Props {
		open: boolean;
		workspace: SpaceSummary | null;
		onClose: () => void;
	}

	let { open, workspace, onClose }: Props = $props();

	let confirmingDelete = $state(false);

	// Static dot positions for Virtues logo
	const dotPositions = [
		{ left: "34%", top: "0%" },
		{ left: "0%", top: "67%" },
		{ left: "67%", top: "67%" },
	];

	// Reset state when modal opens
	$effect(() => {
		if (open && workspace) {
			confirmingDelete = false;
		}
	});

	async function handleDeleteWorkspace() {
		if (!workspace || workspace.is_system) return;

		try {
			await spaceStore.deleteSpace(workspace.id);
			onClose();
		} catch (e) {
			console.error("Failed to delete workspace:", e);
		}
	}

	// Get item counts from workspace store
	const itemCount = $derived.by(() => {
		if (!workspace) return 0;
		const items = spaceStore.getSpaceItems(workspace.id);
		const views = spaceStore.getViewsForSpace(workspace.id);
		return items.length + views.length;
	});

	// Check if icon is an emoji
	function isEmoji(val: string | null | undefined): boolean {
		if (!val) return false;
		return !val.includes(":");
	}

	// Format ISO date string
	function formatDate(isoDate: string): string {
		try {
			const date = new Date(isoDate);
			return date.toLocaleDateString(undefined, {
				year: 'numeric',
				month: 'short',
				day: 'numeric',
			});
		} catch {
			return isoDate;
		}
	}
</script>

<Modal open={open && workspace !== null} {onClose} width="sm">
	{#snippet children()}
		{#if workspace}
			<!-- Hero Section (static display) -->
			<div class="hero-section">
				{#if workspace.is_system}
					<div class="hero-icon system">
						<div class="virtues-logo">
							{#each dotPositions as pos}
								<div class="dot" style="left: {pos.left}; top: {pos.top};"></div>
							{/each}
						</div>
					</div>
				{:else}
					<div
						class="hero-icon user"
						style={workspace.accent_color ? `--accent: ${workspace.accent_color}` : ""}
					>
						{#if workspace.icon}
							{#if isEmoji(workspace.icon)}
								<span class="icon-emoji">{workspace.icon}</span>
							{:else}
								<Icon icon={workspace.icon} width="32" />
							{/if}
						{:else if workspace.accent_color}
							<div class="color-fill"></div>
						{:else}
							<Icon icon="ri:folder-line" width="32" />
						{/if}
					</div>
				{/if}

				<h2 class="hero-name">
					{workspace.is_system ? "Virtues" : workspace.name}
				</h2>
			</div>

			<!-- Info Rows -->
			<div class="info-section">
				{#if !workspace.is_system}
					<div class="info-row">
						<span class="info-label">Items</span>
						<span class="info-value">{itemCount} {itemCount === 1 ? "item" : "items"}</span>
					</div>
				{/if}

				<div class="info-row">
					<span class="info-label">Created</span>
					<span class="info-value">{formatDate(workspace.created_at)}</span>
				</div>

				<div class="info-row">
					<span class="info-label">Updated</span>
					<span class="info-value">{formatDate(workspace.updated_at)}</span>
				</div>

				<div class="info-row">
					<span class="info-label">Space ID</span>
					<code class="info-value mono">{workspace.id}</code>
				</div>
			</div>

			<!-- Delete Zone (non-system only) -->
			{#if !workspace.is_system}
				<div class="delete-zone">
					{#if confirmingDelete}
						<div class="delete-confirm">
							<span class="delete-confirm-text">Delete "{workspace.name}"?</span>
							<div class="delete-confirm-actions">
								<button class="btn-cancel" onclick={() => confirmingDelete = false}>Cancel</button>
								<button class="btn-confirm-delete" onclick={handleDeleteWorkspace}>Delete</button>
							</div>
						</div>
					{:else}
						<button class="btn-danger" onclick={() => confirmingDelete = true}>
							<Icon icon="ri:delete-bin-line" width="14" />
							Delete Space
						</button>
					{/if}
				</div>
			{/if}
		{/if}
	{/snippet}
</Modal>

<style>
	/* Hero Section */
	.hero-section {
		display: flex;
		flex-direction: column;
		align-items: center;
		padding: 12px 4px 24px;
		margin: -20px -20px 0;
		background: linear-gradient(
			to bottom,
			color-mix(in srgb, var(--color-primary) 8%, transparent),
			transparent
		);
	}

	.hero-icon {
		width: 80px;
		height: 80px;
		border-radius: 20px;
		display: flex;
		align-items: center;
		justify-content: center;
		margin-bottom: 16px;
	}

	.hero-icon.system {
		background: linear-gradient(135deg, var(--color-surface) 0%, var(--color-surface-overlay) 100%);
		border: 1px solid var(--color-border);
	}

	.hero-icon.user {
		background: color-mix(in srgb, var(--accent, var(--color-primary)) 15%, var(--color-surface));
		border: 1px solid color-mix(in srgb, var(--accent, var(--color-primary)) 30%, transparent);
		color: var(--accent, var(--color-primary));
	}

	.icon-emoji {
		font-size: 36px;
		line-height: 1;
	}

	.color-fill {
		width: 100%;
		height: 100%;
		background: var(--accent, var(--color-primary));
		border-radius: 18px;
		opacity: 0.3;
	}

	.virtues-logo {
		position: relative;
		width: 28px;
		height: 28px;
	}

	.virtues-logo .dot {
		position: absolute;
		width: 8px;
		height: 8px;
		border-radius: 50%;
		background: var(--color-foreground-muted);
	}

	.hero-name {
		font-family: "Charter", "Georgia", serif;
		font-size: 22px;
		font-weight: 400;
		color: var(--color-foreground);
		margin: 0;
	}

	/* Info Section */
	.info-section {
		display: flex;
		flex-direction: column;
		gap: 2px;
		margin: 0 -4px;
	}

	.info-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 10px 4px;
		border-bottom: 1px solid var(--color-border-subtle);
	}

	.info-label {
		font-size: 13px;
		color: var(--color-foreground-muted);
	}

	.info-value {
		font-size: 13px;
		color: var(--color-foreground);
	}

	/* Monospace text for IDs */
	.info-value.mono {
		font-family: var(--font-mono);
		font-size: 11px;
		color: var(--color-foreground-muted);
		word-break: break-all;
	}

	/* Delete Zone */
	.delete-zone {
		margin-top: 16px;
		padding-top: 16px;
		border-top: 1px solid var(--color-border-subtle);
	}

	.btn-danger {
		display: flex;
		align-items: center;
		gap: 6px;
		width: 100%;
		padding: 8px 12px;
		font-size: 13px;
		color: var(--color-foreground-muted);
		background: none;
		border: none;
		border-radius: 6px;
		cursor: pointer;
		transition: all 150ms ease;
	}

	.btn-danger:hover {
		color: var(--color-error);
		background: color-mix(in srgb, var(--color-error) 8%, transparent);
	}

	.delete-confirm {
		display: flex;
		flex-direction: column;
		gap: 10px;
	}

	.delete-confirm-text {
		font-size: 13px;
		color: var(--color-foreground);
	}

	.delete-confirm-actions {
		display: flex;
		gap: 8px;
		justify-content: flex-end;
	}

	.btn-cancel {
		padding: 6px 14px;
		font-size: 12px;
		font-weight: 500;
		color: var(--color-foreground);
		background: var(--color-surface-overlay);
		border: 1px solid var(--color-border);
		border-radius: 6px;
		cursor: pointer;
		transition: all 150ms ease;
	}

	.btn-cancel:hover {
		background: var(--color-surface-elevated);
	}

	.btn-confirm-delete {
		padding: 6px 14px;
		font-size: 12px;
		font-weight: 500;
		color: white;
		background: var(--color-error);
		border: none;
		border-radius: 6px;
		cursor: pointer;
		transition: all 150ms ease;
	}

	.btn-confirm-delete:hover {
		filter: brightness(1.1);
	}
</style>
