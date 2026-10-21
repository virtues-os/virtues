<script lang="ts">
	/**
	 * Memory: the notes the assistant keeps across chats. Every live note
	 * rides in the chat system prompt (chat.rs, the `memory` block), so the
	 * person sees each one and can rewrite or remove it. A note the person edits is theirs: the
	 * memory tool revises and retires only its own (executor.rs,
	 * `execute_update_memory`).
	 *
	 * Edit and remove stay visible on every row. A control that appears only
	 * on hover cannot be reached on a phone.
	 */
	import { onDestroy, onMount, tick } from 'svelte';
	import Button from '$lib/components/Button.svelte';
	import IconButton from '$lib/components/IconButton.svelte';
	import Textarea from '$lib/components/Textarea.svelte';
	import {
		ApiError,
		editAssistantMemory,
		listAssistantMemories,
		retireAssistantMemory,
		type AssistantMemory,
	} from '$lib/api/client';
	import { inComposition } from '$lib/utils/ime';
	import { displayNote, LANE_CAP, LANES, orderNotes } from '$lib/utils/memoryNote';

	let { assistantName = 'Ari' }: { assistantName?: string } = $props();
	const name = $derived(assistantName?.trim() || 'Ari');

	const uid = $props.id();

	/** A removed note can be brought back for this long before the removal is sent. */
	const UNDO_MS = 6000;
	/** The server refuses a note over 500 characters (assistant_memories.rs `edit_memory`). */
	const MAX_CHARS = 500;

	let memories = $state<AssistantMemory[]>([]);
	let loading = $state(true);
	let loadFailed = $state(false);

	/** A note is being edited while it has a draft. The draft stays until the save succeeds. */
	let drafts = $state<Record<number, string>>({});
	let saving = $state<Record<number, true>>({});
	/** Notes removed on screen whose removal has not been sent yet. */
	let removing = $state<Record<number, true>>({});
	/** A removed row keeps its height for the undo window, so the list does not jump. */
	let pinnedHeight = $state<Record<number, number>>({});
	/** A failure, shown beside the note it happened to. */
	let noteError = $state<Record<number, string>>({});

	const timers = new Map<number, ReturnType<typeof setTimeout>>();
	let listEl = $state<HTMLUListElement | null>(null);

	const notes = $derived(orderNotes(memories));
	const fullLanes = $derived(
		LANES.filter(
			(lane) =>
				notes.filter((m) => m.lane === lane.key && !removing[m.id]).length >= LANE_CAP[lane.key],
		),
	);

	onMount(load);

	// Leaving the page sends the removals still waiting out their undo window.
	onDestroy(sendPendingRemovals);

	async function load() {
		loading = true;
		loadFailed = false;
		try {
			memories = await listAssistantMemories();
		} catch {
			loadFailed = true;
		} finally {
			loading = false;
		}
	}

	const fieldId = (id: number) => `${uid}-note-${id}`;

	function row(id: number): HTMLElement | null {
		return listEl?.querySelector<HTMLElement>(`[data-note="${id}"]`) ?? null;
	}

	/** Keyboard focus goes to the row's first control, so it is not lost when the row changes. */
	async function focusRow(id: number) {
		await tick();
		row(id)?.querySelector<HTMLElement>('button')?.focus();
	}

	async function startEdit(m: AssistantMemory) {
		drafts[m.id] = displayNote(m.body);
		delete noteError[m.id];
		await tick();
		const field = document.getElementById(fieldId(m.id)) as HTMLTextAreaElement | null;
		if (!field) return;
		field.maxLength = MAX_CHARS;
		field.focus();
		field.setSelectionRange(field.value.length, field.value.length);
	}

	function cancelEdit(m: AssistantMemory) {
		delete drafts[m.id];
		delete noteError[m.id];
		void focusRow(m.id);
	}

	async function saveEdit(m: AssistantMemory, fromKeyboard = false) {
		const draft = drafts[m.id];
		if (draft === undefined || saving[m.id]) return;
		const body = draft.trim();
		// Unchanged or emptied: there is nothing to save, so the note stays as it was.
		if (!body || body === displayNote(m.body)) {
			delete drafts[m.id];
			delete noteError[m.id];
			if (fromKeyboard) void focusRow(m.id);
			return;
		}

		saving[m.id] = true;
		try {
			const updated = await editAssistantMemory(m.id, body);
			memories = memories.map((x) => (x.id === m.id ? updated : x));
			delete drafts[m.id];
			delete noteError[m.id];
			if (fromKeyboard) void focusRow(m.id);
		} catch {
			noteError[m.id] = "Your server couldn't save this note, so it kept the earlier wording. Press Enter to try again.";
		} finally {
			delete saving[m.id];
		}
	}

	function onFieldKey(e: KeyboardEvent, m: AssistantMemory) {
		if (inComposition(e)) return;
		if (e.key === 'Enter' && !e.shiftKey) {
			e.preventDefault();
			void saveEdit(m, true);
		} else if (e.key === 'Escape') {
			e.preventDefault();
			e.stopPropagation();
			cancelEdit(m);
		}
	}

	function remove(m: AssistantMemory) {
		delete noteError[m.id];
		const height = row(m.id)?.offsetHeight;
		if (height) pinnedHeight[m.id] = height;
		removing[m.id] = true;
		timers.set(
			m.id,
			setTimeout(() => void sendRemoval(m.id), UNDO_MS),
		);
		void focusRow(m.id);
	}

	function undo(id: number) {
		clearTimeout(timers.get(id));
		timers.delete(id);
		delete removing[id];
		delete pinnedHeight[id];
		void focusRow(id);
	}

	async function sendRemoval(id: number) {
		timers.delete(id);
		const note = memories.find((x) => x.id === id);
		if (!note) return;

		// The row leaves the list now. If its Undo had focus, focus moves to a neighbour.
		const hadFocus = row(id)?.contains(document.activeElement) ?? false;
		const index = notes.findIndex((x) => x.id === id);
		const neighbour = notes[index + 1] ?? notes[index - 1];
		memories = memories.filter((x) => x.id !== id);
		delete removing[id];
		delete pinnedHeight[id];
		if (hadFocus && neighbour) void focusRow(neighbour.id);

		try {
			await retireAssistantMemory(id);
		} catch (e) {
			// Already gone on the server (removed from another device): what the person wanted.
			if (e instanceof ApiError && e.status === 404) return;
			memories = [...memories, note];
			noteError[id] = "Your server couldn't remove this note, so it's still here. Try again in a moment.";
		}
	}

	/**
	 * Sends every removal still waiting, as if its undo window had run out. Each
	 * row leaves the list too: a page restored from the back/forward cache must
	 * not offer Undo for a note the server has already retired. After teardown
	 * a failure has nowhere to show; the note stays and appears on the next visit.
	 */
	function sendPendingRemovals() {
		for (const [id, timer] of [...timers]) {
			clearTimeout(timer);
			void sendRemoval(id);
		}
	}
