<!--
	What `record_introductions` heard, played back under the model's turn:
	one line, then one button on its own row. THE BUTTON WRITES, through the
	same profile endpoints the old form used; the tool wrote nothing. A
	correction is a reply, not a form.
-->
<script lang="ts">
	import { updateProfile, updateAssistantProfile, type Profile } from "$lib/api/client";
	import { gettingStarted } from "$lib/stores/gettingStarted.svelte";
	import Act from "./ui/Act.svelte";
	import Choices from "./ui/Choices.svelte";

	interface Fields {
		preferred_name?: string | null;
		assistant_name?: string | null;
		home_place?: string | null;
		home_timezone?: string | null;
		birth_date?: string | null;
	}
	let { fields }: { fields: Fields } = $props();

	let saving = $state(false);
	let saved = $state(false);
	let error = $state<string | null>(null);
	const settled = $derived(saved || gettingStarted.step("introductions")?.status === "done");

	const summary = $derived(
		[
			fields.preferred_name ? `you are ${fields.preferred_name}` : null,
			fields.assistant_name ? `I am ${fields.assistant_name}` : null,
			fields.home_place ?? fields.home_timezone ? `home is ${fields.home_place ?? fields.home_timezone}` : null,
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

<p class="said">{settled ? "Recorded" : "So"}: {summary}.</p>
{#if !settled}
	<Choices>
		{#snippet foot()}
			{#if error}<span class="error">{error}</span>{:else}Reply to correct anything.{/if}
		{/snippet}
		<Act variant="primary" disabled={saving} onclick={save}>{saving ? "Saving…" : "That's right"}</Act>
	</Choices>
{/if}

<style>
	.said {
		margin: 0.5rem 0 0;
	}
	.error {
		color: var(--color-error, #9a2b2e);
	}
</style>
