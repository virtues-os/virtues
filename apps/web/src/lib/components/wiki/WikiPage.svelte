<script lang="ts">
	import type { WikiPage as WikiPageType } from "$lib/wiki";
	import { PAGE_TYPE_META } from "$lib/wiki";
	import WikiEditor from "./WikiEditor.svelte";
	import WikiCitations from "./WikiCitations.svelte";
	import WikiLinkedPages from "./WikiLinkedPages.svelte";
	import WikiRelatedPages from "./WikiRelatedPages.svelte";
	import WikiRightRail from "./WikiRightRail.svelte";

	interface Props {
		page: WikiPageType;
	}

	let { page }: Props = $props();

	// Local state for editable markdown content
	let markdownContent = $state<string>(page.content ?? "");

	// When navigating between slugs, load the new page content into the editor.
	$effect(() => {
		markdownContent = page.content ?? "";
	});

	const typeMeta = $derived(PAGE_TYPE_META[page.type]);

	function formatDate(date: Date): string {
		return date.toLocaleDateString("en-US", {
			month: "long",
			day: "numeric",
			year: "numeric",
		});
	}

	function handleSave(newContent: string) {
		markdownContent = newContent;
	}
</script>

<div class="flex h-full w-full overflow-hidden">
	<article class="wiki-article flex-1 min-w-0 min-h-0 overflow-y-auto p-8">
		<!-- Cover image -->
		{#if page.cover}
			<div class="relative h-[200px] -m-8 -mt-8 mb-0 overflow-hidden">
				<img
					src={page.cover}
					alt=""
					class="w-full h-full object-cover"
				/>
				<div
					class="absolute bottom-0 left-0 right-0 h-20 bg-gradient-to-t from-background to-transparent"
				></div>
			</div>
		{/if}

		<!-- Main content area -->
		<div class="max-w-3xl mx-auto">
			<!-- Content column -->
			<div class="flex-1 min-w-0">
				<!-- Page header -->
				<header class="mb-6">
					<!-- Title -->
					<h1
						class="font-serif text-3xl font-normal leading-tight text-foreground m-0"
					>
						{page.title}
					</h1>

					<!-- Subtitle -->
					{#if page.subtitle}
						<p class="text-base text-foreground-muted mt-1 mb-0">
							{page.subtitle}
						</p>
					{/if}

					<!-- Divider -->
					<hr class="my-3 border-0 border-t border-border" />

					<!-- Meta line -->
					<div
						class="flex items-center gap-2 text-[0.8125rem] text-foreground-subtle"
					>
						<span class="text-foreground-muted"
							>{typeMeta.label}</span
						>
						<span class="text-foreground-subtle">·</span>
						<span class="text-foreground-subtle">
							{#if page.lastEditedBy === "ai"}
								AI-assisted
							{:else}
								Last edited
							{/if}
							{formatDate(page.updatedAt)}
						</span>
					</div>

					<!-- Tags -->
					{#if page.tags.length > 0}
						<div class="flex flex-wrap gap-1.5 mt-3">
							{#each page.tags as tag}
								<span
									class="text-[0.6875rem] px-2 py-0.5 bg-surface-elevated border border-border rounded-full text-foreground-muted"
									>#{tag}</span
								>
							{/each}
						</div>
					{/if}
				</header>

				<!-- Body content (markdown editor) -->
				<div class="py-4">
					<WikiEditor
						bind:content={markdownContent}
						linkedPages={page.linkedPages}
						onSave={handleSave}
					/>
				</div>

				<!-- Citations -->
				{#if page.citations.length > 0}
					<div class="mt-6">
						<WikiCitations citations={page.citations} />
					</div>
				{/if}

				<!-- Linked Pages -->
				{#if page.linkedPages && page.linkedPages.length > 0}
					<div class="mt-6">
						<WikiLinkedPages linkedPages={page.linkedPages} />
					</div>
				{/if}

				<!-- Related Pages -->
				{#if page.relatedPages && page.relatedPages.length > 0}
					<div class="mt-6">
						<WikiRelatedPages relatedPages={page.relatedPages} />
					</div>
				{/if}
			</div>
		</div>
	</article>

	<!-- Right Rail Panel -->
	<WikiRightRail content={markdownContent} />
</div>

<style>
	/* Scrollbar styling */
	article {
		scrollbar-width: thin;
		scrollbar-color: transparent transparent;
	}

	article:hover {
		scrollbar-color: var(--color-border) transparent;
	}

	article::-webkit-scrollbar {
		width: 6px;
	}

	article::-webkit-scrollbar-track {
		background: transparent;
	}

	article::-webkit-scrollbar-thumb {
		background-color: transparent;
		border-radius: 3px;
		transition: background-color 0.2s ease;
	}

	article:hover::-webkit-scrollbar-thumb {
		background-color: var(--color-border);
	}

	article::-webkit-scrollbar-thumb:hover {
		background-color: var(--color-border-strong);
	}

	/* Responsive */
	@media (max-width: 900px) {
		.flex.h-full {
			flex-direction: column;
		}

		article {
			height: auto;
			overflow: visible;
		}
	}
</style>
