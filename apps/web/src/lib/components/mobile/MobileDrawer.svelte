<script lang="ts">
	/**
	 * The phone's drawer — the app's entire navigation.
	 *
	 * Chat-first means the drawer IS the chat list, not a menu that links to
	 * one: New chat at the top, recents inline, and a pinned foot holding the
	 * two doors that aren't conversations — This device and Settings. That is
	 * the whole surface. Home, Search, Pages and the rest deliberately have no
	 * phone door for now; when one earns its way back it arrives as a row here,
	 * not as a bar.
	 *
	 * This component only renders content. Position, width, the slide and the
	 * gesture all belong to MobileShell, which parks this under the viewport
	 * and slides the viewport off it — the drawer itself never moves.
	 */
	import Icon from "$lib/components/Icon.svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { mobileLayout } from "$lib/stores/mobileLayout.svelte";
	import { chatSessions } from "$lib/stores/chatSessions.svelte";

	// Refresh the list whenever the drawer opens: it is the moment the user is
	// looking at it, the GET is small, and a stale list here reads as lost
	// conversations. The layout's boot-time load covers first paint.
	$effect(() => {
		if (mobileLayout.drawerOpen) void chatSessions.refresh();
	});

	const activeRoute = $derived(windowShellStore.activeTab?.route ?? "");

	function go(route: string, label: string) {
		windowShellStore.openTabFromRoute(route, { label });
		mobileLayout.closeDrawer();
	}
</script>

<nav class="drawer" aria-label="Navigation">
	<button class="new-chat" onclick={() => go("/chat", "Chat")}>
		<Icon icon="ri:chat-new-line" width={18} />
		<span>New chat</span>
	</button>

	<div class="chats">
		{#each chatSessions.sessions as s (s.conversation_id)}
			{@const route = `/chat/${s.conversation_id}`}
			<button
				class="chat-row"
				class:active={activeRoute === route}
				aria-current={activeRoute === route ? "page" : undefined}
				onclick={() => go(route, s.title || "Chat")}
			>
				<span class="chat-title">{s.title || "Untitled"}</span>
			</button>
		{:else}
			<div class="empty">No conversations yet</div>
		{/each}
	</div>

	<footer class="foot">
		{#if mobileLayout.isNativeShell}
			<button class="foot-row" onclick={() => go("/virtues/devices/this", "This device")}>
				<Icon icon="ri:smartphone-line" width={18} />
				<span>This device</span>
			</button>
		{/if}
		<button class="foot-row" onclick={() => go("/virtues/you", "Settings")}>
			<Icon icon="ri:settings-3-line" width={18} />
			<span>Settings</span>
		</button>
	</footer>
</nav>

<style>
	/* Elevated, not surface: the viewport slides off this, and the two planes
	   reading as the same material loses the depth that says which one moved. */
	.drawer {
		display: flex;
		flex-direction: column;
		height: 100%;
		padding-top: max(12px, env(safe-area-inset-top));
		background: var(--color-surface-elevated, var(--color-surface));
		color: var(--color-foreground);
	}

	.new-chat {
		display: flex;
		align-items: center;
		gap: 10px;
		margin: 4px 12px 10px;
		padding: 11px 12px;
		border: 1px solid var(--color-border);
		border-radius: 10px;
		background: var(--color-surface);
		color: var(--color-foreground);
		font-size: 15px;
		font-weight: 550;
		text-align: left;
		cursor: pointer;
		-webkit-tap-highlight-color: transparent;
	}
	.new-chat:active {
		opacity: 0.6;
	}

	.chats {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		-webkit-overflow-scrolling: touch;
		overscroll-behavior: contain;
		padding: 0 8px;
	}

	.chat-row {
		display: block;
		width: 100%;
		padding: 10px 10px;
		border: 0;
		border-radius: 8px;
		background: transparent;
		color: var(--color-foreground);
		font-size: 14px;
		text-align: left;
		cursor: pointer;
		-webkit-tap-highlight-color: transparent;
	}
	.chat-row.active {
		background: color-mix(in srgb, var(--color-foreground) 8%, transparent);
	}
	.chat-row:active {
		background: color-mix(in srgb, var(--color-foreground) 6%, transparent);
	}

	.chat-title {
		display: block;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.empty {
		padding: 16px 10px;
		font-size: 13px;
		color: var(--color-foreground-muted);
	}

	.foot {
		flex: none;
		padding: 6px 8px calc(10px + env(safe-area-inset-bottom));
		border-top: 1px solid var(--color-border);
	}

	.foot-row {
		display: flex;
		align-items: center;
		gap: 10px;
		width: 100%;
		padding: 11px 10px;
		border: 0;
		border-radius: 8px;
		background: transparent;
		color: var(--color-foreground);
		font-size: 14px;
		text-align: left;
		cursor: pointer;
		-webkit-tap-highlight-color: transparent;
	}
	.foot-row:active {
		background: color-mix(in srgb, var(--color-foreground) 6%, transparent);
	}
	.foot-row :global(svg) {
		color: var(--color-foreground-muted);
	}
</style>
