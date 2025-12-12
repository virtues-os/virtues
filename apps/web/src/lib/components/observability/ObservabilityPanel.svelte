<script lang="ts">
	import "iconify-icon";
	import TraceItem from "./TraceItem.svelte";
	import ExpandableContent from "./ExpandableContent.svelte";
	import type { ConversationTrace, SpanTrace } from "$lib/types/Trace";

	interface ToolPart {
		type: string;
		text?: string;
		toolCallId?: string;
		toolName?: string;
		input?: Record<string, unknown>;
		state?: "pending" | "output-available" | "output-error";
		output?: unknown;
		errorText?: string;
	}

	interface Message {
		id: string;
		role: "user" | "assistant" | "system";
		parts: ToolPart[];
	}

	let { messages = [], trace = null } = $props<{
		messages: Message[];
		trace?: ConversationTrace | null;
	}>();

	// Tab state
	type Tab = "overview" | "request" | "tools";
	let activeTab = $state<Tab>("overview");

	// Selected span index for viewing
	let selectedSpan = $state<number | null>(null);

	// Get the span trace for the selected or most recent span
	const currentSpan = $derived.by((): SpanTrace | null => {
		if (!trace?.spans || trace.spans.length === 0) return null;

		const index = selectedSpan ?? trace.spans.length - 1;
		return trace.spans[index] ?? null;
	});

	// Extract tool call parts from all messages
	const flattenedToolCalls = $derived.by(() => {
		const toolCalls: Array<{
			part: ToolPart;
			index: number;
			messageId: string;
		}> = [];

		let globalIndex = 0;
		for (const msg of messages) {
			if (msg.role === "assistant") {
				const tools = msg.parts.filter(
					(p: ToolPart) => p.type.startsWith("tool-") && p.toolName,
				);
				for (const tool of tools) {
					toolCalls.push({
						part: tool,
						index: globalIndex++,
						messageId: msg.id,
					});
				}
			}
		}
		return toolCalls;
	});

	// Count total tool calls
	const totalToolCalls = $derived(
		messages.reduce((count: number, msg: Message) => {
			return (
				count +
				msg.parts.filter((p: ToolPart) => p.type.startsWith("tool-"))
					.length
			);
		}, 0),
	);

	// Calculate total tokens from all spans
	const totalTokens = $derived.by(() => {
		if (!trace?.spans) return null;
		let prompt = 0;
		let completion = 0;
		for (const span of trace.spans) {
			if (span.promptTokens) prompt += span.promptTokens;
			if (span.completionTokens) completion += span.completionTokens;
		}
		return prompt + completion > 0
			? { prompt, completion, total: prompt + completion }
			: null;
	});

	// Parse stringified JSON safely
	function parseJson(str: string | undefined): unknown {
		if (!str) return null;
		try {
			return JSON.parse(str);
		} catch {
			return null;
		}
	}

	// Format token numbers
	function formatTokens(n: number): string {
		if (n >= 1000) {
			return (n / 1000).toFixed(1) + "k";
		}
		return n.toString();
	}

	// Format duration in milliseconds
	function formatDuration(ms: number | undefined): string {
		if (!ms) return "-";
		if (ms < 1000) return `${Math.round(ms)}ms`;
		return `${(ms / 1000).toFixed(2)}s`;
	}
</script>

