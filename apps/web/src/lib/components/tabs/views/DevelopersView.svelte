<script lang="ts">
	import { tick } from 'svelte';
	import { animate } from 'motion';
	import type { Tab } from '$lib/tabs/types';
	import { spaceStore } from '$lib/stores/space.svelte';
	import DeveloperSqlView from './DeveloperSqlView.svelte';
	import DeveloperTerminalView from './DeveloperTerminalView.svelte';
	import DeveloperLakeView from './DeveloperLakeView.svelte';

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	type SubTab = 'sql' | 'terminal' | 'lake';

	function parseSubTab(route: string): SubTab {
		const m = route.match(/^\/developers\/(sql|terminal|lake)$/);
		if (m) return m[1] as SubTab;
		return 'sql';
	}

	const subTab = $derived<SubTab>(parseSubTab(tab.route));

	function switchTo(next: SubTab) {
		const route = `/developers/${next}`;
		if (tab.route === route) return;
		spaceStore.updateTab(tab.id, { route });
	}

	const tabs: { id: SubTab; label: string }[] = [
		{ id: 'sql', label: 'SQL' },
		{ id: 'terminal', label: 'Terminal' },
		{ id: 'lake', label: 'Lake' }
	];

	let subtabsEl: HTMLElement | null = $state(null);
	let underlineEl: HTMLElement | null = $state(null);
	let btnRefs = $state<Record<SubTab, HTMLButtonElement | null>>({
		sql: null,
		terminal: null,
		lake: null
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
		void subTab;
		if (!hasMounted) {
			void positionUnderline(false).then(() => {
				hasMounted = true;
			});
		} else {
			void positionUnderline(true);
		}
	});

	$effect(() => {
		if (!subtabsEl) return;
		const ro = new ResizeObserver(() => {
			void positionUnderline(false);
		});
		ro.observe(subtabsEl);
		return () => ro.disconnect();
	});
</script>

<div class="developers-view">
	<nav class="subtabs" bind:this={subtabsEl} aria-label="Developers sections">
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
		{#if subTab === 'sql'}
			<DeveloperSqlView {tab} {active} />
		{:else if subTab === 'terminal'}
			<DeveloperTerminalView {tab} {active} />
		{:else if subTab === 'lake'}
			<DeveloperLakeView {tab} {active} />
		{/if}
	</main>
</div>

<style>
	.developers-view {
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
		overflow: hidden;
		min-height: 0;
		display: flex;
		flex-direction: column;
	}
</style>
