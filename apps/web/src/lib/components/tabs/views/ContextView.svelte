<script lang="ts">
	import type { Tab } from '$lib/tabs/types';
	import { onMount } from 'svelte';

	interface Props {
		tab: Tab;
		active: boolean;
	}

	let { tab, active }: Props = $props();

	// Session usage data from API
	interface SessionUsage {
		session_id: string;
		model: string;
		context_window: number;
		total_tokens: number;
		usage_percentage: number;
		input_tokens: number;
		output_tokens: number;
		reasoning_tokens: number;
		cache_read_tokens: number;
		cache_write_tokens: number;
		total_cost_usd: number;
		user_message_count: number;
		assistant_message_count: number;
		first_message_at: string | null;
		last_message_at: string | null;
		compaction_status: {
			summary_exists: boolean;
			messages_summarized: number;
			messages_verbatim: number;
			summary_version: number;
			last_compacted_at: string | null;
		};
		context_status: string;
	}

	interface ToolCall {
		tool_name: string;
		tool_call_id?: string;
		arguments: unknown;
		result?: unknown;
		timestamp: string;
	}

	interface Message {
		id: string;
		role: string;
		content: string;
		timestamp: string;
		model?: string;
		tool_calls?: ToolCall[];
		reasoning?: string;
		subject?: string;
	}

	interface SessionDetail {
		conversation: {
			conversation_id: string;
			title: string;
			first_message_at: string;
			last_message_at: string;
			message_count: number;
			model?: string;
			provider?: string;
		};
		messages: Message[];
	}

	interface Breakdown {
		user: { tokens: number; pct: number };
		assistant: { tokens: number; pct: number };
		toolCalls: { tokens: number; pct: number };
		other: { tokens: number; pct: number };
	}

	let usage = $state<SessionUsage | null>(null);
	let session = $state<SessionDetail | null>(null);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let compacting = $state(false);
	let expandedMessages = $state<Set<string>>(new Set());

	function toggleExpand(id: string) {
		if (expandedMessages.has(id)) {
			expandedMessages.delete(id);
		} else {
			expandedMessages.add(id);
		}
		expandedMessages = new Set(expandedMessages);
	}

	const conversationId = $derived(tab.linkedConversationId);

	// Fetch session usage and detail data
	async function fetchData() {
		if (!conversationId) {
			error = 'No conversation ID';
			loading = false;
			return;
		}

		try {
			const [usageRes, sessionRes] = await Promise.all([
				fetch(`/api/chats/${conversationId}/usage`),
				fetch(`/api/chats/${conversationId}`)
			]);

			if (!usageRes.ok) throw new Error(`Failed to fetch usage: ${usageRes.status}`);
			if (!sessionRes.ok) throw new Error(`Failed to fetch session: ${sessionRes.status}`);

			usage = await usageRes.json();
			session = await sessionRes.json();
			error = null;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Unknown error';
		} finally {
			loading = false;
		}
	}

	// Compact the session
	async function handleCompact() {
		if (!conversationId || compacting) return;

		compacting = true;
		try {
			const res = await fetch(`/api/chats/${conversationId}/compact`, {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ force: true })
			});

			if (!res.ok) throw new Error(`Failed to compact: ${res.status}`);
			await fetchData();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Compaction failed';
		} finally {
			compacting = false;
		}
	}

	// Calculate token breakdown from messages
	function calculateBreakdown(messages: Message[]): Breakdown {
		let user = 0,
			assistant = 0,
			toolCalls = 0,
			other = 0;

		for (const msg of messages) {
			const contentTokens = Math.ceil((msg.content?.length || 0) / 4);

			if (msg.role === 'user') {
				user += contentTokens;
			} else if (msg.role === 'assistant') {
				assistant += contentTokens;
				if (msg.tool_calls) {
					for (const tc of msg.tool_calls) {
						toolCalls += Math.ceil(JSON.stringify(tc).length / 4);
					}
				}
				if (msg.reasoning) {
					other += Math.ceil(msg.reasoning.length / 4);
				}
			} else {
				other += contentTokens;
			}
		}

		const total = user + assistant + toolCalls + other || 1;
		return {
			user: { tokens: user, pct: (user / total) * 100 },
			assistant: { tokens: assistant, pct: (assistant / total) * 100 },
			toolCalls: { tokens: toolCalls, pct: (toolCalls / total) * 100 },
			other: { tokens: other, pct: (other / total) * 100 }
		};
	}

	// Format helpers
	function formatTokens(tokens: number): string {
		if (tokens >= 1_000_000) return `${(tokens / 1_000_000).toFixed(2)}M`;
		if (tokens >= 1_000) return `${(tokens / 1_000).toFixed(1)}K`;
		return tokens.toLocaleString();
	}

	function formatCost(cost: number): string {
		if (cost < 0.01) return `$${cost.toFixed(4)}`;
		return `$${cost.toFixed(2)}`;
	}

	function formatDate(date: string | null): string {
		if (!date) return '—';
		return new Date(date).toLocaleString();
	}

	function formatShortDate(date: string): string {
		return new Date(date).toLocaleString('en-US', {
			month: 'short',
			day: 'numeric',
			hour: 'numeric',
			minute: '2-digit'
		});
	}

	onMount(() => {
		fetchData();
	});

	$effect(() => {
		if (active && conversationId) {
			fetchData();
		}
	});

	const breakdown = $derived(session ? calculateBreakdown(session.messages) : null);
