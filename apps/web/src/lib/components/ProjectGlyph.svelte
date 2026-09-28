<script lang="ts">
	/**
	 * A project's mark, the same on every surface that shows one: the emoji or
	 * icon the user picked, else the Atlas closed book, in the project's color
	 * when it has one.
	 *
	 * Each surface used to draw this itself, and each got a different part
	 * wrong: the color showed in the sidebar but not on the project's own page,
	 * and the default was a folder in one place and a notebook in another.
	 */
	import Icon from '$lib/components/Icon.svelte';
	import { accentCss } from '$lib/sidebar/pin-colors';
	import { PROJECT_ICON } from '$lib/utils/iconHelpers';

	interface Props {
		icon?: string | null;
		color?: string | null;
		size?: number;
	}

	let { icon, color, size = 15 }: Props = $props();

	const tint = $derived(accentCss(color));
</script>

<Icon
	icon={icon || PROJECT_ICON}
	width={size}
	height={size}
	style={tint ? `color: ${tint}` : undefined}
	aria-hidden="true"
/>
