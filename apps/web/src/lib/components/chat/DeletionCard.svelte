<script lang="ts">
	/**
	 * DeletionCard
	 *
	 * A deletion the assistant proposed (`delete_data`), in the reply where it
	 * was proposed. Framed, because this is where you answer
	 * (agents/build/design.md, "Framed and unframed blocks"). The tool's
	 * result carries only the deletion's id and title; what it covers is read
	 * from your server each time the card is drawn, so the sample rows are
	 * never stored with the chat and go when the rows do.
	 *
	 *   proposed   Delete …?  what it covers, what stays · Not now · Delete
	 *   running    Deleting …
	 *   done       Deleted …  what went, what stays · Let them be collected again
	 *   cancelled  Not deleted
	 *   failed     why, and Try again
	 *
	 * Delete needs the app's owner proof, as an admin command's Allow does.
	 */
	import Button from "$lib/components/Button.svelte";
	import Card from "$lib/components/Card.svelte";
	import TextAction from "$lib/components/TextAction.svelte";
	import {
		allowDeletion,
		cancelDeletion,
		getDeletion,
		liftTombstones,
		type Deletion,
		type DeletionTarget,
	} from "$lib/api/deletions";
	import { formatDate } from "$lib/utils/dateUtils";
	import { alsoRemoved, span, stillKept } from "$lib/settings/deletions";

	interface Props {
		deletionId: string;
		/** The tool's title for it, until your server answers. */
		summary: string;
	}

	let { deletionId, summary }: Props = $props();

	let deletion = $state<Deletion | null>(null);
	let loadError = $state("");
	let actionError = $state("");
	let working = $state(false);

	const status = $derived(deletion?.status ?? "loading");
	const title = $derived(deletion?.summary || summary);
	const targets = $derived((deletion?.targets ?? []).filter((t) => t.count > 0));
	const receipt = $derived(deletion?.receipt ?? {});
	const lifted = $derived(receipt.tombstones_lifted !== undefined);

	let timer: ReturnType<typeof setTimeout> | undefined;

	async function load() {
		try {
			deletion = await getDeletion(deletionId);
			loadError = "";
		} catch (e) {
			loadError = e instanceof Error ? e.message : "Couldn't reach your server.";
		}
		// Only a running deletion changes on its own.
		if (deletion?.status === "running") timer = setTimeout(load, 1_000);
	}

	$effect(() => {
		void deletionId;
		load();
		return () => clearTimeout(timer);
	});

	/** Run one of the card's actions, showing why if your server refuses. */
	async function act(action: () => Promise<Deletion | void>) {
		if (working) return;
		working = true;
		actionError = "";
		try {
			const next = await action();
			if (next) deletion = next;
			clearTimeout(timer);
			if (deletion?.status === "running") timer = setTimeout(load, 1_000);
		} catch (e) {
			actionError = e instanceof Error ? e.message : "Your server didn't do that. Try again.";
		} finally {
			working = false;
		}
	}

	const allow = () => act(() => allowDeletion(deletionId));
	const cancel = () => act(() => cancelDeletion(deletionId));
	const lift = () =>
		act(async () => {
			await liftTombstones(deletionId);
			await load();
		});

	// ── Words ────────────────────────────────────────────────────────────────

	const count = (n: number | undefined) => (n ?? 0).toLocaleString();

	/** "4 rows · March 3 to March 7, 2026". */
	function factsOf(target: DeletionTarget): string {
		const rows = `${count(target.count)} ${target.count === 1 ? "row" : "rows"}`;
		const when = span(target.earliest, target.latest);
		return when ? `${rows} · ${when}` : rows;
	}
	const removedText = $derived(alsoRemoved(receipt));
	const keptText = $derived(stillKept(receipt));
</script>

