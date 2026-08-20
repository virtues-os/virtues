<script lang="ts">
	/**
	 * History — every edit the record made to its own prose.
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
	 * the default posture is a feed you can skim and ignore.
	 */
	import { onMount } from 'svelte';
	import { listHistory, getArticleHistory, type HistoryEntry, type ArticleRevision } from '$lib/wiki/api';

	let entries = $state<HistoryEntry[]>([]);
	let loading = $state(true);
	/** subject key → its revisions, fetched only when someone opens one. */
	let opened = $state<Record<string, ArticleRevision[]>>({});
	let openKey = $state<string | null>(null);

	const key = (e: HistoryEntry) => `${e.subject_type}/${e.subject_id}/${e.version_number}`;

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
		Nothing has been rewritten yet. Articles are only maintained when you ask
		them to be — turn on "Keep this updated" on an article and its edits will
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
					{@const rev = revs.find((r) => r.version_number === e.version_number)}
					{#if rev && rev.diff.length}
						<pre class="diff">{#each rev.diff as line}<span class="l {line.kind}">{line.kind === 'add' ? '+' : line.kind === 'del' ? '−' : ' '} {line.text}</span>
{/each}</pre>
					{:else}
						<p class="quiet small">No textual change recorded for this edit.</p>
					{/if}
				{/if}
			</li>
		{/each}
	</ul>
{/if}

<style>
	@reference "../../../app.css";

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
