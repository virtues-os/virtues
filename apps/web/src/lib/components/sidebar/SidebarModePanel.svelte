<script lang="ts">
	/**
	 * The sidebar while it's in a sub-navigation mode (Settings, Developer).
	 *
	 * The exit row is gone: the path mast now reads `Virtues / Settings`, and
	 * clicking the root leaves the mode. A mode used to be the one place in
	 * the app you left by a bespoke control that existed nowhere else; now
	 * every "you are inside something" state — a pinned notebook, a mode, and
	 * whatever we add next — is entered and left through the same breadcrumb.
	 * That is the whole reason the mast became a path.
	 *
	 * Sections open into one reused tab rather than a new one each time. A few
	 * minutes in Settings would otherwise leave a row of tabs to clean up.
	 */
	import Icon from '$lib/components/Icon.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
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

	.mode-row {
		display: flex;
		align-items: center;
		gap: var(--sidebar-interactive-gap);
		width: 100%;
		height: var(--sidebar-interactive-height);
		padding: 0 var(--sidebar-padding-left-base);
		border: none;
		border-radius: var(--sidebar-interactive-radius);
		background: none;
		cursor: pointer;
		text-align: left;
		font-size: var(--sidebar-interactive-font-size);
		color: var(--color-foreground-muted);
		animation: modeRowIn 200ms cubic-bezier(0.2, 0, 0, 1) backwards;
	}

	.mode-row :global(svg) {
		opacity: var(--sidebar-icon-opacity);
		transition: opacity var(--sidebar-transition-duration) ease;
	}

	.mode-row {
		font-weight: 500;
	}

	.mode-row:hover {
		background: var(--sidebar-hover-bg);
		color: var(--color-foreground);
	}

	.mode-row:hover :global(svg),
	.mode-row.active :global(svg) {
		opacity: 1;
	}

	.mode-row.active {
		background: color-mix(in srgb, var(--color-foreground) 9%, transparent);
		color: var(--color-foreground);
	}

	.mode-row:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: -2px;
	}

	@media (prefers-reduced-motion: reduce) {
		.mode-row {
			animation: none;
		}
	}
</style>
