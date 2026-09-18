<script lang="ts">
	/**
	 * The Chats panel — the conversation list, which is what the sidebar shows
	 * most of the time, because talking to the box is what most sessions are.
	 *
	 * Grouped by when, not ranked by relevance. The shelf's own rule applies to
	 * a list as much as to a set of doors: a column that reorders itself is how
	 * a stable list stops being a place. Recency GROUPING is not reordering —
	 * the groups are fixed and a chat only ever moves down through them.
	 *
	 * This BROWSES; `/chat-history` still MANAGES — that page keeps the grid,
	 * the multi-select and the columns a 208px column can never carry.
	 */
	import { chatSessions, type ChatSession } from '$lib/stores/chatSessions.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import AtlasIcon from '../AtlasIcon.svelte';

	const UNTITLED = 'New chat';

	function titleOf(s: ChatSession): string {
		return s.title?.trim() || UNTITLED;
	}

	/** Midnight-relative, so "yesterday" means the calendar day, not 24 hours. */
	function bucketOf(s: ChatSession): 'today' | 'yesterday' | 'earlier' {
		if (!s.last_updated) return 'earlier';
		const then = new Date(s.last_updated);
		if (Number.isNaN(then.getTime())) return 'earlier';

		const startOfToday = new Date();
		startOfToday.setHours(0, 0, 0, 0);
		if (then >= startOfToday) return 'today';

		const startOfYesterday = new Date(startOfToday);
		startOfYesterday.setDate(startOfYesterday.getDate() - 1);
		if (then >= startOfYesterday) return 'yesterday';

		return 'earlier';
	}

	const groups = $derived.by(() => {
		const buckets: Record<string, ChatSession[]> = { today: [], yesterday: [], earlier: [] };
		for (const s of chatSessions.sessions) buckets[bucketOf(s)].push(s);
		return [
			{ id: 'today', label: 'Today', items: buckets.today },
			{ id: 'yesterday', label: 'Yesterday', items: buckets.yesterday },
			{ id: 'earlier', label: 'Earlier', items: buckets.earlier },
		].filter((g) => g.items.length > 0);
	});

	const activeRoute = $derived.by(() => {
		const pane = windowShellStore.activePane;
		const tab = pane?.tabs.find((t) => t.id === pane.activeTabId);
		return tab?.route ?? null;
	});

	function open(s: ChatSession) {
		windowShellStore.openTabFromRoute(`/chat/${s.conversation_id}`, {
			label: titleOf(s),
			focusExisting: true,
		});
	}

	function newChat() {
		windowShellStore.openTabFromRoute('/', { label: 'New chat', forceNew: true });
	}

	/** The All chats page IS the search: open it with the caret in its field. */
	function searchChats() {
		chatSessions.requestSearchFocus();
		windowShellStore.openTabFromRoute('/chat-history', {
			label: 'All chats',
			focusExisting: true,
		});
	}
</script>

<!-- Two doors before the list, the same two the mobile drawer leads with:
     the app's primary verb, then the way into the archive. They wear Atlas
     like every nav door in the shell. "Search chats" is not a field here —
     the search field lives on the All chats page, which has the room for
     results; the door only carries you there with the caret placed. -->
<div class="panel-doors">
	<button type="button" class="panel-row panel-door" onclick={newChat}>
		<AtlasIcon name="new-chat" size={16} bare />
		<span class="panel-row-text">New chat</span>
	</button>
	<button type="button" class="panel-row panel-door" onclick={searchChats}>
		<AtlasIcon name="search" size={16} bare />
		<span class="panel-row-text">Search chats</span>
	</button>
</div>

{#if chatSessions.sessions.length === 0}
	<p class="panel-empty">No chats yet.</p>
{:else}
	{#each groups as group (group.id)}
		<div class="panel-group-label">{group.label}</div>
		{#each group.items as session (session.conversation_id)}
			<button
				type="button"
				class="panel-row"
				class:active={activeRoute === `/chat/${session.conversation_id}`}
				onclick={() => open(session)}
			>
				<span class="panel-row-text">{titleOf(session)}</span>
			</button>
		{/each}
	{/each}
{/if}

<style>
	/* Sentence case, no tracking, no rule — design.md forbids uppercase with
	   wide tracking as something that "signals considered while doing no work",
	   and there are no hairline separators in the sidebar. The group is told
	   from its rows by being SMALLER and quieter, and by the air above it. */
	.panel-group-label {
		display: flex;
		align-items: center;
		height: 24px;
		padding-left: 12px;
		margin-top: 12px;
		font-size: 12px;
		color: var(--color-foreground-subtle);
	}

	.panel-group-label:first-child {
		margin-top: 0;
	}

	/* The doors sit in the flow above the list, told apart from it by air,
	   not a rule — the same way the groups are told from their rows. */
	.panel-doors {
		display: flex;
		flex-direction: column;
		margin-bottom: 12px;
	}

	.panel-door {
		gap: 8px;
		color: var(--color-foreground);
	}

	.panel-row {
		display: flex;
		align-items: center;
		width: 100%;
		height: var(--sidebar-interactive-height);
		padding: 0 12px;
		border: none;
		border-radius: var(--sidebar-interactive-radius);
		background: none;
		cursor: pointer;
		text-align: left;
		font-size: var(--sidebar-interactive-font-size);
		color: var(--color-foreground);
	}

	.panel-row:hover {
		background: var(--sidebar-hover-bg);
	}

	.panel-row.active {
		background: color-mix(in srgb, var(--color-foreground) 9%, transparent);
	}

	.panel-row:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: -2px;
	}

	.panel-row-text {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.panel-empty {
		margin: 0;
		padding: 0 12px;
		font-size: var(--sidebar-interactive-font-size);
		color: var(--color-foreground-muted);
	}
</style>
