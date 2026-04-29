<script lang="ts">
	import { tick } from 'svelte';
	import { animate } from 'motion';
	import ActionsPanel from '$lib/components/actions/ActionsPanel.svelte';
	import TemplatesPanel from '$lib/components/actions/TemplatesPanel.svelte';
	import ConnectionsPanel from '$lib/components/actions/ConnectionsPanel.svelte';
	import HistoryPanel from '$lib/components/actions/HistoryPanel.svelte';

	type SubTab = 'actions' | 'templates' | 'sources' | 'history';

	function parseHash(): SubTab {
		if (typeof window === 'undefined') return 'actions';
		const h = window.location.hash.replace(/^#/, '');
		if (h === 'actions' || h === 'templates' || h === 'sources' || h === 'history') return h;
		return 'actions';
	}

	let subTab = $state<SubTab>(parseHash());

	function switchTo(tab: SubTab) {
		subTab = tab;
		if (typeof window !== 'undefined') {
			const url = new URL(window.location.href);
			url.hash = tab;
			window.history.replaceState({}, '', url.toString());
		}
	}

	const tabs: { id: SubTab; label: string }[] = [
		{ id: 'actions', label: 'Actions' },
		{ id: 'templates', label: 'Templates' },
		{ id: 'sources', label: 'Sources' },
		{ id: 'history', label: 'History' }
	];

	// Animated underline — slides between the active tab buttons.
	let subtabsEl: HTMLElement | null = $state(null);
	let underlineEl: HTMLElement | null = $state(null);
	let btnRefs = $state<Record<SubTab, HTMLButtonElement | null>>({
		actions: null,
		templates: null,
		sources: null,
		history: null
	});
	let hasMounted = $state(false);

	async function positionUnderline(animated: boolean) {
		if (!underlineEl || !subtabsEl) return;
		const btn = btnRefs[subTab];
		if (!btn) return;
		await tick();
		const containerLeft = subtabsEl.getBoundingClientRect().left;
		const rect = btn.getBoundingClientRect();
		const x = rect.left - containerLeft;
		const width = rect.width;
		if (!animated) {
			underlineEl.style.transform = `translateX(${x}px)`;
			underlineEl.style.width = `${width}px`;
			return;
		}
		animate(
			underlineEl,
			{ transform: `translateX(${x}px)`, width: `${width}px` },
			{ duration: 0.22, ease: [0.32, 0.72, 0, 1] }
		);
	}

	$effect(() => {
		// Re-measure whenever the active subtab changes.
		void subTab;
		if (!hasMounted) {
			// First paint — set position instantly, then enable animation.
			void positionUnderline(false).then(() => {
				hasMounted = true;
			});
		} else {
			void positionUnderline(true);
		}
	});

	$effect(() => {
		// Snap on container resize (pane split, window resize).
		if (!subtabsEl) return;
		const ro = new ResizeObserver(() => {
			void positionUnderline(false);
		});
		ro.observe(subtabsEl);
		return () => ro.disconnect();
	});
</script>

<div class="actions-view">
	<nav class="subtabs" bind:this={subtabsEl} aria-label="Actions sections">
		{#each tabs as t}
			<button
				type="button"
				bind:this={btnRefs[t.id]}
				class:active={subTab === t.id}
				onclick={() => switchTo(t.id)}
			>
				{t.label}
			</button>
		{/each}
		<span class="underline" bind:this={underlineEl} aria-hidden="true"></span>
	</nav>

	<main class="content">
		{#if subTab === 'actions'}
			<ActionsPanel />
		{:else if subTab === 'templates'}
			<TemplatesPanel />
		{:else if subTab === 'sources'}
			<ConnectionsPanel />
		{:else if subTab === 'history'}
			<HistoryPanel />
		{/if}
	</main>
</div>

<style>
	.actions-view {
		display: flex;
		flex-direction: column;
		height: 100%;
		min-height: 0;
	}

	.subtabs {
		position: relative;
		display: flex;
		gap: 1.25rem;
		padding: 0.5rem 1.5rem 0;
		flex-shrink: 0;
	}
	.subtabs button {
		font: inherit;
		font-size: 0.75rem;
		font-weight: 500;
		padding: 0.25rem 0;
		background: transparent;
		border: none;
		color: var(--color-foreground-subtle, #9ca3af);
		cursor: pointer;
		transition: color 120ms ease;
	}
	.subtabs button:hover {
		color: var(--color-foreground-muted, #6b7280);
	}
	.subtabs button.active {
		color: var(--color-foreground, inherit);
	}
	.underline {
		position: absolute;
		bottom: 0;
		left: 0;
		height: 1px;
		width: 0;
		background: var(--color-foreground, #111827);
		transform: translateX(0);
		pointer-events: none;
	}

	.content {
		flex: 1;
		overflow-y: auto;
		padding: 1.25rem 1.5rem 2rem;
		max-width: 1100px;
		width: 100%;
		margin: 0 auto;
	}
</style>