<div class="observability-panel">
	<!-- Header with tabs -->
	<div class="panel-header">
		<div class="header-left">
			<div class="header-title">
				<iconify-icon icon="ri:pulse-line" width="18" height="18"
				></iconify-icon>
				<span>Trace</span>
			</div>
			{#if trace?.spans && trace.spans.length > 1}
				<select class="exchange-select" bind:value={selectedSpan}>
					{#each trace.spans as span, i}
						<option value={i}>Span {i + 1}</option>
					{/each}
				</select>
			{/if}
		</div>
		<div class="header-stats">
			{#if totalTokens}
				<span class="stat" title="Total tokens used">
					<iconify-icon icon="ri:cpu-line" width="14" height="14"
					></iconify-icon>
					{formatTokens(totalTokens.total)}
				</span>
			{/if}
			<span class="stat">
				<iconify-icon icon="ri:tools-line" width="14" height="14"
				></iconify-icon>
				{totalToolCalls}
			</span>
		</div>
	</div>

	<!-- Tab navigation -->
	<div class="tab-nav">
		<button
			class="tab-button"
			class:active={activeTab === "overview"}
			onclick={() => (activeTab = "overview")}
		>
			Overview
		</button>
		<button
			class="tab-button"
			class:active={activeTab === "request"}
			onclick={() => (activeTab = "request")}
		>
			Request
		</button>
		<button
			class="tab-button"
			class:active={activeTab === "tools"}
			onclick={() => (activeTab = "tools")}
		>
			Tools
		</button>
	</div>

	<!-- Tab content -->
	<div class="panel-content">
		{#if activeTab === "overview"}
			<!-- Overview Tab -->
			<div class="overview-tab">
				{#if currentSpan}
					<!-- Metadata section -->
					<div class="section">
						<h3 class="section-title">
							<iconify-icon icon="ri:information-line" width="16"
							></iconify-icon>
							Metadata
						</h3>
						<div class="metadata-grid">
							<div class="metadata-item">
								<span class="metadata-label">Agent</span>
								<span
									class="metadata-value agent-badge"
									data-agent={currentSpan.agentId}
								>
									{currentSpan.agentId}
								</span>
							</div>
							<div class="metadata-item">
								<span class="metadata-label">Model</span>
								<span class="metadata-value"
									>{currentSpan.model}</span
								>
							</div>
							<div class="metadata-item">
								<span class="metadata-label">Routing</span>
								<span class="metadata-value">
									{currentSpan.wasExplicit
										? "Explicit"
										: "Auto-routed"}
								</span>
							</div>
							{#if currentSpan.promptTokens || currentSpan.completionTokens}
								<div class="metadata-item">
									<span class="metadata-label">Tokens</span>
									<span class="metadata-value">
										{formatTokens(
											currentSpan.promptTokens || 0,
										)} in / {formatTokens(
											currentSpan.completionTokens || 0,
										)} out
									</span>
								</div>
							{/if}
							{#if currentSpan.durationMs}
								<div class="metadata-item">
									<span class="metadata-label">Duration</span>
									<span class="metadata-value"
										>{formatDuration(
											currentSpan.durationMs,
										)}</span
									>
								</div>
							{/if}
							{#if currentSpan.msToFirstChunk}
								<div class="metadata-item">
									<span class="metadata-label"
										>Time to First Chunk</span
									>
									<span class="metadata-value"
										>{formatDuration(
											currentSpan.msToFirstChunk,
										)}</span
									>
								</div>
							{/if}
						</div>
					</div>

					<!-- Routing reason -->
					{#if currentSpan.routingReason}
						<div class="section">
							<h3 class="section-title">
								<iconify-icon icon="ri:route-line" width="16"
								></iconify-icon>
								Routing Decision
							</h3>
							<div class="routing-reason">
								{currentSpan.routingReason}
							</div>
						</div>
					{/if}

					<!-- Available tools preview (parsed from promptTools JSON) -->
					{@const toolsData = parseJson(
						currentSpan.promptTools,
					) as Array<{ name: string }> | null}
					{#if toolsData && toolsData.length > 0}
						<div class="section">
							<h3 class="section-title">
								<iconify-icon icon="ri:tools-line" width="16"
								></iconify-icon>
								Available Tools ({toolsData.length})
							</h3>
							<div class="tool-chips">
								{#each toolsData as tool}
									<span class="tool-chip">{tool.name}</span>
								{/each}
							</div>
						</div>
					{/if}
				{:else}
					<div class="empty-state">
						<iconify-icon
							icon="ri:information-line"
							width="32"
							height="32"
						></iconify-icon>
						<p>No trace data available</p>
						<span class="empty-hint"
							>Trace metadata will appear here after the assistant
							responds</span
						>
					</div>
				{/if}
			</div>
		{:else if activeTab === "request"}
			<!-- Request Tab - Full request data from AI SDK telemetry -->
			<div class="request-tab">
				{#if currentSpan?.prompt || currentSpan?.promptMessages}
					<!-- Full system prompt (ai.prompt - captured by AI SDK) -->
					{#if currentSpan.prompt}
						<div class="section">
							<h3 class="section-title">
								<iconify-icon
									icon="ri:file-text-line"
									width="16"
								></iconify-icon>
								System Prompt
							</h3>
							<ExpandableContent
								title="Complete system instructions sent to model"
								content={currentSpan.prompt}
								maxHeight={400}
								defaultExpanded={true}
							/>
						</div>
					{/if}

					<!-- Message history sent to model (parsed from promptMessages JSON) -->
					{@const messagesData = parseJson(
						currentSpan.promptMessages,
					) as Array<{ role: string; content: string }> | null}
					{#if messagesData && messagesData.length > 0}
						<div class="section">
							<h3 class="section-title">
								<iconify-icon
									icon="ri:chat-history-line"
									width="16"
								></iconify-icon>
								Messages Sent ({messagesData.length})
							</h3>
							<div class="message-list">
								{#each messagesData as msg, i}
									<div
										class="message-item"
										data-role={msg.role}
									>
										<div class="message-header">
											<span class="message-role"
												>{msg.role}</span
											>
											<span class="message-index"
												>#{i + 1}</span
											>
										</div>
										<div class="message-content">
											{typeof msg.content === "string"
												? msg.content.length > 500
													? msg.content.slice(
															0,
															500,
														) + "..."
													: msg.content
												: JSON.stringify(
														msg.content,
													).slice(0, 500)}
										</div>
									</div>
								{/each}
							</div>
						</div>
					{/if}

					<!-- Response text -->
					{#if currentSpan.responseText}
						<div class="section">
							<h3 class="section-title">
								<iconify-icon
									icon="ri:message-2-line"
									width="16"
								></iconify-icon>
								Response
							</h3>
							<ExpandableContent
								title="Model response"
								content={currentSpan.responseText}
								maxHeight={200}
							/>
						</div>
					{/if}
				{:else}
					<div class="empty-state">
						<iconify-icon
							icon="ri:file-text-line"
							width="32"
							height="32"
						></iconify-icon>
						<p>No prompt data available</p>
						<span class="empty-hint"
							>System prompts and message history will appear here</span
						>
					</div>
				{/if}
			</div>
		{:else if activeTab === "tools"}
			<!-- Tools Tab - Tool calls and results -->
			<div class="tools-tab">
				{#if flattenedToolCalls.length === 0}
					<div class="empty-state">
						<iconify-icon
							icon="ri:tools-line"
							width="32"
							height="32"
						></iconify-icon>
						<p>No tool calls yet</p>
						<span class="empty-hint"
							>Tool calls will appear here as the assistant uses
							them</span
						>
					</div>
				{:else}
					<div class="trace-list">
						{#each flattenedToolCalls as tc, i (`tool-${tc.messageId}-${i}`)}
							<TraceItem
								part={{
									type: tc.part.type,
									toolCallId: tc.part.toolCallId || "",
									toolName: tc.part.toolName || "",
									input: tc.part.input || {},
									state: tc.part.state || "pending",
									output: tc.part.output,
									errorText: tc.part.errorText,
								}}
								index={tc.index + 1}
							/>
						{/each}
					</div>
				{/if}
			</div>
		{/if}
	</div>
</div>

<style>
	.observability-panel {
		display: flex;
		flex-direction: column;
		height: 100%;
		background: var(--color-background);
	}

	.panel-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 12px 16px;
		border-bottom: 1px solid var(--color-border);
		background: var(--color-surface);
	}

	.header-left {
		display: flex;
		align-items: center;
		gap: 12px;
	}

	.header-title {
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: 0.875rem;
		font-weight: 600;
		color: var(--color-foreground);
	}

	.exchange-select {
		font-size: 0.75rem;
		padding: 4px 8px;
		border: 1px solid var(--color-border);
		border-radius: 4px;
		background: var(--color-background);
		color: var(--color-foreground);
		cursor: pointer;
	}

	.header-stats {
		display: flex;
		align-items: center;
		gap: 12px;
	}

	.stat {
		display: flex;
		align-items: center;
		gap: 4px;
		font-size: 0.75rem;
		color: var(--color-foreground-muted);
	}

	/* Tab navigation */
	.tab-nav {
		display: flex;
		border-bottom: 1px solid var(--color-border);
		background: var(--color-surface);
		padding: 0 16px;
	}

	.tab-button {
		padding: 10px 16px;
		font-size: 0.8125rem;
		font-weight: 500;
		color: var(--color-foreground-muted);
		background: none;
		border: none;
		border-bottom: 2px solid transparent;
		cursor: pointer;
		transition: all 0.15s ease;
		margin-bottom: -1px;
	}

	.tab-button:hover {
		color: var(--color-foreground);
	}

	.tab-button.active {
		color: var(--color-primary);
		border-bottom-color: var(--color-primary);
	}

	.panel-content {
		flex: 1;
		overflow-y: auto;
		padding: 16px;
	}

	/* Section styling */
	.section {
		margin-bottom: 20px;
	}

	.section:last-child {
		margin-bottom: 0;
	}

	.section-title {
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: 0.75rem;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--color-foreground-muted);
		margin: 0 0 12px 0;
	}

	/* Metadata grid */
	.metadata-grid {
		display: grid;
		grid-template-columns: repeat(2, 1fr);
		gap: 12px;
	}

	.metadata-item {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	.metadata-label {
		font-size: 0.6875rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--color-foreground-subtle);
	}

	.metadata-value {
		font-size: 0.8125rem;
		color: var(--color-foreground);
		font-weight: 500;
	}

	/* Agent badge colors - using semantic colors */
	.agent-badge {
		display: inline-block;
		padding: 2px 8px;
		border-radius: 4px;
		font-size: 0.75rem;
		background: var(--color-surface-elevated);
	}

	.agent-badge[data-agent="analytics"] {
		background: color-mix(in srgb, var(--color-primary) 10%, transparent);
		color: var(--color-primary);
	}

	.agent-badge[data-agent="research"] {
		background: color-mix(in srgb, var(--color-accent) 10%, transparent);
		color: var(--color-accent);
	}

	.agent-badge[data-agent="general"] {
		background: color-mix(
			in srgb,
			var(--color-foreground-subtle) 10%,
			transparent
		);
		color: var(--color-foreground-subtle);
	}

	.agent-badge[data-agent="action"] {
		background: color-mix(in srgb, var(--color-success) 10%, transparent);
		color: var(--color-success);
	}

	/* Routing reason */
	.routing-reason {
		font-size: 0.8125rem;
		color: var(--color-foreground);
		line-height: 1.5;
		padding: 12px;
		background: var(--color-surface-elevated);
		border-radius: 6px;
	}

	/* Thinking blocks */
	.thinking-blocks {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}

	/* Tool chips */
	.tool-chips {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
	}

	.tool-chip {
		font-size: 0.6875rem;
		padding: 4px 8px;
		background: var(--color-surface-elevated);
		border-radius: 4px;
		color: var(--color-foreground-muted);
		font-family: var(--font-mono);
	}

	/* Layers */
	.layers {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}

	/* Message list */
	.message-list {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}

	.message-item {
		padding: 10px 12px;
		background: var(--color-surface-elevated);
		border-radius: 6px;
		border-left: 3px solid var(--color-border);
	}

	.message-item[data-role="user"] {
		border-left-color: var(--color-primary);
	}

	.message-item[data-role="assistant"] {
		border-left-color: var(--color-foreground-subtle);
	}

	.message-item[data-role="system"] {
		border-left-color: var(--color-accent);
	}

	.message-header {
		display: flex;
		justify-content: space-between;
		margin-bottom: 6px;
	}

	.message-role {
		font-size: 0.6875rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		font-weight: 600;
		color: var(--color-foreground-muted);
	}

	.message-index {
		font-size: 0.6875rem;
		color: var(--color-foreground-subtle);
	}

	.message-content {
		font-size: 0.8125rem;
		color: var(--color-foreground);
		line-height: 1.5;
		white-space: pre-wrap;
		word-break: break-word;
	}

	/* Trace list */
	.trace-list {
		display: flex;
		flex-direction: column;
		gap: 12px;
	}

	/* Empty state */
	.empty-state {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		height: 200px;
		gap: 12px;
		color: var(--color-foreground-subtle);
		text-align: center;
	}

	.empty-state p {
		margin: 0;
		font-size: 0.875rem;
		font-weight: 500;
		color: var(--color-foreground-muted);
	}

	.empty-hint {
		font-size: 0.75rem;
	}
</style>
