<script lang="ts">
	/**
	 * Home's tiles, under the composer: the first pin, the chat you were last
	 * in, and last night's sleep. Each says what is in it: a line from inside
	 * the thing, or the night's length against the rest of the week. A tile
	 * with nothing to show is not drawn, and a line that can't be read is
	 * left out. No counts.
	 *
	 * A small switch above them picks how they are shown (`homeLayout`):
	 * cards, rows, or one short paragraph that says the same things in words.
	 * The row it sits on also holds `lead`, home's notes line (HomeNotes), so
	 * the line sits just above the tiles instead of on a row of its own.
	 */
	import type { Snippet } from 'svelte';
	import { fade } from 'svelte/transition';
	import Icon from '$lib/components/Icon.svelte';
	import AtlasIcon from '$lib/components/sidebar/AtlasIcon.svelte';
	import { pinsStore } from '$lib/stores/pins.svelte';
	import { chatSessions } from '$lib/stores/chatSessions.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { formatRelativeTimestamp } from '$lib/utils/dateUtils';
	import { hoursMinutes, lastReplyLine, pinLine, recentNights, type Nights } from './homeTiles';
	import { homeLayout, type HomeLayout } from './homeLayout.svelte';

	let { currentChatId = null, lead }: { currentChatId?: string | null; lead?: Snippet } = $props();

	const pin = $derived(pinsStore.pins[0] ?? null);
	const pinTitle = $derived(pin ? (pin.title ?? pin.label ?? 'Untitled') : '');
	const last = $derived(
		[...chatSessions.sessions]
			.filter((s) => s.conversation_id !== currentChatId && (s.message_count ?? 0) > 0)
			.sort((a, b) => (b.last_message_at ?? '').localeCompare(a.last_message_at ?? ''))[0] ?? null
	);

	const PIN_KICKER: Record<string, string> = { page: 'Pinned page', chat: 'Pinned chat', thread: 'Pinned thread' };

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
	const USUAL_WORDS = { about: 'about your usual', longer: 'longer than usual', shorter: 'shorter than usual' };

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

	/** When you were last in a chat, as the start of a sentence. */
	function whenWords(iso: string): string {
		const when = formatRelativeTimestamp(iso);
		if (when.includes(':')) return 'Earlier today';
		if (when === 'Yesterday') return 'Yesterday';
		return `On ${when}`;
	}

	interface Tile {
		key: string;
		kicker: string;
		head: string;
		line: string | null;
		open: (() => void) | null;
		/** Where `open` goes, so the paragraph's names are real links. */
		url: string | null;
		sleep?: boolean;
		/** The same thing as a sentence, for the paragraph layout. */
		before: string;
		after: string;
	}

	const tiles = $derived(
		[
			last && {
				key: 'continue',
				kicker: last.last_message_at ? chatWhen(last.last_message_at) : 'Chat',
				head: last.title || 'Untitled chat',
				line: lastText,
				open: () =>
					windowShellStore.openTabFromRoute(`/chat/${last.conversation_id}`, {
						label: last.title || undefined,
						focusExisting: true
					}),
				url: `/chat/${last.conversation_id}`,
				before: `${last.last_message_at ? whenWords(last.last_message_at) : 'Lately'} you were in `,
				after: '.'
			},
			pin && {
				key: 'pin',
				kicker: PIN_KICKER[pin.kind ?? ''] ?? 'Pinned',
				head: pinTitle,
				line: pinText,
				open: () => windowShellStore.openTabFromRoute(pin.url, { label: pinTitle, focusExisting: true }),
				url: pin.url,
				// The sentence never starts on a title, whose case is the author's.
				before: 'You pinned ',
				after: '.'
			},
			nights?.last != null && {
				key: 'sleep',
				kicker: 'Sleep, last night',
				head: hoursMinutes(nights.last),
				line: nights.usual ? USUAL_LINE[nights.usual] : null,
				open: nights.lastUrl ? () => windowShellStore.openTabFromRoute(nights!.lastUrl!, { focusExisting: true }) : null,
				url: nights.lastUrl,
				sleep: true,
				before: 'Last night you slept ',
				after: nights.usual ? `, ${USUAL_WORDS[nights.usual]}.` : '.'
			}
		].filter(Boolean) as Tile[]
	);

	/** Cards and rows put the pin first. The paragraph starts with the chat,
	 *  since "you were in" reads as the opening of a sentence. */
	const ORDER = ['pin', 'continue', 'sleep'];
	const ordered = $derived([...tiles].sort((a, b) => ORDER.indexOf(a.key) - ORDER.indexOf(b.key)));

	const LAYOUTS: { key: HomeLayout; label: string; icon: string }[] = [
		{ key: 'cards', label: 'Show cards', icon: 'mynaui:columns' },
		{ key: 'rows', label: 'Show rows', icon: 'mynaui:list' },
		{ key: 'prose', label: 'Show a sentence', icon: 'mynaui:text-align-left' }
	];

	let switchEl = $state<HTMLElement | null>(null);

	/** Arrow keys move through the switch, as in any radio group. */
	function onSwitchKey(e: KeyboardEvent) {
		const step = e.key === 'ArrowRight' || e.key === 'ArrowDown' ? 1 : e.key === 'ArrowLeft' || e.key === 'ArrowUp' ? -1 : 0;
		if (!step) return;
		e.preventDefault();
		const at = LAYOUTS.findIndex((l) => l.key === homeLayout.value);
		const next = LAYOUTS[(at + step + LAYOUTS.length) % LAYOUTS.length];
		homeLayout.set(next.key);
		switchEl?.querySelector<HTMLElement>(`[data-layout="${next.key}"]`)?.focus();
	}
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

