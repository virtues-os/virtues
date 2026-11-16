<script lang="ts">
	/**
	 * Things: what goes on in your life and you named yourself.
	 *
	 * Every other room in this wiki lists what your record produced: days it
	 * segmented, years it partitioned, people it resolved. This one lists what
	 * nothing produced. "Sailing" is here because someone named it, and its
	 * page keeps the projects and tasks they put under it.
	 *
	 * The empty state is an invitation with examples, because a thing is
	 * easier to recognize than to explain.
	 */
	import { onMount } from 'svelte';
	import TextAction from '$lib/components/TextAction.svelte';
	import { thingsStore } from '$lib/stores/things.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import type { ThingApi } from '$lib/wiki/api';

	let naming = $state(false);
	let draft = $state('');
	let failed = $state<string | null>(null);

	const EXAMPLES = ['The garden', 'Sailing', 'The bike', 'Guitar'];

	onMount(() => void thingsStore.load());

	async function name() {
		const title = draft.trim();
		naming = false;
		draft = '';
		if (!title) return;
		failed = null;
		try {
			await thingsStore.create(title);
		} catch (e) {
			failed = e instanceof Error ? e.message : "Your server couldn't name that. Try again.";
		}
	}

	function open(t: ThingApi) {
		windowShellStore.openTabFromRoute(`/thing/${t.id}`);
	}

	function span(t: ThingApi): string | null {
		if (!t.started_at && !t.ended_at) return null;
		const y = (d: string | null) => (d ? d.slice(0, 4) : null);
		const a = y(t.started_at);
		const b = y(t.ended_at);
		if (a && b) return a === b ? a : `${a}–${b}`;
		return a ? `from ${a}` : `until ${b}`;
	}
</script>

<div class="mast">
	<h1>Things</h1>
	<p class="standfirst">
		The parts of your life that go on and can't be finished, like a garden, a bike or guitar.
		Put projects and tasks under one, and its page keeps them together.
	</p>
</div>

{#if !thingsStore.loaded && thingsStore.loading}
	<p class="quiet">Loading…</p>
{:else if !thingsStore.loaded && thingsStore.error}
	<p class="quiet">
		{thingsStore.error}
		<TextAction inline onclick={() => thingsStore.load()}>Try again</TextAction>
	</p>
{:else}
	{#if thingsStore.things.length === 0 && !naming}
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
		<ul class="things">
			{#each thingsStore.things as t (t.id)}
				<li>
					<button type="button" class="open" onclick={() => open(t)}>
						<span class="title">{t.title}</span>
						{#if span(t)}<span class="span">{span(t)}</span>{/if}
					</button>
					{#if t.summary}<p class="summary">{t.summary}</p>{/if}
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
			aria-label="Name a thing"
			placeholder="What would you call it?"
			onblur={name}
			onkeydown={(e) => e.key === 'Enter' && name()}
		/>
	{:else}
		<div class="add">
			<TextAction onclick={() => (naming = true)}>Name a thing</TextAction>
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
		margin: 0.5rem 0 0;
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
		padding: 0.25rem 0;
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

	.things {
		list-style: none;
		margin: 0;
		padding: 0;
	}

	.things li {
		padding: 0.75rem 0;
		border-bottom: 1px solid var(--color-border);
	}

	.open {
		background: none;
		border: 0;
		padding: 0;
		cursor: pointer;
		display: flex;
		gap: 0.75rem;
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
		font-size: 0.875rem;
		color: var(--color-foreground-subtle);
	}

	.failed {
		color: var(--color-error);
		font-size: 0.875rem;
	}
</style>
