<script lang="ts">
	/**
	 * WorkspaceSwitcher - Visual dot indicator for workspaces
	 *
	 * Shows dots for each workspace with the active one highlighted.
	 * Click a dot to switch workspaces. For full workspace management,
	 * use the WorkspaceDropdown via the header.
	 */
	import { spaceStore } from "$lib/stores/space.svelte";
	import type { SpaceSummary } from "$lib/api/client";

	function handleDotClick(workspace: SpaceSummary) {
		spaceStore.switchSpace(workspace.id, true);
	}

	// Current workspace index for dot highlighting
	const currentIndex = $derived(
		spaceStore.spaces.findIndex(
			(w) => w.id === spaceStore.activeSpaceId
		)
	);

	// Show max 5 dots, centered around current
	const visibleDots = $derived.by(() => {
		const total = spaceStore.spaces.length;
		if (total <= 5) return spaceStore.spaces;

		const start = Math.max(0, Math.min(currentIndex - 2, total - 5));
		return spaceStore.spaces.slice(start, start + 5);
	});
</script>

<div
	class="workspace-switcher"
	role="navigation"
	aria-label="Workspace navigation"
>
	<div class="dots">
		{#each visibleDots as workspace}
			<button
				class="dot"
				class:active={workspace.id === spaceStore.activeSpaceId}
				class:system={workspace.is_system}
				onclick={() => handleDotClick(workspace)}
				title={workspace.name}
				aria-label={workspace.name}
				aria-current={workspace.id === spaceStore.activeSpaceId
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
	</div>
</div>

<style>
	@reference "../../../app.css";

	:root {
		--ease-premium: cubic-bezier(0.2, 0, 0, 1);
	}

	.workspace-switcher {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 100%;
		padding: 8px 0;
		user-select: none;
	}

	.dots {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 8px;
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
</style>
