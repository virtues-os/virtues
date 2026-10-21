<script lang="ts">
	/**
	 * The ∴ mark at the left of the window header, and the inbox behind it.
	 *
	 * The mark is the app's identity, and it says whether anything needs you:
	 * its top dot, the "therefore", takes the accent while the server lists
	 * something (`/api/attention`). Nothing is counted. The dot goes out on its
	 * own when the server sees the problem fixed, because every item is a
	 * condition the server observes, not a message to be read.
	 *
	 * Clicking opens the list. Each row leads with what is wrong and opens the
	 * page that fixes it.
	 */
	import { onMount } from 'svelte';
	import { Popover } from '$lib/floating';
	import { attention } from '$lib/stores/attention.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import type { AttentionItem } from '$lib/api/client';

	let open = $state(false);

	const items = $derived(attention.items);
	const lit = $derived(items.length > 0);

	onMount(() => {
		attention.start();
		return () => attention.stop();
	});

	function openItem(item: AttentionItem, close: () => void) {
		close();
		windowShellStore.openTabFromRoute(item.route, { focusExisting: true });
	}
</script>

<Popover bind:open placement="bottom-start" offset={6}>
	{#snippet trigger({ toggle })}
		<button
			type="button"
			class="header-mark"
			class:lit
			aria-label={lit ? 'Inbox. Something needs you.' : 'Inbox'}
			aria-haspopup="dialog"
			aria-expanded={open}
			title={lit ? 'Something needs you' : 'Inbox'}
			onclick={() => {
				if (!open) void attention.refresh();
				toggle();
			}}
		>
			<!-- Drawn, not typed. A typed ∴ glyph is text-weight — a 21px glyph
			     put a 12px figure with 2px dots over a column of 20px line icons.
			     The geometry is the app icon's and ThinkingMark's, exactly:
			     equilateral, side 15, r 3, on the 24-unit box `virtues:logo` uses
			     in icons.ts, so the header, the chat, and the Dock all show one
			     mark. -->
			<svg viewBox="0 0 24 24" width="17" height="17" aria-hidden="true" focusable="false">
				<circle class="mark-dot apex" cx="12" cy="5" r="3" />
				<circle class="mark-dot left" cx="4.5" cy="18" r="3" />
				<circle class="mark-dot right" cx="19.5" cy="18" r="3" />
			</svg>
		</button>
	{/snippet}

	{#snippet children({ close })}
		<div class="inbox" role="dialog" aria-label="Inbox">
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

<style>
	.header-mark {
		display: grid;
		place-items: center;
		width: 32px;
		height: 32px;
		flex: none;
		padding: 0;
		border: none;
		border-radius: 6px;
		background: transparent;
		color: var(--color-foreground);
		cursor: pointer;
		/* The header's motion: hover on in 80ms, off over 200ms; a press
		   sinks to 94% and springs back (WindowHeader's .lead-btn). */
		transition:
			background-color 200ms var(--ease-premium),
			transform 180ms var(--ease-premium);
	}

	.header-mark:hover {
		background: var(--hover-bg);
		transition-duration: 80ms, 180ms;
	}

	.header-mark:active {
		background: var(--press-bg);
		transform: scale(0.94);
		transition-duration: 60ms;
	}

	.header-mark:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: -2px;
	}

	.header-mark svg {
		display: block;
		/* The app opening scales the dots past the 24-box. */
		overflow: visible;
	}

	.mark-dot {
		fill: currentColor;
		transform-box: fill-box;
		transform-origin: center;
		transition: fill 300ms var(--ease-premium);
	}

	/* The theme's accent, not red: something is waiting, nothing is on fire.
	   As it lights, the dot swells once, the conclusion beat of the mark's
	   own syllogism (ThinkingMark's 1.26), and then holds still: a dot that
	   kept pulsing would say the server is working, which it is not. */
	.lit .mark-dot.apex {
		fill: var(--color-primary);
		animation: dot-lights 640ms var(--ease-premium) 1;
	}

	@keyframes dot-lights {
		0% { transform: scale(1); }
		35% { transform: scale(1.26); }
		100% { transform: scale(1); }
	}

	@media (prefers-reduced-motion: reduce) {
		.header-mark,
		.mark-dot,
		.inbox-row {
			transition: none;
		}

		.header-mark:active {
			transform: none;
		}

		.lit .mark-dot.apex {
			animation: none;
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
		gap: 2px;
		width: 100%;
		padding: 8px;
		border: none;
		border-radius: 6px;
		background: transparent;
		text-align: left;
		cursor: pointer;
		transition: background-color 200ms var(--ease-premium);
	}

	.inbox-row:hover {
		background: var(--hover-bg);
		transition-duration: 80ms;
	}

	.inbox-row:active {
		background: var(--press-bg);
		transition-duration: 60ms;
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
</style>
