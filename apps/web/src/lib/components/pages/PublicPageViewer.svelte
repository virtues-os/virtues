<script lang="ts">
	/**
	 * PublicPageViewer - Read-only CodeMirror renderer for shared pages.
	 *
	 * Uses the same decorations and theme as the full editor — ensuring
	 * consistent rendering without Yjs, editing plugins, or WebSocket connections.
	 */
	import { onMount, onDestroy } from "svelte";
	import type { EditorView } from "@codemirror/view";
	import { createReadOnlyEditor } from "$lib/codemirror/editor";
	import "$lib/codemirror/theme.css";

	interface Props {
		markdown: string;
	}

	let { markdown }: Props = $props();

	let container: HTMLDivElement;
	let view: EditorView | undefined;

	onMount(() => {
		view = createReadOnlyEditor({
			parent: container,
			content: markdown,
		});
	});

	onDestroy(() => {
		view?.destroy();
	});
</script>

<div class="public-page-viewer" bind:this={container}></div>

<style>
	.public-page-viewer {
		/* CodeMirror theme.css handles all internal styling */
	}

	/* Remove focus outline in read-only mode */
	.public-page-viewer :global(.cm-editor) {
		outline: none;
	}

	.public-page-viewer :global(.cm-editor .cm-content) {
		cursor: default;
	}
</style>
