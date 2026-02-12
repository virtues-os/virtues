<script lang="ts">
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
		messages: Array<{
			id: string;
			role: string;
			content: string;
			timestamp: string;
			model?: string;
			tool_calls?: Array<{
				tool_name: string;
				tool_call_id?: string;
				arguments: unknown;
				result?: unknown;
				timestamp: string;
			}>;
			reasoning?: string;
		}>;
	}

	interface Breakdown {
		user: { tokens: number; pct: number };
		assistant: { tokens: number; pct: number };
		toolCalls: { tokens: number; pct: number };
		other: { tokens: number; pct: number };
	}

	interface Props {
		conversationId: string | undefined;
		active: boolean;
		onCompacted?: () => void;
	}

	let { conversationId, active, onCompacted }: Props = $props();

	let sessionUsage = $state<SessionUsage | null>(null);
	let sessionDetail = $state<SessionDetail | null>(null);
	let contextViewLoading = $state(false);
	let contextViewError = $state<string | null>(null);
	let compacting = $state(false);

	async function fetchContextViewData() {
		if (!conversationId) {
			contextViewError = 'No conversation ID';
			return;
		}

		contextViewLoading = true;
		contextViewError = null;

		try {
			const [usageRes, sessionRes] = await Promise.all([
				fetch(`/api/chats/${conversationId}/usage`),
				fetch(`/api/chats/${conversationId}`)
			]);

			if (!usageRes.ok) throw new Error(`Failed to fetch usage: ${usageRes.status}`);
			if (!sessionRes.ok) throw new Error(`Failed to fetch session: ${sessionRes.status}`);

			sessionUsage = await usageRes.json();
			sessionDetail = await sessionRes.json();
		} catch (e) {
			contextViewError = e instanceof Error ? e.message : 'Unknown error';
		} finally {
			contextViewLoading = false;
		}
	}

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

			await fetchContextViewData();
			onCompacted?.();
		} catch (e) {
			contextViewError = e instanceof Error ? e.message : 'Compaction failed';
		} finally {
			compacting = false;
		}
	}

	function calculateBreakdown(messages: SessionDetail['messages']): Breakdown {
		let user = 0, assistant = 0, toolCalls = 0, other = 0;

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

	const breakdown = $derived(sessionDetail ? calculateBreakdown(sessionDetail.messages) : null);

	$effect(() => {
		if (active && conversationId) {
			fetchContextViewData();
		}
	});
</script>

<div class="context-view">
	{#if contextViewLoading}
		<div class="cv-loading">Loading...</div>
	{:else if contextViewError}
		<div class="cv-error">
			<span>{contextViewError}</span>
			<button type="button" onclick={fetchContextViewData}>Retry</button>
		</div>
	{:else if sessionUsage && sessionDetail}
		<dl class="info-grid">
			<dt>Session</dt>
			<dd class="title">{sessionDetail.conversation.title || 'Untitled'}</dd>

			<dt>Messages</dt>
			<dd>{sessionDetail.conversation.message_count}</dd>

			<dt>Provider</dt>
			<dd>{sessionDetail.conversation.provider || '—'}</dd>

			<dt>Model</dt>
			<dd class="mono">{sessionUsage.model}</dd>

			<dt>Context Limit</dt>
			<dd class="mono">{formatTokens(sessionUsage.context_window)}</dd>

			<dt>Total Tokens</dt>
			<dd class="mono">{formatTokens(sessionUsage.total_tokens)}</dd>

			<dt>Usage</dt>
			<dd class="mono">{sessionUsage.usage_percentage.toFixed(1)}%</dd>

			<dt>Input Tokens</dt>
			<dd class="mono">{formatTokens(sessionUsage.input_tokens)}</dd>

			<dt>Output Tokens</dt>
			<dd class="mono">{formatTokens(sessionUsage.output_tokens)}</dd>

			<dt>Reasoning Tokens</dt>
			<dd class="mono">{formatTokens(sessionUsage.reasoning_tokens)}</dd>

			<dt>Cache Tokens</dt>
			<dd class="mono">{formatTokens(sessionUsage.cache_read_tokens)} / {formatTokens(sessionUsage.cache_write_tokens)}</dd>

			<dt>User Messages</dt>
			<dd>{sessionUsage.user_message_count}</dd>

			<dt>Assistant Messages</dt>
			<dd>{sessionUsage.assistant_message_count}</dd>

			<dt>Total Cost</dt>
			<dd class="mono">{formatCost(sessionUsage.total_cost_usd)}</dd>

			<dt>Session Created</dt>
			<dd>{formatDate(sessionUsage.first_message_at)}</dd>

			<dt>Last Activity</dt>
			<dd>{formatDate(sessionUsage.last_message_at)}</dd>
		</dl>

		<!-- Context Breakdown Bar -->
		<div class="cv-breakdown">
			<div class="cv-breakdown-label">Context Breakdown</div>
			{#if breakdown && (breakdown.user.pct > 0 || breakdown.assistant.pct > 0 || breakdown.toolCalls.pct > 0 || breakdown.other.pct > 0)}
				<div class="cv-bar">
					{#if breakdown.user.pct > 0}
						<div class="cv-segment cv-user" style="width: {breakdown.user.pct}%"></div>
					{/if}
					{#if breakdown.assistant.pct > 0}
						<div class="cv-segment cv-assistant" style="width: {breakdown.assistant.pct}%"></div>
					{/if}
					{#if breakdown.toolCalls.pct > 0}
						<div class="cv-segment cv-tools" style="width: {breakdown.toolCalls.pct}%"></div>
					{/if}
					{#if breakdown.other.pct > 0}
						<div class="cv-segment cv-other" style="width: {breakdown.other.pct}%"></div>
					{/if}
				</div>
				<div class="cv-legend">
					<span><i class="cv-dot cv-user"></i> User {breakdown.user.pct.toFixed(1)}%</span>
					<span><i class="cv-dot cv-assistant"></i> Assistant {breakdown.assistant.pct.toFixed(1)}%</span>
					<span><i class="cv-dot cv-tools"></i> Tool Calls {breakdown.toolCalls.pct.toFixed(1)}%</span>
					<span><i class="cv-dot cv-other"></i> Other {breakdown.other.pct.toFixed(1)}%</span>
				</div>
			{:else}
				<div class="cv-bar cv-empty"></div>
				<div class="cv-empty-note">No message data available for breakdown</div>
			{/if}
		</div>

		<!-- Raw Messages -->
		<div class="cv-raw-messages">
			<div class="cv-section-label">Raw messages ({sessionDetail.messages?.length || 0})</div>
			{#if sessionDetail.messages && sessionDetail.messages.length > 0}
				<ul>
					{#each sessionDetail.messages as msg, i}
						<li>
							<span class="cv-role">{msg.role}</span>
							<span class="cv-msg-id">{msg.id || `msg_${i}`}</span>
							<span class="cv-timestamp">{formatShortDate(msg.timestamp)}</span>
						</li>
					{/each}
				</ul>
			{:else}
				<div class="cv-empty-note">No messages found in session data</div>
			{/if}
		</div>

		{#if sessionUsage.usage_percentage > 20}
			<button class="cv-compact-btn" onclick={handleCompact} disabled={compacting}>
				{compacting ? 'Compacting...' : 'Compact Session'}
			</button>
		{/if}
	{:else}
		<div class="cv-loading">Loading session data...</div>
	{/if}
</div>

<style>
	.context-view {
		height: 100%;
		overflow-y: auto;
		padding: 1.5rem;
		max-width: 600px;
	}

	.cv-loading,
	.cv-error {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.75rem;
		padding: 3rem;
		color: var(--color-foreground-muted);
	}

	.cv-error {
		color: var(--color-error);
	}

	.cv-error button {
		padding: 0.5rem 1rem;
		border: 1px solid var(--color-border);
		background: var(--color-surface);
		border-radius: 6px;
		cursor: pointer;
	}

	.info-grid {
		display: grid;
		grid-template-columns: 140px 1fr;
		gap: 0.5rem 1rem;
		margin: 0;
	}

	.info-grid dt {
		color: var(--color-foreground-muted);
		font-size: 0.875rem;
	}

	.info-grid dd {
		margin: 0;
		font-size: 0.875rem;
		color: var(--color-foreground);
	}

	.info-grid dd.title {
		font-weight: 500;
	}

	.info-grid dd.mono {
		font-family: var(--font-mono);
	}

	.cv-breakdown {
		margin-top: 2rem;
	}

	.cv-breakdown-label,
	.cv-section-label {
		font-size: 0.75rem;
		color: var(--color-foreground-muted);
		margin-bottom: 0.5rem;
	}

	.cv-bar {
		display: flex;
		height: 8px;
		border-radius: 4px;
		overflow: hidden;
		background: var(--color-surface-elevated);
	}

	.cv-segment {
		min-width: 2px;
	}

	.cv-segment.cv-user { background: #10b981; }
	.cv-segment.cv-assistant { background: #ec4899; }
	.cv-segment.cv-tools { background: #eab308; }
	.cv-segment.cv-other { background: #6b7280; }

	.cv-legend {
		display: flex;
		flex-wrap: wrap;
		gap: 1rem;
		margin-top: 0.5rem;
		font-size: 0.75rem;
		color: var(--color-foreground-muted);
	}

	.cv-dot {
		display: inline-block;
		width: 8px;
		height: 8px;
		border-radius: 50%;
		margin-right: 4px;
		vertical-align: middle;
	}

	.cv-dot.cv-user { background: #10b981; }
	.cv-dot.cv-assistant { background: #ec4899; }
	.cv-dot.cv-tools { background: #eab308; }
	.cv-dot.cv-other { background: #6b7280; }

	.cv-bar.cv-empty {
		background: var(--color-surface-elevated);
	}

	.cv-empty-note {
		font-size: 0.75rem;
		color: var(--color-foreground-muted);
		font-style: italic;
		margin-top: 0.5rem;
	}

	.cv-raw-messages {
		margin-top: 2rem;
	}

	.cv-raw-messages ul {
		list-style: none;
		padding: 0;
		margin: 0.5rem 0 0 0;
		max-height: 300px;
		overflow-y: auto;
	}

	.cv-raw-messages li {
		display: flex;
		gap: 0.75rem;
		padding: 0.375rem 0;
		font-size: 0.8125rem;
		border-bottom: 1px solid var(--color-border);
	}

	.cv-raw-messages li:last-child {
		border-bottom: none;
	}

	.cv-role {
		min-width: 70px;
		color: var(--color-foreground-muted);
	}

	.cv-msg-id {
		flex: 1;
		font-family: var(--font-mono);
		font-size: 0.75rem;
		color: var(--color-foreground-muted);
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.cv-timestamp {
		color: var(--color-foreground-muted);
		white-space: nowrap;
	}

	.cv-compact-btn {
		margin-top: 2rem;
		padding: 0.5rem 1rem;
		background: transparent;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		font-size: 0.875rem;
		color: var(--color-foreground);
		cursor: pointer;
		transition: background-color 0.15s ease;
	}

	.cv-compact-btn:hover:not(:disabled) {
		background: var(--color-surface-hover);
	}

	.cv-compact-btn:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}
</style>
