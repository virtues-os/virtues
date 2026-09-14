<!--
	Not a step. One line: the first day is written overnight from what the
	sources hold. It appears once something is flowing, turns into the day's
	door when narration lands, and retires itself; there is nothing to skip.
-->
<script lang="ts">
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { gettingStarted } from "$lib/stores/gettingStarted.svelte";

	const firstDay = $derived(gettingStarted.state?.first_day ?? null);

	function pretty(d: string): string {
		const [y, m, day] = d.split("-").map(Number);
		return new Date(y, m - 1, day).toLocaleDateString(undefined, { month: "long", day: "numeric" });
	}
	function open() {
		if (firstDay) windowShellStore.openTabFromRoute(`/day/day_${firstDay}`, { label: "Your first day" });
	}
</script>

<p class="promise">
	{#if firstDay}
		Your first day is written up.
		<button type="button" class="link" onclick={open}>Read {pretty(firstDay)} →</button>
	{:else}
		Your first day is written overnight from what your sources hold, and every day after writes itself.
	{/if}
</p>

<style>
	.promise {
		margin: 0.25rem 0;
		font-size: 0.9rem;
		color: var(--color-foreground-muted);
	}
	.link {
		background: none;
		border: 0;
		padding: 0;
		font: inherit;
		color: var(--color-foreground);
		cursor: pointer;
	}
	.link:hover {
		text-decoration: underline;
	}
</style>
