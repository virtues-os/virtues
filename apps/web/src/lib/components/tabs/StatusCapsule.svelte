<script lang="ts">
	/**
	 * The status capsule at the right end of the window header: the one place
	 * the window says what your server is doing and whether anything needs
	 * you. It is drawn as the app's ∴, and shows one state, the first of
	 * these that holds:
	 *
	 *   - Your server isn't answering: the dots turn to outlines and it says
	 *     so. Neutral colors, because a server asleep is ordinary.
	 *   - A chat is replying, here or on another device: the thinking mark
	 *     and the words. The tooltip names the model.
	 *   - Your server lists something you have not looked at yet
	 *     (`attention.unseen`): the top dot grows and takes the accent while
	 *     the other two recede, and one ring spreads out, once. The shape
	 *     changes as well as the color. Pointing at it widens the capsule to
	 *     name the first such thing, in words.
	 *   - Otherwise: the ∴ in quiet ink, and no words.
	 *
	 * Clicking opens the list of what needs you, whatever it shows, with a row
	 * that leads to /reconnect while the server is away. Opening it marks
	 * everything in it seen, so the mark goes quiet while the items stay
	 * listed. Nothing is counted. Every item goes out on its own when the
	 * server sees its condition end.
	 */
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import ThinkingMark from '$lib/components/ThinkingMark.svelte';
	import { Popover } from '$lib/floating';
	import type { AttentionItem } from '$lib/api/client';
	import { attention } from '$lib/stores/attention.svelte';
	import { chatActivity } from '$lib/stores/chatActivity.svelte';
	import { reachability } from '$lib/stores/reachability.svelte';
	import { getDefaultModel, getSelectedModel } from '$lib/stores/models.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';

	// The app's own copy stays up when the server goes away and carries
	// /reconnect. A browser on the LAN has no pairing to repair.
	const canReconnect =
		typeof window !== 'undefined' &&
		!!(window as unknown as { __VIRTUES_BACKEND_ORIGIN__?: string }).__VIRTUES_BACKEND_ORIGIN__;

	let open = $state(false);
	const items = $derived(attention.items);
	const fresh = $derived(attention.unseen);
	const lit = $derived(fresh.length > 0);
	/** Changes when something new arrives, so the mark lights again for it. */
	const litKey = $derived(fresh.map((item) => item.key).join(' '));

	// Whatever is in the list while it is open has been seen, including an
	// item that arrives with it open.
	$effect(() => {
		void attention.items;
		if (open) attention.markSeen();
	});

	const mode = $derived(
		reachability.unreachable ? 'offline' : chatActivity.anyRunning ? 'working' : lit ? 'lit' : 'rest'
	);

	const writingTip = $derived.by(() => {
		const model = getSelectedModel() ?? getDefaultModel();
		return model ? `${model.displayName} is writing a reply` : 'Writing a reply';
	});

	const tip = $derived(
		mode === 'offline'
			? "Your server isn't answering"
			: mode === 'working'
				? writingTip
				: mode === 'lit'
					? `Something new needs you: ${fresh[0]?.title ?? ''}`
					: items.length > 0
						? 'What needs you'
						: 'Nothing needs you'
	);

	const DOTS: [number, number][] = [
		[12, 5],
		[4.5, 18],
		[19.5, 18]
	];

	onMount(() => {
		attention.start();
		return () => attention.stop();
	});

	function openItem(item: AttentionItem, close: () => void) {
		close();
		windowShellStore.openTabFromRoute(item.route, { focusExisting: true });
	}
</script>

