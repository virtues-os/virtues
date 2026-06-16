<!--
  Tier 3 — Chat import (one-time / manual source).

  Teaches the "living vs one-time" distinction with two visible buckets, then
  takes a Claude / ChatGPT / Gemini export (.json) and uploads it to
  /api/chat-import/upload. The box parses + dedups it (box-side), so the
  "Imported N messages" summary appears once the run completes. Re-importing a
  fresher export later only adds new messages (ON CONFLICT DO NOTHING).
-->
<script lang="ts">
	import { Button } from "$lib";
	import * as api from "$lib/api/client";

	let file = $state<File | null>(null);
	let provider = $state<"claude" | "chatgpt" | "gemini">("claude");
	let uploading = $state(false);
	let result = $state<string | null>(null);
	let error = $state<string | null>(null);

	function onPick(e: Event) {
		const f = (e.target as HTMLInputElement).files?.[0] ?? null;
		file = f;
		result = null;
		error = null;
		// Best-effort provider guess from the filename.
		const n = f?.name.toLowerCase() ?? "";
		if (n.includes("chatgpt") || n.includes("openai")) provider = "chatgpt";
		else if (n.includes("gemini") || n.includes("takeout")) provider = "gemini";
		else if (n.includes("claude")) provider = "claude";
	}

	async function doImport() {
		if (!file) return;
		uploading = true;
		error = null;
		result = null;
		try {
			const res = await api.uploadChatImport(file, provider);
			result = res.summary;
			file = null;
		} catch (e) {
			error = e instanceof Error ? e.message : "Import failed.";
		} finally {
			uploading = false;
		}
	}
</script>

<div class="rounded-lg border border-border p-4 space-y-4">
	<div>
		<p class="font-serif text-base text-foreground mb-1">Bring your chat history</p>
		<p class="text-sm text-foreground-muted">Two ways to feed Virtues:</p>
	</div>

	<!-- The two buckets teach cron-vs-one-time without the word "cron". -->
	<div class="grid grid-cols-2 gap-3">
		<div class="rounded-md border border-border p-3">
			<p class="text-xs font-medium text-foreground-muted uppercase tracking-wide mb-1">Living</p>
			<p class="text-sm text-foreground">Stays current on its own — Calendar, Email, your devices.</p>
		</div>
		<div class="rounded-md border border-primary/40 bg-primary/5 p-3">
			<p class="text-xs font-medium text-primary uppercase tracking-wide mb-1">One-time import</p>
			<p class="text-sm text-foreground">A snapshot of the past — drop a Claude, ChatGPT, or Gemini export.</p>
		</div>
	</div>

	{#if result}
		<div class="p-3 bg-success-subtle border border-success rounded-lg">
			<p class="text-sm text-success">{result}</p>
		</div>
	{:else}
		<div class="space-y-3">
			<label class="block">
				<span class="text-sm text-foreground-muted">Export file (.json)</span>
				<input
					type="file"
					accept=".json,application/json,.zip"
					onchange={onPick}
					class="mt-1 block w-full text-sm text-foreground file:mr-3 file:rounded-md file:border-0 file:bg-surface-elevated file:px-3 file:py-1.5 file:text-sm file:text-foreground"
				/>
			</label>

			<label class="block">
				<span class="text-sm text-foreground-muted">Source</span>
				<select bind:value={provider} class="mt-1 block w-full rounded-md border border-border bg-surface px-2 py-1.5 text-sm text-foreground">
					<option value="claude">Claude</option>
					<option value="chatgpt">ChatGPT</option>
					<option value="gemini">Gemini</option>
				</select>
			</label>

			<Button variant="primary" onclick={doImport} disabled={!file || uploading}>
				{uploading ? "Importing…" : "Import"}
			</Button>
		</div>
	{/if}

	{#if error}
		<div class="p-3 bg-error-subtle border border-error rounded-lg">
			<p class="text-sm text-error">{error}</p>
		</div>
	{/if}
</div>
