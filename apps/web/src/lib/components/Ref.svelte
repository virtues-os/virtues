<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import RefPreview from "$lib/components/RefPreview.svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { getEntityRoute, refIcon, getEntityTypeFromRoute } from "$lib/utils/refRoutes";
	import { createRefHover } from "$lib/utils/refHover.svelte";

	let { displayName, entityId, url, entityType, mimeType } = $props<{
		displayName: string;
		entityId?: string;
		url?: string;
		entityType?: string;
		mimeType?: string;
	}>();

	// Get the navigation URL
	function getNavigationUrl(): string {
		if (url) return url;
		if (entityId) return getEntityRoute(entityId);
		return "#";
	}

	// Type drives the leading icon; derive from the url if not passed explicitly.
	const resolvedType = $derived(entityType ?? (url ? getEntityTypeFromRoute(url) : null));

	function open() {
		// Open beside — in the pane next to the one you're in (splits if needed),
		// so you keep your place. See the Phase 5 click model.
		windowShellStore.openRouteBeside(getNavigationUrl(), displayName);
	}

	function handleClick(e: MouseEvent) {
		e.preventDefault();
		e.stopPropagation();
		// Click model: ⌘/Ctrl-click opens; plain click peeks (never yanks you away).
		if (e.metaKey || e.ctrlKey) {
			open();
		} else {
			hover.pin(e.currentTarget as HTMLElement);
		}
	}

	const hover = createRefHover();
</script>

<button
	class="ref-pill"
	onclick={handleClick}
	title="View {displayName}"
	onmouseenter={(e) => hover.enter(e.currentTarget)}
	onmouseleave={() => hover.leave()}
	onfocus={(e) => hover.enter(e.currentTarget)}
	onblur={() => hover.leave()}
	><Icon icon={refIcon(resolvedType, { mimeType, filename: displayName })} width="11" class="ref-pill-icon" />@{displayName}</button
>
{#if hover.visible && hover.anchor}
	<RefPreview
		anchor={hover.anchor}
		type={resolvedType}
		label={displayName}
		url={getNavigationUrl()}
		{mimeType}
		onOpen={open}
		onTurnInto={(d) => d === "full" && open()}
		oncardenter={() => hover.cancelHide()}
		oncardleave={() => hover.leave()}
	/>
{/if}

<style>
	.ref-pill :global(.ref-pill-icon) {
		display: inline-block;
		vertical-align: -0.13em;
		margin-right: 1px;
		opacity: 0.85;
	}
</style>
