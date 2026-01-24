<script lang="ts">
	import "iconify-icon";

	interface WorkspaceInfo {
		name: string;
		icon: string | null;
		accentColor: string | null;
		isSystem: boolean;
	}

	interface Props {
		collapsed?: boolean;
		onNewChat: () => void;
		onGoHome: () => void;
		onToggleCollapse: () => void;
		onSearch?: () => void;
		logoAnimationDelay?: number;
		actionsAnimationDelay?: number;
		/** Scroll progress for title animation: -1 to 1, where 0 = current, negative = going left, positive = going right */
		scrollProgress?: number;
		/** Workspace info for transition: [previous, current, next] */
		transitionWorkspaces?: [WorkspaceInfo | null, WorkspaceInfo, WorkspaceInfo | null];
	}

	const defaultWorkspace: WorkspaceInfo = { name: "Virtues", icon: null, accentColor: null, isSystem: true };

	let {
		collapsed = false,
		onNewChat,
		onGoHome,
		onToggleCollapse,
		onSearch,
		logoAnimationDelay = 0,
		actionsAnimationDelay = 30,
		scrollProgress = 0,
		transitionWorkspaces = [null, defaultWorkspace, null],
	}: Props = $props();

	// Current workspace info from props
	const currentWs = $derived(transitionWorkspaces[1]);
	const prevWs = $derived(transitionWorkspaces[0]);
	const nextWs = $derived(transitionWorkspaces[2]);

	// Calculate Y offset based on scroll progress
	// progress > 0 means swiping right (going to next), title moves up
	// progress < 0 means swiping left (going to prev), title moves down
	const offset = $derived(scrollProgress * -32);
	const currentOpacity = $derived(1 - Math.abs(scrollProgress));

	// Incoming workspace (from above or below depending on direction)
	const incomingOffset = $derived(
		scrollProgress > 0 
			? 32 + (scrollProgress * -32)  // Coming from below
			: -32 + (scrollProgress * -32) // Coming from above
	);
	const incomingOpacity = $derived(Math.abs(scrollProgress));
	const incomingWs = $derived(scrollProgress > 0 ? nextWs : prevWs);

	// Static dot positions for triangle logo (used in system workspace)
	const dotPositions = [
		{ left: "34%", top: "0%" },
		{ left: "0%", top: "67%" },
		{ left: "67%", top: "67%" },
	];
</script>

<div class="header-container" class:collapsed>
	<!-- Row 1: Logo/Workspace + Collapse -->
	<div
		class="workspace-row animate-row"
		style="animation-delay: {logoAnimationDelay}ms; --stagger-delay: {logoAnimationDelay}ms"
	>
		<button class="logo-area" onclick={onGoHome} title="Home">
			<div class="logo-transition-wrapper">
				<!-- Current workspace (icon + name) -->
				<div 
					class="workspace-identity"
					style="transform: translateY({offset}px); opacity: {currentOpacity};"
				>
					{#if currentWs.isSystem}
						<div class="logo">
							<div class="logo-dots">
								{#each dotPositions as pos}
									<div class="dot" style="left: {pos.left}; top: {pos.top};"></div>
								{/each}
							</div>
						</div>
					{:else}
						<div class="workspace-icon">
							{#if currentWs.icon}
								<iconify-icon icon={currentWs.icon} width="18"></iconify-icon>
							{:else if currentWs.accentColor}
								<div class="color-dot" style="background: {currentWs.accentColor}"></div>
							{:else}
								<iconify-icon icon="ri:folder-line" width="18"></iconify-icon>
							{/if}
						</div>
					{/if}
					{#if !collapsed}
						<span class="workspace-name">{currentWs.name}</span>
					{/if}
				</div>

				<!-- Incoming workspace (shown during swipe) -->
				{#if incomingWs && Math.abs(scrollProgress) > 0.01}
					<div 
						class="workspace-identity incoming"
						style="transform: translateY({incomingOffset}px); opacity: {incomingOpacity};"
					>
						{#if incomingWs.isSystem}
							<div class="logo">
								<div class="logo-dots">
									{#each dotPositions as pos}
										<div class="dot" style="left: {pos.left}; top: {pos.top};"></div>
									{/each}
								</div>
							</div>
						{:else}
							<div class="workspace-icon">
								{#if incomingWs.icon}
									<iconify-icon icon={incomingWs.icon} width="18"></iconify-icon>
								{:else if incomingWs.accentColor}
									<div class="color-dot" style="background: {incomingWs.accentColor}"></div>
								{:else}
									<iconify-icon icon="ri:folder-line" width="18"></iconify-icon>
								{/if}
							</div>
						{/if}
						{#if !collapsed}
							<span class="workspace-name">{incomingWs.name}</span>
						{/if}
					</div>
				{/if}
			</div>
		</button>

		{#if !collapsed}
			<button
				class="collapse-btn"
				onclick={onToggleCollapse}
				title="Collapse sidebar (Cmd+[)"
			>
				<iconify-icon icon="ri:arrow-left-s-line" width="18"></iconify-icon>
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
				onclick={onSearch}
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

	:root {
		--ease-premium: cubic-bezier(0.2, 0, 0, 1);
	}

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

	.animate-row {
		animation: fadeSlideIn 200ms var(--ease-premium) backwards;
		opacity: 1;
		transform: translateX(0);
		transition:
			opacity 200ms var(--ease-premium) var(--stagger-delay, 0ms),
			transform 200ms var(--ease-premium) var(--stagger-delay, 0ms);
	}

	.workspace-row {
		@apply flex items-center justify-between;
	}

	.logo-area {
		@apply flex items-center cursor-pointer;
		padding: 4px 6px;
		border-radius: 8px;
		background: transparent;
		transition: all 200ms var(--ease-premium);
	}

	.logo-area:hover .workspace-name {
		color: var(--primary);
	}

	.logo-area:hover .dot {
		background: var(--primary);
	}

	.logo-area:active {
		transform: scale(0.98);
	}

	/* Transition wrapper for logo + name */
	.logo-transition-wrapper {
		position: relative;
		height: 32px;
		display: flex;
		align-items: center;
		/* Clip Y axis only for animation, not X */
		clip-path: inset(-4px -100px -4px -4px);
	}

	.workspace-identity {
		display: flex;
		align-items: center;
		gap: 0;
		will-change: transform, opacity;
	}

	.workspace-identity.incoming {
		position: absolute;
		left: 0;
		top: 0;
	}

	.logo {
		@apply relative flex items-center justify-center;
		width: 32px;
		height: 32px;
		flex-shrink: 0;
	}

	.logo-dots {
		@apply relative w-[14px] h-[14px];
	}

	.dot {
		@apply absolute w-[4px] h-[4px] rounded-full bg-secondary;
		transition: background 200ms var(--ease-premium);
	}

	.workspace-icon {
		@apply flex items-center justify-center;
		width: 32px;
		height: 32px;
		flex-shrink: 0;
		color: var(--color-foreground-muted);
	}

	.color-dot {
		width: 14px;
		height: 14px;
		border-radius: 50%;
	}

	.workspace-name {
		font-family: "Charter", "Georgia", "Times New Roman", serif;
		color: var(--foreground);
		font-size: 17px;
		font-weight: 400;
		letter-spacing: 0.02em;
		white-space: nowrap;
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
