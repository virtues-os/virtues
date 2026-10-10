<!--
	MarkdownView: a block page as markdown, read only. The page's text is a
	tree; this is its export, the same text search, publishing and Copy
	markdown read (`GET /api/pages/:id/markdown`, live from the server's copy).
	It follows the page: half a second after the last change to `ydoc`, it
	reads the export again.
-->
<script lang="ts">
	import { onDestroy, onMount } from "svelte";
	import type * as Y from "yjs";
	import Button from "$lib/components/Button.svelte";
	import { getPageMarkdown } from "$lib/api/client";

	interface Props {
		pageId: string;
		/** The page's document, to follow its changes; without one the view reads once. */
		ydoc?: Y.Doc | null;
	}

	let { pageId, ydoc = null }: Props = $props();

	const REFRESH_MS = 500;

	let markdown = $state<string | null>(null);
	let failed = $state(false);
	let copied = $state(false);
	let refreshTimer: ReturnType<typeof setTimeout> | null = null;
	let copiedTimer: ReturnType<typeof setTimeout> | null = null;
	/** Bumped per read, so an older answer arriving late does not replace a newer one. */
	let reading = 0;

	async function read() {
		const ticket = ++reading;
		try {
			const res = await getPageMarkdown(pageId);
			if (ticket !== reading) return;
			markdown = res.markdown;
			failed = false;
		} catch (err) {
			if (ticket !== reading) return;
			console.error("Failed to read the page as markdown:", err);
			failed = true;
		}
	}

	function onUpdate() {
		if (refreshTimer) clearTimeout(refreshTimer);
		refreshTimer = setTimeout(() => {
			refreshTimer = null;
			void read();
		}, REFRESH_MS);
	}

	async function copy() {
		if (markdown === null) return;
		try {
			await navigator.clipboard.writeText(markdown);
			copied = true;
			if (copiedTimer) clearTimeout(copiedTimer);
			copiedTimer = setTimeout(() => (copied = false), 2000);
		} catch (err) {
			console.error("Failed to copy markdown:", err);
		}
	}

	onMount(() => {
		void read();
		ydoc?.on("update", onUpdate);
	});

	onDestroy(() => {
		ydoc?.off("update", onUpdate);
		if (refreshTimer) clearTimeout(refreshTimer);
		if (copiedTimer) clearTimeout(copiedTimer);
		reading++;
	});
</script>

<div class="markdown-view">
	<div class="markdown-actions">
		<span class="markdown-hint">Read only. Edit the page itself.</span>
		<Button
			variant="secondary"
			size="sm"
			icon={copied ? "ri:check-line" : "ri:file-copy-line"}
			disabled={markdown === null}
			onclick={copy}
		>
			{copied ? "Copied" : "Copy markdown"}
		</Button>
	</div>
	{#if failed && markdown === null}
		<p class="markdown-failed" role="alert">
			Your server couldn't show this page as markdown. Close this and try again.
		</p>
	{:else if markdown === null}
		<p class="markdown-loading">Reading the page…</p>
	{:else}
		<pre class="markdown-source">{markdown}</pre>
	{/if}
</div>

<style>
	.markdown-view {
		display: flex;
		flex-direction: column;
		gap: 12px;
		min-width: 0;
	}

	.markdown-actions {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
	}

	.markdown-hint,
	.markdown-loading,
	.markdown-failed {
		margin: 0;
		font-size: 13px;
		color: var(--color-foreground-muted);
	}

	.markdown-source {
		margin: 0;
		max-height: 60vh;
		overflow: auto;
		padding: 12px;
		border: 1px solid var(--color-border-subtle, var(--color-border));
		border-radius: 6px;
		background: var(--color-surface-elevated);
		color: var(--color-foreground);
		/* design-ok: the page's markdown source, where a fixed advance shows its syntax, as TextPane's raw text */
		font-family: var(--font-mono);
		font-size: 13px;
		line-height: 1.5;
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
</style>
