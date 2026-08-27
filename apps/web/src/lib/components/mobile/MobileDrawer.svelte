<script lang="ts">
	/**
	 * The phone's drawer — the app's entire navigation, as a full-screen room.
	 *
	 * Chat-first means the drawer IS the chat list, not a menu that links to
	 * one. The layout grammar is deliberately strict — the previous version
	 * had five text stylings and a bordered "New chat" card, and read as
	 * assembled rather than designed:
	 *
	 *   - TWO text voices in the list: row (16px) and caption (13px muted).
	 *     The serif masthead is the one exception, and it is the brand.
	 *   - Conversations carry their own relative time as a caption instead of
	 *     shouting day-bucket headers between them — one quiet section label,
	 *     then rows.
	 *   - Actions are not dressed as list rows: search, settings and compose
	 *     live in a pinned bottom bar (the thumb's home), compose as the one
	 *     filled control in the room — the primary verb, spent once.
	 *
	 * Because the viewport slides ALL the way off (see MobileShell), this is a
	 * standalone screen, so it carries the app's masthead — the same drawn ∴
	 * and serif wordmark as the desktop sidebar's mast — and its own close
	 * control, sitting in the exact slot the hamburger occupied so the toggle
	 * reads as one control changing state.
	 *
	 * Position, the slide and the gesture all belong to MobileShell; this
	 * component only renders content (the shell moves it for parallax).
	 */
	import Icon from "$lib/components/Icon.svelte";
	import AtlasIcon from "$lib/components/sidebar/AtlasIcon.svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { mobileLayout } from "$lib/stores/mobileLayout.svelte";
	import { chatSessions } from "$lib/stores/chatSessions.svelte";
	import { search } from "$lib/stores/search.svelte";

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

	function openSearch() {
		mobileLayout.closeDrawer();
		search.show();
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
</script>

<nav class="drawer" aria-label="Navigation">
	<header class="mast">
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
		<!-- Close rides top-RIGHT, and it's a double chevron, not an X: the
		     chat is parked off the right edge, and this points back at it —
		     "return", not "dismiss". It also matches the closing swipe's
		     direction, so the button and the gesture tell one story. -->
		<button class="close-btn" onclick={() => mobileLayout.closeDrawer()} aria-label="Back to chat">
			<Icon icon="ri:arrow-right-double-line" width={22} />
		</button>
	</header>

	<div class="body">
		<!-- Doors wear Atlas (the shell's drawn set), matching the desktop
		     sidebar's rule: Atlas for nav doors, Remix for interface symbols
		     (the close X above). This device leads: on a phone the device IS
		     the sensor, so "is this thing collecting?" outranks everything
		     below it. -->
		<button class="row" onclick={() => go("/virtues/devices/this", "This device")}>
			<AtlasIcon name="device" bare />
			<span class="row-text">This device</span>
		</button>

		<div class="section-label">Conversations</div>
		{#each chatSessions.sessions as s (s.conversation_id)}
			{@const route = `/chat/${s.conversation_id}`}
			<button
				class="chat-row"
				class:active={activeRoute === route}
				aria-current={activeRoute === route ? "page" : undefined}
				onclick={() => go(route, s.title || "Chat")}
			>
				<span class="chat-title">{s.title || "Untitled"}</span>
				<span class="chat-when">{when(s.last_message_at || s.first_message_at)}</span>
			</button>
		{:else}
			<div class="empty">Conversations you start will collect here.</div>
		{/each}
	</div>

	<footer class="bar">
		<button class="search-pill" onclick={openSearch}>
			<AtlasIcon name="search" bare />
			<span>Search</span>
		</button>
		<button class="bar-circle" onclick={() => go("/virtues/you", "Settings")} aria-label="Settings">
			<AtlasIcon name="settings" bare />
		</button>
		<button class="bar-circle filled" onclick={() => go("/chat", "Chat")} aria-label="New chat">
			<AtlasIcon name="new-chat" bare />
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
		/* NO safe-area padding of its own: the drawer lives inside `main`,
		   which already pads the status bar (see main.is-mobile in the app
		   layout). Padding it again pushed the mast a full notch-height below
		   the viewport's top bar, and the two bars are meant to share a
		   baseline — the » lands where the ghost/compose control sits. */
		background: var(--color-surface-elevated, var(--color-surface));
		color: var(--color-foreground);
	}

	/* Mirrors the shell's topbar height so the wordmark sits on the same
	   baseline the view's chrome does; the left inset lines the mark up with
	   the list rows' ink below it. */
	.mast {
		flex: none;
		display: flex;
		align-items: center;
		gap: 8px;
		height: 48px;
		padding: 0 6px 0 20px;
		user-select: none;
		-webkit-user-select: none;
	}

	.mark-word {
		flex: 1;
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
		padding: 4px 10px 12px;
	}

	/* Voice 1 of 2: a row. One size, one weight, everywhere in the list. */
	.row {
		display: flex;
		align-items: center;
		gap: 12px;
		width: 100%;
		min-height: 48px;
		padding: 0 10px;
		border: 0;
		border-radius: 10px;
		background: transparent;
		color: var(--color-foreground);
		text-align: left;
		cursor: pointer;
		-webkit-tap-highlight-color: transparent;
		transition: background-color 0.25s ease-out;
	}
	.row:active {
		background: color-mix(in srgb, var(--color-foreground) 6%, transparent);
		transition-duration: 0s;
	}
	.row :global(svg) {
		color: var(--color-foreground-muted);
	}

	.row-text {
		font-size: 16px;
	}

	/* Voice 2 of 2: a caption. The section label and the row times share it. */
	.section-label {
		margin: 18px 10px 6px;
		font-size: 13px;
		color: var(--color-foreground-muted);
	}

	.chat-row {
		display: block;
		width: 100%;
		min-height: 48px;
		padding: 7px 10px;
		border: 0;
		border-radius: 10px;
		background: transparent;
		color: var(--color-foreground);
		text-align: left;
		cursor: pointer;
		-webkit-tap-highlight-color: transparent;
		transition: background-color 0.25s ease-out;
	}
	.chat-row.active {
		background: color-mix(in srgb, var(--color-foreground) 7%, transparent);
	}
	.chat-row:active {
		background: color-mix(in srgb, var(--color-foreground) 6%, transparent);
		transition-duration: 0s;
	}

	.chat-title {
		display: block;
		font-size: 16px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.chat-when {
		display: block;
		margin-top: 1px;
		font-size: 13px;
		color: var(--color-foreground-muted);
	}

	.empty {
		padding: 16px 10px;
		font-size: 13px;
		color: var(--color-foreground-muted);
	}

	/* The thumb's row: search as a pill, settings quiet, compose filled — the
	   room's one filled control, because starting a conversation is the app's
	   primary verb. */
	.bar {
		flex: none;
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 10px 12px calc(10px + env(safe-area-inset-bottom));
	}

	.search-pill {
		flex: 1;
		display: flex;
		align-items: center;
		gap: 9px;
		min-height: 44px;
		padding: 0 16px;
		border: 0;
		border-radius: 999px;
		background: color-mix(in srgb, var(--color-foreground) 5%, transparent);
		color: var(--color-foreground-muted);
		font-size: 15px;
		text-align: left;
		cursor: pointer;
		-webkit-tap-highlight-color: transparent;
		transition: background-color 0.25s ease-out;
	}
	.search-pill:active {
		background: color-mix(in srgb, var(--color-foreground) 10%, transparent);
		transition-duration: 0s;
	}

	.bar-circle {
		flex: none;
		display: flex;
		align-items: center;
		justify-content: center;
		width: 44px;
		height: 44px;
		border: 0;
		border-radius: 999px;
		background: color-mix(in srgb, var(--color-foreground) 5%, transparent);
		color: var(--color-foreground);
		cursor: pointer;
		-webkit-tap-highlight-color: transparent;
		transition: background-color 0.25s ease-out;
	}
	.bar-circle:active {
		background: color-mix(in srgb, var(--color-foreground) 10%, transparent);
		transition-duration: 0s;
	}

	/* Atlas ships a .sidebar-icon color of its own (the desktop sidebar's);
	   in this room the buttons say what their glyphs wear. */
	.search-pill :global(svg),
	.bar-circle :global(svg) {
		color: currentColor;
	}

	.bar-circle.filled {
		background: var(--color-foreground);
		color: var(--color-surface);
	}
	.bar-circle.filled:active {
		background: color-mix(in srgb, var(--color-foreground) 85%, var(--color-surface));
	}
</style>