</script>

<div class="context-view">
	{#if loading}
		<div class="loading">Loading...</div>
	{:else if error}
		<div class="error">
			<span>{error}</span>
			<button type="button" onclick={fetchData}>Retry</button>
		</div>
	{:else if usage && session}
		<dl class="info-grid">
			<dt>Session</dt>
			<dd class="title">{session.conversation.title || 'Untitled'}</dd>

			<dt>Messages</dt>
			<dd>{session.conversation.message_count}</dd>

			<dt>Provider</dt>
			<dd>{session.conversation.provider || '—'}</dd>

			<dt>Model</dt>
			<dd class="mono">{usage.model}</dd>

			<dt>Context Limit</dt>
			<dd class="mono">{formatTokens(usage.context_window)}</dd>

			<dt>Total Tokens</dt>
			<dd class="mono">{formatTokens(usage.total_tokens)}</dd>

			<dt>Usage</dt>
			<dd class="mono">{usage.usage_percentage.toFixed(1)}%</dd>

			<dt>Input Tokens</dt>
			<dd class="mono">{formatTokens(usage.input_tokens)}</dd>

			<dt>Output Tokens</dt>
			<dd class="mono">{formatTokens(usage.output_tokens)}</dd>

			<dt>Reasoning Tokens</dt>
			<dd class="mono">{formatTokens(usage.reasoning_tokens)}</dd>

			<dt>Cache Tokens</dt>
			<dd class="mono">{formatTokens(usage.cache_read_tokens)} / {formatTokens(usage.cache_write_tokens)}</dd>

			<dt>User Messages</dt>
			<dd>{usage.user_message_count}</dd>

			<dt>Assistant Messages</dt>
			<dd>{usage.assistant_message_count}</dd>

			<dt>Total Cost</dt>
			<dd class="mono">{formatCost(usage.total_cost_usd)}</dd>

			<dt>Session Created</dt>
			<dd>{formatDate(usage.first_message_at)}</dd>

			<dt>Last Activity</dt>
			<dd>{formatDate(usage.last_message_at)}</dd>
		</dl>

		<!-- Context Breakdown Bar -->
		<div class="breakdown">
			<div class="breakdown-label">Context Breakdown</div>
			{#if breakdown && (breakdown.user.pct > 0 || breakdown.assistant.pct > 0 || breakdown.toolCalls.pct > 0 || breakdown.other.pct > 0)}
				<div class="bar">
					{#if breakdown.user.pct > 0}
						<div class="segment user" style="width: {breakdown.user.pct}%"></div>
					{/if}
					{#if breakdown.assistant.pct > 0}
						<div class="segment assistant" style="width: {breakdown.assistant.pct}%"></div>
					{/if}
					{#if breakdown.toolCalls.pct > 0}
						<div class="segment tools" style="width: {breakdown.toolCalls.pct}%"></div>
					{/if}
					{#if breakdown.other.pct > 0}
						<div class="segment other" style="width: {breakdown.other.pct}%"></div>
					{/if}
				</div>
				<div class="legend">
					<span><i class="dot user"></i> User {breakdown.user.pct.toFixed(1)}%</span>
					<span><i class="dot assistant"></i> Assistant {breakdown.assistant.pct.toFixed(1)}%</span>
					<span><i class="dot tools"></i> Tool Calls {breakdown.toolCalls.pct.toFixed(1)}%</span>
					<span><i class="dot other"></i> Other {breakdown.other.pct.toFixed(1)}%</span>
				</div>
			{:else}
				<div class="bar empty"></div>
				<div class="empty-note">No message data available for breakdown</div>
			{/if}
		</div>

		<!-- Raw Messages -->
		<div class="raw-messages">
			<div class="section-label">Raw messages ({session.messages?.length || 0})</div>
			{#if session.messages && session.messages.length > 0}
				<ul>
					{#each session.messages as msg, i}
						{@const msgId = msg.id || `msg_${i}`}
						{@const isExpanded = expandedMessages.has(msgId)}
						<li class="message-row" class:expanded={isExpanded}>
							<button class="row-toggle" onclick={() => toggleExpand(msgId)}>
								<span class="chevron">{isExpanded ? '▼' : '▶'}</span>
								<span class="role {msg.role}">{msg.role}</span>
								<span class="msg-id">{msgId}</span>
								<span class="timestamp">{formatShortDate(msg.timestamp)}</span>
							</button>
							{#if isExpanded}
								<div class="message-content">
									<pre>{msg.content || '(empty)'}</pre>
									{#if msg.reasoning}
										<div class="content-label">Reasoning:</div>
										<pre class="reasoning">{msg.reasoning}</pre>
									{/if}
								</div>
							{/if}
						</li>

						{#if msg.tool_calls}
							{#each msg.tool_calls as tc, ti}
								{@const tcId = `${msgId}_tool_${ti}`}
								{@const tcExpanded = expandedMessages.has(tcId)}
								<li class="message-row tool-call" class:expanded={tcExpanded}>
									<button class="row-toggle" onclick={() => toggleExpand(tcId)}>
										<span class="chevron">{tcExpanded ? '▼' : '▶'}</span>
										<span class="role tool">tool</span>
										<span class="tool-name">{tc.tool_name}</span>
										<span class="tool-id">{tc.tool_call_id || ''}</span>
									</button>
									{#if tcExpanded}
										<div class="message-content">
											<div class="content-label">Arguments:</div>
											<pre>{JSON.stringify(tc.arguments, null, 2)}</pre>
											{#if tc.result !== undefined}
												<div class="content-label">Result:</div>
												<pre>{JSON.stringify(tc.result, null, 2)}</pre>
											{/if}
										</div>
									{/if}
								</li>
							{/each}
						{/if}
					{/each}
				</ul>
			{:else}
				<div class="empty-note">No messages found in session data</div>
			{/if}
		</div>

		{#if usage.usage_percentage > 20}
			<button class="compact-btn" onclick={handleCompact} disabled={compacting}>
				{compacting ? 'Compacting...' : 'Compact Session'}
			</button>
		{/if}
	{/if}
</div>

<style>
	.context-view {
		height: 100%;
		overflow-y: auto;
		padding: 1.5rem;
		max-width: 600px;
	}

	.loading,
	.error {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.75rem;
		padding: 3rem;
		color: var(--color-text-secondary);
	}

	.error {
		color: var(--color-error);
	}

	.error button {
		padding: 0.5rem 1rem;
		border: 1px solid var(--color-border);
		background: var(--color-surface);
		border-radius: var(--radius-md);
		cursor: pointer;
	}

	.info-grid {
		display: grid;
		grid-template-columns: 140px 1fr;
		gap: 0.5rem 1rem;
		margin: 0;
	}

	.info-grid dt {
		color: var(--color-text-secondary);
		font-size: 0.875rem;
	}

	.info-grid dd {
		margin: 0;
		font-size: 0.875rem;
		color: var(--color-text);
	}

	.info-grid dd.title {
		font-weight: 500;
	}

	.info-grid dd.mono {
		font-family: var(--font-mono);
	}

	/* Breakdown bar */
	.breakdown {
		margin-top: 2rem;
	}

	.breakdown-label {
		font-size: 0.75rem;
		color: var(--color-text-secondary);
		margin-bottom: 0.5rem;
	}

	.bar {
		display: flex;
		height: 8px;
		border-radius: 4px;
		overflow: hidden;
		background: var(--color-surface-alt);
	}

	.segment {
		min-width: 2px;
	}

	.segment.user {
		background: #10b981;
	}
	.segment.assistant {
		background: #ec4899;
	}
	.segment.tools {
		background: #eab308;
	}
	.segment.other {
		background: #6b7280;
	}

	.legend {
		display: flex;
		flex-wrap: wrap;
		gap: 1rem;
		margin-top: 0.5rem;
		font-size: 0.75rem;
		color: var(--color-text-secondary);
	}

	.dot {
		display: inline-block;
		width: 8px;
		height: 8px;
		border-radius: 50%;
		margin-right: 4px;
		vertical-align: middle;
	}

	.dot.user {
		background: #10b981;
	}
	.dot.assistant {
		background: #ec4899;
	}
	.dot.tools {
		background: #eab308;
	}
	.dot.other {
		background: #6b7280;
	}

	.bar.empty {
		background: var(--color-surface-alt);
	}

	.empty-note {
		font-size: 0.75rem;
		color: var(--color-text-secondary);
		font-style: italic;
		margin-top: 0.5rem;
	}

	.section-label {
		font-size: 0.75rem;
		color: var(--color-text-secondary);
		margin-bottom: 0.5rem;
	}

	/* Raw messages */
	.raw-messages {
		margin-top: 2rem;
	}

	.raw-messages ul {
		list-style: none;
		padding: 0;
		margin: 0.5rem 0 0 0;
		max-height: 400px;
		overflow-y: auto;
	}

	.message-row {
		border-bottom: 1px solid var(--color-border);
	}

	.message-row:last-child {
		border-bottom: none;
	}

	.message-row.tool-call {
		padding-left: 1rem;
		background: var(--color-surface-alt);
	}

	.row-toggle {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		width: 100%;
		padding: 0.375rem 0.25rem;
		font-size: 0.8125rem;
		background: none;
		border: none;
		cursor: pointer;
		text-align: left;
		color: inherit;
	}

	.row-toggle:hover {
		background: var(--color-surface-hover);
	}

	.chevron {
		font-size: 0.625rem;
		color: var(--color-text-secondary);
		width: 0.75rem;
		flex-shrink: 0;
	}

	.raw-messages .role {
		min-width: 60px;
		color: var(--color-text-secondary);
		font-size: 0.75rem;
	}

	.raw-messages .role.user {
		color: #10b981;
	}

	.raw-messages .role.assistant {
		color: #ec4899;
	}

	.raw-messages .role.tool {
		color: #eab308;
	}

	.raw-messages .msg-id {
		flex: 1;
		font-family: var(--font-mono);
		font-size: 0.75rem;
		color: var(--color-text-secondary);
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.raw-messages .tool-name {
		flex: 1;
		font-family: var(--font-mono);
		font-size: 0.75rem;
		color: var(--color-text);
	}

	.raw-messages .tool-id {
		font-family: var(--font-mono);
		font-size: 0.6875rem;
		color: var(--color-text-secondary);
		overflow: hidden;
		text-overflow: ellipsis;
		max-width: 120px;
	}

	.raw-messages .timestamp {
		color: var(--color-text-secondary);
		white-space: nowrap;
		font-size: 0.75rem;
	}

	.message-content {
		padding: 0.5rem 0.5rem 0.75rem 1.75rem;
		border-top: 1px solid var(--color-border);
		background: var(--color-surface);
	}

	.message-content pre {
		margin: 0;
		padding: 0.5rem;
		font-family: var(--font-mono);
		font-size: 0.75rem;
		line-height: 1.4;
		white-space: pre-wrap;
		word-break: break-word;
		background: var(--color-surface-alt);
		border-radius: var(--radius-sm);
		max-height: 200px;
		overflow-y: auto;
	}

	.message-content pre.reasoning {
		color: var(--color-text-secondary);
		font-style: italic;
	}

	.content-label {
		font-size: 0.6875rem;
		color: var(--color-text-secondary);
		margin: 0.75rem 0 0.25rem 0;
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}

	.content-label:first-child {
		margin-top: 0;
	}

	/* Compact button */
	.compact-btn {
		margin-top: 2rem;
		padding: 0.5rem 1rem;
		background: transparent;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		font-size: 0.875rem;
		color: var(--color-text);
		cursor: pointer;
		transition: background-color 0.15s ease;
	}

	.compact-btn:hover:not(:disabled) {
		background: var(--color-surface-hover);
	}

	.compact-btn:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}
</style>