<div class="capsule-anchor">
	<Popover bind:open placement="bottom-end" offset={6}>
		{#snippet trigger({ toggle })}
			<button
				type="button"
				class="status-capsule"
				class:rest={mode === 'rest'}
				class:lit={mode === 'lit'}
				aria-label={tip}
				aria-haspopup="dialog"
				aria-expanded={open}
				data-tip={open || mode === 'lit' ? undefined : tip}
				onclick={() => {
					if (!open) void attention.refresh();
					toggle();
				}}
			>
				{#if mode === 'offline'}
					<svg class="mark away" viewBox="0 0 24 24" width="13" height="13" aria-hidden="true">
						{#each DOTS as [x, y] (x)}<circle cx={x} cy={y} r="2.6" />{/each}
					</svg>
					<span class="label">Can't reach your server</span>
				{:else if mode === 'working'}
					<ThinkingMark depth={3} size={16} />
					<span class="label">Writing a reply</span>
				{:else if mode === 'lit'}
					{#key litKey}
						<svg class="mark lights" viewBox="0 0 24 24" width="13" height="13" aria-hidden="true">
							<circle class="ring" cx="12" cy="5" r="3.6" />
							<circle class="apex" cx="12" cy="5" r="3.6" />
							<circle class="base" cx="4.5" cy="18" r="2.4" />
							<circle class="base" cx="19.5" cy="18" r="2.4" />
						</svg>
					{/key}
					<span class="peek" aria-hidden="true">{fresh[0]?.title}</span>
				{:else}
					<svg class="mark" viewBox="0 0 24 24" width="13" height="13" aria-hidden="true">
						{#each DOTS as [x, y] (x)}<circle cx={x} cy={y} r="3" />{/each}
					</svg>
				{/if}
			</button>
		{/snippet}
		{#snippet children({ close })}
			<div class="inbox" role="dialog" aria-label="Needs you">
				{#if mode === 'offline'}
					{#if canReconnect}
						<button
							type="button"
							class="inbox-row"
							onclick={() => {
								close();
								void goto('/reconnect');
							}}
						>
							<span class="inbox-title">Can't reach your server</span>
							<span class="inbox-body">Find out why and fix it.</span>
						</button>
					{:else}
						<div class="inbox-row static">
							<span class="inbox-title">Can't reach your server</span>
							<span class="inbox-body">This page catches up when your server answers again.</span>
						</div>
					{/if}
				{/if}
				<div class="inbox-head">Needs you</div>
				{#if items.length === 0}
					<p class="inbox-empty">
						{attention.loaded ? 'Nothing needs you.' : 'Checking your server…'}
					</p>
				{:else}
					<ul class="inbox-list">
						{#each items as item (item.key)}
							<li>
								<button type="button" class="inbox-row" onclick={() => openItem(item, close)}>
									<span class="inbox-title">{item.title}</span>
									<span class="inbox-body">{item.body}</span>
								</button>
							</li>
						{/each}
					</ul>
				{/if}
			</div>
		{/snippet}
	</Popover>
</div>

<style>
	/* WindowHeader places the corner. The anchor wraps only the capsule, so
	   the list opens under the capsule itself. */
	.capsule-anchor {
		position: relative;
		display: flex;
	}

	.status-capsule {
		height: 30px;
		min-width: 30px;
		padding: 0 12px;
		box-sizing: border-box;
		border: 1px solid var(--color-border);
		border-radius: 15px;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		gap: 8px;
		font: inherit;
		font-size: 12px;
		white-space: nowrap;
		color: var(--color-foreground);
		background: var(--glass-bg, var(--color-surface));
		backdrop-filter: var(--glass-blur, none);
		-webkit-backdrop-filter: var(--glass-blur, none);
		cursor: pointer;
		animation: capsule-in 220ms var(--ease-premium);
		transition: border-color var(--motion-hover-out) var(--ease-premium);
	}

	.status-capsule:hover,
	.status-capsule[aria-expanded='true'] {
		border-color: var(--color-border-strong);
		transition-duration: var(--motion-hover-in);
	}

	.status-capsule:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: 2px;
	}

	.status-capsule.rest {
		padding: 0;
		width: 30px;
		animation: none;
	}

	/* Lit, it rests at the round size and widens to name the first new
	   thing while you point at it or tab to it. */
	.status-capsule.lit {
		gap: 0;
		padding: 0 8px;
		animation: none;
		transition:
			border-color var(--motion-hover-out) var(--ease-premium),
			padding var(--motion-layout) var(--ease-premium);
	}

	.peek {
		max-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		opacity: 0;
		transition:
			max-width var(--motion-layout) var(--ease-premium),
			margin var(--motion-layout) var(--ease-premium),
			opacity var(--motion-state) var(--ease-premium);
	}

	.status-capsule.lit:hover,
	.status-capsule.lit:focus-visible {
		padding: 0 12px 0 8px;
	}

	.status-capsule.lit:hover .peek,
	.status-capsule.lit:focus-visible .peek {
		max-width: 260px;
		margin-left: 8px;
		opacity: 1;
	}

	.mark {
		flex: none;
		overflow: visible;
		fill: var(--color-foreground-muted);
	}

	.mark.away circle {
		fill: none;
		stroke: var(--color-foreground-muted);
		stroke-width: 1.6;
	}

	/* The theme's accent, not red: something is waiting, nothing is on fire.
	   The top dot swells once as it lights and then holds still, since a
	   mark that kept pulsing would say the server is working. */
	.lights .apex {
		fill: var(--color-primary);
		transform-box: fill-box;
		transform-origin: center;
		animation: apex-lights 640ms var(--ease-premium) 1;
	}

	.lights .base {
		fill: var(--color-foreground-subtle);
	}

	.lights .ring {
		fill: none;
		stroke: var(--color-primary);
		stroke-width: 1.4;
		opacity: 0;
		transform-box: fill-box;
		transform-origin: center;
		animation: ring-spreads 900ms cubic-bezier(0.22, 1, 0.36, 1) 120ms 1;
	}

	@keyframes apex-lights {
		0% {
			transform: scale(0.7);
		}
		40% {
			transform: scale(1.3);
		}
		100% {
			transform: scale(1);
		}
	}

	@keyframes ring-spreads {
		0% {
			opacity: 0.7;
			transform: scale(1);
		}
		100% {
			opacity: 0;
			transform: scale(3.4);
		}
	}

	@keyframes capsule-in {
		from {
			opacity: 0;
			transform: scale(0.96);
		}
	}

	.inbox {
		width: 320px;
		max-width: calc(100vw - 32px);
		padding: 6px;
	}

	.inbox-head {
		padding: 6px 8px 4px;
		font-size: 12px;
		font-weight: 500;
		color: var(--color-foreground-muted);
	}

	.inbox-empty {
		margin: 0;
		padding: 6px 8px 8px;
		font-size: 13px;
		color: var(--color-foreground-muted);
	}

	.inbox-list {
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.inbox-row {
		display: flex;
		flex-direction: column;
		gap: 4px;
		width: 100%;
		padding: 8px;
		border: none;
		border-radius: var(--radius-md);
		background: transparent;
		text-align: left;
		cursor: pointer;
		transition: background-color var(--motion-hover-out) var(--ease-premium);
	}

	.inbox-row.static {
		cursor: default;
	}

	button.inbox-row:hover {
		background: var(--hover-bg);
		transition-duration: var(--motion-hover-in);
	}

	button.inbox-row:active {
		background: var(--press-bg);
		transition-duration: var(--motion-press);
	}

	.inbox-row:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: -2px;
	}

	.inbox-title {
		font-size: 13px;
		font-weight: 500;
		color: var(--color-foreground);
	}

	.inbox-body {
		font-size: 12px;
		line-height: 1.4;
		color: var(--color-foreground-muted);
	}

	@media (prefers-reduced-motion: reduce) {
		.status-capsule,
		.lights .apex,
		.lights .ring {
			animation: none;
		}

		.status-capsule,
		.status-capsule.lit,
		.peek,
		.inbox-row {
			transition: none;
		}
	}
</style>
