<!--
	CalloutNode: the chrome of a callout block, beside its content. The tone's
	icon is its picker (an IconButton, a thumb's 44pt on touch): Note, Tip or
	Warning, written as `tone`. On a page that is read only it is the icon
	alone. The content is the editor's, drawn beside this.
-->
<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import IconButton from "$lib/components/IconButton.svelte";
	import { contextMenu } from "$lib/stores/contextMenu.svelte";
	import type { ViewState } from "$lib/document/nodeviews.svelte";

	let { view }: { view: ViewState } = $props();

	const TONES = [
		{ tone: "note", label: "Note", icon: "ri:information-line" },
		{ tone: "tip", label: "Tip", icon: "ri:lightbulb-line" },
		{ tone: "warning", label: "Warning", icon: "ri:alert-line" },
	] as const;

	const current = $derived(TONES.find((t) => t.tone === view.attrs.tone) ?? TONES[0]);

	function pick(e: MouseEvent) {
		if (!view.editable) return;
		const box = (e.currentTarget as HTMLElement).getBoundingClientRect();
		contextMenu.show(
			{ x: box.left, y: box.bottom },
			TONES.map((t) => ({
				id: `tone-${t.tone}`,
				label: t.label,
				icon: t.icon,
				checked: t.tone === current.tone,
				role: "menuitemradio" as const,
				action: () => view.setAttrs({ tone: t.tone }),
			})),
		);
	}
</script>

<!-- The page keeps its focus, so the tone's menu acts on this block. -->
<span class="doc-callout-tone" role="presentation" onmousedown={(e) => e.preventDefault()}>
	{#if view.editable}
		<IconButton icon={current.icon} label={`${current.label}: change the kind`} size="sm" haspopup="menu" onclick={pick} />
	{:else}
		<Icon icon={current.icon} width="16" aria-label={current.label} />
	{/if}
</span>

<style>
	.doc-callout-tone {
		display: flex;
		align-items: center;
		justify-content: center;
		color: inherit;
	}

	/* The tone's own colour, which the chrome around it sets. */
	.doc-callout-tone :global(.v-iconbtn) {
		color: inherit;
	}
</style>
