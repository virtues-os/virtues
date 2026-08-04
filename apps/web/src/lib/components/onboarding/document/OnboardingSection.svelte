<!--
  OnboardingSection — one chapter of the editorial document.

  Renders a <section id> (observed by OnboardingToc for scroll-spy + jumps), a
  mono kicker, a serif <h2>, and the body. Rises + fades in when it mounts —
  which, because the shell gates sections behind {#if}, is exactly the moment
  the previous chapter completes and the next "streams in". Under reduced
  motion the entrance is instant.

  The content column is position:relative so <Marginalia> can hang off its edge.
-->
<script lang="ts">
	import type { Snippet } from "svelte";
	import { fly } from "svelte/transition";
	import { expoOut } from "svelte/easing";

	interface Props {
		id: string;
		kicker?: string;
		title: string;
		reduced?: boolean;
		children: Snippet;
	}

	let { id, kicker, title, reduced = false, children }: Props = $props();
</script>

<section
	{id}
	class="section"
	in:fly={{ y: reduced ? 0 : 18, duration: reduced ? 0 : 560, easing: expoOut }}
>
	{#if kicker}
		<p class="kicker">{kicker}</p>
	{/if}
	<h2 class="title">{title}</h2>
	<div class="body">
		{@render children()}
	</div>
</section>

<style>
	@reference "../../../../app.css";

	.section {
		position: relative;
		padding-block: 4.5rem;
	}

	.section + :global(.section) {
		border-top: 1px solid var(--color-border-subtle);
	}

	.kicker {
		font-family: var(--font-mono);
		font-size: 0.6875rem;
		color: var(--color-foreground-subtle);
		margin: 0 0 1rem;
	}

	.title {
		font-family: var(--font-serif);
		font-weight: 400;
		font-size: clamp(1.75rem, 4vw, 2.4rem);
		line-height: 1.1;
		letter-spacing: -0.01em;
		color: var(--color-foreground);
		margin: 0;
	}

	.body {
		margin-top: 1.5rem;
		font-size: 1.0625rem;
		line-height: 1.6;
		color: var(--color-foreground-muted);
	}
</style>
