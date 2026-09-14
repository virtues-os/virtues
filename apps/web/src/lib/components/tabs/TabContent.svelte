<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import { type Tab, routeToEntityId } from "$lib/tabs/types";
	import { tabRegistry, getComponent, getVirtuesComponent } from "$lib/tabs/registry";
	import { loadView, peekView, type View, type ViewLoader } from "$lib/tabs/lazy";

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	// Which view this tab wants, as a loader (see $lib/tabs/lazy.ts). Handles
	// detail variants for entity namespaces.
	const loader = $derived.by((): ViewLoader => {
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

	// The resolved view. Eager views (chat, home) and any chunk that has
	// already arrived resolve synchronously, so a tab whose view is loaded
	// never renders an empty frame. A first-time view shows nothing for the
	// few milliseconds its chunk takes — a spinner for that would flash.
	let ViewComponent = $state<View | null>(null);
	$effect(() => {
		const wanted = loader;
		const ready = peekView(wanted);
		if (ready) {
			ViewComponent = ready;
			return;
		}
		ViewComponent = null;
		let cancelled = false;
		loadView(wanted).then((view) => {
			if (!cancelled) ViewComponent = view;
		});
		return () => {
			cancelled = true;
		};
	});
</script>

<div class="tab-content" class:active style:display={active ? "flex" : "none"}>
	{#if ViewComponent}
		<ViewComponent {tab} {active} />
	{:else if !tabRegistry[tab.type]}
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
