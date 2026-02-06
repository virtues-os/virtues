<script lang="ts">
	/**
	 * SidebarTooltip
	 *
	 * A tooltip specifically styled for the sidebar, always positioned to the right.
	 * Uses the new Tooltip primitive from $lib/floating.
	 */
	import type { Snippet } from 'svelte';
	import { Tooltip } from '$lib/floating';

	interface Props {
		content: string;
		delay?: number;
		disabled?: boolean;
		children: Snippet;
	}

	let { content, delay = 300, disabled = false, children }: Props = $props();
</script>

<Tooltip {content} {delay} {disabled} placement="right">
	{@render children()}
</Tooltip>
