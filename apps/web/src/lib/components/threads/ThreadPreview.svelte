<script lang="ts">
	/**
	 * A thread in the Threads door's preview column, in five parts: its tile
	 * and name, where it stands, one note if your server raised one, this
	 * week's time as a small chart, and its next tasks with New task.
	 * Everything else is on the thread's page.
	 */
	import type { ProjectDetail, ProjectSummary, Week } from '$lib/api/client';
	import { Button, IconButton } from '$lib';
	import ProjectGlyph from '$lib/components/ProjectGlyph.svelte';
	import PreviewHead from '$lib/components/threads/PreviewHead.svelte';
	import PreviewNote from '$lib/components/threads/PreviewNote.svelte';
	import ThreadUpNext from '$lib/components/threads/ThreadUpNext.svelte';
	import ThreadWeekBars from '$lib/components/threads/ThreadWeekBars.svelte';
	import { projectColor } from '$lib/sidebar/pin-colors';
	import { notesStore } from '$lib/stores/notes.svelte';
	import { outcomeLabel } from '$lib/utils/projectActions';

	let {
		thread,
		detail,
		week,
		onOpen,
		onStep,
	}: {
		thread: ProjectSummary;
		/** The thread's records, once they have loaded. */
		detail: ProjectDetail | null;
		week: Week | null;
		onOpen: () => void;
		onStep: (by: number) => void;
	} = $props();

	const color = $derived(projectColor(thread));
	const sentence = $derived(thread.current_status || thread.instructions || null);
	const note = $derived(notesStore.about(`/thread/${thread.id}`)[0] ?? null);

	/** Where it sits, said only when it is not an active thread: "Finished Nov 17", "In your backlog". */
	const standing = $derived.by(() => {
		if (thread.archived_at) {
			const day = new Date(thread.archived_at).toLocaleDateString('en-US', { month: 'short', day: 'numeric' });
			return `${outcomeLabel(thread)} ${day}`;
		}
		return thread.is_backlog ? 'In your backlog' : null;
	});

	const days = $derived(week?.threads.find((t) => t.id === thread.id)?.days ?? null);
</script>

<PreviewHead {color} title={thread.name || 'Untitled thread'}>
	{#snippet glyph()}<ProjectGlyph project={thread} size={18} />{/snippet}
	{#snippet actions()}
		<IconButton icon="mynaui:chevron-up" label="Previous thread" onclick={() => onStep(-1)} />
		<IconButton icon="mynaui:chevron-down" label="Next thread" onclick={() => onStep(1)} />
		<Button variant="secondary" onclick={onOpen}>Open</Button>
	{/snippet}
</PreviewHead>

{#if standing}
	<p class="standing">{standing}</p>
{/if}
{#if sentence}
	<p class="stands">{sentence}</p>
{/if}

{#if note}
	<div class="block"><PreviewNote {note} /></div>
{/if}

{#if days}
	<div class="block"><ThreadWeekBars {days} today={week?.today ?? null} {color} /></div>
{/if}

<div class="block"><ThreadUpNext threadId={thread.id} {detail} canAdd={!thread.archived_at} /></div>

<style>
	.stands {
		display: -webkit-box;
		-webkit-line-clamp: 3;
		line-clamp: 3;
		-webkit-box-orient: vertical;
		overflow: hidden;
		margin: 8px 0 0;
		font-family: var(--font-serif);
		font-size: var(--text-body);
		line-height: 24px;
		color: var(--color-foreground-muted);
	}
	.standing {
		margin: 4px 0 0;
		font-size: var(--text-small);
		line-height: 20px;
		color: var(--color-foreground-muted);
	}
	.block {
		margin-top: 24px;
	}
	.block:empty {
		display: none;
	}
</style>
