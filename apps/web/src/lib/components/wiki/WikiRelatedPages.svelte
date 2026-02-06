<script lang="ts">
	import type { RelatedPage } from "$lib/wiki";
	import WikiCollapsibleSection from "./WikiCollapsibleSection.svelte";

	interface Props {
		relatedPages: RelatedPage[];
	}

	let { relatedPages }: Props = $props();
</script>

<div>
	<WikiCollapsibleSection
		title="Related Pages"
		count={relatedPages.length}
		defaultOpen={true}
	>
		<ul class="list-none m-0 p-0">
			{#each relatedPages as page}
				<li>
					<a href="/wiki/{page.id}" class="wiki-link">
						<span class="link-text">{page.title}</span>
					</a>
				</li>
			{/each}
		</ul>
	</WikiCollapsibleSection>
</div>

<style>
	.wiki-link {
		display: block;
		padding: 0.375rem 0;
		color: var(--color-primary);
		text-decoration: none;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.link-text {
		display: inline;
		position: relative;
		background-image: linear-gradient(
			to top,
			color-mix(in srgb, var(--color-primary) 15%, transparent),
			color-mix(in srgb, var(--color-primary) 15%, transparent)
		);
		background-repeat: no-repeat;
		background-size: 100% 0%;
		background-position: 0 100%;
		transition: background-size 0.2s ease;
	}

	.wiki-link:hover .link-text {
		background-size: 100% 100%;
	}
</style>
