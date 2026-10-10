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
	 * waits for Done. Its steps collect in an Activity row under the page, off
	 * unless you open it: most people watch the page, not a log.
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
	let trail = $state<HTMLOListElement | null>(null);

	const STEPS_KEY = 'virtues.browser.steps';
	let showSteps = $state(readShowSteps());
	function readShowSteps(): boolean {
		try {
			return localStorage.getItem(STEPS_KEY) === '1';
		} catch {
			return false;
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

	const latestId = $derived(browserAgent.steps.at(-1)?.id ?? null);

	// A new step comes into view.
	$effect(() => {
		void latestId;
		if (trail) requestAnimationFrame(() => trail && (trail.scrollLeft = trail.scrollWidth));
	});

	/** Steps as chips: a run of the same step on the same page is one chip
	 *  with a count. */
	const rows = $derived.by(() => {
		const out: { step: (typeof browserAgent.steps)[number]; times: number }[] = [];
		for (const step of browserAgent.steps) {
			const prev = out.at(-1);
			if (prev && prev.step.what === step.what && prev.step.url === step.url && prev.step.ok === step.ok) {
				prev.step = step;
				prev.times += 1;
				continue;
			}
			out.push({ step, times: 1 });
		}
		return out;
	});

	const GLYPHS: Record<string, string> = {
		open: 'ri:global-line',
		snapshot: 'ri:file-text-line',
		click: 'ri:cursor-line',
		type: 'ri:keyboard-line',
		press: 'ri:corner-down-left-line',
		scroll: 'ri:arrow-up-down-line',
		screenshot: 'ri:eye-line',
		handoff: 'ri:hand'
	};
	function glyph(op: string | undefined) {
		return GLYPHS[op ?? ''] ?? 'ri:checkbox-blank-circle-line';
	}

	// Minutes tick over without a new step.
	let now = $state(Date.now());
	$effect(() => {
		const timer = setInterval(() => (now = Date.now()), 30_000);
		return () => clearInterval(timer);
	});
	function ago(at: number) {
		const s = Math.max(0, Math.round((now - at) / 1000));
		if (s < 45) return 'just now';
		const m = Math.round(s / 60);
		if (m < 60) return `${m} min ago`;
		return new Date(at).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });
	}

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
		// The bar, frame and Activity column move the page.
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
		<div class="nav">
			<button class="tool" title="Back" aria-label="Back" onclick={() => void browserPaneGo('back')} disabled={!available}>
				<Icon icon="ri:arrow-left-line" width="16" />
			</button>
			<button class="tool" title="Forward" aria-label="Forward" onclick={() => void browserPaneGo('forward')} disabled={!available}>
				<Icon icon="ri:arrow-right-line" width="16" />
			</button>
			<span class="divider" aria-hidden="true"></span>
			<button class="tool" title="Reload" aria-label="Reload" onclick={() => void browserPaneGo('reload')} disabled={!available}>
				<Icon icon="ri:refresh-line" width="16" />
			</button>
		</div>
		<input
			class="address"
			class:editing
			value={editing ? address : hostOf(address) || address}
			placeholder="Enter an address or search"
			spellcheck="false"
			autocapitalize="off"
			oninput={(e) => (address = e.currentTarget.value)}
			onfocus={(e) => {
				editing = true;
				const input = e.currentTarget;
				requestAnimationFrame(() => input.select());
			}}
			onblur={() => (editing = false)}
			onkeydown={(e) => {
				if (e.key === 'Enter' && address.trim()) {
					go(resolve(address));
					(e.currentTarget as HTMLInputElement).blur();
				} else if (e.key === 'Escape') {
					(e.currentTarget as HTMLInputElement).blur();
				}
			}}
			disabled={!available}
		/>
		{#if browserAgent.steps.length > 0}
			<button
				class="round"
				class:on={showSteps}
				title={showSteps ? 'Hide activity' : 'Show what your assistant did'}
				aria-label="Activity"
				aria-pressed={showSteps}
				onclick={toggleSteps}
			>
				<Icon icon="ri:history-line" width="16" />
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

	<div class="body">
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

	</div>

	{#if showSteps && browserAgent.steps.length > 0}
		<div class="activity" aria-label="Activity">
			<ol class="trail" bind:this={trail}>
				{#each rows as { step, times } (step.id)}
					<li
						class="chip"
						class:failed={!step.ok}
						class:live={step.id === latestId && browserAgent.driving}
						title={`${step.what}${times > 1 ? ` ×${times}` : ''} · ${hostOf(step.url)} · ${ago(step.at)}`}
					>
						{#if step.ok && step.thumb}
							<img src={step.thumb} alt="" />
						{:else}
							<span class="glyph"><Icon icon={step.ok ? glyph(step.op) : 'ri:error-warning-line'} width="13" /></span>
						{/if}
						<span class="what">{step.what}</span>
						{#if times > 1}<span class="times">×{times}</span>{/if}
					</li>
				{/each}
			</ol>
			<button class="link" onclick={() => browserAgent.clearSteps()}>Clear</button>
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
		gap: 8px;
		padding: 8px 12px;
	}
	.nav {
		display: flex;
		align-items: center;
		gap: 4px;
		height: 36px;
		padding: 0 4px;
		border-radius: 999px;
		background: var(--surface-elevated, var(--surface));
		flex-shrink: 0;
	}
	.divider {
		width: 1px;
		height: 16px;
		background: var(--border);
	}
	.tool {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 28px;
		height: 28px;
		border-radius: 50%;
		color: var(--text-muted);
	}
	.tool:hover:not(:disabled) {
		background: var(--surface);
		color: var(--text);
	}
	.tool:disabled {
		opacity: 0.4;
	}
	.address {
		flex: 1;
		min-width: 0;
		height: 36px;
		padding: 0 16px;
		border-radius: 999px;
		border: 1px solid transparent;
		background: var(--surface-elevated, var(--surface));
		color: var(--text);
		font-size: 13px;
		text-align: center;
		text-overflow: ellipsis;
	}
	.address.editing {
		text-align: left;
		border-color: var(--border-focus, var(--border));
		background: var(--surface);
		outline: none;
	}
	.round {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 36px;
		height: 36px;
		border-radius: 50%;
		background: var(--surface-elevated, var(--surface));
		color: var(--text-muted);
		flex-shrink: 0;
	}
	.round:hover,
	.round.on {
		color: var(--text);
	}

	.agent-bar {
		display: flex;
		align-items: center;
		gap: 8px;
		margin: 0 12px 8px;
		padding: 8px 8px 8px 16px;
		border-radius: 999px;
		font-size: 13px;
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
	/* The page is a rounded card; the shell rounds the native view to match
	   (browser_host.rs, PAGE_RADIUS). */
	.stage {
		flex: 1;
		min-height: 0;
		display: flex;
		margin: 0 12px 12px;
		border: 2px solid transparent;
		border-radius: 12px;
		overflow: hidden;
	}
	.stage.driving {
		border-color: var(--primary);
	}
	.stage.handoff {
		border-color: var(--warning);
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

	.body {
		flex: 1;
		min-height: 0;
		display: flex;
	}

	.activity {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 0 12px 12px;
		flex-shrink: 0;
	}
	.trail {
		flex: 1;
		min-width: 0;
		display: flex;
		gap: 8px;
		list-style: none;
		margin: 0;
		padding: 0;
		overflow-x: auto;
		scrollbar-width: none;
	}
	.chip {
		flex-shrink: 0;
		display: flex;
		align-items: center;
		gap: 8px;
		height: 32px;
		max-width: 240px;
		padding-right: 12px;
		border: 1px solid var(--border);
		border-radius: 999px;
		overflow: hidden;
		background: var(--surface);
	}
	.chip img {
		width: 44px;
		height: 100%;
		object-fit: cover;
		object-position: top left;
		border-right: 1px solid var(--border);
	}
	.chip .glyph {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 32px;
		height: 100%;
		color: var(--text-muted);
	}
	.chip.failed .glyph {
		color: var(--warning);
	}
	.chip.live {
		border-color: var(--primary);
	}
	.what {
		font-size: 12px;
		color: var(--text);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.chip:not(:last-child) .what,
	.chip.failed .what {
		color: var(--text-muted);
	}
	.times {
		font-size: 12px;
		color: var(--text-muted);
		font-variant-numeric: tabular-nums;
	}
	.link {
		flex-shrink: 0;
		font-size: 12px;
		color: var(--text-muted);
	}
	.link:hover {
		color: var(--text);
	}
	.note {
		color: var(--text-muted);
		font-size: 14px;
	}
</style>