</script>

<svelte:window onpagehide={sendPendingRemovals} />

<section aria-busy={loading}>
	<h2 class="settings-label">Memory</h2>
	<p class="sec-hint">
		What {name} picked up in your chats and keeps in mind for the next one. A note you edit
		becomes yours, and {name} won't change it.
	</p>

	{#if loading}
		<ul class="notes" aria-hidden="true">
			{#each [72, 56, 64] as width (width)}
				<li class="note placeholder">
					<span class="placeholder-bar" style:width="{width}%"></span>
				</li>
			{/each}
		</ul>
	{:else if loadFailed}
		<p class="note-error" role="alert">Your server couldn't load {name}'s memory.</p>
		<div>
			<Button variant="secondary" size="sm" onclick={load}>Try again</Button>
		</div>
	{:else if notes.length === 0}
		<p class="empty">Nothing yet. What {name} decides to keep from your chats appears here.</p>
	{:else}
		<ul class="notes" bind:this={listEl}>
			{#each notes as m (m.id)}
				<li
					class="note"
					data-note={m.id}
					style:min-height={pinnedHeight[m.id] ? `${pinnedHeight[m.id]}px` : undefined}
				>
					{#if removing[m.id]}
						<div class="note-removed">
							<p class="note-status" role="status">Removed.</p>
							<Button variant="ghost" size="sm" onclick={() => undo(m.id)}>Undo</Button>
						</div>
					{:else if drafts[m.id] !== undefined}
						<label class="sr-only" for={fieldId(m.id)}>Note</label>
						<!-- Enter saves, Shift+Enter is a new line, Escape puts the note back.
						     The $lib Textarea takes no key handler, so the keys are read here
						     as they bubble out of it. -->
						<!-- svelte-ignore a11y_no_static_element_interactions -->
						<div class="note-field" onkeydown={(e) => onFieldKey(e, m)}>
							<Textarea
								id={fieldId(m.id)}
								bind:value={drafts[m.id]}
								rows={1}
								autoResize
								delight={false}
								onblur={() => saveEdit(m)}
							/>
						</div>
					{:else}
						<p class="note-text">{displayNote(m.body)}{#if m.author === 'human'}<span class="note-yours">&nbsp;· yours</span>{/if}</p>
						<div class="note-actions">
							<IconButton
								icon="ri:edit-line"
								label="Edit note"
								size="sm"
								onclick={() => startEdit(m)}
							/>
							<IconButton
								icon="ri:close-line"
								label="Remove note"
								size="sm"
								onclick={() => remove(m)}
							/>
						</div>
					{/if}
					{#if noteError[m.id]}
						<p class="note-error" role="alert">{noteError[m.id]}</p>
					{/if}
				</li>
			{/each}
		</ul>
		{#each fullLanes as lane (lane.key)}
			<p class="sec-hint">
				{name} has kept as many notes about {lane.phrase} as it can ({LANE_CAP[lane.key]}). Remove
				one to make room.
			</p>
		{/each}
	{/if}
</section>

<style>
	section {
		display: flex;
		flex-direction: column;
		gap: 12px;
	}

	.sec-hint {
		margin: 0;
		font-size: 12px;
		color: var(--color-foreground-subtle);
		max-width: 60ch;
	}

	.notes {
		display: flex;
		flex-direction: column;
		gap: 12px;
		margin: 0;
		padding: 0;
		list-style: none;
	}

	/* The note, then its two controls; a field, a status, or an error takes
	   the whole row. The text's 24px line is the controls' height, so a
	   one-line note and its buttons share a center. */
	.note {
		display: grid;
		grid-template-columns: minmax(0, 1fr) auto;
		align-items: start;
		column-gap: 12px;
		row-gap: 8px;
	}

	.note-text {
		margin: 0;
		font-size: 15px;
		line-height: 24px;
		color: var(--color-foreground);
		white-space: pre-line;
		overflow-wrap: anywhere;
	}

	.note-yours {
		font-size: 13px;
		color: var(--color-foreground-muted);
	}

	.note-actions {
		display: flex;
		gap: 8px;
	}

	/* Each control's touch halo is 44px; spaced this far apart, the two do
	   not overlap and a tap lands on the one under the thumb. */
	@media (pointer: coarse) {
		.note-actions {
			gap: 20px;
		}
	}

	.note-field,
	.note-removed,
	.note-error {
		grid-column: 1 / -1;
	}

	.note-removed {
		display: flex;
		align-items: center;
		gap: 12px;
	}

	.note-status,
	.empty {
		margin: 0;
		font-size: 13px;
		color: var(--color-foreground-muted);
	}

	.note-error {
		margin: 0;
		font-size: 12px;
		color: var(--color-error);
	}

	/* Loading rows in the list's own shape: one bar on the note's 24px line. */
	.placeholder {
		align-items: center;
		min-height: 24px;
	}

	.placeholder-bar {
		display: block;
		height: 12px;
		border-radius: 6px;
		background: var(--hover-bg);
	}
</style>
