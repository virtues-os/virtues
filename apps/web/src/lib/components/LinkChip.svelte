<script lang="ts">
	// External-link citation. Renders in the same register as an internal `Ref`
	// (`.ref-link`, prose-sized, underlined) rather than as a filled `.ref-pill`
	// capsule — see the header of ref-badge.css: the pill belongs to EDITABLE
	// surfaces (the chat composer, CodeMirror), and rendered output uses the
	// link treatment. This was the only rendered-output consumer still wearing
	// the editor's costume, which is why a web citation shouted in a paragraph
	// where a citation of the owner's own data whispered.
	//
	// The favicon is gone with it. It cost a request to google.com per citation
	// on a box whose whole premise is that nothing leaves, and fetching it is
	// what forced `display: inline-flex` — a flex box on a text baseline, which
	// is why the chip never sat straight in a line of prose.
	import Icon from "$lib/components/Icon.svelte";
	import RefPreview from "$lib/components/RefPreview.svelte";
	import { createRefHover } from "$lib/utils/refHover.svelte";

	let { href, label, variant = "link" } = $props<{
		href: string;
		label: string;
		variant?: "link" | "quiet";
	}>();

	const hover = createRefHover();

	function open() {
		window.open(href, "_blank", "noopener,noreferrer");
	}

	function handleClick(e: MouseEvent) {
		// Plain click opens, matching `Ref`. These used to disagree: clicking an
		// internal citation took you to the record, while clicking an external
		// one refused to navigate and only peeked, so the same gesture meant two
		// things in one paragraph. Hover already peeks for both.
		if (e.metaKey || e.ctrlKey) return; // let the anchor open a background tab
		e.preventDefault();
		open();
	}

	function domainOf(url: string): string {
		try {
			return new URL(url).hostname.replace(/^www\./, "");
		} catch {
			return "";
		}
	}

	const domain = $derived(domainOf(href));
</script>

<a
	class="ref-link {variant === 'quiet' ? 'ref-link--quiet' : ''}"
	{href}
	target="_blank"
	rel="noopener noreferrer"
	title={domain ? `${label} — ${domain}` : href}
	onclick={handleClick}
	onmouseenter={(e) => hover.enter(e.currentTarget)}
	onmouseleave={() => hover.leave()}
	onfocus={(e) => hover.enter(e.currentTarget)}
	onblur={() => hover.leave()}
	>{#if variant !== "quiet"}<Icon
			icon="ri:global-line"
			width="11"
			class="ref-pill-icon"
		/>{/if}{label}</a
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
