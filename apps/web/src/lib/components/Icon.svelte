<script lang="ts">
	import Icon from '@iconify/svelte';
	// Import icon registry to ensure icons are pre-loaded
	import '$lib/icons';
	import { isEmoji } from '$lib/utils/iconHelpers';

	interface Props {
		icon: string;
		width?: number | string;
		height?: number | string;
		class?: string;
		style?: string;
		[key: string]: unknown;
	}

	let { icon, width, height, class: className = '', style, ...rest }: Props = $props();

	// A user-picked icon is an iconify name or an emoji, and both arrive in the
	// same field. Drawing the emoji here, once, is what lets a menu, a tab or a
	// grid row take whatever the user chose; handed to iconify, an emoji drew
	// nothing and left a gap where the icon should be.
	const emoji = $derived(!!icon && isEmoji(icon));
	const box = $derived(Number(width ?? height ?? 16) || 16);
</script>

{#if emoji}
	<span
		class="v-icon-emoji {className}"
		style="width: {box}px; height: {box}px; font-size: {Math.round(box * 0.85)}px;{style ? ` ${style}` : ''}"
		aria-hidden="true">{icon}</span
	>
{:else}
	<Icon {icon} {width} {height} class={className} {style} {...rest} />
{/if}

<style>
	.v-icon-emoji {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
		line-height: 1;
	}
</style>
