<script lang="ts">
	/**
	 * The phone's drawer: the app's navigation, as a full-screen room.
	 *
	 * It reads as a menu on the plain page. The mark and Virtues sit small at
	 * the top with search, Settings and the way back. Under them, the doors
	 * the owner arranged on the desktop are tiles with ink icons, so color is
	 * left for threads. Threads come next, the six used most recently, each
	 * folding open to its latest chats along a tree line; the thread you are
	 * in opens by itself. Chats outside any thread follow. New chat is the one
	 * solid button, at the bottom where the thumb is.
	 *
	 * Position, the slide and the gesture all belong to MobileShell; this
	 * component only renders content (the shell moves it for parallax).
	 */


	import { untitled } from "$lib/refs/identity.svelte";
	import Icon from "$lib/components/Icon.svelte";
	import AtlasIcon from "$lib/components/sidebar/AtlasIcon.svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { mobileLayout } from "$lib/stores/mobileLayout.svelte";
	import { chatSessions } from "$lib/stores/chatSessions.svelte";
	import { chatActivity } from "$lib/stores/chatActivity.svelte";
	import { projectStore } from "$lib/stores/project.svelte";
	import { pinsStore } from "$lib/stores/pins.svelte";
	import { sidebarDoors, type DoorId } from "$lib/stores/sidebarDoors.svelte";
	import { pinIdentity } from "$lib/refs/identity.svelte";
	import { accentCss } from "$lib/sidebar/pin-colors";
	import { isEmoji, kindGlyph } from "$lib/utils/iconHelpers";
	import { openWebLink } from "$lib/utils/webLinks";
	import type { Pin } from "$lib/api/client";
	import ProjectGlyph from "$lib/components/ProjectGlyph.svelte";
	import { search } from "$lib/stores/search.svelte";

	// Refresh the list whenever the drawer opens: it is the moment the user is
	// looking at it, the GET is small, and a stale list here reads as lost
	// conversations. The layout's boot-time load covers first paint.
	$effect(() => {
		if (mobileLayout.drawerOpen) {
			void chatSessions.refresh();
			void projectStore.load();
			void pinsStore.load();
			void sidebarDoors.load();
		}
	});

	const activeRoute = $derived(windowShellStore.activeTab?.route ?? "");


	function go(route: string, label: string) {
		windowShellStore.openTabFromRoute(route, { label });
		mobileLayout.closeDrawer();
	}

	function openSearch() {
		mobileLayout.closeDrawer();
		search.show();
	}


	/** Each door, as on the desktop panel. The owner arranges them on the desktop. */
	const doorActions: Record<DoorId, { open: () => void }> = {
		"new-chat": { open: () => go("/chat", "Chat") },
		pages: { open: () => go("/page", "Pages") },
		threads: { open: () => go("/inbox", "Inbox") },
		applets: { open: () => go("/sites", "Sites") },
		wiki: { open: () => go("/wiki", "Wiki") },
		storage: { open: () => go("/storage", "Storage") },
	};

	function openPin(pin: Pin, title: string) {
		if (/^https?:\/\//i.test(pin.url)) {
			mobileLayout.closeDrawer();
			void openWebLink(pin.url);
			return;
		}
		go(pin.url, title);
	}


	/**
	 * A conversation's recency, said the way a person would: clock time today,
	 * a weekday inside the week, a date beyond it. Rows carry this as their
	 * caption, which is what lets the list get by with one section label.
	 *
	 * The parse is Safari-proof on purpose: WebKit returns NaN for the
	 * Postgres-style shapes Chromium happily accepts ("2026-08-27 18:22:33",
	 * a bare "+00" offset, no timezone at all) — which is exactly why the
	 * captions rendered in the dev browser and vanished on the phone. Space
	 * becomes T, "+00" becomes "+00:00", and a timestamp with no zone is
	 * declared UTC, which is what the server writes.
	 */
	function when(iso: string): string {
		let s = iso || "";
		if (!s.includes("T")) s = s.replace(" ", "T");
		if (/[+-]\d\d$/.test(s)) s += ":00";
		else if (!/([zZ]|[+-]\d\d:\d\d)$/.test(s)) s += "Z";
		const d = new Date(s);
		if (Number.isNaN(d.getTime())) return "";
		const midnight = new Date();
		midnight.setHours(0, 0, 0, 0);
		const day = 24 * 60 * 60 * 1000;
		const t = d.getTime();
		if (t >= midnight.getTime())
			return d.toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });
		if (t >= midnight.getTime() - day) return "Yesterday";
		if (t >= midnight.getTime() - 6 * day)
			return d.toLocaleDateString(undefined, { weekday: "long" });
		return d.toLocaleDateString(undefined, { month: "short", day: "numeric" });
	}

	const THREAD_CHATS = 3;
	const OTHER_CAP = 8;
	const activeChatId = $derived(activeRoute.startsWith("/chat/") ? activeRoute.slice("/chat/".length) : null);
	const activeThreadId = $derived(
		activeRoute.startsWith("/thread/")
			? activeRoute.slice("/thread/".length)
			: (chatSessions.sessions.find((s) => s.conversation_id === activeChatId)?.thread_id ?? null),
	);
	// The threads you used last, by their latest chat, then the rest in the
	// store's order; All threads holds the full list.
	const THREAD_CAP = 6;
	const threads = $derived.by(() => {
		const lastUsed = new Map<string, number>();
		chatSessions.sessions.forEach((s, i) => {
			if (s.thread_id && !lastUsed.has(s.thread_id)) lastUsed.set(s.thread_id, i);
		});
		const rank = (id: string) => lastUsed.get(id) ?? Number.MAX_SAFE_INTEGER;
		return [...projectStore.projects].sort((a, b) => rank(a.id) - rank(b.id)).slice(0, THREAD_CAP);
	});
	const otherChats = $derived(chatSessions.sessions.filter((s) => !s.thread_id).slice(0, OTHER_CAP));
	function chatsIn(threadId: string) {
		return chatSessions.sessions.filter((s) => s.thread_id === threadId).slice(0, THREAD_CHATS);
	}

	// Which threads are folded open. The one you are in opens by itself; a
	// tap on a chevron opens or closes any other.
	let opened = $state<Record<string, boolean>>({});
	function isOpen(id: string): boolean {
		return opened[id] ?? id === activeThreadId;
	}
	function toggle(id: string) {
		opened = { ...opened, [id]: !isOpen(id) };
	}

	// The doors other than new chat, as tiles. New chat has its own button.
	const tileDoors = $derived(sidebarDoors.visible.filter((d) => d.id !== "new-chat"));

	function newChat() {
		go("/chat", "Chat");
	}
