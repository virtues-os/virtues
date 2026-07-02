<script lang="ts">
	import { goto } from "$app/navigation";
	import Icon from "$lib/components/Icon.svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { contextMenu } from "$lib/stores/contextMenu.svelte";

	interface Props {
		collapsed?: boolean;
		animationDelay?: number;
		/** Opens the search / command modal (the field is the omnipresent Ask). */
		onSearch?: () => void;
	}

	let { collapsed = false, animationDelay = 0, onSearch }: Props = $props();

	// The masthead is the app's face: the ∴ mark is the app menu, and the field
	// beside it is the omnipresent Ask (⌘K). Together they replace both the old
	// wordmark and the separate search row — the corner now *does* something.

	async function handleLogout() {
		try {
			await fetch("/auth/signout", { method: "POST" });
		} catch (err) {
			console.error("[Logout] Error:", err);
		} finally {
			windowShellStore.closeAllTabs();
			await goto("/pair");
		}
	}

	function open(route: string, label: string, preferEmptyPane = false) {
		windowShellStore.openTabFromRoute(route, { label, preferEmptyPane });
	}

	function openMenu(e: MouseEvent) {
		const btn = e.currentTarget as HTMLElement;
		const r = btn.getBoundingClientRect();
		contextMenu.show(
			{ x: r.left, y: r.bottom },
			[
				{
					id: "home",
					label: "Home",
					icon: "ri:home-5-line",
					action: () => open("/home", "Home", true),
				},
				{
					id: "account",
					label: "Account",
					icon: "ri:user-3-line",
					action: () => open("/virtues/account", "Account"),
				},
				{
					id: "system",
					label: "System",
					icon: "ri:computer-line",
					action: () => open("/virtues/system", "System"),
				},
				{
					id: "signout",
					label: "Sign out",
					icon: "ri:logout-box-r-line",
					variant: "destructive",
					dividerBefore: true,
					action: handleLogout,
				},
			],
			{
				anchor: { x: r.left, y: r.top, width: r.width, height: r.height },
				placement: "bottom-start",
			},
		);
	}
</script>

<div class="masthead" class:collapsed>
	<button
		type="button"
		class="app-mark animate-row"
		style="animation-delay: {animationDelay}ms; --stagger-delay: {animationDelay}ms"
		onclick={openMenu}
		title="Menu"
		aria-label="App menu"
	>
		<span class="mark">∴</span>
	</button>

	<button
		type="button"
		class="ask-bar animate-row"
		style="animation-delay: {animationDelay + 20}ms; --stagger-delay: {animationDelay + 20}ms"
		onclick={() => onSearch?.()}
		title="Ask or search (⌘K)"
	>
		<span class="ask-leading">
			<Icon icon="ri:search-line" width="14" />
			<span class="ask-label">Ask or search…</span>
		</span>
		<kbd class="ask-kbd">⌘K</kbd>
	</button>
</div>

<style>
	:root {
		--ease-premium: cubic-bezier(0.2, 0, 0, 1);
	}

	@keyframes fadeSlideIn {
		from {
			opacity: 0;
			transform: translateX(-8px);
		}
		to {
			opacity: 1;
			transform: translateX(0);
		}
	}

	.masthead {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 14px 8px 8px;
	}

	.masthead.collapsed {
		opacity: 0;
		transform: translateX(-8px);
		pointer-events: none;
		transition:
			opacity 150ms var(--ease-premium),
			transform 150ms var(--ease-premium);
	}

	.animate-row {
		animation: fadeSlideIn 200ms var(--ease-premium) backwards;
	}

	/* ── ∴ app menu mark ── */
	.app-mark {
		flex-shrink: 0;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 30px;
		height: 30px;
		box-sizing: border-box;
		border-radius: 6px;
		background: none;
		border: none;
		cursor: pointer;
		transition: background 0.15s ease;
	}

	.app-mark:hover {
		background: color-mix(in srgb, var(--color-foreground) 6%, transparent);
	}

	.mark {
		font-family: var(--font-serif, serif);
		font-size: 18px;
		line-height: 1;
		color: var(--color-foreground);
		letter-spacing: 0.02em;
	}

	/* ── Ask / command field — the omnipresent Ask, styled as a quiet input ── */
	.ask-bar {
		flex: 1;
		min-width: 0;
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 4px;
		height: 30px;
		box-sizing: border-box;
		padding: 0 8px;
		background: transparent;
		border: 1px solid var(--color-border-subtle);
		border-radius: 6px;
		font-family: var(--font-sans);
		font-size: 12px;
		color: var(--color-foreground-subtle);
		cursor: text;
		transition:
			background 0.15s ease,
			border-color 0.15s ease,
			color 0.15s ease;
	}

	.ask-bar:hover {
		background: color-mix(in srgb, var(--color-foreground) 4%, transparent);
		border-color: var(--color-border);
		color: var(--color-foreground-muted);
	}

	.ask-leading {
		display: flex;
		align-items: center;
		gap: 6px;
		overflow: hidden;
	}

	.ask-label {
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.ask-kbd {
		flex-shrink: 0;
		font-family: inherit;
		font-size: 10px;
		color: var(--color-foreground-subtle);
		opacity: 0.7;
	}
</style>
