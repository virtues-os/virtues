<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import RefPreview from "$lib/components/RefPreview.svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { contextMenu } from "$lib/stores/contextMenu.svelte";
	import { openMenuItems } from "$lib/utils/openMenuItems";
	import { refIcon, getEntityTypeFromRoute } from "$lib/utils/refRoutes";
	import { createRefHover } from "$lib/utils/refHover.svelte";
	import { identityOf, ownedRef } from "$lib/refs/identity.svelte";

	let { displayName, url, entityType, mimeType, variant = "link" } = $props<{
		displayName: string;
		url: string;
		entityType?: string;
		mimeType?: string;
		// "link" (default): accent name + leading type icon + `@` — for chat answers,
		// previews. "quiet": bare name with a dotted underline, inheriting the prose
		// color — for entities woven into flowing text (the day biography). See
		// ref-badge.css (.ref-link--quiet) and the link-when-reading refs doctrine.
		variant?: "link" | "quiet";
	}>();

	// Type drives the leading icon; derive from the url if not passed explicitly.
	const resolvedType = $derived(entityType ?? getEntityTypeFromRoute(url));

	// A chat, page or project is named by its title now, so a pill written
	// before a rename reads the new name (refs/identity). A person or place
	// keeps the words it was written with: "Nick" in a sentence is the
	// writer's phrasing, and swapping in the record's full name would rewrite
	// their prose. The written text is also the fallback for anything gone.
	const shown = $derived(ownedRef(url) ? identityOf(url, { title: displayName }).title : displayName);

	// A ref opens like any link: here, in the tab you are on (Back returns),
	// or in the tab already showing it. ⌘-click (Ctrl elsewhere) opens a new
	// tab; right-click offers a new tab or the pane beside.
	function open() {
		windowShellStore.openTabFromRoute(url, { label: shown, focusExisting: true });
	}

	function handleClick(e: MouseEvent) {
		e.preventDefault();
		e.stopPropagation();
		if (e.metaKey || e.ctrlKey) windowShellStore.openInNewTab(url, shown);
		else open();
	}

	function handleContextMenu(e: MouseEvent) {
		e.preventDefault();
		e.stopPropagation();
		contextMenu.show({ x: e.clientX, y: e.clientY }, openMenuItems(url, shown));
	}

	const hover = createRefHover();
</script>

<button
	class="ref-link {variant === 'quiet' ? 'ref-link--quiet' : ''}"
	onclick={handleClick}
	oncontextmenu={handleContextMenu}
	title="View {shown}"
	onmouseenter={(e) => hover.enter(e.currentTarget)}
	onmouseleave={() => hover.leave()}
	onfocus={(e) => hover.enter(e.currentTarget)}
	onblur={() => hover.leave()}
	>{#if variant !== "quiet"}<Icon
			icon={refIcon(resolvedType, { mimeType, filename: displayName })}
			width="11"
			class="ref-pill-icon"
		/>@{/if}{shown}</button
><!-- No whitespace before the preview block: it would render as a space
     after the name, before the punctuation that follows it. -->{#if hover.visible && hover.anchor}
	<RefPreview
		anchor={hover.anchor}
		type={resolvedType}
		label={shown}
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

