<script lang="ts">
	/**
	 * The owner's notes on how the assistant should talk to them.
	 *
	 * Replaced the persona picker on 2026-09-25 (migration 0035). The picker
	 * offered four stored personas and whichever one was chosen REPLACED the
	 * assistant's character in the prompt. The written character was not one of
	 * the four, so choosing anything switched it off. Now there is one
	 * character, and these notes are added beneath it; where the two disagree
	 * about manner, the notes win (see `prompt::style_notes_block`).
	 *
	 * A box that had chosen a persona found that persona's text here after the
	 * upgrade, so nobody's assistant changed voice without them seeing why.
	 */
	import { onMount } from 'svelte';
	import { getAssistantProfile, updateAssistantProfile } from '$lib/api/client';

	/** Mirrors `STYLE_NOTES_MAX_CHARS` in assistant_profile.rs. */
	const MAX_CHARS = 1000;

	let notes = $state('');
	let saved = $state('');
	let loading = $state(true);
	let saving = $state(false);
	let saveError = $state<string | null>(null);

	onMount(load);

	async function load() {
		try {
			const profile = await getAssistantProfile<{ style_notes?: string | null }>();
			saved = profile.style_notes ?? '';
			notes = saved;
		} catch (error) {
			console.error('Failed to load style notes:', error);
		} finally {
			loading = false;
		}
	}

	async function save() {
		const next = notes.trim();
		// Show what is saved: the server trims, so the box does too.
		notes = next;
		if (next === saved.trim() || saving) return;

		saving = true;
		saveError = null;
		const previous = saved;
		saved = next; // optimistic

		try {
			// An empty string clears the notes; the server stores it as NULL.
			await updateAssistantProfile({ style_notes: next });
		} catch (error) {
			saved = previous;
			notes = previous;
			saveError = "Your server couldn't save these notes. It kept your earlier ones.";
			console.error('Failed to save style notes:', error);
		} finally {
			saving = false;
		}
	}
</script>

<div class="bg-surface border border-border rounded-lg">
	<div class="flex items-center justify-between px-4 py-3 border-b border-border">
		<h2 class="text-sm font-medium text-foreground">Style</h2>
		{#if saveError}
			<span class="text-xs text-error">{saveError}</span>
		{/if}
	</div>

	<div class="p-4 space-y-2">
		<div class="text-sm font-medium text-foreground">
			How should your assistant talk to you?
		</div>
		<textarea
			bind:value={notes}
			onblur={save}
			disabled={loading || saving}
			maxlength={MAX_CHARS}
			rows="4"
			placeholder="Short answers unless I ask for more. Push back when I'm wrong. No lists for simple questions."
			class="w-full px-3 py-2 bg-background border border-border rounded-md text-sm text-foreground placeholder:text-foreground-subtle hover:border-border-strong focus:border-border-strong focus:outline-none transition-colors disabled:opacity-60 resize-y"
		></textarea>
		<div class="flex items-start justify-between gap-4 text-xs text-foreground-subtle">
			<p>
				Your assistant has its own character. What you write here adds to it, and
				where they differ on tone, yours wins. Leave it empty to keep the
				character as it is.
			</p>
			<span class="shrink-0 tabular-nums">{notes.length}/{MAX_CHARS}</span>
		</div>
	</div>
</div>
