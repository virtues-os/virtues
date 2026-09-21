<script lang="ts">
	/**
	 * Stories — the subjects you name yourself.
	 *
	 * Every other room in this wiki lists things the record produced: days it
	 * segmented, years it partitioned, people it resolved. This one lists
	 * things nothing produced. "Piano & Composition" exists because somebody
	 * decided it mattered, and the record then goes looking for it.
	 *
	 * So the empty state is not an apology for having no data. It is an
	 * invitation, and the examples are there because the idea is easier to
	 * recognise than to explain.
	 */
	import { onMount } from 'svelte';
	import TextAction from '$lib/components/TextAction.svelte';
	import {
		listStories,
		createStory,
		startStoryArticle,
		deleteStory,
		getArticle,
		type StoryApi
	} from '$lib/wiki/api';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';

	let stories = $state<StoryApi[]>([]);
	let loading = $state(true);
	let naming = $state(false);
	let draft = $state('');
	let busy = $state<string | null>(null);
	let failed = $state<string | null>(null);

	const EXAMPLES = [
		'Piano & Composition',
		'Books Written',
		'How I learned to pray',
		'My relationship with animals'
	];

	onMount(load);

	async function load() {
		loading = true;
		stories = await listStories();
		loading = false;
	}

	async function start() {
		const title = draft.trim();
		naming = false;
		draft = '';
		if (!title) return;
		try {
			await createStory(title);
			await load();
		} catch (e) {
			failed = e instanceof Error ? e.message : "Your server couldn't start that. Try again.";
		}
	}

	async function openPage(s: StoryApi) {
		busy = s.id;
		failed = null;
		try {
			if (!s.has_article) await startStoryArticle(s.id);
			// The page editor is where a story's prose lives, same as every
			// other article's — the story room is an index, not a second editor.
			const article = await getArticle('story', s.id);
			await load();
			if (article?.page_id) windowShellStore.openTabFromRoute(`/page/${article.page_id}`);
			else failed = 'That story has no page yet.';
		} catch (e) {
			failed = e instanceof Error ? e.message : "Your server couldn't open that. Try again.";
		} finally {
			busy = null;
		}
	}

	async function remove(s: StoryApi) {
		if (!confirm(`Remove "${s.title}"? Its page goes too.`)) return;
		try {
			await deleteStory(s.id);
			await load();
		} catch (e) {
			failed = e instanceof Error ? e.message : "Your server couldn't remove that. Try again.";
		}
	}

	function span(s: StoryApi): string | null {
		if (!s.started_at && !s.ended_at) return null;
		const y = (d: string | null) => (d ? d.slice(0, 4) : null);
		const a = y(s.started_at);
		const b = y(s.ended_at);
		if (a && b) return a === b ? a : `${a}–${b}`;
		return a ? `from ${a}` : `until ${b}`;
	}
</script>

<div class="mast">
	<h1>Stories</h1>
	<p class="standfirst">
		The parts of your life that are not a day, a year or a person — the ones you would
		name yourself. Name one and the record goes looking for it.
	</p>
</div>

{#if loading}
	<p class="quiet">Loading…</p>
{:else}
	{#if stories.length === 0 && !naming}
		<div class="empty">
			<p>Nothing named yet. People usually start with something like:</p>
			<ul class="examples">
				{#each EXAMPLES as e}
					<li>
						<button
							type="button"
							class="example"
							onclick={() => {
								draft = e;
								naming = true;
							}}>{e}</button
						>
					</li>
				{/each}
			</ul>
		</div>
	{:else}
		<ul class="stories">
			{#each stories as s (s.id)}
				<li>
					<button type="button" class="open" disabled={busy === s.id} onclick={() => openPage(s)}>
						<span class="title">{s.title}</span>
						{#if span(s)}<span class="span">{span(s)}</span>{/if}
					</button>
					{#if s.summary}<p class="summary">{s.summary}</p>{/if}
					<p class="row-actions">
						<span class="quiet">
							{#if busy === s.id}
								Opening…
							{:else if s.has_article}
								has a page
							{:else}
								no page yet
							{/if}
						</span>
						<TextAction quiet onclick={() => remove(s)}>Remove</TextAction>
					</p>
				</li>
			{/each}
		</ul>
	{/if}

	{#if naming}
		<!-- svelte-ignore a11y_autofocus -->
		<input
			class="namer"
			bind:value={draft}
			autofocus
			placeholder="What would you call it?"
			onblur={start}
			onkeydown={(e) => e.key === 'Enter' && start()}
		/>
	{:else}
		<div class="add">
			<TextAction onclick={() => (naming = true)}>Name a story</TextAction>
		</div>
	{/if}

	{#if failed}<p class="failed">{failed}</p>{/if}
{/if}

<style>
	.mast {
		margin-bottom: 1.5rem;
	}

	h1 {
		font-family: var(--font-serif);
		font-size: 1.75rem;
		font-weight: 400;
		margin: 0;
	}

	.standfirst {
		color: var(--color-foreground-subtle);
		margin: 0.35rem 0 0;
		max-width: 38rem;
		line-height: 1.55;
	}

	.empty p {
		margin: 0 0 0.5rem;
		color: var(--color-foreground-subtle);
	}

	.examples {
		list-style: none;
		margin: 0 0 1rem;
		padding: 0;
	}

	.examples li {
		padding: 0.15rem 0;
	}

	.example {
		background: none;
		border: 0;
		padding: 0;
		cursor: pointer;
		font-family: var(--font-serif);
		font-size: 1rem;
		color: var(--color-foreground);
		text-decoration: underline;
		text-decoration-style: dotted;
		text-underline-offset: 3px;
	}

	.stories {
		list-style: none;
		margin: 0;
		padding: 0;
	}

	.stories li {
		padding: 0.75rem 0;
		border-bottom: 1px solid var(--color-border);
	}

	.open {
		background: none;
		border: 0;
		padding: 0;
		cursor: pointer;
		display: flex;
		gap: 0.6rem;
		align-items: baseline;
		text-align: left;
	}

	.title {
		font-family: var(--font-serif);
		font-size: 1.125rem;
		color: var(--color-foreground);
	}

	.open:hover .title {
		text-decoration: underline;
		text-underline-offset: 3px;
	}

	.span {
		font-size: 0.8125rem;
		color: var(--color-foreground-subtle);
		font-variant-numeric: tabular-nums;
	}

	.summary {
		margin: 0.25rem 0 0;
		color: var(--color-foreground-subtle);
		line-height: 1.5;
		max-width: 38rem;
	}

	.row-actions {
		margin: 0.35rem 0 0;
		display: flex;
		gap: 0.75rem;
		align-items: baseline;
	}

	.namer {
		width: 100%;
		max-width: 26rem;
		margin-top: 1rem;
		font-family: var(--font-serif);
		font-size: 1.125rem;
		background: none;
		border: 0;
		border-bottom: 1px solid var(--color-border);
		color: var(--color-foreground);
		padding: 0.25rem 0;
	}

	.add {
		margin-top: 1rem;
	}

	.quiet {
		font-size: 0.8125rem;
		color: var(--color-foreground-subtle);
	}

	.failed {
		color: var(--color-danger, #b00);
		font-size: 0.875rem;
	}
</style>
