<script lang="ts">
	/**
	 * The phone's drawer — the app's entire navigation, as a full-screen room.
	 *
	 * Chat-first means the drawer IS the chat list, not a menu that links to
	 * one: New chat at the top, recents grouped by day, and a pinned foot
	 * holding the two doors that aren't conversations — This device and
	 * Settings. That is the whole surface. Home, Search, Pages and the rest
	 * deliberately have no phone door for now; when one earns its way back it
	 * arrives as a row here, not as a bar.
	 *
	 * Because the viewport slides ALL the way off (see MobileShell), this is a
	 * standalone screen, so it carries the app's masthead — the same drawn ∴
	 * and serif wordmark as the desktop sidebar's mast — and its own close
	 * control, sitting in the exact slot the hamburger occupied so the toggle
	 * reads as one control changing state, not two controls trading places.
	 *
	 * Position, the slide and the gesture all belong to MobileShell; this
	 * component only renders content (the shell moves it for parallax).
	 */
	import Icon from "$lib/components/Icon.svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { mobileLayout } from "$lib/stores/mobileLayout.svelte";
	import { chatSessions, type ChatSession } from "$lib/stores/chatSessions.svelte";

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

	// ── Day buckets ──────────────────────────────────────────────────────────
	// Recency is the one thing a chat list's order encodes, so the group labels
	// say it out loud instead of leaving the reader to infer it from titles.
	// Local midnights, not 24h windows — "Yesterday" means the calendar day.
	interface Group {
		label: string;
		sessions: ChatSession[];
	}

	const groups = $derived.by((): Group[] => {
		const midnight = new Date();
		midnight.setHours(0, 0, 0, 0);
		const today = midnight.getTime();
		const day = 24 * 60 * 60 * 1000;

		const buckets: Group[] = [
			{ label: "Today", sessions: [] },
			{ label: "Yesterday", sessions: [] },
			{ label: "Previous 7 days", sessions: [] },
			{ label: "Earlier", sessions: [] },
		];
		for (const s of chatSessions.sessions) {
			const t = new Date(s.last_message_at || s.first_message_at).getTime();
			const idx = Number.isNaN(t) || t >= today ? 0
				: t >= today - day ? 1
				: t >= today - 7 * day ? 2
				: 3;
			buckets[idx].sessions.push(s);
		}
		return buckets.filter((b) => b.sessions.length > 0);
	});
</script>

