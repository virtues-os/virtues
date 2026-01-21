<script lang="ts">
	import type { Citation } from "$lib/wiki";
	import WikiCollapsibleSection from "./WikiCollapsibleSection.svelte";

	interface Props {
		citations: Citation[];
	}

	let { citations }: Props = $props();
</script>

<div>
	<WikiCollapsibleSection
		title="Data Sources"
		count={citations.length}
		defaultOpen={true}
	>
		<ul class="list-none m-0 p-0">
			{#each citations as citation}
				<li
					class="flex items-baseline gap-2 py-1.5"
					id="citation-{citation.index}"
				>
					<span
						class="text-[0.8125rem] font-medium text-primary flex-shrink-0"
						>[{citation.index}]</span
					>
					<span class="text-foreground font-mono text-[0.875rem]"
						>{citation.label}</span
					>
				</li>
			{/each}
		</ul>
	</WikiCollapsibleSection>
</div>
