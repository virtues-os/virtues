<script lang="ts">
	/**
	 * Notes — the margin of a subject's page.
	 *
	 * A note is prose written for later: a correction, an observation, something
	 * the article does not yet say. You can leave one yourself; eventually the
	 * day-summary pass will leave cited ones. Either way this rail is where they
	 * wait, and accepting or dismissing is how they leave.
	 *
	 * A machine note always carries its sources, and that is the whole reason
	 * the design works: a note saying "Sarah may have moved — group thread, Jul
	 * 12" is useful EVEN WHEN WRONG, because the link makes it checkable in
	 * seconds. So the citations are rendered as part of the note, not tucked
	 * behind a disclosure — reading a note and checking it should be one motion.
	 *
	 * Nothing here writes to the record. Accepting a note closes it and hands
	 * the editing back to you; the machine's only channel in is the note itself.
	 */
	import { onMount } from 'svelte';
	import WikiCollapsibleSection from './WikiCollapsibleSection.svelte';
	import { listNotes, createNote, resolveNote, type WikiNote } from '$lib/wiki/api';

	interface Props {
		subjectType: string;
		subjectId: string;
	}

	let { subjectType, subjectId }: Props = $props();

	let notes = $state<WikiNote[]>([]);
	let loaded = $state(false);
	let draft = $state('');
	let busy = $state(false);
	let failed = $state<string | null>(null);

	async function load() {
		try {
			notes = await listNotes(subjectType, subjectId);
		} catch {
			notes = [];
		} finally {
			loaded = true;
		}
	}

	onMount(load);

	async function add() {
		const body = draft.trim();
		if (!body) return;
		busy = true;
		failed = null;
		try {
			await createNote(subjectType, subjectId, body);
			draft = '';
			await load();
		} catch (e) {
			failed = e instanceof Error ? e.message : 'Could not save that note';
		} finally {
			busy = false;
		}
	}

	async function close(id: number, resolution: 'accepted' | 'dismissed') {
		busy = true;
		failed = null;
		try {
			await resolveNote(id, resolution);
			await load();
		} catch (e) {
			failed = e instanceof Error ? e.message : 'Could not close that note';
		} finally {
			busy = false;
		}
	}

	function refs(note: WikiNote): string[] {
		return Array.isArray(note.source_refs) ? (note.source_refs as string[]) : [];
	}
</script>

{#if loaded}
	<section class="section" id="notes">
		<WikiCollapsibleSection title="Notes" count={notes.length} defaultOpen={notes.length > 0}>
			{#if notes.length === 0}
				<p class="quiet empty">
					Nothing in the margin. Notes are things to come back to — a
					correction, something the article should say.
				</p>
			{/if}

			<ul class="notes">
				{#each notes as note (note.id)}
					<li class="note" class:machine={note.author === 'ai'}>
						<p class="body">{note.body}</p>

						{#if refs(note).length}
							<p class="sources">
								{#each refs(note) as r, i}
									<a href={r}>source{refs(note).length > 1 ? ` ${i + 1}` : ''}</a
									>{#if i < refs(note).length - 1}<span class="sep">·</span>{/if}
								{/each}
							</p>
						{/if}

						<p class="actions">
							<span class="by">{note.author === 'ai' ? 'From the record' : 'You'}</span>
							<button type="button" disabled={busy} onclick={() => close(note.id, 'accepted')}>
								Accept
							</button>
							<button type="button" disabled={busy} onclick={() => close(note.id, 'dismissed')}>
								Dismiss
							</button>
						</p>
					</li>
				{/each}
			</ul>

			<form
				class="add"
				onsubmit={(e) => {
					e.preventDefault();
					void add();
				}}
			>
				<input
					type="text"
					bind:value={draft}
					disabled={busy}
					placeholder="Leave a note…"
					aria-label="Leave a note"
				/>
			</form>

			{#if failed}
				<p class="failed">{failed}</p>
			{/if}
		</WikiCollapsibleSection>
	</section>
{/if}

<style>
	@reference "../../../app.css";

	.notes {
		list-style: none;
		margin: 0;
		padding: 0;
	}

	.note {
		padding: 0.5rem 0;
		border-top: 1px solid var(--color-border);
	}

	.note:first-child {
		border-top: none;
	}

	/* A machine note is marked, quietly. You should be able to tell at a glance
	   who is talking without the page shouting about it. */
	.note.machine {
		border-left: 2px solid var(--color-border);
		padding-left: 0.625rem;
	}

	.body {
		margin: 0;
		font-size: 0.875rem;
		line-height: 1.5;
	}

	.sources {
		margin: 0.25rem 0 0;
		font-size: 0.75rem;
	}

	.sources a {
		color: var(--color-foreground-subtle);
		text-decoration: underline;
		text-underline-offset: 2px;
	}

	.sep {
		margin: 0 0.25rem;
		color: var(--color-foreground-subtle);
	}

	.actions {
		display: flex;
		align-items: center;
		gap: 0.625rem;
		margin: 0.375rem 0 0;
		font-size: 0.75rem;
	}

	.by {
		color: var(--color-foreground-subtle);
	}

	.actions button {
		background: none;
		border: none;
		padding: 0;
		font: inherit;
		color: var(--color-accent, currentColor);
		text-decoration: underline;
		text-underline-offset: 2px;
		cursor: pointer;
	}

	.actions button:disabled {
		opacity: 0.5;
		cursor: default;
	}

	.add {
		margin-top: 0.5rem;
	}

	.add input {
		width: 100%;
		padding: 0.25rem 0;
		border: none;
		border-bottom: 1px solid var(--color-border);
		background: transparent;
		font-size: 0.8125rem;
		color: var(--color-foreground);
	}

	.add input:focus {
		outline: none;
		border-bottom-color: var(--color-foreground-subtle);
	}

	.empty {
		font-size: 0.8125rem;
		margin: 0 0 0.5rem;
	}

	.failed {
		margin: 0.375rem 0 0;
		font-size: 0.75rem;
		color: var(--color-danger, #b00);
	}
</style>