</script>

<nav class="drawer" aria-label="Navigation">
	<header class="mast">
		<span class="mark-word">
			<svg viewBox="0 0 12 10.5" width="13" height="11" fill="currentColor" aria-hidden="true">
				<circle cx="6" cy="2.4" r="1.5" />
				<circle cx="2.6" cy="8.1" r="1.5" />
				<circle cx="9.4" cy="8.1" r="1.5" />
			</svg>
			Virtues
		</span>
		<button class="square lq" onclick={openSearch} aria-label="Search">
			<AtlasIcon name="search" size={17} bare />
		</button>
		<button class="square lq" onclick={() => go("/virtues/devices/this", "Settings")} aria-label="Settings">
			<AtlasIcon name="settings" size={17} bare />
		</button>
		<!-- Back to the chat, parked off the right edge. The swipe closes the
		     drawer too; this is the one you can see. -->
		<button class="square lq" onclick={() => mobileLayout.closeDrawer()} aria-label="Back to chat">
			<Icon icon="mynaui:x" width={17} />
		</button>
	</header>

	{#if tileDoors.length > 0}
		<div class="tiles" style:--tile-count={Math.min(tileDoors.length, 5)}>
			{#each tileDoors as door (door.id)}
				<button class="tile" onclick={doorActions[door.id].open}>
					<span class="tile-face"><AtlasIcon name={door.glyph} size={20} bare /></span>
					<span class="tile-label">{door.label}</span>
				</button>
			{/each}
		</div>
	{/if}

	<div class="body">
		{#if pinsStore.pins.length > 0}
			<div class="section"><span>Pinned</span></div>
			{#each pinsStore.pins as pin (pin.id)}
				{@const look = pinIdentity(pin)}
				<button
					class="row"
					class:active={activeRoute === pin.url}
					aria-current={activeRoute === pin.url ? "page" : undefined}
					onclick={() => openPin(pin, look.title)}
				>
					<span class="mark" aria-hidden="true" style={accentCss(look.color) ? `color: ${accentCss(look.color)}` : undefined}>
						{#if look.icon && isEmoji(look.icon)}
							{look.icon}
						{:else if look.icon?.startsWith("atlas:")}
							<AtlasIcon name={look.icon.slice("atlas:".length)} size={16} bare />
						{:else if look.icon}
							<Icon icon={look.icon} width={16} />
						{:else}
							<AtlasIcon name={kindGlyph(pin.url)} size={16} bare />
						{/if}
					</span>
					<span class="text">{look.title}</span>
				</button>
			{/each}
		{/if}

		<div class="section">
			<span>Threads</span>
			<button class="more" onclick={() => go("/threads", "Threads")}>
				All threads<Icon icon="mynaui:chevron-right" width={12} />
			</button>
		</div>
		{#each threads as t (t.id)}
			{@const open = isOpen(t.id)}
			{@const route = `/thread/${t.id}`}
			<div class="thread-row">
				<button
					class="row thread"
					class:active={activeRoute === route}
					aria-current={activeRoute === route ? "page" : undefined}
					onclick={() => go(route, t.name)}
				>
					<span class="mark"><ProjectGlyph project={t} size={16} /></span>
					<span class="text">{t.name}</span>
				</button>
				<button
					class="fold"
					class:open
					onclick={() => toggle(t.id)}
					aria-expanded={open}
					aria-label={open ? `Hide chats in ${t.name}` : `Show chats in ${t.name}`}
				>
					<Icon icon="mynaui:chevron-right" width={15} />
				</button>
			</div>
			{#if open}
				<!-- Its chats hang off the thread along a tree line. -->
				<div class="branch">
					{#each chatsIn(t.id) as s (s.conversation_id)}
						{@const chatRoute = `/chat/${s.conversation_id}`}
						<button
							class="row nested"
							class:active={activeRoute === chatRoute}
							aria-current={activeRoute === chatRoute ? "page" : undefined}
							onclick={() => go(chatRoute, s.title || "Chat")}
						>
							<span class="text">{s.title || untitled("chat")}</span>
							{#if chatActivity.running(s.conversation_id)}
								<Icon icon="mynaui:spinner" width="14" class="spin state-mark" />
							{:else if chatActivity.unread(s.conversation_id)}
								<span class="row-unread" role="img" aria-label="New reply"></span>
							{/if}
						</button>
					{:else}
						<div class="empty">No chats in this thread yet.</div>
					{/each}
				</div>
			{/if}
		{/each}

		<div class="section">
			<span>Other chats</span>
			<button class="more" onclick={() => go("/chat-history", "All Chats")}>
				All chats<Icon icon="mynaui:chevron-right" width={12} />
			</button>
		</div>
		{#each otherChats as s (s.conversation_id)}
			{@const route = `/chat/${s.conversation_id}`}
			<button
				class="row"
				class:active={activeRoute === route}
				aria-current={activeRoute === route ? "page" : undefined}
				onclick={() => go(route, s.title || "Chat")}
			>
				<span class="text">{s.title || untitled("chat")}</span>
				{#if chatActivity.running(s.conversation_id)}
					<Icon icon="mynaui:spinner" width="14" class="spin state-mark" />
				{:else if chatActivity.unread(s.conversation_id)}
					<span class="row-unread" role="img" aria-label="New reply"></span>
				{:else}
					<span class="when">{when(s.last_message_at || s.first_message_at)}</span>
				{/if}
			</button>
		{:else}
			<div class="empty">Chats you start outside a thread collect here.</div>
		{/each}
	</div>

	<footer class="dock">
		<button class="compose" onclick={newChat}>
			<Icon icon="mynaui:plus" width={18} />New chat
		</button>
	</footer>
</nav>

<style>
	.drawer {
		display: flex;
		flex-direction: column;
		height: 100%;
		background-color: var(--color-page);
		color: var(--color-foreground);
	}

	.mast {
		flex: none;
		display: flex;
		align-items: center;
		height: 44px;
		margin: 8px 12px 0;
	}
	.mark-word {
		flex: 1;
		display: flex;
		align-items: center;
		gap: 8px;
		padding-left: 4px;
		font-family: var(--font-serif);
		font-size: 1.25rem;
	}

	/* 36px of button with a 44px reach: the margin is part of the target. */
	.square {
		flex: none;
		display: flex;
		align-items: center;
		justify-content: center;
		width: 36px;
		height: 36px;
		margin: 4px;
		border: 0;
		border-radius: var(--radius-lg);
		color: var(--color-foreground);
		cursor: pointer;
		-webkit-tap-highlight-color: transparent;
	}

	.tiles {
		flex: none;
		display: grid;
		grid-template-columns: repeat(var(--tile-count), minmax(0, 1fr));
		gap: 4px;
		padding: 12px 12px 4px;
	}
	.tile {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 4px;
		border: 0;
		background: transparent;
		color: var(--color-foreground);
		font: inherit;
		cursor: pointer;
		-webkit-tap-highlight-color: transparent;
	}
	.tile-face {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 48px;
		height: 48px;
		border-radius: var(--radius-xl);
		background: var(--color-surface);
		box-shadow: 0 0 0 0.5px var(--color-border);
	}
	.tile-label {
		font-size: 0.6875rem;
		color: var(--color-foreground-muted);
	}

	.body {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		/* Clears the fade above New chat, so the last row can be reached. */
		padding: 0 12px 32px;
	}

	.section {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 12px 8px 4px;
		font-size: 0.75rem;
		color: var(--color-foreground-muted);
	}
	.more {
		display: flex;
		align-items: center;
		gap: 2px;
		min-height: 32px;
		border: 0;
		background: transparent;
		color: var(--color-foreground);
		font: inherit;
		font-size: 0.75rem;
		cursor: pointer;
	}

	.row {
		display: flex;
		align-items: center;
		gap: 12px;
		width: 100%;
		min-height: 44px;
		padding: 0 8px;
		border: 0;
		border-radius: var(--radius-lg);
		background: transparent;
		color: var(--color-foreground);
		font: inherit;
		font-size: 1rem;
		text-align: left;
		cursor: pointer;
		-webkit-tap-highlight-color: transparent;
	}
	.row.active {
		background: color-mix(in srgb, var(--wash-ink) 6%, transparent);
	}

	.thread-row {
		display: flex;
		align-items: center;
	}
	.thread-row .thread {
		flex: 1;
		min-width: 0;
	}

	.fold {
		flex: none;
		display: flex;
		align-items: center;
		justify-content: center;
		width: 44px;
		height: 44px;
		border: 0;
		background: transparent;
		color: var(--color-foreground-subtle);
		cursor: pointer;
		-webkit-tap-highlight-color: transparent;
		transition: transform 0.2s ease-out;
	}
	.fold.open {
		transform: rotate(90deg);
	}

	/* The tree line starts under the thread's icon. */
	.branch {
		margin: 0 0 4px 16px;
		padding-left: 12px;
		border-left: 1px solid var(--color-border);
	}
	.row.nested {
		min-height: 40px;
		font-size: 0.9375rem;
		color: var(--color-foreground-muted);
	}
	.row.nested.active {
		color: var(--color-foreground);
	}

	.mark {
		flex: none;
		display: flex;
		align-items: center;
		justify-content: center;
		width: 16px;
	}
	.text {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.when {
		flex: none;
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
	}
	.empty {
		padding: 8px;
		font-size: 0.875rem;
		color: var(--color-foreground-muted);
	}
	.row-unread {
		flex: none;
		width: 8px;
		height: 8px;
		border-radius: var(--radius-full);
		background: var(--color-success);
	}
	.row :global(.state-mark) {
		flex: none;
		color: var(--color-foreground-muted);
	}

	/* The list fades out above the one solid button. */
	.dock {
		flex: none;
		padding: 8px 16px calc(16px + env(safe-area-inset-bottom));
		background: linear-gradient(transparent, var(--color-page) 24px);
		margin-top: -24px;
		position: relative;
	}
	.compose {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 8px;
		width: 100%;
		height: 48px;
		border: 0;
		border-radius: var(--radius-xl);
		background: var(--color-foreground);
		color: var(--color-surface);
		font: inherit;
		font-size: 1rem;
		cursor: pointer;
		-webkit-tap-highlight-color: transparent;
	}

	/* The press: a quick shrink under the finger and a spring back. */
	.row,
	.square,
	.tile,
	.compose {
		transition:
			transform 0.32s cubic-bezier(0.34, 1.56, 0.64, 1),
			background-color 0.2s ease-out;
	}
	.square:active,
	.tile:active,
	.compose:active {
		transform: scale(0.94);
		transition-duration: 0.08s;
	}
	.row:active {
		transform: scale(0.98);
		background: color-mix(in srgb, var(--wash-ink) 6%, transparent);
		transition-duration: 0.08s;
	}
	@media (prefers-reduced-motion: reduce) {
		.row,
		.square,
		.tile,
		.compose,
		.fold {
			transition: none;
		}
		.row:active,
		.square:active,
		.tile:active,
		.compose:active {
			transform: none;
		}
	}
</style>
