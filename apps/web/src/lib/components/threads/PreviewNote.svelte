<script lang="ts">
	/**
	 * A note in the preview column: the accent dot, the sentence, and "Why
	 * this?" after it. No box: the column is already a quiet place, and the
	 * dot is the one color that means "now". The note's own page holds its
	 * action and "Not now".
	 */
	import type { Note } from '$lib/api/client';
	import TextAction from '$lib/components/TextAction.svelte';
	import { openInsight } from '$lib/components/threads/NoteBox.svelte';

	let { note }: { note: Note } = $props();
</script>

<p class="note">
	<span class="mark" aria-hidden="true"></span>
	<span>{note.text} <TextAction inline onclick={() => openInsight(note)}>Why this?</TextAction></span>
</p>

<style>
	.note {
		display: flex;
		gap: 12px;
		margin: 0;
		font-size: var(--text-label);
		line-height: 20px;
		color: var(--color-foreground);
	}
	.mark {
		flex: none;
		width: 6px;
		height: 6px;
		margin-top: 8px;
		border-radius: 50%;
		background: var(--color-primary);
	}
</style>
