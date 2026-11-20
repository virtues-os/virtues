<!--
  Chat import (one-time / manual source).

  Takes the export Claude or ChatGPT emails (the zip, or the
  conversations.json inside it) and uploads it to /api/chat-import/upload.
  The server works out which site each chat came from, so there is nothing
  to choose. `importChatExport` follows the background run to its summary
  or its error.
  Re-importing a fresher export later only adds new messages.
-->
<script lang="ts">
	import { onDestroy } from "svelte";
	import { Button } from "$lib";
	import { importChatExport, isAbort, type ImportPhase } from "$lib/chat-import";

	let file = $state<File | null>(null);
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
		error = null;
		result = null;
		try {
			result = await importChatExport(file, { onPhase: (p) => (busy = p), signal: stop.signal });
			file = null;
		} catch (e) {
			if (isAbort(e)) return;
			error = e instanceof Error ? e.message : "The upload failed. Try again.";
		} finally {
			busy = null;
		}
	}
</script>

<div class="rounded-lg border border-border p-4 space-y-4">
	<!-- No heading here: the step or modal around it supplies the title. -->

	{#if result}
		<div class="p-3 bg-success-subtle border border-success rounded-lg">
			<p class="text-sm text-success">{result}</p>
		</div>
	{:else}
		<div class="space-y-3">
			<p class="text-sm text-foreground-muted">
				In ChatGPT, open Settings, then Data controls, then Export data. In Claude, open Settings,
				then Privacy, then Export data. Each one emails you a zip. Upload it here as it is.
			</p>

			<label class="block">
				<span class="text-sm text-foreground-muted">Export file</span>
				<input
					type="file"
					accept=".zip,application/zip,.json,application/json"
					onchange={onPick}
					disabled={busy !== null}
					class="mt-1 block w-full text-sm text-foreground file:mr-3 file:rounded-md file:border-0 file:bg-surface-elevated file:px-3 file:py-1.5 file:text-sm file:text-foreground"
				/>
			</label>


			<Button variant="primary" onclick={doImport} disabled={!file || busy !== null}>
				{busy?.step === "uploading" ? `Uploading, ${busy.percent}%` : busy?.step === "reading" ? "Reading your chats…" : "Import"}
			</Button>
		</div>
	{/if}

	{#if error}
		<div class="p-3 bg-error-subtle border border-error rounded-lg">
			<p class="text-sm text-error">{error}</p>
		</div>
	{/if}
</div>
