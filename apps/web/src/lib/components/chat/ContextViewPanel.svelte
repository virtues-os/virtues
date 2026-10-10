<script lang="ts">
	import { untitled } from '$lib/refs/identity.svelte';
	import { getChatUsage, getChat, compactChat, getNextTurnContext, type NextTurnContext, type NextTurnPart } from '$lib/api/client';
	import { chatUsage } from '$lib/stores/chatUsage.svelte';
	import { chatInstances } from '$lib/stores/chatInstances.svelte';
	import { formatDateTime } from '$lib/utils/dateUtils';

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
	let nextTurn = $state<NextTurnContext | null>(null);
	let nextTurnError = $state<string | null>(null);

	async function fetchContextViewData() {
		if (!conversationId) {
			contextViewError = 'No conversation ID';
			return;
		}

		contextViewLoading = true;
		contextViewError = null;

		try {
			const [usage, session] = await Promise.all([
				getChatUsage<SessionUsage>(conversationId),
				getChat<SessionDetail>(conversationId)
			]);

			sessionUsage = usage;
			sessionDetail = session;
		} catch (e) {
			contextViewError = e instanceof Error ? e.message : 'Unknown error';
		} finally {
			contextViewLoading = false;
		}
		await fetchNextTurn();
	}

	/** The next message's request, asked with what the composer would send
	 *  beside it: the same getters its send reads. */
	async function fetchNextTurn() {
		if (!conversationId) return;
		const inputs = chatInstances.turnInputs(conversationId) ?? {
			agentMode: 'chat',
			chatMode: 'open',
			timezone: Intl.DateTimeFormat().resolvedOptions().timeZone
		};
		try {
			nextTurn = await getNextTurnContext(conversationId, inputs);
			nextTurnError = null;
		} catch (e) {
			nextTurnError = e instanceof Error ? e.message : "Your server couldn't build the next request";
		}
	}

	async function handleCompact() {
		if (!conversationId || compacting) return;

		compacting = true;
		try {
			await compactChat(conversationId, true);

			await fetchContextViewData();
			onCompacted?.();
		} catch (e) {
			contextViewError = e instanceof Error ? e.message : 'Compaction failed';
		} finally {
			compacting = false;
		}
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
		return formatDateTime(date);
	}

	const sum = (parts: NextTurnPart[]) => parts.reduce((n, p) => n + p.tokens, 0);

	/** The next request in four parts, the system prompt split where the
	 *  cache marker falls. */
	const nextTotals = $derived.by(() => {
		if (!nextTurn) return null;
		const held = sum(nextTurn.sections.filter((p) => p.cached));
		const perTurn = sum(nextTurn.sections.filter((p) => !p.cached));
		const tools = sum(nextTurn.tools);
		const conversation = sum(nextTurn.messages);
		const all = held + perTurn + tools + conversation;
		const pct = (n: number) => (all ? (n / all) * 100 : 0);
		return { held, perTurn, tools, conversation, all, pct };
	});

	const toolsBySize = $derived(nextTurn ? [...nextTurn.tools].sort((a, b) => b.tokens - a.tokens) : []);

	const SECTION_NAMES: Record<string, string> = {
		base: 'Character and guidance',
		precedence: 'Precedence',
		memory: 'Memory',
		circumstances: 'Circumstances',
		coverage: 'Coverage',
		active_project: 'Project',
		scoped: 'Scoped to the project',
		compacted_conversation: 'Earlier conversation, summarized',
		skill: 'Skill',
		active_context: 'Open page',
		rules: 'Rules',
		interview: 'Interview',
		getting_started: 'Getting started'
	};

	const ROLE_NAMES: Record<string, string> = { user: 'You', assistant: 'Assistant', tool: 'Tool result' };

	function firstLine(text: string | undefined): string {
		return (text ?? '').split('\n').find((line) => line.trim() && !line.startsWith('[Sent ')) ?? '';
	}

	// Read again whenever the chat saves a turn, not only on opening.
	$effect(() => {
		if (active && conversationId) {
			chatUsage.version(conversationId);
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
			<dd class="title">{sessionDetail.conversation.title || untitled('chat')}</dd>

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

			<dt>Read from cache</dt>
			<dd class="mono">{formatTokens(sessionUsage.cache_read_tokens)}</dd>

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

		<section class="cv-next">
			<div class="cv-next-head">
				<div class="cv-section-label">What the next message carries</div>
				<button type="button" class="cv-refresh" onclick={fetchNextTurn}>Refresh</button>
			</div>
			{#if nextTurnError}
				<div class="cv-empty-note">{nextTurnError}</div>
			{:else if nextTurn && nextTotals}
				<p class="cv-next-summary">
					About {formatTokens(nextTotals.all)} tokens go to {nextTurn.model} before your
					message{nextTurn.mode === 'chat' ? '' : `, in ${nextTurn.mode}`}.
				</p>
				<div class="cv-bar">
					<div class="cv-segment cv-held" style="width: {nextTotals.pct(nextTotals.held)}%"></div>
					{#if nextTotals.perTurn}
						<div class="cv-segment cv-per-turn" style="width: {nextTotals.pct(nextTotals.perTurn)}%"></div>
					{/if}
					<div class="cv-segment cv-tools" style="width: {nextTotals.pct(nextTotals.tools)}%"></div>
					<div class="cv-segment cv-conversation" style="width: {nextTotals.pct(nextTotals.conversation)}%"></div>
				</div>
				<div class="cv-legend">
					<span><i class="cv-dot cv-held"></i> Prompt {formatTokens(nextTotals.held)}</span>
					{#if nextTotals.perTurn}
						<span><i class="cv-dot cv-per-turn"></i> Prompt, per turn {formatTokens(nextTotals.perTurn)}</span>
					{/if}
					<span><i class="cv-dot cv-tools"></i> Tools {formatTokens(nextTotals.tools)}</span>
					<span><i class="cv-dot cv-conversation"></i> Conversation {formatTokens(nextTotals.conversation)}</span>
				</div>

				<div class="cv-group-label">System prompt</div>
				<ul class="cv-parts">
					{#each nextTurn.sections as part, i (i)}
						<li>
							<details>
								<summary>
									<span class="cv-part-name">{SECTION_NAMES[part.name] ?? part.name}</span>
									<span class="cv-part-note">{part.cached ? 'Same each turn' : 'Can change each turn'}</span>
									<span class="cv-part-tokens">{formatTokens(part.tokens)}</span>
								</summary>
								<pre class="cv-text">{part.text?.trim()}</pre>
							</details>
						</li>
					{/each}
				</ul>

				<details class="cv-group">
					<summary>
						<span class="cv-part-name">Tools ({nextTurn.tools.length})</span>
						<span class="cv-part-tokens">{formatTokens(nextTotals.tools)}</span>
					</summary>
					<ul class="cv-parts">
						{#each toolsBySize as tool (tool.name)}
							<li class="cv-row">
								<span class="cv-part-name">{tool.name}</span>
								<span class="cv-part-tokens">{formatTokens(tool.tokens)}</span>
							</li>
						{/each}
					</ul>
				</details>

				<details class="cv-group">
					<summary>
						<span class="cv-part-name">Conversation ({nextTurn.messages.length} messages)</span>
						<span class="cv-part-tokens">{formatTokens(nextTotals.conversation)}</span>
					</summary>
					<ul class="cv-parts">
						{#each nextTurn.messages as message, i (i)}
							<li>
								<details>
									<summary>
										<span class="cv-role">{ROLE_NAMES[message.name] ?? message.name}</span>
										<span class="cv-part-note cv-first-line">{firstLine(message.text)}</span>
										<span class="cv-part-tokens">{formatTokens(message.tokens)}</span>
									</summary>
									<pre class="cv-text">{message.text}</pre>
								</details>
							</li>
						{/each}
					</ul>
				</details>

				{#if nextTurn.recent_calls.length}
					<div class="cv-group-label">Latest calls</div>
					<table class="cv-calls">
						<thead>
							<tr><th>Step</th><th>Sent</th><th>From cache</th><th>Could be cached</th><th>Changed at</th></tr>
						</thead>
						<tbody>
							{#each [...nextTurn.recent_calls].reverse() as call (call.at)}
								<tr>
									<td>{call.step}</td>
									<td class="cv-num">{formatTokens(call.prompt_tokens)}</td>
									<td class="cv-num">{formatTokens(call.cache_read_tokens)}</td>
									<td class="cv-num">{formatTokens(call.reusable_tokens)}</td>
									<td>{call.diverged_at || '-'}</td>
								</tr>
							{/each}
						</tbody>
					</table>
					<p class="cv-empty-note">
						When a call read far less from cache than it could have, the provider missed. When Changed at names a place,
						the request itself changed there. Your server keeps this list until it restarts.
					</p>
				{/if}
			{:else}
				<div class="cv-empty-note">Loading...</div>
			{/if}
		</section>

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

	.cv-next {
		margin-top: 2rem;
	}

	.cv-next-head {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
	}

	.cv-refresh {
		background: none;
		border: none;
		padding: 0;
		font-size: 0.75rem;
		color: var(--color-foreground-muted);
		cursor: pointer;
	}

	.cv-refresh:hover {
		color: var(--color-foreground);
	}

	.cv-next-summary {
		margin: 0 0 0.75rem;
		font-size: 0.875rem;
		color: var(--color-foreground);
	}

	.cv-section-label,
	.cv-group-label {
		font-size: 0.75rem;
		color: var(--color-foreground-muted);
		margin-bottom: 0.5rem;
	}

	.cv-group-label {
		margin-top: 1.5rem;
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

	.cv-segment.cv-held { background: var(--cat-emerald); }
	.cv-segment.cv-per-turn { background: var(--cat-pink); }
	.cv-segment.cv-tools { background: var(--cat-yellow); }
	.cv-segment.cv-conversation { background: var(--color-foreground-muted); }

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

	.cv-dot.cv-held { background: var(--cat-emerald); }
	.cv-dot.cv-per-turn { background: var(--cat-pink); }
	.cv-dot.cv-tools { background: var(--cat-yellow); }
	.cv-dot.cv-conversation { background: var(--color-foreground-muted); }

	.cv-empty-note {
		font-size: 0.75rem;
		color: var(--color-foreground-muted);
		font-style: italic;
		margin-top: 0.5rem;
	}

	.cv-parts {
		list-style: none;
		padding: 0;
		margin: 0;
	}

	.cv-parts li,
	.cv-group {
		border-bottom: 1px solid var(--color-border);
	}

	.cv-group {
		margin-top: 1rem;
	}

	.cv-parts summary,
	.cv-group > summary,
	.cv-row {
		display: flex;
		gap: 0.75rem;
		align-items: baseline;
		padding: 0.375rem 0;
		font-size: 0.8125rem;
		cursor: pointer;
	}

	.cv-row {
		cursor: default;
	}

	.cv-part-name {
		color: var(--color-foreground);
	}

	.cv-part-note {
		flex: 1;
		min-width: 0;
		color: var(--color-foreground-muted);
		font-size: 0.75rem;
	}

	.cv-first-line {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.cv-part-tokens,
	.cv-num {
		font-variant-numeric: tabular-nums;
	}

	.cv-part-tokens {
		margin-left: auto;
		font-size: 0.75rem;
		color: var(--color-foreground-muted);
		white-space: nowrap;
	}

	.cv-role {
		min-width: 70px;
		color: var(--color-foreground-muted);
	}

	.cv-text {
		max-height: 320px;
		overflow: auto;
		margin: 0 0 0.75rem;
		padding: 0.75rem;
		background: var(--color-surface-elevated);
		border-radius: 6px;
		font-family: inherit;
		font-size: 0.75rem;
		white-space: pre-wrap;
		word-break: break-word;
	}

	.cv-calls {
		width: 100%;
		border-collapse: collapse;
		font-size: 0.75rem;
	}

	.cv-calls th,
	.cv-calls td {
		text-align: left;
		padding: 0.25rem 0.5rem 0.25rem 0;
		border-bottom: 1px solid var(--color-border);
	}

	.cv-calls th {
		font-weight: normal;
		color: var(--color-foreground-muted);
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
