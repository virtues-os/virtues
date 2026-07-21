<script lang="ts">
	// Text pane for AssetView: markdown renders through the app's Markdown
	// component (Raw toggle available); code files get shiki highlighting by
	// round-tripping through a fenced block; big or plain files fall back to a
	// bare <pre>. Content is fetched with a byte cap — no Range support on the
	// download route yet, so the stream is cancelled at the cap.
	import Icon from "$lib/components/Icon.svelte";
	import Markdown from "$lib/components/Markdown.svelte";
	import { fetchTextCapped } from "./text";

	let {
		url,
		filename,
		flavor,
	}: {
		url: string;
		filename: string;
		flavor: "markdown" | "code" | "plain";
	} = $props();

	// Read at most 2 MB; highlight (shiki) only below 300 KB — beyond that,
	// highlighting cost outweighs its value and a plain <pre> stays snappy.
	const FETCH_CAP = 2 * 1024 * 1024;
	const HIGHLIGHT_CAP = 300 * 1024;

	let text = $state<string | null>(null);
	let truncated = $state(false);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let showRaw = $state(false);

	$effect(() => {
		const target = url;
		loading = true;
		error = null;
		text = null;
		truncated = false;
		fetchTextCapped(target, FETCH_CAP)
			.then((r) => {
				text = r.text;
				truncated = r.truncated;
			})
			.catch((e) => {
				error = e instanceof Error ? e.message : "Failed to load file";
			})
			.finally(() => {
				loading = false;
			});
	});

	const ext = $derived(filename.slice(filename.lastIndexOf(".") + 1).toLowerCase());

	// Minified JSON reads as one line — pretty-print when it parses.
	const displayText = $derived.by(() => {
		if (text === null) return null;
		if (ext === "json" && !truncated) {
			try {
				return JSON.stringify(JSON.parse(text), null, 2);
			} catch {
				return text;
			}
		}
		return text;
	});

	// Fence with more backticks than any run inside the content, so embedded
	// fences can't break out.
	const fenced = $derived.by(() => {
		const body = displayText ?? "";
		const longestRun = body.match(/`+/g)?.reduce((m, r) => Math.max(m, r.length), 0) ?? 0;
		const fence = "`".repeat(Math.max(3, longestRun + 1));
		return `${fence}${ext}\n${body}\n${fence}`;
	});

	const highlightable = $derived(
		flavor === "code" && displayText !== null && displayText.length <= HIGHLIGHT_CAP
	);
</script>

<div class="text-pane">
	{#if truncated}
		<div class="text-banner">
			<Icon icon="ri:scissors-cut-line" width="13" />
			Large file — showing the first 2 MB. Download for the full contents.
		</div>
	{/if}

	{#if flavor === "markdown" && !loading && !error}
		<div class="text-toolbar">
			<button class="text-toggle" class:active={!showRaw} onclick={() => (showRaw = false)}>
				Rendered
			</button>
			<button class="text-toggle" class:active={showRaw} onclick={() => (showRaw = true)}>
				Raw
			</button>
		</div>
	{/if}

	{#if loading}
		<div class="text-status"><Icon icon="ri:loader-4-line" width="22" class="spin" /></div>
	{:else if error}
		<div class="text-status error">
			<Icon icon="ri:error-warning-line" width="22" />
			<span>{error}</span>
		</div>
	{:else if flavor === "markdown" && !showRaw}
		<div class="text-prose"><Markdown content={displayText ?? ""} /></div>
	{:else if highlightable}
		<div class="text-code"><Markdown content={fenced} /></div>
	{:else}
		<pre class="text-plain" class:nowrap={flavor === "code"}>{displayText}</pre>
	{/if}
</div>

<style>
	.text-pane {
		display: flex;
		flex-direction: column;
		width: 100%;
		height: 100%;
		min-height: 0;
	}

	.text-banner {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 6px 14px;
		font-size: 0.75rem;
		color: var(--color-foreground-muted);
		border-bottom: 1px solid var(--color-border);
		flex-shrink: 0;
	}

	.text-toolbar {
		display: flex;
		gap: 2px;
		padding: 8px 14px 0;
		flex-shrink: 0;
	}
	.text-toggle {
		padding: 3px 10px;
		font-size: 0.75rem;
		border-radius: 6px;
		border: 1px solid transparent;
		background: transparent;
		color: var(--color-foreground-muted);
		cursor: pointer;
	}
	.text-toggle.active {
		border-color: var(--color-border);
		color: var(--color-foreground);
	}

	.text-prose {
		flex: 1;
		min-height: 0;
		overflow: auto;
		padding: 20px 24px 40px;
	}
	.text-prose > :global(*) {
		max-width: 72ch;
		margin-inline: auto;
	}

	.text-code {
		flex: 1;
		min-height: 0;
		overflow: auto;
		padding: 12px 16px 32px;
	}

	.text-plain {
		flex: 1;
		min-height: 0;
		overflow: auto;
		margin: 0;
		padding: 16px 20px 32px;
		font-family: var(--font-mono, ui-monospace, monospace);
		font-size: 0.8125rem;
		line-height: 1.55;
		color: var(--color-foreground);
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
	.text-plain.nowrap {
		white-space: pre;
		overflow-wrap: normal;
	}

	.text-status {
		flex: 1;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 12px;
		color: var(--color-foreground-muted);
		font-size: 0.8125rem;
	}
	.text-status.error {
		color: var(--color-danger, #e5484d);
	}
	.text-status :global(.spin) {
		animation: text-pane-spin 0.8s linear infinite;
	}
	@keyframes text-pane-spin {
		to {
			transform: rotate(360deg);
		}
	}
</style>
