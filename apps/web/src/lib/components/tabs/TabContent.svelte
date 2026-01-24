<script lang="ts">
	import type { Tab } from "$lib/tabs/types";
	import { tabRegistry, getDetailComponent } from "$lib/tabs/registry";
	import type { Component } from "svelte";

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	// Get the component to render from the registry
	// Handles detail variants for types that have them (wiki, data-sources)
	const ViewComponent = $derived.by((): Component => {
		const def = tabRegistry[tab.type];
		if (!def) {
			// Fallback to chat if type not found
			return tabRegistry.chat.component;
		}

		// Check if this type has a detail variant and if we should use it
		if (def.hasDetail) {
			// For wiki: use detail view if slug is present
			if (tab.type === "wiki" && "slug" in tab && tab.slug) {
				const detailComponent = getDetailComponent("wiki");
				if (detailComponent) return detailComponent;
			}
			// For data-sources: use detail view if sourceId is present
			if (tab.type === "data-sources" && "sourceId" in tab && tab.sourceId) {
				const detailComponent = getDetailComponent("data-sources");
				if (detailComponent) return detailComponent;
			}
		}

		return def.component;
	});
</script>

<div class="tab-content" class:active style:display={active ? "flex" : "none"}>
	{#if ViewComponent}
		<ViewComponent {tab} {active} />
	{:else}
		<!-- Placeholder for unknown tab types -->
		<div class="placeholder">
			<iconify-icon icon="ri:file-line"></iconify-icon>
			<span class="title">Unknown View</span>
			<span class="subtitle">Tab type: {tab.type}</span>
		</div>
	{/if}
</div>

<style>
	.tab-content {
		position: absolute;
		inset: 0;
		flex-direction: column;
		overflow: hidden;
	}

	.placeholder {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 8px;
		height: 100%;
		color: var(--color-foreground-muted);
	}

	.placeholder iconify-icon {
		font-size: 48px;
		opacity: 0.4;
		margin-bottom: 8px;
	}

	.placeholder .title {
		font-size: 18px;
		font-weight: 500;
		color: var(--color-foreground);
	}

	.placeholder .subtitle {
		font-size: 14px;
		opacity: 0.7;
	}

</style>
