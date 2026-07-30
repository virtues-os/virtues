<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import { type Tab, routeToEntityId } from "$lib/tabs/types";
	import { tabRegistry, getComponent, getVirtuesComponent } from "$lib/tabs/registry";
	import { loadDetail } from "$lib/applet-views";
	import { getApplet } from "$lib/api/client";
	import type { Component } from "svelte";

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	// View-runtime override: if the tab represents an action whose manifest
	// declares config.view.name AND a matching Detail.svelte exists in
	// `apps/web/src/lib/applets/<name>/`, render that instead of
	// AppletDetailView. Lazy-loaded so non-action tabs pay no cost.
	// While the lookup is in flight, the generic detail renders; the override
	// swaps in once the action fetch resolves.
	let actionViewComponent = $state<Component | null>(null);

	$effect(() => {
		actionViewComponent = null;

		if (tab.type !== 'applet') return;
		const id = routeToEntityId(tab.route);
		if (!id) return;

		void getApplet(id)
			.then((action) => {
				if (action.runtime !== 'view') return;
				const cfg = (action.config ?? {}) as { view?: { name?: string } };
				const name = cfg?.view?.name;
				if (!name) return;
				actionViewComponent = loadDetail(name);
			})
			.catch(() => {
				/* fall back to generic detail */
			});
	});

	// Get the component to render from the registry.
	// Handles detail variants for entity namespaces.
	const ViewComponent = $derived.by((): Component => {
		// view-runtime override wins when present
		if (actionViewComponent) return actionViewComponent;

		const def = tabRegistry[tab.type];
		if (!def) {
			// Fallback to session if type not found
			return tabRegistry.chat.component;
		}

		// Handle virtues namespace specially - dispatch to correct component
		if (tab.type === 'virtues' && tab.virtuesPage) {
			return getVirtuesComponent(tab.virtuesPage);
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
