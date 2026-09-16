<script lang="ts">
	import { slide } from "svelte/transition";
	import { cubicOut } from "svelte/easing";

	interface ToolCallPart {
		type: string;
		toolCallId?: string;
		toolName?: string;
		input?: Record<string, unknown>;
		state?:
			| "pending"
			| "input-available"
			| "output-available"
			| "output-error";
		output?: unknown;
		errorText?: string;
	}

	interface Props {
		/** Whether the AI is actively thinking/processing */
		isThinking: boolean;
		/** Tool call parts from the message */
		toolCalls: ToolCallPart[];
		/** Reasoning/thinking text from the model */
		reasoningContent?: string;
		/**
		 * What the model said on its way here — every text run in this turn
		 * except the last, which is the reply. These are the lines it writes
		 * before reaching for a tool; the newest one is the status label.
		 */
		narration?: string[];
		/** Duration in seconds spent thinking */
		duration?: number;
	}

	let {
		isThinking,
		toolCalls = [],
		reasoningContent = "",
		narration = [],
		duration = 0,
	}: Props = $props();

	// Expansion state - always starts collapsed (user can expand manually)
	let expanded = $state(false);

	// Track thinking duration
	let thinkingStartTime = $state<number | null>(null);
	let calculatedDuration = $state(0);
	let hasStartedThinking = $state(false);

	/**
	 * THE LABEL IS WHAT IS HAPPENING, not a word drawn from a hat.
	 *
	 * This used to be one of ninety whimsical verbs — "Pontificating",
	 * "Combobulating" — chosen at random and re-rolled every four seconds. It
	 * carried no information by construction, it changed while the thing it
	 * described did not, and some of them actively misdescribe: a person
	 * waiting on their own records was told the box was speaking pompously at
	 * length.
	 *
	 * Everything needed to say something true was already on screen. The model
	 * narrates before it reaches for a tool, and the tool call itself says what
	 * it is doing. So: the model's own last clause, or the tool in flight, and
	 * only then a plain fallback. Nothing new is fetched and no second model is
	 * asked — a lite model reading the tool's arguments could only paraphrase
	 * what `getToolDescription` already derives for free, and would not know
	 * WHY the call is being made, which is the one thing the narration does.
	 */
	const thinkingLabel = $derived.by(() => {
		const said = lastIntent(narration);
		if (said) return said;

		// Nothing said yet — the tool in flight is the next best truth. Prefer
		// one still running; fall back to the most recent, which is what the
		// gap between a tool returning and the next one starting looks like.
		const pending = toolCalls.filter(
			(t) => t.state === "pending" || t.state === "input-available" || !t.state,
		);
		const current = pending.at(-1) ?? toolCalls.at(-1);
		if (current && getToolName(current) !== "think") {
			return getToolDescription(current, true);
		}
		return "Thinking";
	});

	/**
	 * The last clause of the newest thing the model said.
	 *
	 * The prompt asks for the line to END with what it is about to do, because
	 * the sentence before it is usually a finding — worth reading, but not a
	 * status. Taking the last sentence gets the intent without the preamble,
	 * and degrades to the whole line for a model that ignores the shape.
	 */
	function lastIntent(lines: string[]): string {
		const line = lines.at(-1)?.trim();
		if (!line) return "";
		const sentences = line.split(/(?<=[.!?])\s+/).filter((t) => t.trim());
		const last = (sentences.at(-1) ?? line).trim();
		// A whole paragraph is not a label; better to fall through to the tool.
		if (last.length > 90) return "";
		return last;
	}

	// Track thinking start time - only trigger once per thinking session
	$effect(() => {
		if (isThinking && !hasStartedThinking) {
			hasStartedThinking = true;
			thinkingStartTime = Date.now();
		} else if (!isThinking && hasStartedThinking) {
			calculatedDuration = thinkingStartTime ? (Date.now() - thinkingStartTime) / 1000 : 0;
			thinkingStartTime = null;
			hasStartedThinking = false;
		}
	});


	/** What we actually timed: the caller's measurement, or this session's clock.
	 *  Zero means we were not here for it — see the header. */
	const knownDuration = $derived(duration || calculatedDuration);

	// Format duration
	function formatDuration(seconds: number): string {
		if (seconds < 1) return "<1s";
		if (seconds < 60) return `${Math.round(seconds)}s`;
		const mins = Math.floor(seconds / 60);
		const secs = Math.round(seconds % 60);
		return `${mins}m ${secs}s`;
	}

	// Get readable tool name
	function getToolName(tool: ToolCallPart): string {
		if (tool.toolName) return tool.toolName;
		if (tool.type?.startsWith("tool-")) return tool.type.slice(5);
		return tool.type || "tool";
	}

	/**
	 * A tool's name as a person would say it, for the collapsed header.
	 *
	 * The header used to print the raw ids — `semantic_search, sql_query` —
	 * while the expanded list beneath it spoke prose. Two registers for one
	 * fact, and the register the user meets FIRST was the internal one.
	 */
	const TOOL_NOUNS: Record<string, string> = {
		think: "thinking",
		web_search: "the web",
		semantic_search: "your records",
		sql_query: "your data",
		read_asset: "a file",
		get_page_content: "a page",
		create_page: "a new page",
		edit_page: "a page",
		generate_image: "an image",
		code_interpreter: "a calculation",
		dispatch_subagents: "a parallel search",
		run_applet: "an applet",
		update_memory: "memory",
		write_it_up: "an article",
		revise_article: "an article",
		dayline_event: "your day",
		get_project_item: "a project",
	};

	function humanToolName(name: string): string {
		return TOOL_NOUNS[name] ?? name.replace(/_/g, " ");
	}

	/** `pending ? a : b` — one place, so no description forgets the distinction. */
	function tense(pending: boolean, doing: string, done: string): string {
		return pending ? doing : done;
	}

	/**
	 * What one tool call is doing, or did.
	 *
	 * TENSE IS A PARAMETER, not a per-string accident. This list read
	 * "Searching:", "Searched the web for", "Queried messages" and "Planning:"
	 * in one column — every tense at once, so nothing told you whether a line
	 * was happening or had happened. The row already knows (`isPending` draws
	 * the spinner); it just wasn't telling the words.
	 *
	 * The twelve branches that used to sit at the bottom of this switch —
	 * `calendar`, `get_contacts`, `recall`, `query_database` and friends — named
	 * no tool that exists. Meanwhile two dozen real ones (pages, applets,
	 * images, subagents) had no prose at all and fell to the default, which
	 * `.replace(/\b\w/g, (c) => c)` left lowercase: it replaced each word's
	 * first letter with itself. An unmapped tool rendered as "create page".
	 */
	function getToolDescription(tool: ToolCallPart, pending = false): string {
		const name = getToolName(tool);
		const input = tool.input || {};

		switch (name) {
			case "think": {
				const thought = (input.thought as string) || "";
				const preview = thought.length > 80 ? thought.slice(0, 80) + "…" : thought;
				return `${tense(pending, "Planning", "Planned")}: "${preview}"`;
			}
			case "web_search":
				return `${tense(pending, "Searching", "Searched")} the web for "${input.query || "information"}"`;
			case "semantic_search": {
				// The tool takes `queries` (up to four phrasings of one need) and
				// keeps `query` only for back-compat — and its own description tells
				// the model to prefer the array. Reading `query` alone rendered
				// `Searching: ""` for every call that followed that advice, which
				// read as a search with nothing in it rather than the widest search
				// we do. Same precedence the tool itself applies; the other
				// phrasings become a count, since four wordings of one question are
				// noise to read in full.
				const list = (
					Array.isArray(input.queries) ? (input.queries as unknown[]) : []
				).filter((q): q is string => typeof q === "string" && q.trim() !== "");
				if (list.length === 0 && typeof input.query === "string" && input.query.trim()) {
					list.push(input.query);
				}
				const verb = tense(pending, "Searching", "Searched");
				if (list.length === 0) return `${verb} your records`;
				const first = list[0].slice(0, 60);
				const more = list.length > 1 ? ` +${list.length - 1} more` : "";
				return `${verb} your records for "${first}"${more}`;
			}
			case "sql_query": {
				const op = input.operation as string;
				if (op === "list_tables") {
					return tense(pending, "Listing what data there is", "Listed what data there is");
				}
				if (op === "get_schema") {
					const tables = input.tables as string[] | undefined;
					const verb = tense(pending, "Checking the shape of", "Checked the shape of");
					if (tables?.length) {
						const formatted = tables
							.slice(0, 2)
							.map((t) => plainTableName(t))
							.join(", ");
						const more = tables.length > 2 ? ` +${tables.length - 2} more` : "";
						return `${verb} ${formatted}${more}`;
					}
					return `${verb} a table`;
				}
				if (op === "query") {
					const sql = (input.sql as string) || "";
					const tableName = plainTableName(sql.match(/FROM\s+([a-z_]+)/i)?.[1] ?? "");
					const verb = tense(pending, "Reading", "Read");
					return tableName ? `${verb} your ${tableName}` : `${verb} your data`;
				}
				return tense(pending, "Reading your data", "Read your data");
			}
			case "read_asset":
				return tense(pending, "Opening a file", "Opened a file");
			case "get_page_content":
				return tense(pending, "Reading a page", "Read a page");
			case "create_page":
				return tense(pending, "Writing a new page", "Wrote a new page");
			case "edit_page":
				return tense(pending, "Editing a page", "Edited a page");
			case "generate_image":
				return tense(pending, "Making an image", "Made an image");
			case "code_interpreter":
				return tense(pending, "Working something out", "Worked something out");
			case "dispatch_subagents":
				return tense(pending, "Searching several ways at once", "Searched several ways at once");
			case "run_applet":
				return tense(pending, "Running an applet", "Ran an applet");
			case "update_memory":
				return tense(pending, "Noting something to remember", "Noted something to remember");
			case "write_it_up":
				return tense(pending, "Writing it up", "Wrote it up");
			case "revise_article":
				return tense(pending, "Revising an article", "Revised an article");
			case "dayline_event":
				return tense(pending, "Marking your day", "Marked your day");
			default: {
				// Sentence case, not the machine's name. Whatever is here is a
				// tool nobody has written a line for yet, so at least say it the
				// way a person would read it.
				const words = humanToolName(name);
				return words.charAt(0).toUpperCase() + words.slice(1);
			}
		}
	}

	/** `data_communication_message` -> `messages`. The prefixes are our namespaces. */
	function plainTableName(table: string): string {
		return table
			.replace(/^(data|wiki|narrative)_/, "")
			.replace(/^(communication|health|financial|activity|content)_/, "")
			.replace(/_/g, " ");
	}

	// Check if we have content
	const hasContent = $derived(
		reasoningContent || toolCalls.length > 0 || narration.length > 0,
	);

	// Get unique tools for collapsed summary (filter out "think" — rendered inline, not as a tool)
	const uniqueToolNames = $derived.by(() => {
		const names = toolCalls
			.map((t) => getToolName(t))
			.filter((n) => n !== "think")
			.map(humanToolName);
		return [...new Set(names)];
	});
