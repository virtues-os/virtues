<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import RefPreview from "$lib/components/RefPreview.svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { refIcon, getEntityTypeFromRoute } from "$lib/utils/refRoutes";
	import { createRefHover } from "$lib/utils/refHover.svelte";

	let { displayName, url, entityType, mimeType, variant = "link" } = $props<{
		displayName: string;
		url: string;
		entityType?: string;
		mimeType?: string;
		// "link" (default): accent name + leading type icon + `@` — for chat answers,
		// previews. "quiet": bare name with a dotted underline, inheriting the prose
		// colour — for entities woven into flowing text (the day biography). See
		// ref-badge.css (.ref-link--quiet) and the link-when-reading refs doctrine.
		variant?: "link" | "quiet";
	}>();

	// Type drives the leading icon; derive from the url if not passed explicitly.
	const resolvedType = $derived(entityType ?? getEntityTypeFromRoute(url));

	function open() {
		// Open beside — in the pane next to the one you're in (splits if needed),
		// so you keep your place. See the Phase 5 click model.
		windowShellStore.openRouteBeside(url, displayName);
	}

	function handleClick(e: MouseEvent) {
		e.preventDefault();
		e.stopPropagation();
		// Rendered refs/citations: plain click opens the source beside you (hover
		// already peeks, so a click means "take me there"). This is the flip of
		// the editor's peek-on-click model — see the Phase C citation decision.
		open();
	}

	const hover = createRefHover();
</script>

<button
	class="ref-link {variant === 'quiet' ? 'ref-link--quiet' : ''}"
	onclick={handleClick}
	title="View {displayName}"
	onmouseenter={(e) => hover.enter(e.currentTarget)}
	onmouseleave={() => hover.leave()}
	onfocus={(e) => hover.enter(e.currentTarget)}
	onblur={() => hover.leave()}
	>{#if variant !== "quiet"}<Icon
			icon={refIcon(resolvedType, { mimeType, filename: displayName })}
			width="11"
			class="ref-pill-icon"
		/>@{/if}{displayName}</button
>
{#if hover.visible && hover.anchor}
	<RefPreview
		anchor={hover.anchor}
		type={resolvedType}
		label={displayName}
		{url}
		{mimeType}
		onOpen={open}
		onTurnInto={(d) => d === "full" && open()}
		oncardenter={() => hover.cancelHide()}
		oncardleave={() => hover.leave()}
	/>
{/if}

<!-- Appearance lives in the shared ref-badge.css (.ref-link) so the pill/link
     treatments stay a single source of truth. -->

