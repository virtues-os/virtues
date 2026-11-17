<script lang="ts" module>
	/** The glyph of what a pin opens, for a pin with no mark of its own. */
	export function pinKindGlyph(url: string): string {
		if (/^https?:\/\//i.test(url)) return 'wiki';
		if (url.startsWith('/chat/')) return 'chats';
		if (url.startsWith('/thread/')) return 'projects';
		if (url.startsWith('/applet')) return 'applets';
		// A task, linked by `?task=`. A pin made before Tasks joined the
		// Threads door still says /tasks, and opens the door's Tasks view.
		if (url.startsWith('/tasks') || url.startsWith('/threads?view=tasks')) return 'tasks';
		if (url.startsWith('/threads')) return 'projects';
		if (url.startsWith('/wiki')) return 'wiki';
		if (url.startsWith('/day/')) return 'day';
		return 'pages';
	}
</script>

<script lang="ts">
	/**
	 * A pin's glyph, shared by the sidebar's Pinned rows and the rail: a
	 * tile tinted with the pin's color, holding the mark the user picked,
	 * else the glyph of what it opens, drawn in that color. The tile gives
	 * the color enough area to show, and it tells a pin from a door that
	 * draws the same glyph. It is a tint, not a solid, so four pins in four
	 * colors stay quieter than the doors' ink. An emoji stands on its own. A pinned chat that is replying shows the
	 * thinking mark. A solid dot read as a status light: red and amber said
	 * error and warning about a page that had neither.
	 */
	import type { Pin } from '$lib/api/client';
	import { pinIdentity } from '$lib/refs/identity.svelte';
	import { clothFor } from '$lib/sidebar/pin-colors';
	import { isEmoji } from '$lib/utils/iconHelpers';
	import { chatActivity } from '$lib/stores/chatActivity.svelte';
	import ThinkingMark from '$lib/components/ThinkingMark.svelte';
	import Icon from '$lib/components/Icon.svelte';
	import AtlasIcon from './AtlasIcon.svelte';

	/** The glyph inside the tile. */
	const MARK_PX = 12;
	/** 1.5px of ink at 12px: a colored line reads thinner than ink. */
	const MARK_STROKE = (1.5 * 24) / MARK_PX;

	let { pin }: { pin: Pin } = $props();

	const look = $derived(pinIdentity(pin));
	const chatId = $derived(pin.url.startsWith('/chat/') ? pin.url.slice('/chat/'.length) : null);
	const color = $derived(clothFor({ url: pin.url, color: look.color }));
</script>

<span class="pin-glyph" aria-hidden="true">
	{#if chatId && chatActivity.running(chatId)}
		<ThinkingMark depth={3} size={14} />
	{:else if look.icon && isEmoji(look.icon)}
		<span class="emoji">{look.icon}</span>
	{:else if look.icon}
		<!-- An Atlas pick goes through AtlasIcon so it keeps the nav glyphs'
		     1.25px line; through iconify at a smaller box it thinned. -->
		<span class="tile" style="--pin: {color}">
			{#if look.icon.startsWith('atlas:')}
				<AtlasIcon name={look.icon.slice('atlas:'.length)} size={MARK_PX} stroke={MARK_STROKE} bare />
			{:else}
				<Icon icon={look.icon} width={MARK_PX} />
			{/if}
		</span>
	{:else}
		<span class="tile" style="--pin: {color}">
			<AtlasIcon name={pinKindGlyph(pin.url)} size={MARK_PX} stroke={MARK_STROKE} bare />
		</span>
	{/if}
</span>

<style>
	/* 18px, a pixel past the row's 16px slot on each side, so the tile's
	   center stays on the icons' column (design.md, "One left edge"). */
	.pin-glyph {
		width: 18px;
		height: 18px;
		margin: 0 -1px;
		flex: none;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.emoji {
		font-size: 15px;
		line-height: 1;
	}

	/* The pin's color at a sixth behind its glyph, and a little more under
	   the pointer and on the pin you are on, as a door's glyph fills in. */
	.tile {
		width: 18px;
		height: 18px;
		border-radius: 5px;
		display: flex;
		align-items: center;
		justify-content: center;
		color: var(--pin);
		background: color-mix(in srgb, var(--pin) 16%, transparent);
		transition: background-color var(--motion-hover-out) var(--ease-premium);
	}

	:global(.panel-row:is(:hover, .active)) .tile {
		background: color-mix(in srgb, var(--pin) 26%, transparent);
		transition-duration: var(--motion-hover-in);
	}
</style>