</script>

<div class="thinking-block">
	<!-- Header -->
	<button
		type="button"
		class="block-header"
		class:has-content={hasContent}
		onclick={() => {
			if (hasContent) {
				expanded = !expanded;
			}
		}}
		aria-expanded={hasContent ? expanded : undefined}
	>
		{#if hasContent}
			<span class="chevron" class:rotated={expanded}>
				<svg width="12" height="12" viewBox="0 0 12 12">
					<path
						d="M4 2.5L7.5 6L4 9.5"
						stroke="currentColor"
						stroke-width="1.25"
						fill="none"
						stroke-linecap="round"
						stroke-linejoin="round"
					/>
				</svg>
			</span>
		{/if}

		<span class="header-content">
			{#if isThinking}
				<span class="thinking-text"
					>{thinkingLabel}</span
				>
			{:else if knownDuration > 0}
				<span class="duration-text">
					Thought for {formatDuration(knownDuration)}
				</span>
			{:else}
				<!-- A turn we did not watch: nothing records how long the box
				     thought, so a reopened chat reported "Thought for <1s" on every
				     block — a measurement of our own absence, stated as a fact
				     about the box. Say what we know instead. -->
				<span class="duration-text">Worked on this</span>
			{/if}

			{#if uniqueToolNames.length > 0}
				<span class="header-tools">
					{uniqueToolNames.slice(0, 3).join(", ")}
					{#if uniqueToolNames.length > 3}
						+{uniqueToolNames.length - 3} more
					{/if}
				</span>
			{/if}
		</span>
	</button>

	<!-- Expandable content with slide animation -->
	{#if expanded && hasContent}
		<div
			class="block-content markdown"
			transition:slide={{ duration: 200, easing: cubicOut }}
		>
			{#if reasoningContent}
				<p class="reasoning-text">{reasoningContent}</p>
			{/if}

			<!-- What the model said on the way here. It used to sit in the
			     transcript forever, above the answer, so reopening a chat meant
			     reading a finished process narrate itself. It belongs with the
			     rest of the working-out: available, not in the way. -->
			{#each narration as line, i (i)}
				<p class="narration-line">{line}</p>
			{/each}

			{#if toolCalls.length > 0}
				<ul class="tool-list">
					{#each toolCalls as tool, index (tool.toolCallId || `tool-${index}`)}
						{@const isPending =
							tool.state === "pending" ||
							tool.state === "input-available" ||
							!tool.state}
						{@const isError = tool.state === "output-error"}
						{@const isThinkTool = getToolName(tool) === "think"}
						{#if isThinkTool}
							<li class="think-item">
								<p class="think-text">{tool.input?.thought || ""}</p>
							</li>
						{:else}
							<li
								class="tool-item"
								class:pending={isPending}
								class:error={isError}
							>
								{#if isPending}
									<span class="tool-spinner"></span>
								{:else}
									<span class="tool-icon" class:error={isError}>·</span>
								{/if}
								<span class="tool-description">
									{getToolDescription(tool, isPending)}
								</span>
							</li>
						{/if}
					{/each}
				</ul>
			{/if}
		</div>
	{/if}
</div>

<style>
	.thinking-block {
		margin-bottom: 10px;
	}

	/* Header with hover effect */
	.block-header {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 4px 12px;
		margin: 0;
		background: transparent;
		border: none;
		/* A pill, not a 6px chip. The hover ground is a lozenge sitting one line
		   above a user bubble that is itself a pill (ChatView, 1.5rem) — two
		   different roundings stacked in the same column read as two different
		   kits. */
		border-radius: var(--radius-full);
		cursor: default;
		color: var(--color-foreground-muted);
		font-size: 13px;
		line-height: 1.5;
		text-align: left;
		transition:
			background-color 0.15s ease,
			color 0.15s ease;
	}

	.block-header.has-content {
		cursor: pointer;
	}

	.block-header.has-content:hover {
		background-color: var(--hover-bg);
		color: var(--color-foreground);
	}

	/* Chevron with rotation animation */
	.chevron {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 12px;
		height: 12px;
		flex-shrink: 0;
		opacity: 0.6;
		transition:
			transform 0.2s cubic-bezier(0.4, 0, 0.2, 1),
			opacity 0.15s ease;
	}

	.chevron.rotated {
		transform: rotate(90deg);
	}

	.block-header.has-content:hover .chevron {
		opacity: 1;
	}

	.header-content {
		display: flex;
		align-items: baseline;
		gap: 8px;
	}

	.thinking-text {
		background: linear-gradient(
			90deg,
			var(--color-foreground-subtle) 0%,
			var(--color-foreground-subtle) 40%,
			var(--color-foreground) 50%,
			var(--color-foreground-subtle) 60%,
			var(--color-foreground-subtle) 100%
		);
		background-size: 200% 100%;
		-webkit-background-clip: text;
		background-clip: text;
		color: transparent;
		animation: shimmer 2s ease-in-out infinite;
	}

	@keyframes shimmer {
		0% {
			background-position: 100% 0;
		}
		100% {
			background-position: -100% 0;
		}
	}

	.duration-text {
		color: var(--color-foreground-muted);
	}

	.header-tools {
		color: var(--color-foreground-muted);
	}

	.header-tools::before {
		content: "·";
		margin-right: 8px;
	}

	/* Content area */
	.block-content {
		margin-top: 8px;
		padding: 12px 16px;
		background: var(--color-surface-elevated);
		border-radius: 8px;
		max-height: 500px;
		overflow-y: auto;
		color: var(--color-foreground);
		font-size: 13px;
		line-height: 1.6;
	}

	/* Scrollbar */
	.block-content::-webkit-scrollbar {
		width: 3px;
	}

	.block-content::-webkit-scrollbar-track {
		background: transparent;
	}

	.block-content::-webkit-scrollbar-thumb {
		background-color: var(--color-border);
		border-radius: 3px;
	}

	/* Think tool content */
	.think-item {
		list-style: none;
	}

	.think-text {
		margin: 0;
		color: var(--color-foreground);
		line-height: 1.5;
		white-space: pre-wrap;
	}

	.narration-line {
		margin: 0 0 12px 0;
		color: var(--color-foreground-muted);
		line-height: 1.5;
	}

	.narration-line:last-child {
		margin-bottom: 0;
	}

	/* Reasoning text - matches .markdown p */
	.reasoning-text {
		margin: 0 0 16px 0;
		color: var(--color-foreground);
		line-height: 1.5;
		white-space: pre-wrap;
	}

	.reasoning-text:last-child {
		margin-bottom: 0;
	}

	/* Tool list - matches .markdown ul */
	.tool-list {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 8px;
	}

	.tool-item {
		display: flex;
		align-items: center;
		gap: 10px;
		color: var(--color-foreground);
		line-height: 1.5;
	}

	.tool-item.pending {
		color: var(--color-foreground-muted);
	}

	.tool-item.error {
		color: var(--color-error);
	}

	.tool-icon {
		font-size: 12px;
		opacity: 0.7;
		flex-shrink: 0;
	}

	.tool-icon.error {
		color: var(--color-error);
		opacity: 1;
	}

	.tool-description {
		flex: 1;
	}

	.tool-spinner {
		width: 12px;
		height: 12px;
		border: 1.5px solid var(--color-border);
		border-top-color: var(--color-foreground-muted);
		border-radius: 50%;
		animation: spin 0.8s linear infinite;
		flex-shrink: 0;
	}

	@keyframes spin {
		to {
			transform: rotate(360deg);
		}
	}

	/* Reduced motion */
	@media (prefers-reduced-motion: reduce) {
		.chevron {
			transition: none;
		}

		.block-header {
			transition: none;
		}

		.tool-spinner {
			animation: none;
		}
	}
</style>
