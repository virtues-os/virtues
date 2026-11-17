<script lang="ts">
	/**
	 * The right corner of the window header: Search, Sources and Settings,
	 * three glass buttons level with the ∴ at the other end.
	 *
	 * Search opens the search palette (SearchModal), as ⌘K does. It searches
	 * everything, so it lives in the window's corner rather than on a page's
	 * toolbar, where it would read as a search of that page. Sources opens
	 * where your data comes from: its overview, catalog, activity, streams and
	 * Query. It carries no dot or badge, even when a source stops: a stopped
	 * source shows on the Sources page and in Settings' Changed lately.
	 * Settings opens
	 * the Settings page, every setting on one scroll. It is something you
	 * open now and then, so it sits in the corner rather
	 * than in the sidebar's list, and lights while the focused pane shows a
	 * Settings page.
	 */
	import AtlasIcon from '$lib/components/sidebar/AtlasIcon.svelte';
	import { SHELL_GLYPH_PX } from '$lib/components/sidebar/AtlasIcon.svelte';
	import { search } from '$lib/stores/search.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';

	const activePath = $derived.by(() => {
		const pane = windowShellStore.activePane;
		return pane?.tabs.find((t) => t.id === pane.activeTabId)?.route?.split('?')[0];
	});
	const inSources = $derived(activePath === '/sources' || !!activePath?.startsWith('/sources/'));
	const inSettings = $derived(activePath === '/virtues' || !!activePath?.startsWith('/virtues/'));

	function openSources() {
		windowShellStore.openTabFromRoute('/sources', { label: 'Sources', focusExisting: true });
	}

	function openSettings() {
		windowShellStore.openTabFromRoute('/virtues', { label: 'Settings', focusExisting: true });
	}
</script>

<div class="header-corner">
	<button type="button" class="corner-button lq" onclick={() => search.show()} aria-label="Search" data-tip="Search" data-tip-key="mod+k">
		<AtlasIcon name="search" size={SHELL_GLYPH_PX} bare />
	</button>
	<button
		type="button"
		class="corner-button lq"
		class:lq-selected={inSources}
		onclick={openSources}
		aria-label="Sources"
		aria-current={inSources ? 'page' : undefined}
		data-tip="Sources"
	>
		<AtlasIcon name="sources" size={SHELL_GLYPH_PX} bare />
	</button>
	<button
		type="button"
		class="corner-button lq"
		class:lq-selected={inSettings}
		onclick={openSettings}
		aria-label="Settings"
		aria-current={inSettings ? 'page' : undefined}
		data-tip="Settings"
	>
		<AtlasIcon name="settings" size={SHELL_GLYPH_PX} bare />
	</button>
</div>

<style>
	.header-corner {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	/* The ∴'s glass and size (HeaderMark), so the two ends of the row match. */
	.corner-button {
		width: var(--chrome-tab-h);
		height: var(--chrome-tab-h);
		padding: 0;
		border: none;
		border-radius: var(--radius-lg);
		display: inline-flex;
		align-items: center;
		justify-content: center;
		color: var(--color-foreground-muted);
		cursor: pointer;
		transition:
			color var(--motion-hover-out) var(--ease-premium),
			box-shadow var(--motion-hover-out) var(--ease-premium),
			transform var(--motion-release) var(--ease-glass);
	}

	.corner-button:hover,
	.corner-button.lq-selected {
		color: var(--color-foreground);
		transition-duration: var(--motion-hover-in), var(--motion-hover-in), var(--motion-release);
	}

	/* Glass gives under the finger: it sinks a little and springs back. */
	.corner-button:active {
		transform: scale(0.92);
		transition-duration: var(--motion-hover-in), var(--motion-hover-in), var(--motion-press);
	}

	.corner-button:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: 2px;
	}

	@media (prefers-reduced-motion: reduce) {
		.corner-button {
			transition: none;
		}

		.corner-button:active {
			transform: none;
		}
	}
</style>
