<script lang="ts">
	/**
	 * The status capsule at the right end of the window header: the one place
	 * the window says what your server is doing and whether anything needs
	 * you. It shows one state, the first of these that holds:
	 *
	 *   - Your server isn't answering: it says so. Neutral colors, because a
	 *     server asleep is ordinary.
	 *   - A chat is replying, here or on another device: a spinner, the words,
	 *     and the cloud mark, because every reply is written by a model off
	 *     your server. The tooltip names the model.
	 *   - Your server lists something that needs you (`/api/attention`): the
	 *     dot takes the accent and swells once, then holds still.
	 *   - Otherwise: a quiet dot, and no words.
	 *
	 * Clicking opens the list of what needs you, whatever it shows, with a row
	 * that leads to /reconnect while the server is away. Nothing is counted.
	 * Every item goes out on its own when the server sees its condition end.
	 */
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import Icon from '$lib/components/Icon.svelte';
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
	const lit = $derived(items.length > 0);

	const mode = $derived(
		reachability.unreachable ? 'offline' : chatActivity.anyRunning ? 'working' : lit ? 'lit' : 'rest'
	);

	const offTip = $derived.by(() => {
		const model = getSelectedModel() ?? getDefaultModel();
		return model
			? `Replies are written by ${model.displayName}, off your server.`
			: 'Replies are written by a model provider, off your server.';
	});

	const tip = $derived(
		mode === 'offline'
			? "Your server isn't answering"
			: mode === 'working'
				? offTip
				: mode === 'lit'
					? 'Something needs you'
					: 'Nothing needs you'
	);

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
				class:rest={mode === 'rest' || mode === 'lit'}
				class:lit={mode === 'lit'}
				aria-label={mode === 'working' ? `Writing a reply. ${tip}` : tip}
				aria-haspopup="dialog"
				aria-expanded={open}
				data-tip={open ? undefined : tip}
				onclick={() => {
					if (!open) void attention.refresh();
					toggle();
				}}
			>
				{#if mode === 'offline'}
					<Icon icon="mynaui:wifi-off" width="14" />
					<span class="label">Can't reach your server</span>
				{:else if mode === 'working'}
					<span class="spin" aria-hidden="true"></span>
					<span class="label">Writing a reply</span>
					<span class="off" aria-hidden="true"><Icon icon="mynaui:cloud" width="16" /></span>
				{:else}
					<span class="dot" aria-hidden="true"></span>
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
	/* The corner of the header. The anchor is positioned, not the capsule,
	   so the list opens under the capsule itself. */
	.capsule-anchor {
		position: absolute;
		top: 7px;
		right: 10px;
		z-index: 3;
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
		background: var(--color-surface);
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

	.dot {
		width: 7px;
		height: 7px;
		border-radius: 50%;
		background: var(--color-success);
		transition: background-color var(--motion-state) var(--ease-premium);
	}

	/* The theme's accent, not red: something is waiting, nothing is on fire.
	   The dot swells once as it lights and then holds still, since a dot
	   that kept pulsing would say the server is working. */
	.lit .dot {
		background: var(--color-primary);
		animation: dot-lights 640ms var(--ease-premium) 1;
	}

	@keyframes dot-lights {
		0% {
			transform: scale(1);
		}
		35% {
			transform: scale(1.4);
		}
		100% {
			transform: scale(1);
		}
	}

	.off {
		display: flex;
		margin-left: 4px;
		color: var(--color-foreground-muted);
	}

	.spin {
		width: 12px;
		height: 12px;
		box-sizing: border-box;
		border-radius: 50%;
		border: 1.5px solid color-mix(in srgb, var(--color-primary) 25%, transparent);
		border-top-color: var(--color-primary);
		animation: spin 1.1s linear infinite;
	}

	@keyframes spin {
		to {
			transform: rotate(360deg);
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
		.spin,
		.status-capsule,
		.lit .dot {
			animation: none;
		}

		.status-capsule,
		.dot,
		.inbox-row {
			transition: none;
		}
	}
</style>
