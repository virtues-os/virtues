<!--
  A one-time import: Past chats, Spotify listening, Amazon orders, Discord
  messages or a Slack workspace.

  Says how to get the file from the service, takes it, and uploads it.
  `importFile` follows the background run to its summary or its error.
  Importing a newer file later adds only what is new.
-->
<script lang="ts">
	import { onDestroy } from "svelte";
	import { Button, Input } from "$lib";
	import { IMPORT_SOURCES, importFile, isAbort, type ImportPhase } from "$lib/imports";

	let { sourceId }: { sourceId: string } = $props();
	const source = $derived(IMPORT_SOURCES[sourceId]);

	let file = $state<File | null>(null);
	let email = $state("");
	let busy = $state<ImportPhase | null>(null);
	let result = $state<string | null>(null);
	let error = $state<string | null>(null);
	const stop = new AbortController();
	onDestroy(() => stop.abort());

	function onPick(e: Event) {
		file = (e.target as HTMLInputElement).files?.[0] ?? null;
		result = null;
		error = null;
	}

	async function doImport() {
		if (!file) return;
		if (source?.emailLabel && !email.trim()) {
			error = `Enter ${source.emailLabel.toLowerCase()}, then import.`;
			return;
		}
		error = null;
		result = null;
		try {
			result = await importFile(sourceId, file, {
				onPhase: (p) => (busy = p),
				signal: stop.signal,
				fields: source?.emailLabel ? { email: email.trim() } : {}
			});
			file = null;
		} catch (e) {
			if (isAbort(e)) return;
			error = e instanceof Error ? e.message : "The upload failed. Try again.";
		} finally {
			busy = null;
		}
	}
</script>

{#if source}
	<div class="rounded-lg border border-border p-4 space-y-4">
		<!-- No heading here: the step or modal around it supplies the title. -->

		{#if result}
			<div class="p-3 bg-success-subtle border border-success rounded-lg">
				<p class="text-sm text-success">{result}</p>
			</div>
		{:else}
			<div class="space-y-3">
				<p class="text-sm text-foreground-muted">{source.how}</p>

				<label class="block">
					<span class="text-sm text-foreground-muted">{source.fileLabel}</span>
					<input
						type="file"
						accept={source.accept}
						onchange={onPick}
						disabled={busy !== null}
						class="mt-1 block w-full text-sm text-foreground file:mr-3 file:rounded-md file:border-0 file:bg-surface-elevated file:px-3 file:py-1.5 file:text-sm file:text-foreground"
					/>
				</label>

				{#if source.emailLabel}
					<label class="block">
						<span class="text-sm text-foreground-muted">{source.emailLabel}</span>
						<Input type="email" bind:value={email} disabled={busy !== null} />
					</label>
				{/if}

				<Button variant="primary" onclick={doImport} disabled={!file || busy !== null}>
					{busy?.step === "uploading" ? `Uploading, ${busy.percent}%` : busy?.step === "reading" ? source.reading : "Import"}
				</Button>
			</div>
		{/if}

		{#if error}
			<div class="p-3 bg-error-subtle border border-error rounded-lg">
				<p class="text-sm text-error">{error}</p>
			</div>
		{/if}
	</div>
{/if}
