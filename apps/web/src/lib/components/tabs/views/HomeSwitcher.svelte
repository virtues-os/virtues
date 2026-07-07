<script lang="ts">
	// HomeSwitcher — TEMPORARY prototype harness. Lets us flip the Home surface
	// between the live version and two redesign prototypes without touching
	// routing. Remove once a direction is chosen; delete HomeViewFolio /
	// HomeViewRefined with it. Choice persists in localStorage.
	import HomeView from "./HomeView.svelte";
	import HomeViewFolio from "./HomeViewFolio.svelte";
	import HomeViewRefined from "./HomeViewRefined.svelte";
	import HomeViewSpread from "./HomeViewSpread.svelte";
	// Consumes tab/active from the tab runtime; the Home variants take no props.
	let {}: { tab?: unknown; active?: boolean } = $props();

	type Variant = "original" | "folio" | "refined" | "spread";
	const KEY = "home-variant-prototype";

	let variant = $state<Variant>(
		(typeof localStorage !== "undefined" &&
			(localStorage.getItem(KEY) as Variant)) ||
			"spread",
	);

	function pick(v: Variant) {
		variant = v;
		try {
			localStorage.setItem(KEY, v);
		} catch {
			/* ignore */
		}
	}

	const options: { id: Variant; label: string }[] = [
		{ id: "original", label: "Original" },
		{ id: "folio", label: "Folio" },
		{ id: "refined", label: "Refined" },
		{ id: "spread", label: "Spread" },
	];
</script>

<div class="switcher-host">
	{#if variant === "original"}
		<HomeView />
	{:else if variant === "folio"}
		<HomeViewFolio />
	{:else if variant === "refined"}
		<HomeViewRefined />
	{:else}
		<HomeViewSpread />
	{/if}

	<div class="switcher" role="radiogroup" aria-label="Home prototype">
		{#each options as o (o.id)}
			<button
				type="button"
				class="seg"
				class:on={variant === o.id}
				role="radio"
				aria-checked={variant === o.id}
				onclick={() => pick(o.id)}
			>
				{o.label}
			</button>
		{/each}
	</div>
</div>

<style>
	.switcher-host {
		position: relative;
		width: 100%;
		height: 100%;
		display: flex;
		min-height: 0;
	}
	.switcher-host > :global(*:first-child) {
		flex: 1;
		min-height: 0;
	}
	.switcher {
		position: absolute;
		bottom: 1rem;
		left: 50%;
		transform: translateX(-50%);
		display: flex;
		gap: 0.125rem;
		padding: 0.1875rem;
		border-radius: 999px;
		border: 1px solid var(--color-border);
		background: var(--color-surface-overlay, var(--color-surface-elevated));
		box-shadow: 0 4px 16px rgb(0 0 0 / 0.18);
		z-index: 50;
	}
	.seg {
		padding: 0.3125rem 0.75rem;
		border-radius: 999px;
		border: none;
		background: transparent;
		color: var(--color-foreground-subtle);
		font-family: var(--font-mono);
		font-size: 0.6875rem;
		letter-spacing: 0.04em;
		text-transform: uppercase;
		cursor: pointer;
		transition:
			background-color var(--duration-fast) ease,
			color var(--duration-fast) ease;
	}
	.seg:hover {
		color: var(--color-foreground);
	}
	.seg.on {
		background: var(--color-foreground);
		color: var(--color-background);
	}
</style>
