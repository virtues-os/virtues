<script lang="ts">
	import { slide } from "svelte/transition";
	import { cubicOut } from "svelte/easing";
	import ThinkingMark from "./ThinkingMark.svelte";

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
		/**
		 * Which mode the turn is running in. Deep Research and Council are, by
		 * construction, turns that go out to the record — so the mark starts a
		 * dimension up rather than waiting for the first tool to prove it.
		 */
		agentMode?: "chat" | "deep_research" | "council";
	}

	let {
		isThinking,
		toolCalls = [],
		reasoningContent = "",
		narration = [],
		duration = 0,
		agentMode = "chat",
	}: Props = $props();

	// Expansion state - always starts collapsed (user can expand manually)
	let expanded = $state(false);

	// Track thinking duration
	let thinkingStartTime = $state<number | null>(null);
	let calculatedDuration = $state(0);
	let hasStartedThinking = $state(false);

	/**
	 * HOW DEEP, in dots. See ThinkingMark for what the dots mean and why they
	 * are dots; this is the only place that decides which number is true.
	 *
	 * There is no enum for "kind of thinking" anywhere in the system, and there
	 * should not be one — nothing on the wire knows it. What we do know is the
	 * tool in flight, how long the turn has run, how many calls it has made, and
	 * the mode the person chose. `ToolCategory` in the registry looks like the
	 * answer and is not: it is a grouping for settings UI, where `think` and
	 * `code_interpreter` are Data and `read_asset` is Search. The switch in
	 * `getToolDescription` below is the real table of tools we have something to
	 * say about, so the depth lives beside it and the two cannot drift.
	 *
	 * Depth is how far it went. The WORDS say what kind of work it is. Keeping
	 * those orthogonal is what stops this needing twenty states.
	 */
	const TOOL_DEPTH: Record<string, 3 | 4 | 5> = {
		// Reasoning in place, with what is already loaded.
		think: 3,
		code_interpreter: 3,
		// Out to something: the record, the web, a file, a page, an applet.
		semantic_search: 4,
		sql_query: 4,
		sql_write: 4,
		web_search: 4,
		read_asset: 4,
		get_page_content: 4,
		create_page: 4,
		edit_page: 4,
		revise_article: 4,
		write_it_up: 4,
		generate_image: 4,
		update_memory: 4,
		set_user_name: 4,
		set_assistant_name: 4,
		propose_narrative_identity_edit: 4,
		get_project_item: 4,
		record_introductions: 4,
		skip_step: 4,
		list_applets: 4,
		get_applet: 4,
		setup_applet: 4,
		edit_applet: 4,
		delete_applet: 4,
		run_applet: 4,
		update_applet_memory: 4,
		// Many passes at once, by definition: this one IS the fan-out.
		dispatch_subagents: 5,
	};

	/** A turn this long is a long one, whatever it is doing. */
	const LONG_TURN_MS = 15_000;
	/** Past this, settle into the calmer form of whatever is playing. */
	const SUSTAINED_MS = 4_000;
	/** Enough calls that the turn is plainly working through something. */
	const MANY_TOOLS = 6;
	/** How long the landing stays on screen after the turn ends. */
	const LANDING_MS = 2_000;

	/**
	 * Elapsed time is the only honest signal for "this is deep". Nothing else
	 * in the system distinguishes a 2-second semantic_search from a 40-second
	 * one — same tool, same arguments, same everything but the clock. Half a
	 * second of resolution is plenty for a threshold measured in seconds.
	 */
	let elapsedMs = $state(0);

	/**
	 * The tool in flight. Prefer one still running; fall back to the most
	 * recent, which is what the gap between a tool returning and the next one
	 * starting looks like. Both the label and the depth read this, so they can
	 * never describe different calls.
	 */
	const toolInFlight = $derived.by(() => {
		const pending = toolCalls.filter(
			(t) => t.state === "pending" || t.state === "input-available" || !t.state,
		);
		return pending.at(-1) ?? toolCalls.at(-1);
	});

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

		// Nothing said yet — the tool in flight is the next best truth.
		const current = toolInFlight;
		if (current && getToolName(current) !== "think") {
			return getToolDescription(current, true);
		}
		// Long and silent: "Thinking" stops being informative somewhere around
		// the fifteen-second mark, and saying so is the one thing we know that
		// the model has not already said.
		return elapsedMs > LONG_TURN_MS ? "Sitting with this one" : "Thinking";
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

	/**
	 * How many dots. The order of these tests is the order of confidence: what
	 * the turn has already done outranks what it happens to be doing right now,
	 * because a turn that has run fifteen seconds is a long one even while its
	 * current call is a quick read.
	 */
	const thinkingDepth = $derived.by((): 1 | 3 | 4 | 5 => {
		if (!isThinking) return 1;
		if (elapsedMs > LONG_TURN_MS) return 5;
		if (toolCalls.length >= MANY_TOOLS) return 5;

		// The same call the label is describing — including the one that just
		// returned, so the gap before the next starts does not flick a dot off.
		const name = toolInFlight ? getToolName(toolInFlight) : "";
		if (name === "dispatch_subagents") return 5;

		const floor = agentMode === "chat" ? 3 : 4;
		const base = name ? (TOOL_DEPTH[name] ?? 4) : 3;
		return Math.max(floor, base) as 3 | 4 | 5;
	});

	/** Past a few seconds, every movement settles into its calmer form. */
	const sustained = $derived(elapsedMs > SUSTAINED_MS);

	/** The landing outlives the turn by one movement. */
	let landing = $state(false);
	let landingTimer: ReturnType<typeof setTimeout> | null = null;

	/**
	 * Pre-effect, deliberately. A plain $effect runs AFTER the template has been
	 * updated, so on the frame the turn ends the template sees `isThinking` false
	 * while `landing` is still false, unmounts the mark, and the effect then
	 * remounts a fresh one — which collapses from a resting mark rather than from
	 * the pose the turn actually ended on. Verified: the mark's DOM node changed
	 * identity across the transition. Running before the DOM update keeps the
	 * instance, and with it the pose it froze.
	 */
	$effect.pre(() => {
		if (isThinking && !hasStartedThinking) {
			hasStartedThinking = true;
			thinkingStartTime = Date.now();
			landing = false;
			if (landingTimer) {
				clearTimeout(landingTimer);
				landingTimer = null;
			}
		} else if (!isThinking && hasStartedThinking) {
			calculatedDuration = thinkingStartTime ? (Date.now() - thinkingStartTime) / 1000 : 0;
			thinkingStartTime = null;
			hasStartedThinking = false;
			// Three premises fall into one conclusion. This is the ONLY place
			// the single dot is used, which is what keeps it meaning something.
			landing = true;
			landingTimer = setTimeout(() => {
				landing = false;
				landingTimer = null;
			}, LANDING_MS);
		}
	});

	// The clock behind the depth thresholds and the long-wait line. Half-second
	// resolution: these are thresholds measured in seconds, and a 60Hz counter
	// would re-derive the label on every frame for no one's benefit.
	$effect(() => {
		if (!isThinking) {
			elapsedMs = 0;
			return;
		}
		const startedAt = Date.now();
		elapsedMs = 0;
		const id = setInterval(() => {
			elapsedMs = Date.now() - startedAt;
		}, 500);
		return () => clearInterval(id);
	});

	$effect(() => {
		return () => {
			if (landingTimer) clearTimeout(landingTimer);
		};
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
			case "sql_write":
				return tense(pending, "Writing to your records", "Wrote to your records");
			case "set_user_name":
			case "set_assistant_name":
				return tense(pending, "Learning a name", "Learned a name");
			case "propose_narrative_identity_edit":
				return tense(
					pending,
					"Suggesting a change to how you're described",
					"Suggested a change to how you're described",
				);
			case "list_applets":
			case "get_applet":
				return tense(pending, "Checking what's set up", "Checked what's set up");
			case "setup_applet":
				return tense(pending, "Setting up an applet", "Set up an applet");
			case "edit_applet":
				return tense(pending, "Changing an applet", "Changed an applet");
			case "delete_applet":
				return tense(pending, "Removing an applet", "Removed an applet");
			case "update_applet_memory":
				return tense(pending, "Noting something for next time", "Noted something for next time");
			case "get_project_item":
				return tense(pending, "Opening something you're working on", "Opened something you're working on");
			case "record_introductions":
				return tense(pending, "Writing the introductions", "Wrote the introductions");
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
			{#if isThinking || landing}
				<!-- How deep, in dots. It says nothing the label does not, and
				     that is the point: it is readable at a glance, from across
				     the desk, without reading a word. -->
				<ThinkingMark depth={thinkingDepth} {sustained} />
			{/if}

			{#if isThinking}
				<!-- The one place the box says it is working. Its only aria was
				     `aria-expanded`, so a screen reader was told a button could
				     be opened and never that anything was happening inside it —
				     and now that the label is true, it is worth hearing. -->
				<span class="thinking-text" role="status" aria-live="polite"
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
