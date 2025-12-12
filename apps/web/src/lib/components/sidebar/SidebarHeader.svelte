<script lang="ts">
	import "iconify-icon";

	interface Props {
		collapsed?: boolean;
		onNewChat: () => void;
		onToggleCollapse: () => void;
		onSearch?: () => void;
		logoAnimationDelay?: number;
		actionsAnimationDelay?: number;
	}

	let {
		collapsed = false,
		onNewChat,
		onToggleCollapse,
		onSearch,
		logoAnimationDelay = 0,
		actionsAnimationDelay = 30,
	}: Props = $props();

	// Static dot positions for triangle logo
	const dotPositions = [
		{ left: "34%", top: "0%" }, // Top center
		{ left: "0%", top: "67%" }, // Bottom left
		{ left: "67%", top: "67%" }, // Bottom right
	];

	function handleSearch() {
		if (onSearch) {
			onSearch();
		}
	}
</script>

<div class="header-container" class:collapsed>
	<!-- Row 1: Logo (clickable to toggle sidebar) -->
	<div
		class="workspace-row animate-row"
		style="animation-delay: {logoAnimationDelay}ms; --stagger-delay: {logoAnimationDelay}ms"
	>
		<button
			class="logo-area"
			onclick={onToggleCollapse}
			title={collapsed
				? "Expand sidebar (Cmd+[)"
				: "Collapse sidebar (Cmd+[)"}
		>
			<!-- Static Triangle Logo -->
			<div class="logo">
				<div class="logo-dots">
					{#each dotPositions as pos}
						<div
							class="dot"
							style="left: {pos.left}; top: {pos.top};"
						></div>
					{/each}
				</div>
			</div>

			{#if !collapsed}
				<span class="app-name">Virtues</span>
			{/if}
		</button>
	</div>

	<!-- Row 2: Action Layer (Search + New) -->
	{#if !collapsed}
		<div
			class="action-layer animate-row"
			style="animation-delay: {actionsAnimationDelay}ms; --stagger-delay: {actionsAnimationDelay}ms"
		>
			<button
				class="action-btn search-btn"
				onclick={handleSearch}
				title="Search (Cmd+K)"
			>
				<iconify-icon icon="ri:search-line" width="14"></iconify-icon>
				<span class="action-label">Search</span>
			</button>
			<button
				class="action-btn new-btn"
				onclick={onNewChat}
				title="New Chat (Cmd+N)"
			>
				<iconify-icon icon="ri:add-line" width="14"></iconify-icon>
				<span class="action-label">New</span>
			</button>
		</div>
	{/if}
</div>

<style>
	@reference "../../../app.css";

	/* Premium easing - heavy friction feel */
	:root {
		--ease-premium: cubic-bezier(0.2, 0, 0, 1);
	}

	/* Staggered fade-slide animation with premium easing */
	@keyframes fadeSlideIn {
		from {
			opacity: 0;
			transform: translateX(-8px);
		}
		to {
			opacity: 1;
			transform: translateX(0);
		}
	}

	.header-container {
		@apply flex flex-col;
		padding: 14px 8px 16px 8px;
		gap: 14px;
	}

	.header-container.collapsed {
		opacity: 0;
		transform: translateX(-8px);
		transition:
			opacity 150ms var(--ease-premium),
			transform 150ms var(--ease-premium);
	}

	.header-container.collapsed .animate-row {
		opacity: 0;
	}

	/* Animated rows - staggered entrance */
	.animate-row {
		/* Staggered load animation (initial mount) */
		animation: fadeSlideIn 200ms var(--ease-premium) backwards;
		/* Staggered expand transition (sidebar open) */
		opacity: 1;
		transform: translateX(0);
		transition:
			opacity 200ms var(--ease-premium) var(--stagger-delay, 0ms),
			transform 200ms var(--ease-premium) var(--stagger-delay, 0ms);
	}

	/* Row 1: Workspace */
	.workspace-row {
		@apply flex items-center justify-between;
	}

	.logo-area {
		@apply flex items-center cursor-pointer;
		gap: 0px;
		padding: 4px 6px;
		border-radius: 8px;
		background: transparent;
		transition: all 200ms var(--ease-premium);
	}

	.logo-area:hover .app-name {
		color: var(--primary);
	}

	.logo-area:hover .dot {
		background: var(--primary);
	}

	.logo-area:active {
		transform: scale(0.98);
	}

	/* Logo container with guardrails - consistent 32x32 touch target */
	.logo {
		@apply relative flex items-center justify-center;
		width: 32px;
		height: 32px;
		overflow: hidden; /* Prevent any overflow during animations */
	}

	.logo-dots {
		@apply relative w-[14px] h-[14px];
	}

	.dot {
		@apply absolute w-[4px] h-[4px] rounded-full bg-secondary;
		transition: background 200ms var(--ease-premium);
	}

	.app-name {
		font-family: "Charter", "Georgia", "Times New Roman", serif;
		color: var(--foreground);
		font-size: 17px;
		font-weight: 400;
		letter-spacing: 0.02em;
		transition: color 200ms var(--ease-premium);
	}

	/* Row 2: Action Layer */
	.action-layer {
		@apply flex gap-1;
	}

	.action-btn {
		@apply flex items-center cursor-pointer;
		border-radius: 8px;
		gap: 6px;
		padding: 6px 10px;
		font-size: 13px;
		color: var(--color-foreground-muted);
		background: transparent;
		transition: all 200ms var(--ease-premium);
	}

	.action-btn:hover {
		background: color-mix(in srgb, var(--color-foreground) 7%, transparent);
		color: var(--color-foreground);
	}

	.action-btn iconify-icon {
		color: currentColor;
	}

	.action-btn:active {
		transform: scale(0.98);
	}

	.action-label {
		font-weight: 400;
		color: inherit;
	}

	.search-btn {
		flex: 1;
	}
</style>
