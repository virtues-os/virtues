<!--
	DocumentPreview: a block page's HTML drawn as the editor draws it, read
	only and bound to nothing: for a version in the page's history (the
	canonical HTML `GET /api/pages/versions/:id` returns). The same schema
	and node views as the editor, so a version looks like the page it was.
-->
<script lang="ts">
	import { onDestroy, onMount } from "svelte";
	import { Editor } from "@tiptap/core";
	import "$lib/document/document.css";
	import { contractExtensions } from "$lib/document/schema";
	import { pageNodeViews } from "$lib/document/views";
	import { codeHighlight } from "$lib/document/code";
	import { links } from "$lib/document/links";
	import { ProposalsRead } from "$lib/document/suggestions";
	import { pageDisplay } from "$lib/stores/pageDisplay.svelte";

	let { html }: { html: string } = $props();

	let host: HTMLDivElement;
	let editor: Editor | null = null;

	onMount(() => {
		editor = new Editor({
			element: host,
			editable: false,
			content: html,
			extensions: [...contractExtensions(pageNodeViews()), codeHighlight(), links(), ProposalsRead],
			editorProps: { attributes: { class: "doc-prose doc-preview" } },
		});
	});

	$effect(() => {
		const next = html;
		if (editor && !editor.isDestroyed && editor.getHTML() !== next) {
			editor.commands.setContent(next, { emitUpdate: false });
		}
	});

	onDestroy(() => {
		editor?.destroy();
		editor = null;
	});
</script>

<div
	class="document-preview"
	style:--editor-font-family={pageDisplay.fontFamily}
	style:--editor-font-size={pageDisplay.fontSize}
	style:--editor-line-height={pageDisplay.lineHeight}
	bind:this={host}
></div>

<style>
	.document-preview :global(.doc-preview) {
		min-height: 0;
		padding-bottom: 0;
	}
</style>
