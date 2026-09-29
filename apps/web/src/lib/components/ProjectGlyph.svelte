<script lang="ts">
	/**
	 * A project's mark, the same on every surface that shows one: the emoji or
	 * icon the user picked, else the Atlas closed book, in the project's color
	 * (`projectColor`: the one it was given, else its automatic one).
	 *
	 * `inherit` leaves the color to a host that already paints it, such as a
	 * tinted chip whose background and glyph are mixed from the same color.
	 */
	import Icon from '$lib/components/Icon.svelte';
	import { projectColor } from '$lib/sidebar/pin-colors';
	import { PROJECT_ICON } from '$lib/utils/iconHelpers';

	interface Props {
		project: { id: string; icon?: string | null; accent_color?: string | null };
		size?: number;
		inherit?: boolean;
	}

	let { project, size = 15, inherit = false }: Props = $props();

	const tint = $derived(inherit ? null : projectColor(project));
</script>

<Icon
	icon={project.icon || PROJECT_ICON}
	width={size}
	height={size}
	style={tint ? `color: ${tint}` : undefined}
	aria-hidden="true"
/>
