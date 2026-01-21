<script lang="ts">
	import { windowTabs } from '$lib/stores/windowTabs.svelte';
	import TabContainer from './TabContainer.svelte';
	import WindowTabBar from './WindowTabBar.svelte';
	import TabContent from './TabContent.svelte';

	let isDragging = $state(false);
	let containerRef = $state<HTMLElement | null>(null);

	const MIN_WIDTH = 25; // minimum 25% (1/4) for each pane

	function handleMouseDown(e: MouseEvent) {
		e.preventDefault();
		isDragging = true;
	}

	function handleMouseMove(e: MouseEvent) {
		if (!isDragging || !containerRef) return;

		const rect = containerRef.getBoundingClientRect();
		const newWidth = ((e.clientX - rect.left) / rect.width) * 100;
		const clampedWidth = Math.max(MIN_WIDTH, Math.min(100 - MIN_WIDTH, newWidth));

		windowTabs.setPaneWidth(clampedWidth);
	}

	function handleMouseUp() {
		isDragging = false;
	}

	function handlePaneClick(paneId: 'left' | 'right') {
		windowTabs.setActivePane(paneId);
	}

	// Get the active tab for a pane
	function getActiveTabForPane(paneId: 'left' | 'right') {
		const pane = paneId === 'left' ? windowTabs.leftPane : windowTabs.rightPane;
		if (!pane) return null;
		return pane.tabs.find((t) => t.id === pane.activeTabId) || null;
	}

	const leftActiveTab = $derived(getActiveTabForPane('left'));
	const rightActiveTab = $derived(getActiveTabForPane('right'));
</script>

<svelte:window on:mousemove={handleMouseMove} on:mouseup={handleMouseUp} />

{#if windowTabs.split.enabled && windowTabs.split.panes}
	<div
		class="split-container"
		bind:this={containerRef}
		class:dragging={isDragging}
	>
		<!-- Left Pane -->
		<div
			class="pane"
			class:active={windowTabs.split.activePaneId === 'left'}
			style:width="{windowTabs.leftPane?.width || 50}%"
			onclick={() => handlePaneClick('left')}
			onkeydown={(e) => e.key === 'Enter' && handlePaneClick('left')}
			role="region"
			tabindex="0"
			aria-label="Left pane"
		>
			<WindowTabBar paneId="left" />
			<div class="pane-content">
				{#if (windowTabs.leftPane?.tabs || []).length === 0}
					<div class="empty-pane">
						<span class="empty-text">No tabs open</span>
					</div>
				{:else}
					{#each windowTabs.leftPane?.tabs || [] as tab (tab.id)}
						<TabContent {tab} active={tab.id === windowTabs.leftPane?.activeTabId} />
					{/each}
				{/if}
			</div>
		</div>

		<!-- Resize Handle -->
		<div
			class="resize-handle"
			class:dragging={isDragging}
			onmousedown={handleMouseDown}
			role="separator"
			aria-orientation="vertical"
			tabindex="0"
		>
			<div class="handle-grip"></div>
		</div>

		<!-- Right Pane -->
		<div
			class="pane"
			class:active={windowTabs.split.activePaneId === 'right'}
			style:width="{windowTabs.rightPane?.width || 50}%"
			onclick={() => handlePaneClick('right')}
			onkeydown={(e) => e.key === 'Enter' && handlePaneClick('right')}
			role="region"
			tabindex="0"
			aria-label="Right pane"
		>
			<WindowTabBar paneId="right" />
			<div class="pane-content">
				{#if (windowTabs.rightPane?.tabs || []).length === 0}
					<div class="empty-pane">
						<span class="empty-text">No tabs open</span>
					</div>
				{:else}
					{#each windowTabs.rightPane?.tabs || [] as tab (tab.id)}
						<TabContent {tab} active={tab.id === windowTabs.rightPane?.activeTabId} />
					{/each}
				{/if}
			</div>
		</div>
	</div>
{:else}
	<!-- Single pane mode -->
	<TabContainer />
{/if}

<style>
	.split-container {
		display: flex;
		flex: 1;
		min-height: 0;
		overflow: hidden;
	}

	.split-container.dragging {
		cursor: col-resize;
		user-select: none;
	}

	.pane {
		display: flex;
		flex-direction: column;
		min-width: 0;
		overflow: hidden;
		border-radius: 0;
		transition: box-shadow 150ms ease;
	}

	.pane.active {
		box-shadow: inset 0 2px 0 0 var(--color-accent);
	}

	.pane-content {
		flex: 1;
		position: relative;
		min-height: 0;
		overflow: hidden;
	}

	.empty-pane {
		display: flex;
		align-items: center;
		justify-content: center;
		height: 100%;
		color: var(--color-foreground-subtle);
	}

	.empty-text {
		font-size: 13px;
		opacity: 0.6;
	}

	.resize-handle {
		width: 1px;
		cursor: col-resize;
		background: var(--color-border);
		display: flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
		transition: width 150ms ease, background-color 150ms ease;
		position: relative;
	}

	.resize-handle::before {
		content: '';
		position: absolute;
		inset: 0;
		width: 11px;
		left: -5px;
		cursor: col-resize;
	}

	.resize-handle:hover,
	.resize-handle.dragging {
		width: 5px;
		background: var(--color-border);
	}

	.handle-grip {
		width: 2px;
		height: 32px;
		background: var(--color-foreground-muted);
		border-radius: 1px;
		opacity: 0;
		transition: opacity 150ms ease;
	}

	.resize-handle:hover .handle-grip,
	.resize-handle.dragging .handle-grip {
		opacity: 0.5;
	}
</style>
