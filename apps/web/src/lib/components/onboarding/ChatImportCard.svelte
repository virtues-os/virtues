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
	<!-- No heading/explainer here — the onboarding step supplies the title +
	     subtitle, and the living-vs-one-time framing lives on the sources step. -->

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

			<!-- This is the highest-disclosure step in onboarding (a full chat
			     history); say plainly where it goes. -->
			<p class="text-xs text-foreground-subtle">
				Parsed and stored on your box — the file never leaves your hardware,
				and nothing is sent to Virtues.
			</p>

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
