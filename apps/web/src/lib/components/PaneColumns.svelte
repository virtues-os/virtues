<script lang="ts">
	/**
	 * A thing beside a list, in two columns, as an issue tracker lays out an
	 * issue: the story on the left (its title in `head`, then `main`), and
	 * every property on the right (`side`, in `SideGroup`s), always in the
	 * same place. When the column it sits in is narrower than 720px, the
	 * properties move between the title and the story.
	 */
	import type { Snippet } from 'svelte';

	let {
		head,
		main,
		side,
		sideLabel,
	}: {
		head: Snippet;
		main: Snippet;
		side: Snippet;
		/** What the properties are about, for a screen reader: "About this task". */
		sideLabel: string;
	} = $props();
</script>

<div class="wrap">
	<div class="cols">
		<div class="head">{@render head()}</div>
		<main class="main">{@render main()}</main>
		<aside class="side" aria-label={sideLabel}>{@render side()}</aside>
	</div>
</div>

<style>
	.wrap {
		container-type: inline-size;
	}
	.cols {
		display: grid;
		grid-template-areas: 'head side' 'main side';
		grid-template-columns: minmax(0, 1fr) 240px;
		grid-template-rows: auto 1fr;
		gap: 24px 48px;
	}
	.head {
		grid-area: head;
		min-width: 0;
	}
	.main {
		display: flex;
		flex-direction: column;
		gap: 32px;
		grid-area: main;
		min-width: 0;
	}
	/* The properties stay in view below the right side's bar as the story scrolls. */
	.side {
		grid-area: side;
		align-self: start;
		position: sticky;
		top: 72px;
	}
	@container (max-width: 720px) {
		.cols {
			grid-template-areas: 'head' 'side' 'main';
			grid-template-columns: minmax(0, 1fr);
		}
		.side {
			position: static;
		}
	}
</style>
