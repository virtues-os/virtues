<script lang="ts">
	import type { Tab } from '$lib/tabs/types';
	import { workspaceStore } from '$lib/stores/workspace.svelte';
	import { Page } from '$lib';
	import { onMount } from 'svelte';

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	interface Session {
		conversation_id: string;
		title: string | null;
		first_message_at: string;
		last_message_at: string | null;
		message_count: number;
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
			const response = await fetch('/api/sessions');
			if (!response.ok) throw new Error('Failed to load sessions');
			const data = await response.json();
			sessions = data.conversations || [];
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load sessions';
		} finally {
			loading = false;
		}
	}

	function formatDate(dateStr: string): string {
		const date = new Date(dateStr);
		const now = new Date();
		const diffMs = now.getTime() - date.getTime();
		const diffDays = Math.floor(diffMs / (1000 * 60 * 60 * 24));

		if (diffDays === 0) {
			return date.toLocaleTimeString('en-US', {
				hour: 'numeric',
				minute: '2-digit'
			});
		} else if (diffDays === 1) {
			return 'Yesterday';
		} else if (diffDays < 7) {
			return date.toLocaleDateString('en-US', { weekday: 'long' });
		} else {
			return date.toLocaleDateString('en-US', {
				month: 'short',
				day: 'numeric',
				year: now.getFullYear() !== date.getFullYear() ? 'numeric' : undefined
			});
		}
	}

	function groupByDate(sessions: Session[]) {
		const groups: { label: string; sessions: Session[] }[] = [];
		const today = new Date();
		today.setHours(0, 0, 0, 0);
		const yesterday = new Date(today);
		yesterday.setDate(yesterday.getDate() - 1);
		const lastWeek = new Date(today);
		lastWeek.setDate(lastWeek.getDate() - 7);
		const lastMonth = new Date(today);
		lastMonth.setMonth(lastMonth.getMonth() - 1);

		const todaySessions: Session[] = [];
		const yesterdaySessions: Session[] = [];
		const lastWeekSessions: Session[] = [];
		const lastMonthSessions: Session[] = [];
		const olderSessions: Session[] = [];

		for (const session of sessions) {
			const date = new Date(session.last_message_at || session.first_message_at);
			date.setHours(0, 0, 0, 0);

			if (date >= today) {
				todaySessions.push(session);
			} else if (date >= yesterday) {
				yesterdaySessions.push(session);
			} else if (date >= lastWeek) {
				lastWeekSessions.push(session);
			} else if (date >= lastMonth) {
				lastMonthSessions.push(session);
			} else {
				olderSessions.push(session);
			}
		}

		if (todaySessions.length > 0) groups.push({ label: 'Today', sessions: todaySessions });
		if (yesterdaySessions.length > 0) groups.push({ label: 'Yesterday', sessions: yesterdaySessions });
		if (lastWeekSessions.length > 0) groups.push({ label: 'Last 7 days', sessions: lastWeekSessions });
		if (lastMonthSessions.length > 0) groups.push({ label: 'Last 30 days', sessions: lastMonthSessions });
		if (olderSessions.length > 0) groups.push({ label: 'Older', sessions: olderSessions });

		return groups;
	}

	const groupedSessions = $derived(groupByDate(sessions));

	function handleSessionClick(conversationId: string, title: string | null) {
		workspaceStore.openTabFromRoute(`/?conversationId=${conversationId}`);
	}
</script>

<Page>
	<div class="max-w-2xl">
		<div class="mb-8">
			<h1 class="text-3xl font-serif font-medium text-foreground mb-2">Chat History</h1>
			<p class="text-foreground-muted">
				{sessions.length} conversation{sessions.length !== 1 ? 's' : ''}
			</p>
		</div>

		{#if loading}
			<div class="text-center py-12 text-foreground-muted">Loading...</div>
		{:else if error}
			<div class="p-4 bg-error-subtle border border-error rounded-lg text-error">
				{error}
			</div>
		{:else if sessions.length === 0}
			<div class="text-center py-12 text-foreground-muted">
				<p>No conversations yet</p>
			</div>
		{:else}
			<div class="space-y-8">
				{#each groupedSessions as group}
					<div>
						<h2 class="text-xs font-medium uppercase tracking-wide text-foreground-muted mb-3">
							{group.label}
						</h2>
						<ul class="space-y-1">
							{#each group.sessions as session}
								<li>
									<button
										onclick={() => handleSessionClick(session.conversation_id, session.title)}
										class="w-full text-left block py-2 px-3 -mx-3 rounded-md hover:bg-surface-elevated transition-colors group"
									>
										<span class="text-foreground group-hover:text-primary transition-colors">
											{session.title || 'Untitled'}
										</span>
										<span class="text-foreground-subtle text-sm ml-2">
											{formatDate(session.last_message_at || session.first_message_at)}
										</span>
									</button>
								</li>
							{/each}
						</ul>
					</div>
				{/each}
			</div>
		{/if}
	</div>
</Page>
