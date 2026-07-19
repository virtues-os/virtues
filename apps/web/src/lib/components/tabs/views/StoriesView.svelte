<!--
	StoriesView.svelte — PLACEHOLDER.

	The route exists so the destination has a home. The feature does not, yet, and
	this page says so rather than pretending otherwise with an empty-state that
	implies "you have no stories" when the truth is "stories cannot be made".

	What is already built: `wiki_stories` (0038) and the magnet that gathers a
	story's evidence (0039) — verified live, gathering the right records and no
	junk. What is missing is the rendering: turning gathered evidence into a cited
	account you would want to read.

	It waits deliberately. A story is an argument built on events, entities and
	days — so those have to be right first, or the story is a confident essay
	about bad data.
-->

<script lang="ts">
	import type { Tab } from '$lib/tabs/types';
	import { Page } from '$lib';
	import Icon from '$lib/components/Icon.svelte';

	let { tab }: { tab: Tab; active: boolean } = $props();
</script>

<Page title="Stories" description="Context scattered across time" maxWidth="prose">
	<div class="placeholder">
		<Icon icon="ri:git-branch-line" width="28" class="mark" />

		<p class="lede">
			A notebook is a container — you put things in it, and you know what is in it.
			A <strong>story</strong> is a claim.
		</p>

		<ul class="examples">
			<li>How I progressed at the gym over two years</li>
			<li>Why I made that decision</li>
			<li>How I act differently on rainy days</li>
		</ul>

		<p class="body">
			You write the claim; the evidence is gathered from across your life and the
			account is rendered with every assertion linked back to the thing that
			proves it.
		</p>

		<p class="note">
			Not built yet — and waiting on purpose. A story is an argument made out of
			events, people and days, so those have to be right before it is worth
			writing one.
		</p>
	</div>
</Page>

<style>
	.placeholder {
		display: flex;
		flex-direction: column;
		gap: 1.25rem;
		padding: 2rem 0;
		max-width: 34rem;
	}

	.placeholder :global(.mark) {
		color: var(--color-foreground-subtle);
	}

	.lede {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 1.125rem;
		line-height: 1.6;
		color: var(--color-foreground);
		margin: 0;
	}

	.examples {
		margin: 0;
		padding-left: 1.1rem;
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
		color: var(--color-foreground-muted);
		font-style: italic;
	}

	.body {
		margin: 0;
		line-height: 1.6;
		color: var(--color-foreground-muted);
	}

	.note {
		margin: 0;
		padding-top: 1rem;
		border-top: 1px solid var(--color-border);
		font-size: 0.875rem;
		line-height: 1.55;
		color: var(--color-foreground-subtle);
	}
</style>
