<script lang="ts">
	/**
	 * Home's tiles, under the composer: the first pin, the chat you were last
	 * in, and last night's sleep. Each says what is in it: a line from inside
	 * the thing, or the night's length against the rest of the week. A tile
	 * with nothing to show is not drawn, and a line that can't be read is
	 * left out. No counts.
	 */
	import { pinsStore } from '$lib/stores/pins.svelte';
	import { chatSessions } from '$lib/stores/chatSessions.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { formatRelativeTimestamp } from '$lib/utils/dateUtils';
	import { hoursMinutes, lastReplyLine, pinLine, recentNights, type Nights } from './homeTiles';

	let { currentChatId = null }: { currentChatId?: string | null } = $props();

	const pin = $derived(pinsStore.pins[0] ?? null);
	const pinTitle = $derived(pin ? (pin.title ?? pin.label ?? 'Untitled') : '');
	const last = $derived(
		[...chatSessions.sessions]
			.filter((s) => s.conversation_id !== currentChatId && (s.message_count ?? 0) > 0)
			.sort((a, b) => (b.last_message_at ?? '').localeCompare(a.last_message_at ?? ''))[0] ?? null
	);

	const PIN_KICKER: Record<string, string> = { page: 'Pinned page', chat: 'Pinned chat', project: 'Pinned project' };

	let pinText = $state<string | null>(null);
	let lastText = $state<string | null>(null);
	let nights = $state<Nights | null>(null);

	// Each line is read when its thing changes. A failed read leaves the tile
	// with its name only.
	$effect(() => {
		const url = pin?.url;
		const title = pinTitle;
		pinText = null;
		if (!url) return;
		let live = true;
		pinLine(url, title).then(
			(line) => live && (pinText = line),
			() => {}
		);
		return () => (live = false);
	});

	$effect(() => {
		const id = last?.conversation_id;
		void last?.last_message_at;
		lastText = null;
		if (!id) return;
		let live = true;
		lastReplyLine(id).then(
			(line) => live && (lastText = line),
			() => {}
		);
		return () => (live = false);
	});

	$effect(() => {
		let live = true;
		recentNights().then(
			(n) => live && (nights = n),
			() => {}
		);
		return () => (live = false);
	});

	const USUAL_LINE = { about: 'About your usual.', longer: 'Longer than usual.', shorter: 'Shorter than usual.' };

	/** The week's nights as bar heights, last night at the end. */
	const bars = $derived.by(() => {
		if (!nights) return [];
		const known = nights.week.map((n) => n.minutes).filter((m): m is number => m !== null);
		const lo = Math.min(...known);
		const hi = Math.max(...known);
		return nights.week.map((n, i) => ({
			key: n.date,
			height: n.minutes === null ? 2 : 8 + Math.round(((n.minutes - lo) / Math.max(hi - lo, 1)) * 24),
			last: i === nights!.week.length - 1,
			missing: n.minutes === null
		}));
	});

	function chatWhen(iso: string): string {
		const when = formatRelativeTimestamp(iso);
		return `Chat, ${when === 'Yesterday' ? 'yesterday' : when}`;
	}

	interface Tile {
		key: string;
		kicker: string;
		head: string;
		line: string | null;
		open: (() => void) | null;
		sleep?: boolean;
	}

	const tiles = $derived(
		[
			pin && {
				key: 'pin',
				kicker: PIN_KICKER[pin.kind ?? ''] ?? 'Pinned',
				head: pinTitle,
				line: pinText,
				open: () => windowShellStore.openTabFromRoute(pin.url, { label: pinTitle, focusExisting: true })
			},
			last && {
				key: 'continue',
				kicker: last.last_message_at ? chatWhen(last.last_message_at) : 'Chat',
				head: last.title || 'Untitled chat',
				line: lastText,
				open: () =>
					windowShellStore.openTabFromRoute(`/chat/${last.conversation_id}`, {
						label: last.title || undefined,
						focusExisting: true
					})
			},
			nights?.last != null && {
				key: 'sleep',
				kicker: 'Sleep, last night',
				head: hoursMinutes(nights.last),
				line: nights.usual ? USUAL_LINE[nights.usual] : null,
				open: nights.lastUrl ? () => windowShellStore.openTabFromRoute(nights!.lastUrl!, { focusExisting: true }) : null,
				sleep: true
			}
		].filter(Boolean) as Tile[]
	);
</script>

{#snippet body(t: Tile)}
	<span class="kicker">{t.kicker}</span>
	<span class="head">{t.head}</span>
	{#if t.line}<span class="line">{t.line}</span>{/if}
	{#if t.sleep}
		<span class="bars" aria-hidden="true">
			{#each bars as b (b.key)}
				<span class="bar" class:last={b.last} class:missing={b.missing} style:height="{b.height}px"></span>
			{/each}
		</span>
	{/if}
{/snippet}

{#if tiles.length}
	<div class="home-tiles">
		{#each tiles as t (t.key)}
			{#if t.open}
				<button class="tile" onclick={t.open}>{@render body(t)}</button>
			{:else}
				<div class="tile still">{@render body(t)}</div>
			{/if}
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

	/* Short enough that the painting's village shows below the row. The
	   height is fixed, so a line arriving after its read does not move it. */
	.tile {
		height: 138px;
		box-sizing: border-box;
		padding: 13px 15px;
		border: 1px solid var(--color-border);
		border-radius: 14px;
		display: flex;
		flex-direction: column;
		gap: 5px;
		overflow: hidden;
		text-align: left;
		font: inherit;
		color: var(--color-foreground);
		background: var(--color-surface);
		cursor: pointer;
		transition: transform 240ms cubic-bezier(0.22, 1, 0.36, 1);
	}

	.tile.still {
		cursor: default;
	}

	button.tile:hover {
		transform: translateY(-2px);
	}

	button.tile:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: 2px;
	}

	.kicker {
		font-size: 12px;
		color: var(--color-foreground-subtle);
	}

	.head,
	.line {
		font-family: var(--font-serif);
		display: -webkit-box;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}

	.head {
		font-size: 18px;
		line-height: 1.2;
		-webkit-line-clamp: 2;
		line-clamp: 2;
	}

	.line {
		font-size: 14px;
		line-height: 1.35;
		color: var(--color-foreground-muted);
		-webkit-line-clamp: 2;
		line-clamp: 2;
		animation: line-in 240ms var(--ease-premium);
	}

	@keyframes line-in {
		from {
			opacity: 0;
		}
	}

	.bars {
		margin-top: auto;
		height: 32px;
		display: flex;
		align-items: flex-end;
		gap: 5px;
	}

	.bar {
		width: 9px;
		border-radius: var(--radius-xs);
		background: color-mix(in srgb, var(--color-foreground) 14%, transparent);
	}

	.bar.missing {
		background: color-mix(in srgb, var(--color-foreground) 8%, transparent);
	}

	.bar.last {
		background: var(--color-primary);
	}

	@media (prefers-reduced-motion: reduce) {
		.tile,
		.line {
			transition: none;
			animation: none;
		}
		button.tile:hover {
			transform: none;
		}
	}

	@media (max-width: 768px), (pointer: coarse) {
		.home-tiles {
			display: none;
		}
	}
</style>
