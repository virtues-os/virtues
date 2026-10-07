<script lang="ts">
	/**
	 * BrowserView — the in-app browser, as a tab beside the current view.
	 *
	 * The page itself is a native WKWebView the Mac shell places over `slot`
	 * (src-tauri/src/browser_host.rs). This view draws the toolbar, reserves
	 * the space, and keeps telling the shell where that space is. A native view
	 * draws above the page, so it is hidden whenever this tab is not on screen:
	 * another tab in front, a modal open, or the phone layout.
	 *
	 * There is one browser, shared by the owner and the assistant. The
	 * assistant's browser_open, and "Log in to X", open this tab beside whatever
	 * is in front (`browserPaneListener.ts`).
	 */
	import { onDestroy, onMount } from 'svelte';
	import Icon from '$lib/components/Icon.svelte';
	import type { Tab } from '$lib/tabs/types';
	import { browserLabel } from '$lib/tabs/registry';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { mobileLayout } from '$lib/stores/mobileLayout.svelte';
	import {
		browserPaneBounds,
		browserPaneGo,
		browserPaneOpen,
		canBrowserPane,
		onBrowserEvent
	} from '$lib/tauri/bridge';

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	const pageUrl = $derived(new URL(tab.route, 'https://app.invalid').searchParams.get('url') ?? '');

	let available = $state<boolean | null>(null);
	let address = $state('');
	let editing = $state(false);
	let modalOpen = $state(false);
	let slot = $state<HTMLDivElement | null>(null);

	function routeFor(url: string) {
		return `/browser?url=${encodeURIComponent(url)}`;
	}

	/** Enter pressed in the address bar: a URL, a bare domain, or a search. */
	function resolve(input: string): string {
		const text = input.trim();
		if (/^https?:\/\//i.test(text)) return text;
		if (/^[\w-]+(\.[\w-]+)+(\/\S*)?$/.test(text)) return `https://${text}`;
		return `https://duckduckgo.com/?q=${encodeURIComponent(text)}`;
	}

	function go(url: string) {
		address = url;
		windowShellStore.updateTab(tab.id, { route: routeFor(url), label: browserLabel(url) });
		void browserPaneOpen(url).catch(() => {});
	}

	// Tell the shell where the pane is, or that it is not on screen.
	let raf = 0;
	function report() {
		cancelAnimationFrame(raf);
		raf = requestAnimationFrame(() => {
			if (!available) return;
			const shown = active && !mobileLayout.isMobile && !modalOpen && slot && slot.offsetParent;
			const r = shown ? slot!.getBoundingClientRect() : null;
			void browserPaneBounds(
				r && r.width > 0 && r.height > 0
					? { x: r.left, y: r.top, width: r.width, height: r.height }
					: null
			).catch(() => {});
		});
	}

	// Pane widths animate (150ms); follow the animation, then settle.
	function reportFor(ms: number) {
		const until = performance.now() + ms;
		const tick = () => {
			report();
			if (performance.now() < until) requestAnimationFrame(tick);
		};
		tick();
	}

	$effect(() => {
		// Re-report whenever visibility inputs change.
		void active;
		void modalOpen;
		void mobileLayout.isMobile;
		void available;
		reportFor(250);
	});

	let unlisten: (() => void) | null = null;
	let resizeObserver: ResizeObserver | null = null;
	let modalObserver: MutationObserver | null = null;

	onMount(() => {
		address = pageUrl;
		void canBrowserPane().then((ok) => {
			available = ok;
			if (ok && pageUrl) void browserPaneOpen(pageUrl).catch(() => {});
		});
		void onBrowserEvent('browser:navigated', (url) => {
			if (!editing) address = url;
			// Keep the tab's route on the page, so a reload or restore comes back to it.
			if (url && url !== pageUrl) {
				windowShellStore.updateTab(tab.id, { route: routeFor(url), label: browserLabel(url) });
			}
		}).then((off) => (unlisten = off));

		resizeObserver = new ResizeObserver(() => report());
		if (slot) resizeObserver.observe(slot);
		window.addEventListener('resize', report);
		// A modal is HTML; the browser is native and would sit on top of it.
		const checkModal = () => {
			modalOpen = !!document.querySelector('[aria-modal="true"]');
		};
		modalObserver = new MutationObserver(checkModal);
		modalObserver.observe(document.body, { childList: true, subtree: true });
		checkModal();
	});

	onDestroy(() => {
		cancelAnimationFrame(raf);
		unlisten?.();
		resizeObserver?.disconnect();
		modalObserver?.disconnect();
		window.removeEventListener('resize', report);
		if (available) void browserPaneBounds(null).catch(() => {});
	});
</script>

<div class="browser">
	<div class="toolbar">
		<button class="tool" title="Back" aria-label="Back" onclick={() => void browserPaneGo('back')} disabled={!available}>
			<Icon icon="ri:arrow-left-line" width="16" />
		</button>
		<button class="tool" title="Forward" aria-label="Forward" onclick={() => void browserPaneGo('forward')} disabled={!available}>
			<Icon icon="ri:arrow-right-line" width="16" />
		</button>
		<button class="tool" title="Reload" aria-label="Reload" onclick={() => void browserPaneGo('reload')} disabled={!available}>
			<Icon icon="ri:refresh-line" width="16" />
		</button>
		<input
			class="address"
			bind:value={address}
			placeholder="Enter an address or search"
			spellcheck="false"
			autocapitalize="off"
			onfocus={() => (editing = true)}
			onblur={() => (editing = false)}
			onkeydown={(e) => {
				if (e.key === 'Enter' && address.trim()) {
					go(resolve(address));
					(e.currentTarget as HTMLInputElement).blur();
				}
			}}
			disabled={!available}
		/>
	</div>

	<div class="slot" bind:this={slot}>
		{#if available === false}
			<p class="note">The browser is in the Virtues app for Mac.</p>
		{:else if !pageUrl}
			<p class="note">Enter an address above to start browsing.</p>
		{/if}
	</div>
</div>

<style>
	.browser {
		display: flex;
		flex-direction: column;
		height: 100%;
		min-height: 0;
	}
	.toolbar {
		display: flex;
		align-items: center;
		gap: 4px;
		padding: 6px 8px;
		border-bottom: 1px solid var(--border);
	}
	.tool {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 28px;
		height: 28px;
		border-radius: 6px;
		color: var(--text-muted);
	}
	.tool:hover:not(:disabled) {
		background: var(--surface-hover, var(--surface-elevated));
		color: var(--text);
	}
	.tool:disabled {
		opacity: 0.4;
	}
	.address {
		flex: 1;
		min-width: 0;
		height: 28px;
		padding: 0 10px;
		border-radius: 6px;
		border: 1px solid var(--border);
		background: var(--surface);
		color: var(--text);
		font-size: 13px;
	}
	.slot {
		position: relative;
		flex: 1;
		min-height: 0;
		display: flex;
		align-items: center;
		justify-content: center;
	}
	.note {
		color: var(--text-muted);
		font-size: 14px;
	}
</style>
