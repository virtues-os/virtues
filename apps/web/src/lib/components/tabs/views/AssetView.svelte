<script lang="ts">
	// Open (full density) for a file reference. Canonical surface for a file
	// target: fetches metadata, then dispatches by MIME to a per-kind surface.
	// Entities keep their own WikiDetailView — "open" is the one density allowed
	// to differ by target type.
	import Icon from "$lib/components/Icon.svelte";
	import CsvPane from "$lib/components/asset/CsvPane.svelte";
	import PdfPane from "$lib/components/asset/PdfPane.svelte";
	import TextPane from "$lib/components/asset/TextPane.svelte";
	import { getDriveFile, type DriveFile } from "$lib/api/client";
	import { refIcon } from "$lib/utils/refRoutes";
	import type { Tab } from "$lib/tabs/types";

	let { tab }: { tab: Tab; active?: boolean } = $props();

	// /drive/file_xxx[?page=N] → file_xxx (+ optional page deep link)
	const lastSegment = $derived(tab.route.split("/").filter(Boolean).pop() ?? "");
	const fileId = $derived(lastSegment.split("?")[0]);
	const routeParams = $derived(new URLSearchParams(lastSegment.split("?")[1] ?? ""));
	const pageParam = $derived.by(() => {
		const n = Number(routeParams.get("page"));
		return Number.isFinite(n) && n > 0 ? n : undefined;
	});
	// Citation landing targets (D2.4): ?q=<quote> flashes the passage,
	// ?hl=<annotation_id> flashes the highlight.
	const quoteParam = $derived(routeParams.get("q") ?? undefined);
	const hlParam = $derived(routeParams.get("hl") ?? undefined);
	const downloadUrl = $derived(`/api/drive/files/${fileId}/download`);
	// Viewer surfaces render in place; the Download button keeps attachment.
	const viewUrl = $derived(`${downloadUrl}?disposition=inline`);

	let file = $state<DriveFile | null>(null);
	let loading = $state(true);
	let error = $state<string | null>(null);

	$effect(() => {
		const id = fileId;
		if (!id) return;
		loading = true;
		error = null;
		file = null;
		getDriveFile(id)
			.then((f) => {
				file = f;
			})
			.catch((e) => {
				error = e instanceof Error ? e.message : "Failed to load file";
			})
			.finally(() => {
				loading = false;
			});
	});

	type Kind = "image" | "audio" | "video" | "pdf" | "csv" | "markdown" | "code" | "plain" | "other";
	const IMAGE_EXT = /\.(jpe?g|png|gif|webp|svg|bmp|ico|heic)$/i;
	const AUDIO_EXT = /\.(mp3|wav|ogg|flac|aac|m4a)$/i;
	const VIDEO_EXT = /\.(mp4|webm|mov|avi|mkv)$/i;
	const CSV_EXT = /\.(csv|tsv)$/i;
	const MD_EXT = /\.(md|markdown)$/i;
	const CODE_EXT =
		/\.(json|ya?ml|toml|xml|html?|css|scss|[mc]?js|tsx?|jsx|svelte|py|rs|go|rb|swift|kt|java|c|h|cpp|hpp|sh|zsh|bash|fish|sql|ini|conf|dockerfile|makefile)$/i;
	const PLAIN_EXT = /\.(txt|log|text)$/i;

	function kindOf(f: DriveFile | null): Kind {
		if (!f) return "other";
		const mime = f.mime_type ?? "";
		const name = f.filename ?? "";
		if (mime.startsWith("image/") || IMAGE_EXT.test(name)) return "image";
		if (mime.startsWith("audio/") || AUDIO_EXT.test(name)) return "audio";
		if (mime.startsWith("video/") || VIDEO_EXT.test(name)) return "video";
		if (mime === "application/pdf" || /\.pdf$/i.test(name)) return "pdf";
		if (mime === "text/csv" || mime === "text/tab-separated-values" || CSV_EXT.test(name))
			return "csv";
		if (mime === "text/markdown" || MD_EXT.test(name)) return "markdown";
		if (CODE_EXT.test(name) || /^application\/(json|xml|x-yaml|yaml|toml|javascript)$/.test(mime))
			return "code";
		if (mime.startsWith("text/") || PLAIN_EXT.test(name)) return "plain";
		return "other";
	}
	const kind = $derived(kindOf(file));
	const isText = $derived(kind === "markdown" || kind === "code" || kind === "plain");

	function formatBytes(n: number): string {
		if (n < 1024) return `${n} B`;
		const units = ["KB", "MB", "GB", "TB"];
		let v = n / 1024;
		let i = 0;
		while (v >= 1024 && i < units.length - 1) {
			v /= 1024;
			i++;
		}
		return `${v.toFixed(v < 10 ? 1 : 0)} ${units[i]}`;
	}

	function download() {
		const a = document.createElement("a");
		a.href = downloadUrl;
		a.download = file?.filename ?? "";
		a.click();
	}
