<script lang="ts">
	/**
	 * Home's tiles, under the composer: the first pin and the chat you were
	 * last in. A tile with nothing to show is not drawn. No counts.
	 */
	import { pinsStore } from '$lib/stores/pins.svelte';
	import { chatSessions } from '$lib/stores/chatSessions.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';

	let { currentChatId = null }: { currentChatId?: string | null } = $props();

	const pin = $derived(pinsStore.pins[0] ?? null);
	const last = $derived(
		[...chatSessions.sessions]
			.filter((s) => s.conversation_id !== currentChatId && (s.message_count ?? 0) > 0)
			.sort((a, b) => (b.last_message_at ?? '').localeCompare(a.last_message_at ?? ''))[0] ?? null
	);

	const tiles = $derived(
		[
			pin && {
				key: 'pin',
				kicker: 'Pinned',
				head: pin.title ?? pin.label ?? 'Untitled',
				open: () => windowShellStore.openTabFromRoute(pin.url, { label: pin.title ?? pin.label ?? undefined, focusExisting: true })
			},
			last && {
				key: 'continue',
				kicker: 'Continue',
				head: last.title || 'Untitled chat',
				open: () =>
					windowShellStore.openTabFromRoute(`/chat/${last.conversation_id}`, {
						label: last.title || undefined,
						focusExisting: true
					})
			}
		].filter(Boolean) as { key: string; kicker: string; head: string; open: () => void }[]
	);
</script>

{#if tiles.length}
	<div class="home-tiles">
		{#each tiles as t (t.key)}
			<button class="tile" onclick={t.open}>
				<span class="kicker">{t.kicker}</span>
				<span class="head">{t.head}</span>
			</button>
		{/each}
	</div>
{/if}

<style>
	.home-tiles {
		width: min(var(--reading-measure, 38rem), calc(100% - 2 * var(--reading-gutter, 1.5rem)));
		display: grid;
		grid-template-columns: repeat(3, minmax(0, 1fr));
		gap: 12px;
		pointer-events: auto;
	}

	.tile {
		aspect-ratio: 1 / 1;
		box-sizing: border-box;
		padding: 12px 16px;
		border: 1px solid var(--color-border);
		border-radius: 14px;
		display: flex;
		flex-direction: column;
		text-align: left;
		font: inherit;
		color: var(--color-foreground);
		background: var(--color-surface);
		cursor: pointer;
		transition: transform 240ms cubic-bezier(0.22, 1, 0.36, 1);
	}

	.tile:hover {
		transform: translateY(-2px);
	}

	.kicker {
		font-size: 12px;
		color: var(--color-foreground-subtle);
	}

	.head {
		margin-top: auto;
		font-family: var(--font-serif);
		font-size: 18px;
		line-height: 1.2;
		display: -webkit-box;
		-webkit-line-clamp: 2;
		line-clamp: 2;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}

	@media (prefers-reduced-motion: reduce) {
		.tile {
			transition: none;
		}
		.tile:hover {
			transform: none;
		}
	}

	@media (max-width: 768px), (pointer: coarse) {
		.home-tiles {
			display: none;
		}
	}
</style>
