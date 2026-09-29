<script lang="ts">
	/**
	 * The line under a local reply: what the NPU just did. In local mode the
	 * machinery is the demonstration, so it is shown rather than hidden.
	 */
	import type { LocalStats } from '$lib/stores/localModel.svelte';

	let { stats }: { stats: LocalStats } = $props();
	const rate = $derived(stats.tokensPerSecond.toFixed(1));
</script>

<div class="local-stats">{stats.generatedTokens} tokens · {rate} a second · on the NPU</div>

<style>
	.local-stats {
		margin-top: 8px;
		font-size: 12px;
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
	}
</style>
