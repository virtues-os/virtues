<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import { type Tab, routeToEntityId } from "$lib/tabs/types";
	import { tabRegistry, getComponent, getVirtuesComponent, getSourceComponent } from "$lib/tabs/registry";
	import type { Component } from "svelte";

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	// Get the component to render from the registry
	// Handles detail variants for entity namespaces
	const ViewComponent = $derived.by((): Component => {
		const def = tabRegistry[tab.type];
		if (!def) {
			// Fallback to session if type not found
			return tabRegistry.chat.component;
		}

		// Handle virtues namespace specially - dispatch to correct component
		if (tab.type === 'virtues' && tab.virtuesPage) {
			return getVirtuesComponent(tab.virtuesPage);
		}

		// Handle source namespace specially
		if (tab.type === 'source') {
			return getSourceComponent(!!routeToEntityId(tab.route));
		}

		// For all other types, use getComponent which handles list vs detail view
		// Derive hasEntityId from route (e.g., '/person/person_abc' has entity, '/wiki' does not)
		// Special case: "/" is a new chat (entityId: 'new' in registry), treat as having entity
		const hasEntityId = tab.route === '/' || !!routeToEntityId(tab.route);
		return getComponent(tab.type, hasEntityId);
	});
</script>

<div class="tab-content" class:active style:display={active ? "flex" : "none"}>
	{#if ViewComponent}
		<ViewComponent {tab} {active} />
	{:else}
		<!-- Placeholder for unknown tab types -->
		<div class="placeholder">
			<Icon icon="ri:file-line" />
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

	.placeholder :global(svg) {
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
