<!--
  Introductions — two names, and nothing else.

  PLACED AFTER THE MAC, NOT BEFORE IT. Connecting the Mac starts the box reading
  years of iMessage, and sync latency is the only irreversible cost in
  onboarding — every minute spent here before it starts is a minute the box is
  idle. So this page fills the wait rather than delaying the work, and naming an
  assistant that is already reading your life means more than naming an empty
  one.

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

	let { onnext, reduced = false }: { onnext: () => void; reduced?: boolean } = $props();

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

<div class="wrap" class:still={reduced}>
	<div class="sheet">
		<p class="kicker">Introductions</p>
		<h1>Two names.</h1>
		<p class="lede">
			Your box is reading your Mac now — that runs by itself and takes a while. In the
			meantime, the only two things it needs from you directly.
		</p>

		{#if loading}
			<p class="quiet">One moment…</p>
		{:else}
			<div class="fields">
				<label>
					<span class="label">What should it call you?</span>
					<input
						type="text"
						bind:value={you}
						placeholder="Whatever your friends call you"
						autocomplete="given-name"
					/>
					<span class="note">The name it will use when it writes to you, and about you.</span>
				</label>

				<label>
					<span class="label">And what will you call it?</span>
					<input type="text" bind:value={assistant} placeholder={assistantDefault} />
					<span class="note">
						It answers to <strong>{assistantDefault}</strong> unless you'd rather it
						didn't. This is yours to change whenever.
					</span>
				</label>
			</div>

			{#if error}
				<p class="err"><Icon icon="ri:error-warning-line" width="14" /> {error}</p>
			{/if}

			<button class="next" onclick={save} disabled={saving}>
				{saving ? "Saving…" : you.trim() || assistant.trim() ? "Continue" : "Skip for now"}
				<Icon icon="ri:arrow-right-line" width="15" />
			</button>
		{/if}
	</div>
</div>

<style>
	.wrap {
		min-height: 100vh;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 4rem 1.5rem;
	}

	.sheet {
		width: 100%;
		max-width: 34rem;
		animation: rise 0.5s ease both;
	}

	.still .sheet {
		animation: none;
	}

	@keyframes rise {
		from {
			opacity: 0;
			transform: translateY(8px);
		}
		to {
			opacity: 1;
			transform: none;
		}
	}

	.kicker {
		font-family: var(--font-mono, ui-monospace, monospace);
		font-size: 11px;
		letter-spacing: 0.18em;
		text-transform: uppercase;
		color: var(--color-foreground-subtle);
		margin: 0 0 0.75rem;
	}

	h1 {
		font-family: var(--font-serif, Georgia, serif);
		font-size: clamp(1.9rem, 3.5vw, 2.4rem);
		line-height: 1.05;
		margin: 0;
	}

	.lede {
		margin: 1rem 0 0;
		font-size: 15px;
		line-height: 1.65;
		color: var(--color-foreground-muted);
		max-width: 32rem;
	}

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

	.label {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 1.15rem;
		color: var(--color-foreground);
	}

	input {
		width: 100%;
		font: inherit;
		font-size: 1rem;
		padding: 0.7rem 0.9rem;
		border-radius: 10px;
		border: 1px solid var(--color-border);
		background: color-mix(in srgb, var(--color-foreground) 3%, transparent);
		color: var(--color-foreground);
	}

	input:focus {
		outline: none;
		border-color: color-mix(in srgb, var(--color-primary) 60%, transparent);
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

	.err {
		margin-top: 1rem;
		display: flex;
		align-items: center;
		gap: 0.4rem;
		font-size: 13px;
		color: #ff9ea1;
	}

	.next {
		margin-top: 2.25rem;
		display: inline-flex;
		align-items: center;
		gap: 0.45rem;
		font: inherit;
		font-size: 15px;
		padding: 0.65rem 1.3rem;
		border-radius: 10px;
		border: 1px solid var(--color-border);
		background: none;
		color: var(--color-foreground);
		cursor: pointer;
	}

	.next:hover:not(:disabled) {
		background: color-mix(in srgb, var(--color-foreground) 7%, transparent);
	}

	.next:disabled {
		opacity: 0.5;
		cursor: default;
	}
</style>
