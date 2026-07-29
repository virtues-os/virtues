<script lang="ts">
	/**
	 * A zone subtitle — "Desk", "Library" — that folds its zone shut.
	 *
	 * One component for both so the two can't drift into slightly different
	 * whispers. The chevron appears on hover and stays visible while the zone
	 * is closed, because a folded zone has to be able to say so: a collapsed
	 * section whose only clue is absence reads as missing data, not as a
	 * decision the user made.
	 *
	 * A header, not a rule. No hairline under it — the desk/library seam is
	 * carried by type (serif names above, sans destinations below), and a line
	 * would say the same thing louder.
	 */
	import { sidebarZones } from "$lib/stores/sidebarZones.svelte";

	interface Props {
		id: string;
		label: string;
		/** Optional trailing control (e.g. a quick-add), shown on hover. */
		children?: import("svelte").Snippet;
	}

	let { id, label, children }: Props = $props();

	const collapsed = $derived(sidebarZones.isCollapsed(id));
</script>

<div class="zone-header">
	<button
		type="button"
		class="zone-toggle"
		class:collapsed
		onclick={() => sidebarZones.toggle(id)}
		aria-expanded={!collapsed}
		title={collapsed ? `Show ${label}` : `Hide ${label}`}
	>
		<span class="zone-title">{label}</span>
		<svg class="chev" width="9" height="6" viewBox="0 0 10 6" fill="none" aria-hidden="true">
			<path
				d="M1 1l4 4 4-4"
				stroke="currentColor"
				stroke-width="1.3"
				stroke-linecap="round"
				stroke-linejoin="round"
			/>
		</svg>
	</button>
	{#if children}
		<span class="zone-actions">{@render children()}</span>
	{/if}
</div>

<style>
	.zone-header {
		display: flex;
		align-items: center;
		height: 26px;
		padding-right: 8px;
		margin-bottom: 2px;
		user-select: none;
	}

	.zone-toggle {
		display: flex;
		align-items: center;
		gap: 6px;
		height: 100%;
		padding: 0 0 0 var(--sidebar-padding-left-base);
		border: none;
		background: none;
		cursor: pointer;
		color: var(--color-foreground-subtle);
		transition: color 150ms var(--ease-premium);
	}

	.zone-toggle:hover {
		color: var(--color-foreground-muted);
	}

	.zone-toggle:focus-visible {
		outline: 2px solid var(--color-border-focus);
		outline-offset: 2px;
		border-radius: 4px;
	}

	.zone-title {
		font-size: 11px;
		font-weight: 500;
		letter-spacing: 0.015em;
	}

	.chev {
		opacity: 0;
		transform: rotate(0deg);
		transition:
			opacity 150ms ease,
			transform 220ms var(--ease-premium);
	}

	.zone-toggle:hover .chev,
	.zone-toggle:focus-visible .chev {
		opacity: 0.8;
	}

	/* Closed: the chevron stays put, pointing at the zone it would reopen. */
	.zone-toggle.collapsed .chev {
		opacity: 0.8;
		transform: rotate(-90deg);
	}

	.zone-actions {
		margin-left: auto;
		display: flex;
		align-items: center;
		opacity: 0;
		transition: opacity 150ms ease;
	}

	.zone-header:hover .zone-actions {
		opacity: 1;
	}

	@media (prefers-reduced-motion: reduce) {
		.chev {
			transition: none;
		}
	}
</style>
