<script lang="ts">
	import "iconify-icon";

	interface Props {
		collapsed?: boolean;
		onNewChat: () => void;
		onGoHome: () => void;
		onToggleCollapse: () => void;
		onSearch?: () => void;
		logoAnimationDelay?: number;
		actionsAnimationDelay?: number;
	}

	let {
		collapsed = false,
		onNewChat,
		onGoHome,
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
	<!-- Row 1: Logo + Collapse -->
	<div
		class="workspace-row animate-row"
		style="animation-delay: {logoAnimationDelay}ms; --stagger-delay: {logoAnimationDelay}ms"
	>
		<button class="logo-area" onclick={onGoHome} title="Home">
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

		{#if !collapsed}
			<button
				class="collapse-btn"
				onclick={onToggleCollapse}
				title="Collapse sidebar (Cmd+[)"
			>
				<iconify-icon icon="ri:arrow-left-s-line" width="18"
				></iconify-icon>
			</button>
		{/if}
	</div>

	<!-- Row 2: Command + New Chat -->
	{#if !collapsed}
		<div
			class="action-layer animate-row"
			style="animation-delay: {actionsAnimationDelay}ms; --stagger-delay: {actionsAnimationDelay}ms"
		>
			<button
				class="action-btn"
				onclick={handleSearch}
				title="Command (Cmd+K)"
			>
				<span class="action-label">Command</span>
				<kbd class="action-kbd">⌘K</kbd>
			</button>
			<button
				class="action-btn"
				onclick={onNewChat}
				title="New Chat (Cmd+N)"
			>
				<span class="action-label">New Chat</span>
				<kbd class="action-kbd">⌘N</kbd>
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

	.collapse-btn {
		@apply flex items-center justify-center cursor-pointer;
		width: 28px;
		height: 28px;
		border-radius: 6px;
		background: transparent;
		color: var(--color-foreground-muted);
		transition: all 200ms var(--ease-premium);
	}

	.collapse-btn:hover {
		background: color-mix(in srgb, var(--color-foreground) 7%, transparent);
		color: var(--color-foreground);
	}

	.collapse-btn:active {
		transform: scale(0.95);
	}

	/* Row 2: Action Layer */
	.action-layer {
		@apply flex;
		gap: 6px;
	}

	.action-btn {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 4px;
		flex: 1;
		padding: 6px 8px;
		background: color-mix(in srgb, var(--color-foreground) 5%, transparent);
		border-radius: 6px;
		font-size: 12px;
		color: var(--color-foreground-subtle);
		cursor: pointer;
		transition: all 0.15s ease;
	}

	.action-btn:hover {
		background: color-mix(in srgb, var(--color-foreground) 8%, transparent);
		color: var(--color-foreground-muted);
	}

	.action-label {
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.action-kbd {
		font-family: inherit;
		font-size: 10px;
		color: var(--color-foreground-subtle);
		opacity: 0.7;
	}
</style>