<Card class="deletion" padding="md">
	<div class="deletion-body" data-state={status} aria-live="polite">
		{#if status === "loading"}
			<p class="title">{title}</p>
			<p class="note">{loadError || "Checking what this covers…"}</p>
		{:else if status === "proposed" || status === "failed"}
			<p class="title">Delete {title}?</p>

			{#if deletion?.preview_error}
				<p class="note error">Your server couldn't count what this covers: {deletion.preview_error}</p>
			{:else if targets.length === 0}
				<p class="note">Nothing matches this anymore, so there is nothing to delete.</p>
			{:else}
				<ul class="targets">
					{#each targets as target (target.table)}
						<li>
							<div class="target-line">
								<span class="label">{target.label}</span>
								<span class="facts">{factsOf(target)}</span>
							</div>
							{#each target.samples as sample}
								{#if sample.preview || sample.label}
									<p class="sample">{sample.preview || sample.label}</p>
								{/if}
							{/each}
						</li>
					{/each}
				</ul>
				<p class="note">
					Your server removes these with their search entries, their files and the raw copy each sync
					kept. It refuses them if a sync sends them again.
				</p>
				<p class="note">
					Copies stay in backups made before today and in pages or chats that already quote them.
				</p>
			{/if}

			{#if status === "failed" && deletion?.error}
				<p class="note error">{deletion.error}</p>
			{/if}
			{#if actionError}
				<p class="note error">{actionError}</p>
			{/if}

			<div class="actions">
				<TextAction quiet onclick={cancel} disabled={working}>Not now</TextAction>
				{#if targets.length > 0 && !deletion?.preview_error}
					<Button variant="danger" onclick={allow} loading={working}>
						{status === "failed" ? "Try again" : "Delete"}
					</Button>
				{/if}
			</div>
		{:else if status === "running"}
			<p class="title">Deleting {title}…</p>
			<p class="note">You can keep working. This card shows what was removed when it finishes.</p>
		{:else if status === "done"}
			<p class="title">
				Deleted {title}
				{#if deletion?.finished_at}
					<time datetime={deletion.finished_at}>{formatDate(deletion.finished_at)}</time>
				{/if}
			</p>
			{#if receipt.tables?.length}
				<ul class="targets">
					{#each receipt.tables as table (table.table)}
						<li>
							<div class="target-line">
								<span class="label">{table.label}</span>
								<span class="facts">{count(table.records)} {table.records === 1 ? "row" : "rows"}</span>
							</div>
						</li>
					{/each}
				</ul>
			{:else}
				<p class="note">Nothing matched by the time it ran, so nothing was removed.</p>
			{/if}
			{#if removedText}
				<p class="note">Also removed: {removedText}.</p>
			{/if}
			{#if lifted}
				<p class="note">You let these be collected again. A source that still has them brings them back on its next sync.</p>
			{:else if receipt.tombstones}
				<p class="note">
					Your server refuses these if a sync sends them again.
					<TextAction inline quiet onclick={lift} disabled={working}>Let them be collected again</TextAction>
				</p>
			{/if}
			{#if receipt.raw_missing}
				<p class="note">
					Your server found no raw copy for {count(receipt.raw_missing)} of them.
				</p>
			{/if}
			{#if keptText}
				<p class="note">Copies stay {keptText}.</p>
			{/if}
			{#if actionError}
				<p class="note error">{actionError}</p>
			{/if}
		{:else}
			<p class="title muted">Not deleted: {title}</p>
		{/if}
	</div>
</Card>

<style>
	:global(.v-card.deletion) {
		margin: 0.75rem 0;
		max-width: 40em;
	}

	.deletion-body {
		font-family: var(--font-sans);
		font-size: 13px;
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}

	.title {
		margin: 0;
		font-size: 14px;
		font-weight: 500;
		color: var(--color-foreground);
		overflow-wrap: anywhere;
	}

	.title.muted {
		font-weight: 400;
		color: var(--color-foreground-muted);
	}

	.title time {
		margin-left: 0.5rem;
		font-size: 12px;
		font-weight: 400;
		font-variant-numeric: tabular-nums;
		color: var(--color-foreground-subtle);
	}

	.targets {
		margin: 0.5rem 0 0;
		padding: 0;
		list-style: none;
	}

	.targets li + li {
		margin-top: 0.375rem;
	}

	/* On a narrow pane the count and dates drop under the name. */
	.target-line {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		justify-content: space-between;
		gap: 0 1rem;
	}

	.label {
		color: var(--color-foreground);
	}

	.facts {
		font-variant-numeric: tabular-nums;
	}

	.sample {
		margin: 0.125rem 0 0;
		font-family: var(--font-serif);
		font-size: 15px;
		line-height: 1.4;
		color: var(--color-foreground-muted);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.note {
		margin: 0.5rem 0 0;
	}

	.note.error {
		color: var(--color-error);
	}

	.actions {
		display: flex;
		align-items: center;
		justify-content: flex-end;
		gap: 1rem;
		margin-top: 0.75rem;
	}
</style>
