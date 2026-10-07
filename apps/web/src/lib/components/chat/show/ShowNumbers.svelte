<!--
	One row of a query as up to four figures, each named by its column's
	alias. Lining, tabular figures so a row of them agrees on a baseline.
-->
<script lang="ts">
	import { columnLabel, formatCell, formatNumber, type Row } from "./show";

	interface Props {
		columns: string[];
		row: Row;
	}
	let { columns, row }: Props = $props();

	const show = (v: unknown) => (typeof v === "number" ? formatNumber(v) : formatCell(v) || "-");
</script>

<dl class="numbers">
	{#each columns as c}
		<div class="figure">
			<dt>{columnLabel(c)}</dt>
			<dd>{show(row[c])}</dd>
		</div>
	{/each}
</dl>

<style>
	.numbers {
		display: flex;
		flex-wrap: wrap;
		gap: 1rem 2.5rem;
		margin: 0;
	}
	.figure {
		display: flex;
		flex-direction: column-reverse;
		gap: 0.125rem;
		min-width: 0;
	}
	dt {
		font-family: var(--font-sans);
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
	}
	dd {
		margin: 0;
		font-family: var(--font-serif);
		font-weight: 400;
		font-size: 2.25rem;
		line-height: 1.1;
		color: var(--color-foreground);
		font-variant-numeric: lining-nums tabular-nums;
	}
</style>
