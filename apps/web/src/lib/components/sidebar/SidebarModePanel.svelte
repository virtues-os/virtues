<script lang="ts">
	/**
	 * The sidebar while it's in a sub-navigation mode (Settings, Developer).
	 *
	 * Carries its own exit row, because there is no other way out — the mode is
	 * entered and left deliberately rather than being derived from the focused
	 * tab, so nothing else will drop you back.
	 *
	 * Sections open into one reused tab rather than a new one each time. A few
	 * minutes in Settings would otherwise leave a row of tabs to clean up.
	 */
	import Icon from '$lib/components/Icon.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { sidebarMode } from '$lib/stores/sidebarMode.svelte';
	import type { SidebarMode } from '$lib/sidebar/modes';

	interface Props {
		mode: SidebarMode;
		stagger?: number;
	}

	let { mode, stagger = 30 }: Props = $props();

	let activeHref = $state<string | null>(null);

	function open(href: string, label: string) {
		activeHref = href;
		// `navigate` swaps the active tab's content in place rather than opening
		// a new one, so moving through six settings sections leaves you with one
		// tab instead of six to close afterwards. It pushes onto that tab's
		// history too, so Back walks the sections.
		windowShellStore.navigate(href, { label: `${mode.title} · ${label}` });
	}
</script>

<div class="mode-panel">
	<button
		type="button"
		class="exit-row"
		style="animation-delay: {stagger}ms"
		onclick={() => sidebarMode.exit()}
	>
		<Icon icon="ri:arrow-left-line" width="15" />
		<span class="exit-label">{mode.title}</span>
	</button>

	<nav class="mode-rows">
		{#each mode.rows as row, i (row.id)}
			<button
				type="button"
				class="mode-row"
				class:active={activeHref === row.href}
				style="animation-delay: {(i + 2) * stagger}ms"
				onclick={() => open(row.href, row.label)}
			>
				<Icon icon={row.icon} width="16" />
				<span>{row.label}</span>
			</button>
		{/each}
	</nav>
</div>

<style>
	/* Slides in from the right as the normal rows slide out to the left — the
	   two halves of one motion, so it reads as a panel swap rather than a
	   replacement. */
	@keyframes modeRowIn {
		from {
			opacity: 0;
			transform: translateX(10px);
		}
		to {
			opacity: 1;
			transform: translateX(0);
		}
	}

	.mode-panel {
		display: flex;
		flex-direction: column;
		gap: 2px;
		padding: 0 6px;
	}

	.exit-row,
	.mode-row {
		display: flex;
		align-items: center;
		gap: 10px;
		width: 100%;
		padding: 6px 10px;
		border: none;
		border-radius: 6px;
		background: none;
		cursor: pointer;
		text-align: left;
		color: var(--color-foreground-muted);
		animation: modeRowIn 200ms cubic-bezier(0.2, 0, 0, 1) backwards;
	}

	.exit-row {
		color: var(--color-foreground-subtle);
		margin-bottom: 4px;
	}

	.exit-label {
		font-family: var(--font-serif);
		font-size: 11px;
		font-weight: 500;
		text-transform: uppercase;
		letter-spacing: 0.14em;
	}

	.mode-row {
		font-size: 13px;
	}

	.exit-row:hover,
	.mode-row:hover {
		background: color-mix(in srgb, var(--color-foreground) 6%, transparent);
		color: var(--color-foreground);
	}

	.mode-row.active {
		background: color-mix(in srgb, var(--color-foreground) 9%, transparent);
		color: var(--color-foreground);
	}

	.exit-row:focus-visible,
	.mode-row:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: -2px;
	}

	@media (prefers-reduced-motion: reduce) {
		.exit-row,
		.mode-row {
			animation: none;
		}
	}
</style>
