<script lang="ts">
	/**
	 * Mobile bottom-tab bar (Instagram "capture-center" shape).
	 *
	 *   Home · Today · ⊕ (new chat) · Pages · You
	 *
	 * Polished chrome: outline icons that go filled when active, small labels,
	 * a raised accent circle for capture, and tap-scale feedback. Navigation
	 * goes through the windowShellStore (the app's custom tab router); "You"
	 * opens the full-height sheet owned by mobileLayout.
	 *
	 * Ships fixed-height; the scroll-linked shrink-to-pill is a later pass.
	 */
	import Icon from "$lib/components/Icon.svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { mobileLayout } from "$lib/stores/mobileLayout.svelte";

	interface Tab {
		id: string;
		label: string;
		icon: string; // outline (inactive)
		iconActive: string; // filled (active)
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
			iconActive: "ri:home-5-fill",
			match: ["/home"],
			activate: () => go("/home", "Home"),
		},
		{
			id: "today",
			label: "Today",
			icon: "ri:sun-line",
			iconActive: "ri:sun-fill",
			match: ["/day"],
			activate: () => go("/day", "Today"),
		},
		{
			id: "capture",
			label: "",
			icon: "ri:add-line",
			iconActive: "ri:add-line",
			match: [],
			activate: () =>
				windowShellStore.openTabFromRoute("/", { label: "New Chat", forceNew: true }),
		},
		{
			id: "pages",
			label: "Pages",
			icon: "ri:file-text-line",
			iconActive: "ri:file-text-fill",
			match: ["/page"],
			activate: () => go("/page", "Pages"),
		},
		{
			id: "settings",
			label: "Settings",
			icon: "ri:settings-3-line",
			iconActive: "ri:settings-3-fill",
			match: [],
			activate: () => mobileLayout.openMenu(),
		},
	];

	const activeRoute = $derived(windowShellStore.activeTab?.route ?? "");

	function isActive(tab: Tab): boolean {
		if (tab.id === "settings") return mobileLayout.menuOpen;
		if (mobileLayout.menuOpen) return false;
		return tab.match.some((p) => activeRoute === p || activeRoute.startsWith(p + "/"));
	}
</script>

<nav class="mobile-tabbar">
	{#each tabs as tab (tab.id)}
		{@const active = isActive(tab)}
		<button
			class="tab"
			class:capture={tab.id === "capture"}
			class:active
			onclick={tab.activate}
			aria-label={tab.label || "New chat"}
			aria-current={active ? "page" : undefined}
		>
			<span class="glyph">
				<Icon
					icon={active ? tab.iconActive : tab.icon}
					width={tab.id === "capture" ? 26 : 24}
				/>
			</span>
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
		align-items: flex-start;
		justify-content: space-around;
		height: calc(50px + env(safe-area-inset-bottom));
		padding-bottom: env(safe-area-inset-bottom);
		background: color-mix(in srgb, var(--color-surface) 80%, transparent);
		backdrop-filter: saturate(1.8) blur(22px);
		-webkit-backdrop-filter: saturate(1.8) blur(22px);
		border-top: 0.5px solid color-mix(in srgb, var(--color-border) 90%, transparent);
		box-shadow: 0 -0.5px 0 color-mix(in srgb, var(--color-foreground) 4%, transparent);
	}

	.tab {
		flex: 1;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: flex-start;
		gap: 2px;
		padding-top: 7px;
		border: 0;
		background: transparent;
		color: var(--color-foreground-muted);
		cursor: pointer;
		-webkit-tap-highlight-color: transparent;
		transition: color 0.15s ease;
	}

	.tab.active {
		color: var(--color-foreground);
	}

	.glyph {
		display: flex;
		align-items: center;
		justify-content: center;
		transition: transform 0.12s cubic-bezier(0.32, 0.72, 0, 1);
	}

	/* Physical tap feedback. */
	.tab:active .glyph {
		transform: scale(0.86);
	}

	.label {
		font-size: 10px;
		line-height: 1;
		font-weight: 500;
		letter-spacing: 0.01em;
	}
	.tab.active .label {
		font-weight: 650;
	}

	/* Center capture: a raised, filled accent circle that pops above the bar. */
	.tab.capture {
		color: #fff;
		justify-content: flex-start;
	}
	.tab.capture .glyph {
		width: 44px;
		height: 44px;
		margin-top: -14px;
		border-radius: 50%;
		background: linear-gradient(
			180deg,
			color-mix(in srgb, var(--color-primary, #2b6cff) 92%, white),
			var(--color-primary, #2b6cff)
		);
		box-shadow:
			0 4px 12px color-mix(in srgb, var(--color-primary, #2b6cff) 40%, transparent),
			0 1px 2px rgba(0, 0, 0, 0.2);
	}
	.tab.capture:active .glyph {
		transform: scale(0.92);
	}
</style>
