<script lang="ts">
	import "iconify-icon";

	interface Props {
		title: string;
		count?: number;
		defaultOpen?: boolean;
	}

	let { title, count, defaultOpen = true }: Props = $props();

	let isOpen = $state(defaultOpen);
	let contentEl: HTMLDivElement | undefined = $state();
	let contentHeight = $state(0);

	function toggle() {
		if (contentEl) {
			contentHeight = contentEl.scrollHeight;
		}
		isOpen = !isOpen;
	}

	$effect(() => {
		if (isOpen && contentEl) {
			contentHeight = contentEl.scrollHeight;
		}
	});
</script>

<section>
	<button
		class="flex items-center gap-1.5 w-full p-0 bg-transparent border-0 cursor-pointer text-left group"
		onclick={toggle}
		aria-expanded={isOpen}
	>
		<h4 class="font-serif text-[0.9375rem] font-normal text-foreground m-0">
			{title}
			{#if count !== undefined}
				<span class="text-foreground-subtle font-normal">· {count}</span
				>
			{/if}
		</h4>
		<iconify-icon
			icon={isOpen ? "ri:arrow-up-s-line" : "ri:arrow-down-s-line"}
			width="16"
			height="16"
			class="text-foreground-subtle opacity-50 transition-opacity duration-150 group-hover:opacity-100"
		></iconify-icon>
	</button>

	<div
		class="section-content-wrapper"
		class:open={isOpen}
		style="--content-height: {contentHeight}px"
	>
		<div class="pt-2" bind:this={contentEl}>
			<slot />
		</div>
	</div>
</section>

<style>
	.section-content-wrapper {
		overflow: hidden;
		height: 0;
		opacity: 0;
		transition:
			height 0.2s ease,
			opacity 0.2s ease;
	}

	.section-content-wrapper.open {
		height: var(--content-height);
		opacity: 1;
	}
</style>
