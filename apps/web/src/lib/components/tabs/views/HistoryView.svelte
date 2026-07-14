<script lang="ts">
	import type { Tab } from "$lib/tabs/types";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { Page } from "$lib";
	import UniversalDataGrid, {
		type Column,
	} from "$lib/components/datagrid/UniversalDataGrid.svelte";
	import { onMount } from "svelte";
	import { formatRelativeTimestamp } from "$lib/utils/dateUtils";

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	interface Session {
		conversation_id: string;
		title: string | null;
		first_message_at: string;
		last_message_at: string | null;
		message_count: number;
	}

	interface ChatItem {
		id: string;
		title: string;
		updated_at: string;
	}

	let sessions = $state<Session[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);

	onMount(async () => {
		await loadSessions();
	});

	async function loadSessions() {
		loading = true;
		error = null;
		try {
			const response = await fetch("/api/chats");
			if (!response.ok) throw new Error("Failed to load sessions");
			const data = await response.json();
			sessions = data.conversations || [];
		} catch (e) {
			error = e instanceof Error ? e.message : "Failed to load sessions";
		} finally {
			loading = false;
		}
	}

	// Grid items: satisfy { id: string }, most recent first by default.
	const items = $derived<ChatItem[]>(
		sessions
			.map((s) => ({
				id: s.conversation_id,
				title: s.title || "Untitled",
				updated_at: s.last_message_at || s.first_message_at,
			}))
			.sort((a, b) => b.updated_at.localeCompare(a.updated_at)),
	);

	const columns: Column<ChatItem>[] = [
		{
			key: "title",
			label: "Title",
			icon: "ri:chat-3-line",
			width: "70%",
			minWidth: "200px",
		},
		{
			key: "updated_at",
			label: "Updated",
			icon: "ri:time-line",
			width: "30%",
			minWidth: "140px",
			format: "relative-date",
			hideOnMobile: true,
		},
	];

	function handleItemClick(item: ChatItem) {
		windowShellStore.openTabFromRoute(`/chat/${item.id}`, {
			label: item.title || "Chat",
		});
	}
</script>

<Page
	title="Chat History"
	description={`${sessions.length} conversation${sessions.length !== 1 ? "s" : ""}`}
	maxWidth="wide"
>
	<UniversalDataGrid
		{items}
		{columns}
		entityType="chat-history"
		{loading}
		{error}
		emptyIcon="ri:chat-history-line"
		emptyMessage="No conversations yet"
		loadingMessage="Loading conversations..."
		searchPlaceholder="Search chats..."
		onItemClick={handleItemClick}
		onRetry={loadSessions}
	>
		{#snippet tableRow(item: ChatItem)}
			<td class="col-title">
				<span class="title-text">{item.title}</span>
			</td>
			<td class="col-updated hide-mobile">
				<span class="date-text"
					>{formatRelativeTimestamp(item.updated_at)}</span
				>
			</td>
		{/snippet}

		{#snippet card(item: ChatItem)}
			<div class="card-content">
				<span class="card-title">{item.title}</span>
				<span class="date-text"
					>{formatRelativeTimestamp(item.updated_at)}</span
				>
			</div>
		{/snippet}
	</UniversalDataGrid>
</Page>

<style>
	.title-text {
		font-weight: 500;
		color: var(--color-foreground);
	}

	.date-text {
		color: var(--color-foreground-muted);
		font-size: 0.8125rem;
	}

	.card-content {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.5rem;
		text-align: center;
	}

	.card-title {
		font-weight: 600;
		font-size: 0.9375rem;
		color: var(--color-foreground);
		line-height: 1.3;
	}

	/* Column classes */
	.col-title {
		width: 70%;
		min-width: 200px;
		padding: 0.625rem 0.75rem;
		padding-left: 0;
	}

	.col-updated {
		width: 30%;
		min-width: 140px;
		padding: 0.625rem 0.75rem;
		padding-right: 0;
	}

	@media (max-width: 768px) {
		.hide-mobile {
			display: none;
		}

		.col-title {
			width: 100%;
		}
	}
</style>