</script>

<div class="asset-view">
	<header class="asset-header">
		<Icon
			icon={refIcon("file", { mimeType: file?.mime_type, filename: file?.filename })}
			width="16"
		/>
		<span class="asset-name">{file?.filename ?? tab.label}</span>
		{#if file}
			<span class="asset-meta">{formatBytes(file.size_bytes)}</span>
		{/if}
		<div class="asset-spacer"></div>
		<button class="asset-btn" onclick={download} title="Download">
			<Icon icon="ri:download-line" width="14" /> Download
		</button>
	</header>

	<div
		class="asset-body"
		class:framed={kind === "image" || kind === "video"}
		class:flush={kind === "csv" || kind === "pdf" || isText}
	>
		{#if loading}
			<div class="asset-status"><Icon icon="ri:loader-4-line" width="22" class="spin" /></div>
		{:else if error}
			<div class="asset-status error">
				<Icon icon="ri:error-warning-line" width="22" />
				<span>{error}</span>
			</div>
		{:else if kind === "image"}
			<img class="asset-image" src={viewUrl} alt={file?.filename} />
		{:else if kind === "audio"}
			<div class="asset-audio">
				<Icon icon="ri:music-2-line" width="40" />
				<!-- Scrubber via native controls. Waveform + timestamped notes: deferred. -->
				<audio controls src={viewUrl}></audio>
			</div>
		{:else if kind === "video"}
			<!-- svelte-ignore a11y_media_has_caption -->
			<video class="asset-video" controls src={viewUrl}></video>
		{:else if kind === "pdf"}
			<!-- pdf.js reader: text layer + page addressing (?page=N).
			     Highlight / margin-notes / OCR: deferred (net-new persistence). -->
			<PdfPane
				url={viewUrl}
				{fileId}
				initialPage={pageParam}
				initialQuote={quoteParam}
				initialHighlight={hlParam}
			/>
		{:else if kind === "csv"}
			<CsvPane url={viewUrl} filename={file?.filename ?? ""} />
		{:else if kind === "markdown" || kind === "code" || kind === "plain"}
			<TextPane url={viewUrl} filename={file?.filename ?? ""} flavor={kind} />
		{:else}
			<div class="asset-status">
				<Icon
					icon={refIcon("file", { mimeType: file?.mime_type, filename: file?.filename })}
					width="44"
				/>
				<span>{file?.filename}</span>
				<button class="asset-btn" onclick={download}>
					<Icon icon="ri:download-line" width="14" /> Download
				</button>
			</div>
		{/if}
	</div>
</div>

<style>
	.asset-view {
		display: flex;
		flex-direction: column;
		height: 100%;
		width: 100%;
	}

	.asset-header {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 10px 14px;
		border-bottom: 1px solid var(--color-border);
		color: var(--color-foreground);
		flex-shrink: 0;
	}
	.asset-name {
		font-weight: 500;
		font-size: 0.875rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.asset-meta {
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
	}
	.asset-spacer {
		flex: 1;
	}
	.asset-btn {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		padding: 4px 10px;
		font-size: 0.75rem;
		border-radius: 6px;
		border: 1px solid var(--color-border);
		background: transparent;
		color: var(--color-foreground-muted);
		cursor: pointer;
	}
	.asset-btn:hover {
		background: var(--ref-pill-bg);
		color: var(--color-primary);
	}

	.asset-body {
		flex: 1;
		min-height: 0;
		display: flex;
		align-items: center;
		justify-content: center;
		overflow: auto;
		padding: 16px;
	}
	.asset-body.framed {
		background: var(--color-surface-sunken, #000);
		padding: 0;
	}
	/* Text/CSV panes own their scroll and padding — fill the body edge-to-edge. */
	.asset-body.flush {
		align-items: stretch;
		justify-content: stretch;
		padding: 0;
		overflow: hidden;
	}

	.asset-image {
		max-width: 100%;
		max-height: 100%;
		object-fit: contain;
	}
	.asset-video {
		max-width: 100%;
		max-height: 100%;
	}
	.asset-audio {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 16px;
		color: var(--color-foreground-muted);
	}
	.asset-audio audio {
		width: min(520px, 80vw);
	}

	.asset-status {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 12px;
		color: var(--color-foreground-muted);
		font-size: 0.8125rem;
	}
	.asset-status.error {
		color: var(--color-danger, #e5484d);
	}
	.asset-status :global(.spin) {
		animation: asset-spin 0.8s linear infinite;
	}
	@keyframes asset-spin {
		to {
			transform: rotate(360deg);
		}
	}
</style>
