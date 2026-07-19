<script lang="ts">
	// Open (full density) for a file reference. Canonical surface for a file
	// target: fetches metadata, then dispatches by MIME to a per-kind surface.
	// Entities keep their own WikiDetailView — "open" is the one density allowed
	// to differ by target type.
	import Icon from "$lib/components/Icon.svelte";
	import { getDriveFile, type DriveFile } from "$lib/api/client";
	import { refIcon } from "$lib/utils/refRoutes";
	import type { Tab } from "$lib/tabs/types";

	let { tab }: { tab: Tab; active?: boolean } = $props();

	// /drive/file_xxx → file_xxx
	const fileId = $derived(tab.route.split("/").filter(Boolean).pop() ?? "");
	const downloadUrl = $derived(`/api/drive/files/${fileId}/download`);

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

	type Kind = "image" | "audio" | "video" | "pdf" | "other";
	const IMAGE_EXT = /\.(jpe?g|png|gif|webp|svg|bmp|ico|heic)$/i;
	const AUDIO_EXT = /\.(mp3|wav|ogg|flac|aac|m4a)$/i;
	const VIDEO_EXT = /\.(mp4|webm|mov|avi|mkv)$/i;

	function kindOf(f: DriveFile | null): Kind {
		if (!f) return "other";
		const mime = f.mime_type ?? "";
		const name = f.filename ?? "";
		if (mime.startsWith("image/") || IMAGE_EXT.test(name)) return "image";
		if (mime.startsWith("audio/") || AUDIO_EXT.test(name)) return "audio";
		if (mime.startsWith("video/") || VIDEO_EXT.test(name)) return "video";
		if (mime === "application/pdf" || /\.pdf$/i.test(name)) return "pdf";
		return "other";
	}
	const kind = $derived(kindOf(file));

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

	<div class="asset-body" class:framed={kind === "image" || kind === "video"}>
		{#if loading}
			<div class="asset-status"><Icon icon="ri:loader-4-line" width="22" class="spin" /></div>
		{:else if error}
			<div class="asset-status error">
				<Icon icon="ri:error-warning-line" width="22" />
				<span>{error}</span>
			</div>
		{:else if kind === "image"}
			<img class="asset-image" src={downloadUrl} alt={file?.filename} />
		{:else if kind === "audio"}
			<div class="asset-audio">
				<Icon icon="ri:music-2-line" width="40" />
				<!-- Scrubber via native controls. Waveform + timestamped notes: deferred. -->
				<audio controls src={downloadUrl}></audio>
			</div>
		{:else if kind === "video"}
			<!-- svelte-ignore a11y_media_has_caption -->
			<video class="asset-video" controls src={downloadUrl}></video>
		{:else if kind === "pdf"}
			<!-- Reader. Highlight / margin-notes / OCR: deferred (net-new persistence). -->
			<iframe class="asset-pdf" src={downloadUrl} title={file?.filename}></iframe>
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

	.asset-image {
		max-width: 100%;
		max-height: 100%;
		object-fit: contain;
	}
	.asset-video {
		max-width: 100%;
		max-height: 100%;
	}
	.asset-pdf {
		width: 100%;
		height: 100%;
		border: none;
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
