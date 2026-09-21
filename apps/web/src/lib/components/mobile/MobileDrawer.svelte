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
	 *   - Two regions, not one list: the mast and the doors (Search, New
	 *     chat, New page, Applets, All chats, Settings) are pinned chrome; only the
	 *     lists scroll — Projects, then Recents, the desktop panel's order —
	 *     and a hairline appears under the doors once the list has slid
	 *     beneath them — the way a navigation bar earns its rule. There used
	 *     to be a bottom bar too (search pill, compose), and search down there
	 *     sat one thumb-width from the chat's own search, so which search you
	 *     were in was never quite clear. Search is now the first door.
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
	import { projectStore } from "$lib/stores/project.svelte";
	import { search } from "$lib/stores/search.svelte";

	// Refresh the list whenever the drawer opens: it is the moment the user is
	// looking at it, the GET is small, and a stale list here reads as lost
	// conversations. The layout's boot-time load covers first paint.
	$effect(() => {
		if (mobileLayout.drawerOpen) {
			void chatSessions.refresh();
			void projectStore.load();
		}
	});

	const activeRoute = $derived(windowShellStore.activeTab?.route ?? "");

	// "Recents", literally: the drawer shows the last stretch of conversation
	// and All Chats carries the archive. Uncapped, the list buried the doors
	// above it and made the archive page redundant-but-worse.
	const RECENTS_CAP = 15;
	const recentSessions = $derived(chatSessions.sessions.slice(0, RECENTS_CAP));

	function go(route: string, label: string) {
		windowShellStore.openTabFromRoute(route, { label });
		mobileLayout.closeDrawer();
	}

	function openSearch() {
		mobileLayout.closeDrawer();
		search.show();
	}

	/** A page is written the way a chat is started, so its door stands under New chat. */
	async function newPage() {
		const { pagesStore } = await import("$lib/stores/pages.svelte");
		const page = await pagesStore.createNewPage();
		go(`/page/${page.id}`, page.title);
	}

	/** The list has scrolled under the doors; draw the rule between them. */
	let scrolled = $state(false);

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

	<!-- The doors: pinned, before the flow of conversations. They wear Atlas
	     (the shell's drawn set), matching the desktop sidebar's rule: Atlas
	     for nav doors, Remix for interface symbols (the close » above).
	     Search first, then New chat — the app's primary verb — then the full
	     archive, then Settings. That last door used to say "This device",
	     which was the truth about what the page held (this phone's streams
	     and its link to the server) and a mystery to anyone looking for
	     settings. One door, the word people look for. -->
	<div class="doors" class:scrolled>
		<!-- Search is a field, not a door: the one filled shape in the column,
		     so the eye finds it without reading. The pill treatment is the
		     old bottom bar's, moved up. -->
		<button class="search-pill" onclick={openSearch}>
			<AtlasIcon name="search" bare />
			<span>Search</span>
		</button>
		<button class="row" onclick={() => go("/chat", "Chat")}>
			<AtlasIcon name="new-chat" bare />
			<span class="row-text">New chat</span>
		</button>
		<button class="row" onclick={newPage}>
			<AtlasIcon name="pages" bare />
			<span class="row-text">New page</span>
		</button>
		<!-- Applets is a door beside New chat, here as on the desktop panel:
		     an applet is something you run from a chat, so its door stands
		     beside the chat's. -->
		<button class="row" onclick={() => go("/applets", "Applets")}>
			<AtlasIcon name="applets" bare />
			<span class="row-text">Applets</span>
		</button>
		<button class="row" onclick={() => go("/chat-history", "All Chats")}>
			<AtlasIcon name="chats" bare />
			<span class="row-text">All chats</span>
		</button>
		<button class="row" onclick={() => go("/virtues/devices/this", "Settings")}>
			<AtlasIcon name="settings" bare />
			<span class="row-text">Settings</span>
		</button>
	</div>

	<div class="body" onscroll={(e) => (scrolled = e.currentTarget.scrollTop > 0)}>
		{#if projectStore.projects.length > 0}
			<!-- The rooms a chat can live in, above the chats themselves: the
			     same order as the desktop panel. No empty state — a section
			     with nothing in it is a feature announcing itself. -->
			<div class="section-label">Projects</div>
			{#each projectStore.projects as p (p.id)}
				{@const route = `/project/${p.id}`}
				<button
					class="chat-row"
					class:active={activeRoute === route}
					aria-current={activeRoute === route ? "page" : undefined}
					onclick={() => go(route, p.name || "Project")}
				>
					<span class="chat-title">{p.name || "Untitled"}</span>
					<span class="chat-when">{p.chat_count === 1 ? "1 chat" : `${p.chat_count} chats`}</span>
				</button>
			{/each}
		{/if}
		<div class="section-label">Recents</div>
		{#each recentSessions as s (s.conversation_id)}
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
</nav>

<style>
	/* The same paint as the viewport (MobileShell), on purpose. It used to be
	   --surface-elevated so the two planes would read as different materials;
	   but the drawer fills the whole screen, so on a warm theme "elevated"
	   read as the page turning beige when the menu opened. Depth is the
	   sliding plane's shadow, which the viewport already casts on this one. */
	.drawer {
		display: flex;
		flex-direction: column;
		height: 100%;
		/* NO safe-area padding of its own: the drawer lives inside `main`,
		   which already pads the status bar (see main.is-mobile in the app
		   layout). Padding it again pushed the mast a full notch-height below
		   the viewport's top bar, and the two bars are meant to share a
		   baseline — the » lands where the ghost/compose control sits. */
		background-color: var(--color-surface);
		background-image: var(--background-image);
		background-blend-mode: multiply;
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

	/* Pinned chrome. The rule beneath is drawn only while the list is under
	   it: at rest the doors and the first rows share one column, and a
	   permanent line there would cut the room in two for no reason. */
	.doors {
		flex: none;
		padding: 2px 10px 6px;
		border-bottom: 1px solid transparent;
		transition: border-color 0.2s ease-out;
	}
	.doors.scrolled {
		border-bottom-color: var(--color-border);
	}

	.search-pill {
		display: flex;
		align-items: center;
		gap: 9px;
		width: 100%;
		min-height: 40px;
		margin: 0 0 6px;
		padding: 0 14px;
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
	/* Atlas ships a .sidebar-icon color of its own (the desktop sidebar's);
	   in this pill the control says what its glyph wears. */
	.search-pill :global(svg) {
		color: currentColor;
	}

	/* No bottom bar any more, so the list covers the home indicator itself. */
	.body {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		-webkit-overflow-scrolling: touch;
		overscroll-behavior: contain;
		padding: 0 10px calc(12px + env(safe-area-inset-bottom));
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
		margin: 12px 10px 6px;
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
</style>
