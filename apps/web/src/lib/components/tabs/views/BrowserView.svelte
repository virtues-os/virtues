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
	 *
	 * The owner always sees who is driving. While the assistant acts, a bar
	 * offers Take control and Stop and the page gets an accent frame; when it
	 * hands the browser over (a sign-in, a code), the bar says what to do and
	 * waits for Done. Its steps collect in a strip under the page.
	 */
	import { onDestroy, onMount } from 'svelte';
	import Icon from '$lib/components/Icon.svelte';
	import type { Tab } from '$lib/tabs/types';
	import { browserLabel } from '$lib/tabs/registry';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { mobileLayout } from '$lib/stores/mobileLayout.svelte';
	import { browserAgent } from '$lib/stores/browserAgent.svelte';
	import { chatInstances } from '$lib/stores/chatInstances.svelte';
	import { cancelChat } from '$lib/api/client';
	import {
		browserPaneAgent,
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
	let strip = $state<HTMLDivElement | null>(null);

	const STEPS_KEY = 'virtues.browser.steps';
	let showSteps = $state(readShowSteps());
	function readShowSteps(): boolean {
		try {
			return localStorage.getItem(STEPS_KEY) !== '0';
		} catch {
			return true;
		}
	}
	function toggleSteps() {
		showSteps = !showSteps;
		try {
			localStorage.setItem(STEPS_KEY, showSteps ? '1' : '0');
		} catch {
			// Remembering the choice is a convenience.
		}
	}

	/** Stop the assistant: end the reply that is using the browser. A reply
	 *  running from another device can't be stopped here, so take control
	 *  instead, which refuses its next step. */
	async function stopAssistant() {
		const drivers = chatInstances.browserDrivers();
		await browserPaneAgent('stop').catch(() => {});
		if (drivers.length === 0) {
			await browserPaneAgent('take').catch(() => {});
			return;
		}
		for (const { conversationId, chat } of drivers) {
			void chat.stop();
			void cancelChat(conversationId).catch(() => {});
		}
	}

	function hostOf(url: string) {
		try {
			return new URL(url).hostname.replace(/^www\./, '');
		} catch {
			return url;
		}
	}

	// Keep the newest step in view.
	$effect(() => {
		void browserAgent.steps.length;
		if (strip) strip.scrollLeft = strip.scrollWidth;
	});

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
		// The bar, frame and step strip move the page.
		void browserAgent.driving;
		void browserAgent.paused;
		void browserAgent.handoff;
		void showSteps;
		void browserAgent.steps.length;
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
		{#if browserAgent.steps.length > 0}
			<button
				class="tool steps-toggle"
				class:on={showSteps}
				title={showSteps ? 'Hide your assistant\'s steps' : 'Show your assistant\'s steps'}
				aria-label="Your assistant's steps"
				aria-pressed={showSteps}
				onclick={toggleSteps}
			>
				<Icon icon="ri:footprint-line" width="16" />
				<span class="count">{browserAgent.steps.length}</span>
			</button>
		{/if}
	</div>

	{#if browserAgent.handoff}
		<div class="agent-bar handoff" role="status">
			<Icon icon="ri:hand" width="16" />
			<span class="say"><strong>Your turn.</strong> {browserAgent.handoff}</span>
			<button class="bar-btn" onclick={() => void browserPaneAgent('decline')}>I can't</button>
			<button class="bar-btn primary" onclick={() => void browserPaneAgent('done')}>Done</button>
		</div>
	{:else if browserAgent.paused}
		<div class="agent-bar paused" role="status">
			<Icon icon="ri:user-line" width="16" />
			<span class="say">You have control. Your assistant waits until you hand it back.</span>
			<button class="bar-btn primary" onclick={() => void browserPaneAgent('resume')}>Hand back</button>
		</div>
	{:else if browserAgent.driving}
		<div class="agent-bar driving" role="status">
			<span class="pulse" aria-hidden="true"></span>
			<span class="say">Your assistant is using the browser</span>
			<button class="bar-btn" onclick={() => void browserPaneAgent('take')}>Take control</button>
			<button class="bar-btn" onclick={() => void stopAssistant()}>Stop</button>
		</div>
	{/if}

	<div
		class="stage"
		class:driving={browserAgent.driving && !browserAgent.paused && !browserAgent.handoff}
		class:handoff={!!browserAgent.handoff}
	>
		<div class="slot" bind:this={slot}>
			{#if available === false}
				<p class="note">The browser is in the Virtues app for Mac.</p>
			{:else if !pageUrl}
				<p class="note">Enter an address above to start browsing.</p>
			{/if}
		</div>
	</div>

	{#if showSteps && browserAgent.steps.length > 0}
		<div class="steps" bind:this={strip} aria-label="Your assistant's steps">
			{#each browserAgent.steps as step (step.id)}
				<figure class="step" class:failed={!step.ok} title={`${step.what}\n${step.url}`}>
					{#if step.thumb}
						<img src={step.thumb} alt="" loading="lazy" />
					{:else}
						<div class="blank"></div>
					{/if}
					<figcaption>
						<span class="what">{step.what}</span>
						<span class="where">{hostOf(step.url)}</span>
					</figcaption>
				</figure>
			{/each}
			<button class="clear" onclick={() => browserAgent.clearSteps()}>Clear</button>
		</div>
	{/if}
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
	.steps-toggle {
		width: auto;
		gap: 4px;
		padding: 0 6px;
	}
	.steps-toggle.on {
		color: var(--text);
	}
	.count {
		font-size: 12px;
		font-variant-numeric: tabular-nums;
	}

	.agent-bar {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 8px 12px;
		font-size: 13px;
		border-bottom: 1px solid var(--border);
		color: var(--text);
	}
	.agent-bar.driving {
		background: color-mix(in srgb, var(--primary) 10%, var(--surface));
	}
	.agent-bar.handoff {
		background: color-mix(in srgb, var(--warning) 14%, var(--surface));
	}
	.agent-bar.paused {
		background: var(--surface-elevated, var(--surface));
	}
	.say {
		flex: 1;
		min-width: 0;
	}
	.pulse {
		width: 8px;
		height: 8px;
		border-radius: 50%;
		background: var(--primary);
		animation: pulse 1.4s ease-in-out infinite;
	}
	@keyframes pulse {
		50% {
			opacity: 0.35;
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.pulse {
			animation: none;
		}
	}
	.bar-btn {
		height: 26px;
		padding: 0 12px;
		border-radius: 999px;
		border: 1px solid var(--border);
		background: var(--surface);
		color: var(--text);
		font-size: 12px;
		white-space: nowrap;
	}
	.bar-btn:hover {
		background: var(--surface-hover, var(--surface-elevated));
	}
	.bar-btn.primary {
		background: var(--primary);
		border-color: var(--primary);
		color: white;
	}

	/* The page is native and draws over .slot; the frame shows around it. */
	.stage {
		flex: 1;
		min-height: 0;
		display: flex;
	}
	.stage.driving {
		border: 3px solid var(--primary);
	}
	.stage.handoff {
		border: 3px solid var(--warning);
	}
	.slot {
		position: relative;
		flex: 1;
		min-height: 0;
		display: flex;
		align-items: center;
		justify-content: center;
		background: var(--surface);
	}

	.steps {
		display: flex;
		gap: 8px;
		padding: 8px;
		overflow-x: auto;
		border-top: 1px solid var(--border);
		flex-shrink: 0;
	}
	.step {
		flex: 0 0 132px;
		margin: 0;
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	.step img,
	.step .blank {
		width: 132px;
		height: 80px;
		object-fit: cover;
		object-position: top;
		border: 1px solid var(--border);
		background: var(--surface-elevated, var(--surface));
	}
	.step figcaption {
		display: flex;
		flex-direction: column;
		font-size: 11px;
		line-height: 1.3;
	}
	.what {
		color: var(--text);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.where {
		color: var(--text-muted);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.step.failed .what {
		color: var(--text-muted);
		text-decoration: line-through;
	}
	.clear {
		align-self: center;
		flex-shrink: 0;
		padding: 0 8px;
		font-size: 12px;
		color: var(--text-muted);
	}
	.note {
		color: var(--text-muted);
		font-size: 14px;
	}
</style>
