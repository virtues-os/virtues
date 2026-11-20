<script lang="ts" module>
	import type { Note } from '$lib/api/client';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';

	/** Open a note's own page, where it says why your server raised it. */
	export function openInsight(n: Note) {
		windowShellStore.openTabFromRoute(`/insight/${encodeURIComponent(n.key)}`, { label: `Insight about ${n.about}`, focusExisting: true });
	}
</script>

<script lang="ts">
	/**
	 * A note on a task's or a thread's page: the sentence, its one action,
	 * "Why this?" and "Not now". The Threads door's preview shows a note as one line instead
	 * (PreviewNote).
	 */
	import { Button } from '$lib';
	import TextAction from '$lib/components/TextAction.svelte';
	import { notesStore } from '$lib/stores/notes.svelte';

	/** `here` is the page it sits on: an action that would open that page is left out. */
	let { note, here }: { note: Note; here?: string } = $props();
</script>

<div class="note">
	<p class="note-text"><span class="mark" aria-hidden="true"></span>{note.text}</p>
	<div class="note-acts">
		{#if note.action_route !== here}
			<Button
				variant="secondary"
				onclick={() => windowShellStore.openTabFromRoute(note.action_route, { focusExisting: true })}
			>
				{note.action}
			</Button>
		{/if}
		<TextAction onclick={() => openInsight(note)}>Why this?</TextAction>
		<TextAction onclick={() => notesStore.dismiss(note)}>Not now</TextAction>
	</div>
</div>

<style>
	.note {
		padding: 12px 16px;
		border-radius: var(--radius-md);
		background: var(--fill-bg);
	}
	.note-text {
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
	.note-acts {
		display: flex;
		min-height: 28px;
		align-items: center;
		flex-wrap: wrap;
		gap: 8px 16px;
		margin: 12px 0 0 16px;
	}
	.note-acts > :global(*) {
		white-space: nowrap;
	}
</style>
