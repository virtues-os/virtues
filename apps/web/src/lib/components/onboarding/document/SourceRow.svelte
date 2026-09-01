<!--
  SourceRow — one connectable source inside "Your data".

  Name + the WHY (why it's worth connecting), and a Connect pill or a connected
  state with a receipt line. Prominence ('anchor' | 'prominent' | 'quiet')
  tunes emphasis through name size ONLY — every Connect is the same quiet
  outlined pill, because the screen has exactly one filled action (Continue)
  and a column of mixed button weights read as a bug, not a hierarchy
  (2026-08-21). The pill shape matches `.ob-btn` so the screen speaks one
  radius vocabulary.

  The icon column was removed 2026-08-31: two rows carried tinted squares and
  two carried bare glyphs, which read as inconsistency rather than hierarchy —
  and brand logos would put four corporate marks on the quietest screen in the
  product. The serif names carry the list.
-->
<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import type { SourceCopy } from "./sources-copy";
	import type { SourceCatalogItem } from "$lib/api/client";

	interface Props {
		source: SourceCatalogItem;
		copy: SourceCopy;
		connected: boolean;
		/** The receipt: WHAT is connected — account names for credentialed
		 *  sources, message counts for imports. One quiet line, not a table. */
		detail?: string | null;
		busy?: boolean;
		onConnect: () => void;
	}

	let { source, copy, connected, detail = null, busy = false, onConnect }: Props = $props();
</script>

<div class="row" class:anchor={copy.prominence === "anchor"} class:connected>
	<div class="text">
		<div class="name-line">
			<span class="name">{source.name}</span>
			{#if connected}
				<span class="badge"><Icon icon="ri:check-line" width="13" /> Connected</span>
			{/if}
		</div>
		<p class="why">{copy.why}</p>
		{#if connected && detail}
			<p class="detail">{detail}</p>
		{/if}
	</div>
	<div class="cta">
		{#if connected}
			<button class="add-more" onclick={onConnect} disabled={busy}>Add another</button>
		{:else}
			<button class="connect" onclick={onConnect} disabled={busy}>
				{busy ? "…" : "Connect"}
			</button>
		{/if}
	</div>
</div>

<style>
	@reference "../../../../app.css";

	.row {
		position: relative;
		display: grid;
		grid-template-columns: 1fr auto;
		align-items: start;
		gap: 0.9rem;
		padding: 1.1rem 0;
	}
	.row + .row {
		border-top: 1px solid var(--color-border-subtle);
	}

	.text {
		min-width: 0;
	}
	.name-line {
		display: flex;
		align-items: baseline;
		gap: 0.6rem;
	}
	.name {
		font-family: var(--font-serif);
		font-size: 1.05rem;
		color: var(--color-foreground);
	}
	.anchor .name {
		font-size: 1.2rem;
	}
	.why {
		margin: 0.25rem 0 0;
		font-size: 0.9rem;
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}
	/* The receipt: what is actually connected, in the quietest voice on the
	   row — it is evidence, not persuasion. */
	.detail {
		margin: 0.35rem 0 0;
		font-size: 0.8rem;
		line-height: 1.4;
		color: var(--color-foreground-subtle);
	}

	.badge {
		display: inline-flex;
		align-items: center;
		gap: 0.15rem;
		font-size: 0.7rem;
		font-weight: 500;
		color: var(--color-success);
	}

	.cta {
		flex-shrink: 0;
		padding-top: 0.15rem;
	}
	/* Same pill as .ob-btn, outlined instead of filled — one radius vocabulary
	   on the screen, one filled control (Continue). */
	.connect {
		font: inherit;
		font-size: 0.85rem;
		padding: 0.45rem 1.1rem;
		border: 1px solid var(--color-border);
		border-radius: 999px;
		background: transparent;
		color: var(--color-foreground);
		cursor: pointer;
		transition:
			border-color 0.15s ease,
			background 0.15s ease;
	}
	.connect:hover:not(:disabled) {
		border-color: var(--color-foreground-subtle);
		background: var(--color-surface-elevated);
	}
	.connect:disabled {
		opacity: 0.45;
		cursor: default;
	}
	.add-more {
		font-size: 0.8rem;
		color: var(--color-foreground-subtle);
		transition: color 0.15s ease;
	}
	.add-more:hover {
		color: var(--color-foreground);
	}
</style>
