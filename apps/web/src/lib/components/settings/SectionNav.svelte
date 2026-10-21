<script lang="ts">
	/**
	 * Settings' sections, as a list down the left of the page.
	 *
	 * The sidebar does not change for Settings: its foot opens Settings, and
	 * every Settings page (Sources and Streams included) carries this list in
	 * the pane, the way a Mac app's settings window carries its own. TabContent
	 * draws it beside any tab whose route is one of the mode's sections.
	 *
	 * Picking a section moves this tab, as any view moving within itself does,
	 * so a few minutes in Settings leave one tab, and Back walks the sections.
	 */
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { rowForRoute, type SidebarMode } from '$lib/sidebar/modes';
	import AtlasIcon from '$lib/components/sidebar/AtlasIcon.svelte';
	import { SHELL_GLYPH_PX } from '$lib/components/sidebar/AtlasIcon.svelte';

	interface Props {
		mode: SidebarMode;
		/** The route of the tab this list sits in: its section is marked. */
		route: string;
	}

	let { mode, route }: Props = $props();

	const activeHref = $derived(rowForRoute(mode, route)?.href ?? null);

	function open(href: string, label: string) {
		windowShellStore.navigate(href, { label: `${mode.title} · ${label}` });
	}
</script>

<div class="mode-panel">
	<nav class="mode-rows" aria-label="{mode.title} sections">
		{#each mode.rows as row, i (row.id)}
			<!-- A heading wherever the group changes; rows of a group are adjacent
			     (modes.ts). -->
			{#if row.group !== mode.rows[i - 1]?.group}
				{#if row.group}
					<!-- A div, not an h3: app.css sets every heading in the serif. -->
					<div class="mode-group" class:first={i === 0}>{row.group}</div>
				{/if}
			{/if}
			<button
				type="button"
				class="mode-row"
				class:active={activeHref === row.href}
				onclick={() => open(row.href, row.label)}
			>
				{#if row.glyph}
					<AtlasIcon name={row.glyph} size={SHELL_GLYPH_PX} />
				{/if}
				<span>{row.label}</span>
			</button>
		{/each}
	</nav>
</div>

<style>
	/* A column inside the card, on the page: no line beside it, the air and
	   the selected fill are enough. The rows match the sidebar's. */
	.mode-panel {
		display: flex;
		flex-direction: column;
		gap: 2px;
		width: 208px;
		flex: none;
		padding: 12px 8px;
		overflow-y: auto;
	}

	/* The sidebar's group label: 12px, subtle, not a control. */
	.mode-group {
		display: flex;
		align-items: center;
		height: 24px;
		margin-top: 12px;
		padding: 0 6px 0 12px;
		font-size: 12px;
		color: var(--color-foreground-subtle);
		user-select: none;
	}

	.mode-group.first {
		margin-top: 4px;
	}


	.mode-row {
		display: flex;
		align-items: center;
		gap: var(--sidebar-interactive-gap);
		width: 100%;
		height: var(--sidebar-interactive-height);
		padding: 0 12px;
		border: none;
		border-radius: var(--sidebar-interactive-radius);
		background: none;
		cursor: pointer;
		text-align: left;
		font-size: var(--sidebar-interactive-font-size);
		color: var(--color-foreground-muted);
	}

	.mode-row :global(svg) {
		opacity: var(--sidebar-icon-opacity);
		transition: opacity var(--motion-hover-out) var(--ease-premium);
	}

	.mode-row {
		font-weight: 400;
		transition: background-color var(--motion-hover-out) var(--ease-premium);
	}

	.mode-row:hover {
		background: var(--hover-bg);
		color: var(--color-foreground);
		transition-duration: var(--motion-hover-in);
	}

	.mode-row:hover :global(svg),
	.mode-row.active :global(svg) {
		opacity: 1;
	}

	.mode-row.active {
		background: var(--active-bg);
		color: var(--color-foreground);
	}

	.mode-row:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: -2px;
	}

	@media (prefers-reduced-motion: reduce) {
		.mode-row,
		.mode-row :global(svg) {
			transition: none;
		}
	}

</style>
