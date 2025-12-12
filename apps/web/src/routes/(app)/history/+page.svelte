<script lang="ts">
	import { Page } from "$lib";
	import type { PageData } from "./$types";

	let { data }: { data: PageData } = $props();

	function formatDate(dateStr: string): string {
		const date = new Date(dateStr);
		const now = new Date();
		const diffMs = now.getTime() - date.getTime();
		const diffDays = Math.floor(diffMs / (1000 * 60 * 60 * 24));

		if (diffDays === 0) {
			return date.toLocaleTimeString("en-US", {
				hour: "numeric",
				minute: "2-digit",
			});
		} else if (diffDays === 1) {
			return "Yesterday";
		} else if (diffDays < 7) {
			return date.toLocaleDateString("en-US", { weekday: "long" });
		} else {
			return date.toLocaleDateString("en-US", {
				month: "short",
				day: "numeric",
				year: now.getFullYear() !== date.getFullYear() ? "numeric" : undefined,
			});
		}
	}

	// Group sessions by date
	function groupByDate(sessions: typeof data.sessions) {
		const groups: { label: string; sessions: typeof data.sessions }[] = [];
		const today = new Date();
		today.setHours(0, 0, 0, 0);
		const yesterday = new Date(today);
		yesterday.setDate(yesterday.getDate() - 1);
		const lastWeek = new Date(today);
		lastWeek.setDate(lastWeek.getDate() - 7);
		const lastMonth = new Date(today);
		lastMonth.setMonth(lastMonth.getMonth() - 1);

		const todaySessions: typeof data.sessions = [];
		const yesterdaySessions: typeof data.sessions = [];
		const lastWeekSessions: typeof data.sessions = [];
		const lastMonthSessions: typeof data.sessions = [];
		const olderSessions: typeof data.sessions = [];

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

		if (todaySessions.length > 0) groups.push({ label: "Today", sessions: todaySessions });
		if (yesterdaySessions.length > 0) groups.push({ label: "Yesterday", sessions: yesterdaySessions });
		if (lastWeekSessions.length > 0) groups.push({ label: "Last 7 days", sessions: lastWeekSessions });
		if (lastMonthSessions.length > 0) groups.push({ label: "Last 30 days", sessions: lastMonthSessions });
		if (olderSessions.length > 0) groups.push({ label: "Older", sessions: olderSessions });

		return groups;
	}

	const groupedSessions = $derived(groupByDate(data.sessions));
</script>

<svelte:head>
	<title>Chat History - Virtues</title>
</svelte:head>

<Page>
	<div class="max-w-2xl">
		<!-- Header -->
		<div class="mb-8">
			<h1 class="text-3xl font-serif font-medium text-foreground mb-2">
				Chat History
			</h1>
			<p class="text-foreground-muted">
				{data.sessions.length} conversation{data.sessions.length !== 1 ? 's' : ''}
			</p>
		</div>

		<!-- Error state -->
		{#if data.error}
			<div class="p-4 bg-error-subtle border border-error rounded-lg text-error">
				{data.error}
			</div>
		{:else if data.sessions.length === 0}
			<!-- Empty state -->
			<div class="text-center py-12 text-foreground-muted">
				<p>No conversations yet</p>
				<a href="/" class="text-primary hover:underline mt-2 inline-block">
					Start a new chat
				</a>
			</div>
		{:else}
			<!-- Grouped list -->
			<div class="space-y-8">
				{#each groupedSessions as group}
					<div>
						<h2 class="text-xs font-medium uppercase tracking-wide text-foreground-muted mb-3">
							{group.label}
						</h2>
						<ul class="space-y-1">
							{#each group.sessions as session}
								<li>
									<a
										href="/?conversationId={session.conversation_id}"
										class="block py-2 px-3 -mx-3 rounded-md hover:bg-surface-elevated transition-colors group"
									>
										<span class="text-foreground group-hover:text-primary transition-colors">
											{session.title || "Untitled"}
										</span>
										<span class="text-foreground-subtle text-sm ml-2">
											{formatDate(session.last_message_at || session.first_message_at)}
										</span>
									</a>
								</li>
							{/each}
						</ul>
					</div>
				{/each}
			</div>
		{/if}
	</div>
</Page>

