<script lang="ts" module>
	/** The glyph of what a pin opens, for a pin with no mark of its own. */
	export function pinKindGlyph(url: string): string {
		if (/^https?:\/\//i.test(url)) return 'wiki';
		if (url.startsWith('/chat/')) return 'chats';
		if (url.startsWith('/project/') || url.startsWith('/notebook/')) return 'projects';
		if (url.startsWith('/applet')) return 'applets';
		if (url.startsWith('/tasks')) return 'tasks';
		if (url.startsWith('/wiki')) return 'wiki';
		if (url.startsWith('/day/')) return 'day';
		return 'pages';
	}
</script>

<script lang="ts">
	/**
	 * A pin's glyph, shared by the sidebar's Pinned rows and the rail: the
	 * mark the user picked, else the glyph of what it opens, in the pin's
	 * color. A pinned chat that is replying shows the thinking mark. A solid
	 * dot read as a status light: red and amber said error and warning about
	 * a page that had neither.
	 */
	import type { Pin } from '$lib/api/client';
	import { pinIdentity } from '$lib/refs/identity.svelte';
	import { clothFor } from '$lib/sidebar/pin-colors';
	import { isEmoji } from '$lib/utils/iconHelpers';
	import { chatActivity } from '$lib/stores/chatActivity.svelte';
	import ThinkingMark from '$lib/components/ThinkingMark.svelte';
	import Icon from '$lib/components/Icon.svelte';
	import AtlasIcon, { SHELL_GLYPH_PX } from './AtlasIcon.svelte';

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
		<span class="kind" style="color: {color}">
			{#if look.icon.startsWith('atlas:')}
				<AtlasIcon name={look.icon.slice('atlas:'.length)} size={SHELL_GLYPH_PX} bare fillable />
			{:else}
				<Icon icon={look.icon} width={SHELL_GLYPH_PX} />
			{/if}
		</span>
	{:else}
		<span class="kind" style="color: {color}">
			<AtlasIcon name={pinKindGlyph(pin.url)} size={SHELL_GLYPH_PX} bare fillable />
		</span>
	{/if}
</span>

<style>
	.pin-glyph {
		width: 16px;
		height: 16px;
		flex: none;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.emoji {
		font-size: 14px;
		line-height: 1;
	}

	.kind {
		display: flex;
	}
</style>
