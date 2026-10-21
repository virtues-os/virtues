<script lang="ts">
	/**
	 * The owner's notes on how the assistant should talk to them. They sit
	 * beneath the assistant's own character in the prompt, and where the two
	 * disagree about manner the notes win (`prompt::style_notes_block`).
	 */
	// By path, not the `$lib` barrel, which the component tests cannot load.
	import Button from '$lib/components/Button.svelte';
	import Textarea from '$lib/components/Textarea.svelte';
	import { updateAssistantProfile } from '$lib/api/client';

	/** Mirrors `STYLE_NOTES_MAX_CHARS` in assistant_profile.rs. */
	const MAX_CHARS = 1000;
	/** The counter stays out of the way until the limit is close. */
	const COUNT_FROM = 900;

	let {
		notes = $bindable(''),
		assistantName = 'Ari',
		disabled = false,
	}: {
		/** The saved notes. Changes only once the server has them. */
		notes?: string;
		assistantName?: string;
		disabled?: boolean;
	} = $props();

	let draft = $derived(notes);
	let error = $state<string | null>(null);
	// The field is held while a save is out, so nothing typed meanwhile is
	// replaced when the saved notes come back.
	let saving = $state(false);

	// `Textarea` has no `maxlength`, so the cap is held here.
	function clamp() {
		if (draft.length > MAX_CHARS) draft = draft.slice(0, MAX_CHARS);
	}

	/** Saves what the field holds. A failure leaves the person's words in the
	 *  field for Try again, and rejects so the Textarea marks itself unsaved. */
	async function commit(value: string) {
		const next = value.trim();
		// Show what is saved: the server trims, so the field does too.
		draft = next;
		if (next === notes) {
			error = null;
			return;
		}

		saving = true;
		try {
			// An empty string clears the notes; the server stores it as NULL.
			await updateAssistantProfile({ style_notes: next });
			notes = next;
			error = null;
		} catch (e) {
			error = "Your server couldn't save these notes. It kept your earlier ones.";
			throw e;
		} finally {
			saving = false;
		}
	}

	function retry() {
		// The error line already says what failed.
		commit(draft).catch(() => {});
	}
</script>

<section>
	<h2 class="settings-label"><label for="style-notes">How it talks to you</label></h2>
	<div class="notes-field">
		<Textarea
			id="style-notes"
			bind:value={draft}
			rows={4}
			placeholder="Short answers unless I ask for more. Push back when I'm wrong. No lists for simple questions."
			autoSave
			onSave={commit}
			oninput={clamp}
			disabled={disabled || saving}
		/>
	</div>
	{#if error}
		<p class="field-error" role="alert">{error}</p>
		<div>
			<Button variant="secondary" size="sm" loading={saving} onclick={retry}>
				Try again
			</Button>
		</div>
	{/if}
	<!-- The counter shares the hint's line, so appearing moves nothing. -->
	<div class="under">
		<p class="sec-hint">
			{assistantName} has a manner of its own. Where yours differs, yours wins.
		</p>
		{#if draft.length > COUNT_FROM}
			<p class="count">
				{draft.length.toLocaleString('en-US')} of {MAX_CHARS.toLocaleString('en-US')}
			</p>
		{/if}
	</div>
</section>

<style>
	section {
		display: flex;
		flex-direction: column;
		gap: 12px;
	}

	.notes-field,
	.under {
		max-width: 60ch;
	}

	.under {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 16px;
	}

	.sec-hint,
	.count,
	.field-error {
		margin: 0;
		font-size: 12px;
	}

	.sec-hint,
	.count {
		color: var(--color-foreground-subtle);
	}

	.count {
		flex-shrink: 0;
		font-variant-numeric: tabular-nums;
	}

	.field-error {
		max-width: 60ch;
		color: var(--color-error);
	}
</style>