{#snippet glyph(t: Tile)}
	<span class="row-icon" aria-hidden="true">
		{#if t.key === 'pin'}
			<AtlasIcon name="pin" size={15} bare />
		{:else if t.key === 'continue'}
			<AtlasIcon name="chats" size={15} bare />
		{:else}
			<Icon icon="mynaui:moon" width="15" />
		{/if}
	</span>
{/snippet}

{#snippet row(t: Tile)}
	{@render glyph(t)}
	<span class="row-head">{t.head}</span>
	{#if t.line}<span class="row-line">{t.line}</span>{/if}
	<span class="row-kicker">{t.kicker}</span>
{/snippet}

{#if tiles.length || lead}
	<div class="home-tiles">
		<div class="tiles-head">
		{@render lead?.()}
		{#if tiles.length}
		<div class="layout-switch" role="radiogroup" aria-label="How home shows these" tabindex="-1" bind:this={switchEl} onkeydown={onSwitchKey}>
			{#each LAYOUTS as l (l.key)}
				{@const on = homeLayout.value === l.key}
				<button
					type="button"
					role="radio"
					aria-checked={on}
					aria-label={l.label}
					data-tip={l.label}
					data-layout={l.key}
					tabindex={on ? 0 : -1}
					class:on
					onclick={() => homeLayout.set(l.key)}
				>
					<Icon icon={l.icon} width="14" />
				</button>
			{/each}
		</div>
		{/if}
		</div>

		{#if tiles.length}
		{#key homeLayout.value}
			<div class="layout" in:fade={{ duration: 160 }}>
				{#if homeLayout.value === 'cards'}
					<div class="cards">
						{#each ordered as t (t.key)}
							{#if t.open}
								<button class="tile lq lq-regular" onclick={t.open}>{@render body(t)}</button>
							{:else}
								<div class="tile lq lq-regular still">{@render body(t)}</div>
							{/if}
						{/each}
					</div>
				{:else if homeLayout.value === 'rows'}
					<div class="rows lq lq-regular">
						{#each ordered as t (t.key)}
							{#if t.open}
								<button class="row" onclick={t.open}>{@render row(t)}</button>
							{:else}
								<div class="row still">{@render row(t)}</div>
							{/if}
						{/each}
					</div>
				{:else}
					<p class="sentence">
						{#each tiles as t, i (t.key)}{#if i > 0}{' '}{/if}{t.before}{#if t.open && t.url}<a
									class="link"
									href={t.url}
									onclick={(e) => {
										e.preventDefault();
										t.open?.();
									}}>{t.head}</a
								>{:else}<span class="named">{t.head}</span>{/if}{t.after}{/each}
					</p>
				{/if}
			</div>
		{/key}
		{/if}
	</div>
{/if}

<style>
	.home-tiles {
		width: min(var(--reading-measure, 38rem), calc(100% - 2 * var(--reading-gutter, 1.5rem)));
		display: flex;
		flex-direction: column;
		gap: 8px;
		pointer-events: auto;
	}

	/* The notes line at the left, the switch at the right, on one row. */
	.tiles-head {
		display: flex;
		align-items: center;
		gap: 16px;
		min-width: 0;
	}

	/* Quiet until you look for it: three small glyphs at the row's right end. */
	.layout-switch {
		flex: none;
		margin-left: auto;
		display: flex;
		gap: 2px;
		padding: 2px;
		border-radius: var(--radius-md);
		background: color-mix(in srgb, var(--color-foreground) 5%, transparent);
		outline: none;
	}

	.layout-switch button {
		width: 26px;
		height: 22px;
		border: 0;
		border-radius: var(--radius-sm);
		display: flex;
		align-items: center;
		justify-content: center;
		color: var(--color-foreground-subtle);
		background: transparent;
		cursor: pointer;
		transition: color var(--motion-hover-out) var(--ease-premium);
	}

	.layout-switch button:hover {
		color: var(--color-foreground);
		transition-duration: var(--motion-hover-in);
	}

	.layout-switch button.on {
		color: var(--color-foreground);
		background: var(--color-surface);
		box-shadow:
			0 0 0 1px color-mix(in srgb, var(--color-foreground) 6%, transparent),
			0 1px 2px rgb(0 0 0 / 0.08);
	}

	.layout-switch button:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: 1px;
	}

	.cards {
		display: grid;
		grid-template-columns: repeat(3, minmax(0, 1fr));
		gap: 12px;
	}

	/* Regular glass on the painting (`.lq-regular`, app.css), because a
	   card holds lines to read. Short enough that the painting's village
	   shows below the row. The height is fixed, so a line arriving after its
	   read does not move it. */
	.tile {
		height: 138px;
		box-sizing: border-box;
		padding: 13px 15px;
		border: none;
		border-radius: var(--radius-xl);
		display: flex;
		flex-direction: column;
		gap: 5px;
		overflow: hidden;
		text-align: left;
		font: inherit;
		color: var(--color-foreground);
		cursor: pointer;
		transition:
			transform 240ms cubic-bezier(0.22, 1, 0.36, 1),
			box-shadow var(--motion-hover-out) var(--ease-premium);
	}

	.tile.still {
		cursor: default;
	}

	button.tile:hover {
		transform: translateY(-2px);
	}

	button.tile:focus-visible,
	button.row:focus-visible,
	.link:focus-visible {
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

	/* One glass card of rows, in the tiles' regular glass. */
	.rows {
		border-radius: var(--radius-xl);
		overflow: hidden;
	}

	.row {
		width: 100%;
		height: 44px;
		box-sizing: border-box;
		padding: 0 14px;
		border: 0;
		display: flex;
		align-items: center;
		gap: 10px;
		text-align: left;
		font: inherit;
		color: var(--color-foreground);
		background: transparent;
		cursor: pointer;
		transition: background-color var(--motion-hover-out) var(--ease-premium);
	}

	.row + .row {
		border-top: 1px solid var(--color-border);
	}

	.row.still {
		cursor: default;
	}

	button.row:hover {
		background: var(--hover-bg);
		transition-duration: var(--motion-hover-in);
	}

	.row-icon {
		flex: none;
		display: flex;
		color: var(--color-foreground-subtle);
	}

	.row-head {
		flex: none;
		max-width: 45%;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-family: var(--font-serif);
		font-size: 17px;
	}

	.row-line {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-family: var(--font-serif);
		font-size: 15px;
		color: var(--color-foreground-muted);
		animation: line-in 240ms var(--ease-premium);
	}

	.row-kicker {
		flex: none;
		margin-left: auto;
		padding-left: 8px;
		font-size: 12px;
		color: var(--color-foreground-subtle);
	}

	.sentence {
		margin: 0;
		font-family: var(--font-serif);
		font-size: 20px;
		line-height: 1.55;
		color: var(--color-foreground-muted);
		overflow-wrap: break-word;
	}

	.link,
	.named {
		font-weight: 500;
		color: var(--color-foreground);
	}

	/* Links, so a long name wraps with the sentence. No underline: the
	   weight marks them. */
	.link {
		text-decoration: none;
		border-radius: var(--radius-xs);
		transition: color var(--motion-hover-out) var(--ease-premium);
	}

	.link:hover {
		color: var(--color-primary);
		transition-duration: var(--motion-hover-in);
	}

	@media (prefers-reduced-motion: reduce) {
		.tile,
		.line,
		.row-line,
		.row,
		.link,
		.layout-switch button {
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
