<!--
	What `record_introductions` played back, as one line under the model's
	turn with one button. THE BUTTON WRITES, through the same profile
	endpoints the old form used; the tool wrote nothing. Corrections are a
	reply, not a form: the model plays the facts back again.
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

	let saving = $state(false);
	let saved = $state(false);
	let error = $state<string | null>(null);
	// A line from history, after the step is done: settled.
	const alreadyDone = $derived(gettingStarted.step("introductions")?.status === "done");

	const summary = $derived(
		[
			fields.preferred_name ? `you are ${fields.preferred_name}` : null,
			fields.assistant_name ? `I am ${fields.assistant_name}` : null,
			fields.home_timezone
				? `home is ${fields.home_place ? `${fields.home_place} (${fields.home_timezone})` : fields.home_timezone}`
				: fields.home_place
					? `home is ${fields.home_place}`
					: null,
			fields.birth_date ? `born ${fields.birth_date}` : null,
		]
			.filter(Boolean)
			.join(", "),
	);

	async function save() {
		if (saving) return;
		saving = true;
		error = null;
		try {
			const profile: Partial<Profile> = {};
			if (fields.preferred_name) profile.preferred_name = fields.preferred_name;
			if (fields.birth_date) profile.birth_date = fields.birth_date;
			if (fields.home_timezone) profile.home_timezone = fields.home_timezone;
			const jobs: Promise<unknown>[] = [];
			if (Object.keys(profile).length) jobs.push(updateProfile(profile));
			if (fields.assistant_name) jobs.push(updateAssistantProfile({ assistant_name: fields.assistant_name }));
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

<div class="confirm">
	{#if saved || alreadyDone}
		<span class="line">Recorded: {summary}.</span>
	{:else}
		<span class="line">So: {summary}.</span>
		<button type="button" class="btn" onclick={save} disabled={saving}>{saving ? "Saving…" : "That's right"}</button>
		<span class="note">Reply to correct anything.</span>
		{#if error}<span class="error">{error}</span>{/if}
	{/if}
</div>

<style>
	.confirm {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.5rem 0.75rem;
		margin: 0.5rem 0 0.25rem;
		font-size: 0.9375rem;
	}
	.line {
		color: var(--color-foreground);
	}
	.btn {
		font: inherit;
		font-size: 0.875rem;
		padding: 0.35rem 0.9rem;
		border-radius: 6px;
		border: 1px solid var(--color-foreground);
		background: var(--color-foreground);
		color: var(--color-background);
		cursor: pointer;
	}
	.btn:disabled {
		opacity: 0.4;
	}
	.note {
		font-size: 0.8125rem;
		color: var(--color-foreground-subtle);
	}
	.error {
		font-size: 0.875rem;
		color: var(--color-error, #9a2b2e);
		flex-basis: 100%;
	}
</style>
