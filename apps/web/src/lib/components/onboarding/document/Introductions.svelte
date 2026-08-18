<!--
  Introductions — two names, and nothing else.

  BETWEEN THE LETTER AND THE WORK. Sync latency is the only irreversible cost in
  onboarding, so the sources want to start as early as possible — but this page
  is thirty seconds and the hour that matters is the interview, which already
  comes after them. Thirty seconds of idle box buys an on-ramp that is not a
  permission prompt: asking for Full Disk Access as the very first act after a
  letter about trust is a jolt, and being asked your own name is not.

  TWO FIELDS. The profile can hold a dozen things — birth date, occupation,
  employer, height, ethnicity — and asking for them here would make the screen
  after the privacy letter a form. The timezone is already known (captured
  silently at setup, and it is the one fact a machine can get right without
  asking). Everything else the interview covers properly, in the person's own
  words, where it belongs.

  Neither field is required. A box that will not proceed until you have named it
  has misunderstood which of you is in charge.
-->
<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import { getProfile, updateProfile, getAssistantProfile, updateAssistantProfile } from "$lib/api/client";
	import { onMount } from "svelte";

	// Shell and progress strip belong to the route; this is only the leaf.
	let { onnext }: { onnext: () => void } = $props();

	let you = $state("");
	let assistant = $state("");
	/** What it is already called, so the field can suggest rather than demand. */
	let assistantDefault = $state("Ari");
	let loading = $state(true);
	let saving = $state(false);
	let error = $state<string | null>(null);

	onMount(async () => {
		try {
			const [p, a] = await Promise.all([
				getProfile().catch(() => null),
				getAssistantProfile<{ assistant_name?: string | null }>().catch(() => null),
			]);
			// `preferred_name` is the only name the profile API exposes — the
			// table also holds full_name, but it is not on the wire, and reaching
			// past the API for it would be borrowing trouble for a placeholder.
			you = p?.preferred_name || "";
			if (a?.assistant_name) assistantDefault = a.assistant_name;
			assistant = a?.assistant_name ?? "";
		} catch {
			// A failed read costs a pre-filled field, nothing more.
		}
		loading = false;
	});

	async function save() {
		saving = true;
		error = null;
		try {
			const jobs: Promise<unknown>[] = [];
			if (you.trim()) jobs.push(updateProfile({ preferred_name: you.trim() }));
			if (assistant.trim()) jobs.push(updateAssistantProfile({ assistant_name: assistant.trim() }));
			await Promise.all(jobs);
		} catch (e) {
			// Say so, and carry on regardless. Being unable to record a nickname
			// is not a reason to hold someone at a door.
			error = e instanceof Error ? e.message : String(e);
		}
		saving = false;
		onnext();
	}
</script>

<div>
	<div>
		<h1 class="ob-h1">Two names.</h1>
		<p class="ob-lede">
			Before the box starts reading anything, the two things it needs from you directly.
			Everything after this is the machine working; this part is thirty seconds.
		</p>

		{#if loading}
			<p class="quiet">One moment…</p>
		{:else}
			<div class="fields">
				<label>
					<span class="label">What should it call you?</span>
					<input
						class="ob-input"
						type="text"
						bind:value={you}
						placeholder="Whatever your friends call you"
						autocomplete="given-name"
					/>
					<span class="note">The name it will use when it writes to you, and about you.</span>
				</label>

				<label>
					<span class="label">And what will you call it?</span>
					<input class="ob-input" type="text" bind:value={assistant} placeholder={assistantDefault} />
					<span class="note">
						It answers to <strong>{assistantDefault}</strong> unless you'd rather it
						didn't. This is yours to change whenever.
					</span>
				</label>
			</div>

			{#if error}
				<p class="ob-err"><Icon icon="ri:error-warning-line" width="14" /> {error}</p>
			{/if}

			<button class="ob-btn" onclick={save} disabled={saving}>
				{saving ? "Saving…" : you.trim() || assistant.trim() ? "Continue" : "Skip for now"}
				<Icon icon="ri:arrow-right-line" width="15" />
			</button>
		{/if}
	</div>
</div>

<style>
	/* Only what belongs to this screen. The shell, type scale, button, input and
	   error style all come from onboarding.css. */
	.fields {
		margin-top: 2.25rem;
		display: flex;
		flex-direction: column;
		gap: 1.75rem;
	}

	label {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}

	/* The questions themselves are prose, so they are set as prose. */
	.label {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 1.15rem;
		color: var(--color-foreground);
	}

	.note {
		font-size: 13px;
		line-height: 1.5;
		color: var(--color-foreground-subtle);
	}

	.note strong {
		color: var(--color-foreground-muted);
		font-weight: 600;
	}

	.quiet {
		font-size: 14px;
		color: var(--color-foreground-subtle);
	}
</style>
