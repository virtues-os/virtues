<script lang="ts">
	import Icon from '@iconify/svelte';
	// Import icon registry to ensure icons are pre-loaded
	import '$lib/icons';

	interface Props {
		icon: string;
		width?: number | string;
		height?: number | string;
		class?: string;
		style?: string;
		[key: string]: unknown;
	}

	let { icon, width, height, class: className = '', style, ...rest }: Props = $props();
</script>

<Icon {icon} {width} {height} class={className} {style} {...rest} />
