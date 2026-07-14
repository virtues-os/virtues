<script lang="ts">
	// App Media — the `.media/` system folder.
	//
	// Assets the app itself made or uses: pasted pictures, generated images, the
	// things rendered inside pages and chats. NOT files the user filed in Drive
	// (which is why the Drive browser hides them), and NOT collected data (that's
	// Streams). Read-only: nothing here is user-authored, and deleting an asset
	// something still references just breaks the thing referencing it.
	import Icon from "$lib/components/Icon.svelte";
	import { onMount } from "svelte";

	interface DriveFile {
		id: string;
		path: string;
		filename: string;
		mime_type: string | null;
		size_bytes: number | null;
		created_at: string;
	}

	let files: DriveFile[] = $state([]);
	let loading = $state(true);
	let error: string | null = $state(null);

	const totalBytes = $derived(files.reduce((n, f) => n + (f.size_bytes ?? 0), 0));

	onMount(load);

	async function load() {
		loading = true;
		error = null;
		try {
			const res = await fetch("/api/drive/media");
			if (!res.ok) throw new Error(`Failed to load app media: ${res.statusText}`);
			files = await res.json();
		} catch (e: any) {
			error = e.message;
		} finally {
			loading = false;
		}
	}

	function formatBytes(bytes: number | null): string {
		if (!bytes) return "—";
		const k = 1024;
		const sizes = ["B", "KB", "MB", "GB"];
		const i = Math.floor(Math.log(bytes) / Math.log(k));
		return `${parseFloat((bytes / Math.pow(k, i)).toFixed(1))} ${sizes[i]}`;
	}

	function formatDate(d: string): string {
		return new Date(d).toLocaleDateString(undefined, {
			month: "short",
			day: "numeric",
			year: "numeric",
		});
	}

	function isImage(f: DriveFile): boolean {
		return (f.mime_type ?? "").startsWith("image/");
	}
</script>

<div class="flex h-full w-full flex-col overflow-auto p-6">
	{#if loading}
		<div class="flex h-full items-center justify-center">
			<Icon icon="ri:loader-4-line" width="20" class="animate-spin text-foreground-muted" />
		</div>
	{:else if error}
		<div class="rounded-md border border-destructive/20 bg-destructive/10 p-4 text-destructive">
			<div class="flex items-center gap-2 font-medium">
				<Icon icon="ri:error-warning-line" />
				Failed to load app media
			</div>
			<p class="mt-1 text-xs">{error}</p>
		</div>
	{:else if files.length === 0}
		<div class="flex h-full flex-col items-center justify-center gap-2 text-foreground-muted">
			<Icon icon="ri:image-2-line" width="28" />
			<p class="text-sm">No app media yet</p>
			<p class="max-w-sm text-center text-xs">
				Images the app generates or you paste into pages and chats are kept here.
			</p>
		</div>
	{:else}
		<div class="mb-4 flex items-baseline justify-between">
			<div class="flex items-baseline gap-2">
				<h2 class="text-xs font-medium text-foreground">
					{files.length.toLocaleString()} asset{files.length === 1 ? "" : "s"}
				</h2>
				<span class="text-xs text-foreground-muted">{formatBytes(totalBytes)}</span>
			</div>
			<div class="flex items-center gap-1.5 text-xs text-foreground-muted">
				<Icon icon="ri:lock-2-line" />
				Read-only — used by the app
			</div>
		</div>

		<div class="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-5">
			{#each files as f (f.id)}
				<div class="overflow-hidden rounded-lg border border-border bg-surface">
					<div class="flex aspect-square items-center justify-center bg-surface-elevated">
						{#if isImage(f)}
							<img
								src={`/api/drive/files/${f.id}/download`}
								alt={f.filename}
								loading="lazy"
								class="h-full w-full object-cover"
							/>
						{:else}
							<Icon icon="ri:file-line" width="22" class="text-foreground-muted" />
						{/if}
					</div>
					<div class="px-2.5 py-2">
						<div class="truncate text-xs text-foreground" title={f.filename}>
							{f.filename}
						</div>
						<div class="mt-0.5 flex items-center justify-between text-[11px] text-foreground-muted">
							<span>{formatBytes(f.size_bytes)}</span>
							<span>{formatDate(f.created_at)}</span>
						</div>
					</div>
				</div>
			{/each}
		</div>
	{/if}
</div>
