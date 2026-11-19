<script lang="ts">
	/**
	 * A thread's mark, the same on every surface that shows one: the emoji or
	 * icon the user picked, else the Threads glyph, in the color the user
	 * picked, else in the text color around it.
	 *
	 * `inherit` leaves the color to a host that already paints it, such as a
	 * tinted chip whose background and glyph are mixed from the same color.
	 */
	import Icon from '$lib/components/Icon.svelte';
	import { accentCss } from '$lib/sidebar/pin-colors';
	import { THREAD_ICON } from '$lib/utils/iconHelpers';

	interface Props {
		project: { id: string; icon?: string | null; accent_color?: string | null };
		size?: number;
		inherit?: boolean;
	}

	let { project, size = 15, inherit = false }: Props = $props();

	const tint = $derived(inherit ? null : accentCss(project.accent_color));
</script>

<Icon
	icon={project.icon || THREAD_ICON}
	width={size}
	height={size}
	style={tint ? `color: ${tint}` : undefined}
	aria-hidden="true"
/>
