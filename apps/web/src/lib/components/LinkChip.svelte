<script lang="ts">
	// External-link reference pill: same .ref-pill shape as entity/file chips,
	// with the site favicon as the leading glyph (globe fallback while it loads).
	import Icon from "$lib/components/Icon.svelte";
	import RefPreview from "$lib/components/RefPreview.svelte";
	import { createRefHover } from "$lib/utils/refHover.svelte";

	let { href, label } = $props<{ href: string; label: string }>();

	const hover = createRefHover();

	function open() {
		window.open(href, "_blank", "noopener,noreferrer");
	}

	function handleClick(e: MouseEvent) {
		// ⌘/Ctrl-click → open the link; plain click → peek (don't navigate away).
		if (e.metaKey || e.ctrlKey) return; // let the anchor's default open a new tab
		e.preventDefault();
		hover.pin(e.currentTarget as HTMLElement);
	}

	function domainOf(url: string): string {
		try {
			return new URL(url).hostname;
		} catch {
			return "";
		}
	}

	const domain = $derived(domainOf(href));
	const faviconUrl = $derived(
		domain ? `https://www.google.com/s2/favicons?domain=${domain}&sz=16` : "",
	);
	let faviconOk = $state(true);
</script>

<a
	class="ref-pill link-chip"
	{href}
	target="_blank"
	rel="noopener noreferrer"
	title={href}
	onclick={handleClick}
	onmouseenter={(e) => hover.enter(e.currentTarget)}
	onmouseleave={() => hover.leave()}
	onfocus={(e) => hover.enter(e.currentTarget)}
	onblur={() => hover.leave()}
>
	{#if faviconUrl && faviconOk}
		<img
			class="link-chip-favicon"
			src={faviconUrl}
			alt=""
			width="12"
			height="12"
			loading="lazy"
			referrerpolicy="no-referrer"
			onerror={() => (faviconOk = false)}
		/>
	{:else}
		<Icon icon="ri:global-line" width="11" class="ref-pill-icon" />
	{/if}{label}</a
>
{#if hover.visible && hover.anchor}
	<RefPreview
		anchor={hover.anchor}
		type="link"
		{label}
		url={href}
		onOpen={open}
		oncardenter={() => hover.cancelHide()}
		oncardleave={() => hover.leave()}
	/>
{/if}

<style>
	.link-chip {
		display: inline-flex;
		align-items: center;
		gap: 3px;
	}
	.link-chip-favicon {
		display: block;
		width: 12px;
		height: 12px;
		border-radius: 2px;
		flex-shrink: 0;
	}
</style>
