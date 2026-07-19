<!--
  TypesetLines — reveal a block of prose line by line, as if it's being
  typeset onto the page. Each line rises + fades on a strong ease-out with a
  small stagger. Under prefers-reduced-motion the lines appear at once.

  Pass `lines` (already split) and an optional element `tag` ("p" by default).
  Used for the Welcome promise and the narrative-identity reveal.
-->
<script lang="ts">
	import { fly } from "svelte/transition";
	import { expoOut } from "svelte/easing";

	interface Props {
		lines: string[];
		/** ms between consecutive lines */
		stagger?: number;
		/** ms before the first line */
		delay?: number;
		reduced?: boolean;
		class?: string;
	}

	let { lines, stagger = 90, delay = 60, reduced = false, class: cls = "" }: Props = $props();
</script>

<div class={cls}>
	{#each lines as line, i (i)}
		{#if reduced}
			<p class="typeset-line">{line}</p>
		{:else}
			<p
				class="typeset-line"
				in:fly={{ y: 12, duration: 520, delay: delay + i * stagger, easing: expoOut }}
			>
				{line}
			</p>
		{/if}
	{/each}
</div>

<style>
	.typeset-line {
		margin: 0;
	}
	.typeset-line + .typeset-line {
		margin-top: 0.35em;
	}
</style>
