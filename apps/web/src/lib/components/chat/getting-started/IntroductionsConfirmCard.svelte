<!--
	The confirmation card for `record_introductions`: the four facts as the
	model heard them, each editable, one button. THE CARD WRITES — through
	the same profile endpoints the old introductions card used — and the tool
	wrote nothing. A field the model could not resolve arrives empty and
	stays empty unless the person fills it.
-->
<script lang="ts">
	import { updateProfile, updateAssistantProfile, type Profile } from "$lib/api/client";
	import { gettingStarted } from "$lib/stores/gettingStarted.svelte";

	interface Fields {
		preferred_name?: string | null;
		assistant_name?: string | null;
		home_place?: string | null;
		home_timezone?: string | null;
		birth_date?: string | null;
	}
	interface Props {
		fields: Fields;
	}
	let { fields }: Props = $props();

	// Initial values on purpose: the person edits from what was heard, and a
	// later re-render of the tool part must not overwrite their corrections.
	// svelte-ignore state_referenced_locally
	let you = $state(fields.preferred_name ?? "");
	// svelte-ignore state_referenced_locally
	let assistant = $state(fields.assistant_name ?? "");
	// svelte-ignore state_referenced_locally
	let tz = $state(fields.home_timezone ?? "");
	// svelte-ignore state_referenced_locally
	let born = $state(fields.birth_date ?? "");
	let saving = $state(false);
	let saved = $state(false);
	let error = $state<string | null>(null);

	// A card from history, after the step is done: show it settled.
	const alreadyDone = $derived(gettingStarted.step("introductions")?.status === "done");

	async function save() {
		if (saving) return;
		saving = true;
		error = null;
		try {
			const profile: Partial<Profile> = {};
			if (you.trim()) profile.preferred_name = you.trim();
			if (born.trim()) profile.birth_date = born.trim();
			if (tz.trim()) profile.home_timezone = tz.trim();
			const jobs: Promise<unknown>[] = [];
			if (Object.keys(profile).length) jobs.push(updateProfile(profile));
			if (assistant.trim()) jobs.push(updateAssistantProfile({ assistant_name: assistant.trim() }));
			await Promise.all(jobs);
			saved = true;
			await gettingStarted.refresh();
		} catch (e) {
			error = e instanceof Error ? e.message : "Could not save.";
		} finally {
			saving = false;
		}
	}
</script>

<section class="card" class:settled={saved || alreadyDone}>
	<h3 class="title">Introductions</h3>
	{#if saved || alreadyDone}
		<p class="line">Recorded. Change any of it in Settings, any time.</p>
	{:else}
		<p class="line">Here is what I heard. Correct anything, then confirm.</p>
		<div class="grid">
			<label><span>Call you</span><input bind:value={you} placeholder="Nick" /></label>
			<label><span>Call it</span><input bind:value={assistant} placeholder="Ari" /></label>
			<label>
				<span>Home time zone{fields.home_place ? ` (${fields.home_place})` : ""}</span>
				<input bind:value={tz} placeholder="America/Chicago" />
			</label>
			<label><span>Born</span><input bind:value={born} type="date" /></label>
		</div>
		{#if error}<p class="error">{error}</p>{/if}
		<div class="row">
			<button type="button" class="confirm" onclick={save} disabled={saving}>{saving ? "Saving…" : "That's right"}</button>
		</div>
	{/if}
</section>

<style>
	.card {
		border: 1px solid var(--color-border);
		border-radius: 8px;
		padding: 0.875rem 1rem;
		margin: 0.5rem 0;
		background: var(--color-background);
	}
	.title {
		font-family: var(--font-serif, Georgia, serif);
		font-weight: 400;
		font-size: 1.125rem;
		margin: 0 0 0.25rem;
	}
	.line {
		margin: 0 0 0.75rem;
		font-size: 0.9rem;
		color: var(--color-foreground-muted);
	}
	.settled .line {
		margin-bottom: 0;
	}
	.grid {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 0.6rem 1rem;
	}
	label {
		display: grid;
		gap: 0.2rem;
		font-size: 0.8125rem;
		color: var(--color-foreground-subtle);
	}
	input {
		font: inherit;
		font-size: 0.9375rem;
		padding: 0.4rem 0.6rem;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		background: var(--color-background);
		color: var(--color-foreground);
	}
	.error {
		color: var(--color-error, #9a2b2e);
		font-size: 0.875rem;
		margin: 0.5rem 0 0;
	}
	.row {
		display: flex;
		justify-content: flex-end;
		margin-top: 0.75rem;
	}
	.confirm {
		font: inherit;
		font-size: 0.875rem;
		padding: 0.35rem 0.9rem;
		border-radius: 6px;
		border: 1px solid var(--color-foreground);
		background: var(--color-foreground);
		color: var(--color-background);
		cursor: pointer;
	}
	.confirm:disabled {
		opacity: 0.4;
	}
	@media (max-width: 480px) {
		.grid {
			grid-template-columns: 1fr;
		}
	}
</style>
