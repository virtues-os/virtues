<script lang="ts">
	/**
	 * History — every edit to the wiki's articles, the record's and your own.
	 *
	 * This is the room that makes "keep this updated" a safe thing to switch on.
	 * Articles are opt-in and maintenance is a second, separate consent; the
	 * bargain for granting it is that every resulting edit is visible and
	 * revertible. Without this page the machine rewrites prose about your life
	 * where nobody can see it, which is the failure the whole opt-in design
	 * exists to avoid.
	 *
	 * Entries read as sentences, not as commits: "the record rewrote Sarah,
	 * Tuesday". The diff is available underneath for anyone who wants it, but
	 * the default posture is a feed you can skim and ignore. Each entry is one
	 * edit: the diff from the version before it (`before_version`) to the
	 * version it made, so undoing it means putting `before_version` back.
	 */
	import { onMount } from 'svelte';
	import TextAction from '$lib/components/TextAction.svelte';
	import {
		listHistory,
		getArticleHistory,
		revertArticle,
		editKey,
		type HistoryEntry,
		type ArticleRevision
	} from '$lib/wiki/api';

	let entries = $state<HistoryEntry[]>([]);
	let loading = $state(true);
	/** entry key → its article's revisions, fetched only when someone opens one. */
	let opened = $state<Record<string, ArticleRevision[]>>({});
	let openKey = $state<string | null>(null);
	/** The entry being reverted, and the outcome once it is done. */
	let reverting = $state<string | null>(null);
	let reverted = $state<Record<string, string>>({});

	/** Two entries for one page can share a version_number, never the pair (`editKey`). */
	const key = (e: HistoryEntry) => `${e.subject_type}/${e.subject_id}/${editKey(e)}`;

	onMount(async () => {
		try {
			entries = await listHistory(50);
		} finally {
			loading = false;
		}
	});

	async function toggle(e: HistoryEntry) {
		const k = key(e);
		if (openKey === k) {
			openKey = null;
			return;
		}
		openKey = k;
		if (!opened[k]) {
			opened[k] = await getArticleHistory(e.subject_type, e.subject_id);
		}
	}

	/**
	 * Undo the edit an entry shows, by putting back the version it started
	 * from.
	 *
	 * Reverting adds a version rather than rewinding one, so this is not a
	 * destructive act and does not ask twice. When the page changed, the feed
	 * reloads, because the revert is itself an edit and belongs in it. The
	 * line afterwards is your server's own, so it says when the page already
	 * read that way, and when your server couldn't save it yet. A feed that
	 * doesn't reload leaves that line as it is: the put-back happened.
	 *
	 * A server that sends no `before_version` (one older than this app) can't
	 * name the version to put back, so its entries offer none.
	 */
	async function revert(e: HistoryEntry) {
		if (e.before_version == null) return;
		const k = key(e);
		reverting = k;
		let changed = false;
		try {
			const outcome = await revertArticle(e.subject_type, e.subject_id, e.before_version);
			reverted[k] = outcome.message || 'Your server put that version back.';
			changed = outcome.changed;
		} catch {
			reverted[k] = "Your server couldn't put that back. The old version is still in this list, so try again.";
		} finally {
			reverting = null;
		}
		if (!changed) return;
		try {
			entries = await listHistory(50);
		} catch {
			// The list on screen stays; the put-back's line already says how it went.
		}
	}

	function when(iso: string): string {
		const d = new Date(iso);
		return d.toLocaleDateString('en-US', { month: 'long', day: 'numeric' });
	}

	/** "the record" reads better than "ai" and is more accurate than "system". */
	function who(author: string): string {
		return author === 'ai' ? 'The record' : 'You';
	}
</script>

{#if loading}
	<p class="quiet">Loading…</p>
{:else if entries.length === 0}
	<p class="quiet">
		The record hasn't rewritten anything yet. The record maintains an article only when
		you ask it to - turn on "Keep this updated" on an article and its edits will
		appear here.
	</p>
{:else}
	<ul class="feed">
		{#each entries as e (key(e))}
			<li class="entry">
				<button type="button" class="line" onclick={() => toggle(e)}>
					<span class="who">{who(e.author)}</span>
					rewrote
					<a class="subject" href={e.route} onclick={(ev) => ev.stopPropagation()}>{e.title}</a>
					<span class="when">{when(e.at)}</span>
				</button>

				{#if openKey === key(e)}
					{@const revs = opened[key(e)] ?? []}
					{@const rev = revs.find((r) => editKey(r) === editKey(e))}
					{#if rev && rev.diff.length}
						<pre class="diff">{#each rev.diff as line}<span class="l {line.kind}">{line.kind === 'add' ? '+' : line.kind === 'del' ? '−' : ' '} {line.text}</span>
{/each}</pre>
					{:else}
						<p class="quiet small">No textual change recorded for this edit.</p>
					{/if}
					{#if reverted[key(e)]}
						<p class="actions">
							<span class="quiet small">{reverted[key(e)]}</span>
						</p>
					{:else if e.before_version != null}
						<p class="actions">
							<TextAction
								loading={reverting === key(e)}
								loadingLabel="Putting back…"
								onclick={() => revert(e)}
							>
								Put the earlier version back
							</TextAction>
						</p>
					{/if}
				{/if}
			</li>
		{/each}
	</ul>
{/if}

<style>
	@reference "../../../app.css";

	.actions {
		margin: 0.5rem 0 0;
	}

	.feed {
		list-style: none;
		margin: 0;
		padding: 0;
	}

	.entry + .entry {
		border-top: 1px solid var(--color-border);
	}

	.line {
		display: block;
		width: 100%;
		padding: 0.5rem 0;
		background: none;
		border: none;
		font: inherit;
		font-size: 0.9375rem;
		text-align: left;
		color: var(--color-foreground);
		cursor: pointer;
	}

	.who {
		font-weight: 500;
	}

	.subject {
		color: var(--color-foreground);
		text-decoration: underline;
		text-underline-offset: 2px;
	}

	.when {
		margin-left: 0.375rem;
		color: var(--color-foreground-subtle);
		font-size: 0.8125rem;
	}

	.diff {
		margin: 0 0 0.75rem;
		padding: 0.5rem 0.625rem;
		border-radius: 4px;
		background: var(--color-surface-raised, rgba(0, 0, 0, 0.03));
		font-family: var(--font-mono, ui-monospace, monospace);
		font-size: 0.75rem;
		line-height: 1.5;
		overflow-x: auto;
		white-space: pre;
	}

	.l {
		display: block;
	}

	.l.add {
		color: var(--color-success, #187d3c);
	}

	.l.del {
		color: var(--color-danger, #b00);
	}

	.l.ctx {
		color: var(--color-foreground-subtle);
	}

	.small {
		font-size: 0.8125rem;
	}
</style>