<nav class="drawer" aria-label="Navigation">
	<header class="mast">
		<button class="close-btn" onclick={() => mobileLayout.closeDrawer()} aria-label="Close menu">
			<Icon icon="ri:close-line" width={22} />
		</button>
		<!-- The mark, drawn: same optical grid as the desktop mast — the
		     JJannon ∴ glyph is text-weight, a masthead needs logo weight. -->
		<span class="mark-glyph" aria-hidden="true">
			<svg viewBox="0 0 12 10.5" width="12" height="10.5" fill="currentColor">
				<circle cx="6" cy="2.4" r="1.5" />
				<circle cx="2.6" cy="8.1" r="1.5" />
				<circle cx="9.4" cy="8.1" r="1.5" />
			</svg>
		</span>
		<span class="mark-word">Virtues</span>
	</header>

	<div class="body">
		<button class="new-chat" onclick={() => go("/chat", "Chat")}>
			<Icon icon="ri:chat-new-line" width={18} />
			<span>New chat</span>
		</button>

		{#each groups as group (group.label)}
			<div class="group-label">{group.label}</div>
			{#each group.sessions as s (s.conversation_id)}
				{@const route = `/chat/${s.conversation_id}`}
				<button
					class="chat-row"
					class:active={activeRoute === route}
					aria-current={activeRoute === route ? "page" : undefined}
					onclick={() => go(route, s.title || "Chat")}
				>
					<span class="chat-title">{s.title || "Untitled"}</span>
				</button>
			{/each}
		{:else}
			<div class="empty">Conversations you start will collect here.</div>
		{/each}
	</div>

	<footer class="foot">
		<!-- This device sits above Settings on purpose: on a phone the device
		     IS the sensor — streams, permissions, the collector state that
		     lives on this hardware rather than on the box — so "is this thing
		     collecting?" outranks configuration. -->
		<button class="foot-row" onclick={() => go("/virtues/devices/this", "This device")}>
			<Icon icon="ri:smartphone-line" width={18} />
			<span>This device</span>
		</button>
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
		padding-top: env(safe-area-inset-top);
		background: var(--color-surface-elevated, var(--color-surface));
		color: var(--color-foreground);
	}

	/* Mirrors the shell's topbar metrics exactly, so the close control lands
	   in the hamburger's slot and the wordmark sits on the same baseline the
	   view's chrome does. */
	.mast {
		flex: none;
		display: flex;
		align-items: center;
		gap: 8px;
		height: 48px;
		padding: 0 6px;
		user-select: none;
		-webkit-user-select: none;
	}

	.close-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 44px;
		height: 44px;
		border: 0;
		border-radius: 10px;
		background: transparent;
		color: var(--color-foreground);
		cursor: pointer;
		-webkit-tap-highlight-color: transparent;
		transition: background-color 0.25s ease-out;
	}
	.close-btn:active {
		background: color-mix(in srgb, var(--color-foreground) 8%, transparent);
		transition-duration: 0s;
	}

	.mark-glyph {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 16px;
		flex-shrink: 0;
	}

	/* The desktop mast's treatment, verbatim: serif, never bold — the hairline
	   stroke is what gives it logo presence at text weight. */
	.mark-word {
		font-family: var(--font-serif, serif);
		font-size: 16px;
		font-weight: 400;
		letter-spacing: 0.025em;
		-webkit-text-stroke: 0.2px currentColor;
	}

	.body {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		-webkit-overflow-scrolling: touch;
		overscroll-behavior: contain;
		padding: 4px 12px 16px;
	}

	.new-chat {
		display: flex;
		align-items: center;
		gap: 10px;
		width: 100%;
		min-height: 44px;
		margin-bottom: 12px;
		padding: 0 12px;
		border: 1px solid var(--color-border);
		border-radius: 10px;
		background: var(--color-surface);
		color: var(--color-foreground);
		font-size: 15px;
		font-weight: 550;
		text-align: left;
		cursor: pointer;
		-webkit-tap-highlight-color: transparent;
		transition: background-color 0.25s ease-out;
	}
	.new-chat:active {
		background: color-mix(in srgb, var(--color-foreground) 6%, transparent);
		transition-duration: 0s;
	}

	.group-label {
		margin: 14px 6px 4px;
		font-size: 11px;
		font-weight: 550;
		letter-spacing: 0.04em;
		color: var(--color-foreground-muted);
	}

	.chat-row {
		display: block;
		width: 100%;
		min-height: 44px;
		padding: 0 10px;
		border: 0;
		border-radius: 8px;
		background: transparent;
		color: var(--color-foreground);
		font-size: 15px;
		text-align: left;
		cursor: pointer;
		-webkit-tap-highlight-color: transparent;
		transition: background-color 0.25s ease-out;
	}
	.chat-row.active {
		background: color-mix(in srgb, var(--color-foreground) 8%, transparent);
	}
	.chat-row:active {
		background: color-mix(in srgb, var(--color-foreground) 6%, transparent);
		transition-duration: 0s;
	}

	.chat-title {
		display: block;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.empty {
		padding: 24px 10px;
		font-size: 14px;
		color: var(--color-foreground-muted);
	}

	.foot {
		flex: none;
		padding: 6px 12px calc(10px + env(safe-area-inset-bottom));
		border-top: 0.5px solid var(--color-border);
	}

	.foot-row {
		display: flex;
		align-items: center;
		gap: 12px;
		width: 100%;
		min-height: 44px;
		padding: 0 10px;
		border: 0;
		border-radius: 8px;
		background: transparent;
		color: var(--color-foreground);
		font-size: 15px;
		text-align: left;
		cursor: pointer;
		-webkit-tap-highlight-color: transparent;
		transition: background-color 0.25s ease-out;
	}
	.foot-row:active {
		background: color-mix(in srgb, var(--color-foreground) 6%, transparent);
		transition-duration: 0s;
	}
	.foot-row :global(svg) {
		color: var(--color-foreground-muted);
	}
</style>
