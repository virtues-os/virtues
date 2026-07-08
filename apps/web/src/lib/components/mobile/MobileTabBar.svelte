<script lang="ts">
	/**
	 * Mobile bottom-tab bar (Instagram "capture-center" shape).
	 *
	 *   Home · Today · ⊕ (new chat) · Pages · You
	 *
	 * Navigation goes through the windowShellStore (the app's custom tab
	 * router), not <a href>. "You" opens the full-height sheet (device toggles,
	 * stream logs, long-tail nav, settings) owned by mobileLayout.
	 *
	 * Ships fixed-height; the scroll-linked shrink-to-pill is a later polish pass.
	 */
	import Icon from "$lib/components/Icon.svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { mobileLayout } from "$lib/stores/mobileLayout.svelte";

	interface Tab {
		id: string;
		label: string;
		icon: string;
		/** Route prefixes that mark this tab active. */
		match: string[];
		activate: () => void;
	}

	function go(route: string, label: string) {
		windowShellStore.openTabFromRoute(route, { label });
	}

	const tabs: Tab[] = [
		{
			id: "home",
			label: "Home",
			icon: "ri:home-5-line",
			match: ["/home"],
			activate: () => go("/home", "Home"),
		},
		{
			id: "today",
			label: "Today",
			icon: "ri:sun-line",
			match: ["/day"],
			activate: () => go("/day", "Today"),
		},
		{
			id: "capture",
			label: "",
			icon: "ri:add-line",
			match: [],
			activate: () =>
				windowShellStore.openTabFromRoute("/", { label: "New Chat", forceNew: true }),
		},
		{
			id: "pages",
			label: "Pages",
			icon: "ri:file-text-line",
			match: ["/page"],
			activate: () => go("/page", "Pages"),
		},
		{
			id: "you",
			label: "You",
			icon: "ri:user-3-line",
			match: [],
			activate: () => mobileLayout.openYou(),
		},
	];

	const activeRoute = $derived(windowShellStore.activeTab?.route ?? "");

	function isActive(tab: Tab): boolean {
		if (tab.id === "you") return mobileLayout.youSheetOpen;
		if (mobileLayout.youSheetOpen) return false;
		return tab.match.some((p) => activeRoute === p || activeRoute.startsWith(p + "/"));
	}
</script>

<nav class="mobile-tabbar" style="padding-bottom: env(safe-area-inset-bottom);">
	{#each tabs as tab (tab.id)}
		<button
			class="tab"
			class:capture={tab.id === "capture"}
			class:active={isActive(tab)}
			onclick={tab.activate}
			aria-label={tab.label || "New chat"}
		>
			<span class="glyph"><Icon icon={tab.icon} width={tab.id === "capture" ? 24 : 22} /></span>
			{#if tab.label}<span class="label">{tab.label}</span>{/if}
		</button>
	{/each}
</nav>

<style>
	.mobile-tabbar {
		position: fixed;
		left: 0;
		right: 0;
		bottom: 0;
		z-index: 50;
		display: flex;
		align-items: stretch;
		justify-content: space-around;
		height: calc(56px + env(safe-area-inset-bottom));
		background: color-mix(in srgb, var(--color-surface) 92%, transparent);
		backdrop-filter: saturate(1.4) blur(18px);
		-webkit-backdrop-filter: saturate(1.4) blur(18px);
		border-top: 1px solid var(--color-border);
	}

	.tab {
		flex: 1;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 2px;
		padding: 6px 0;
		border: 0;
		background: transparent;
		color: var(--color-foreground-muted);
		cursor: pointer;
		-webkit-tap-highlight-color: transparent;
		transition: color 0.12s ease;
	}

	.tab.active {
		color: var(--color-foreground);
	}

	.glyph {
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.label {
		font-size: 10px;
		line-height: 1;
		font-weight: 500;
	}

	/* Center capture: a raised accent pill. */
	.tab.capture .glyph {
		width: 40px;
		height: 30px;
		border-radius: 10px;
		background: var(--color-accent, #2b6cff);
		color: #fff;
	}
	.tab.capture {
		color: var(--color-accent, #2b6cff);
	}
</style>
