<!--
  Marginalia — a sidenote in the document's gutter.

  On wide screens it floats in the right margin beside its anchor (the Tufte
  sidenote pattern); on narrow screens it collapses to an inline block under
  the content. Three tones:
    · why     — the editorial reason a step matters (serif-ish, muted body)
    · receipt — the honest privacy line ("stays on your box"), mono register
    · meta    — machine metadata (counts, timestamps), mono, faint
-->
<script lang="ts">
	import type { Snippet } from "svelte";

	interface Props {
		tone?: "why" | "receipt" | "meta";
		children: Snippet;
	}

	let { tone = "why", children }: Props = $props();
</script>

<aside class="marginalia" class:why={tone === "why"} class:receipt={tone === "receipt"} class:meta={tone === "meta"}>
	{@render children()}
</aside>

<style>
	@reference "../../../../app.css";

	.marginalia {
		font-size: 0.8125rem;
		line-height: 1.5;
		color: var(--color-foreground-subtle);
	}

	.why {
		font-family: var(--font-sans);
		color: var(--color-foreground-muted);
	}

	.receipt,
	.meta {
		font-family: var(--font-mono);
		font-size: 0.75rem;
		letter-spacing: -0.01em;
		color: var(--color-foreground-subtle);
	}

	/* Wide screens: float into the right gutter, hung off the content edge. */
	@media (min-width: 900px) {
		.marginalia {
			position: absolute;
			left: calc(100% + 2.5rem);
			top: 0;
			width: 14rem;
		}
	}
</style>
