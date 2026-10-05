<script lang="ts">
	import { slide } from "svelte/transition";
	import { cubicOut } from "svelte/easing";
	import { untrack } from "svelte";
	import ThinkingMark, { LAND_MS } from "./ThinkingMark.svelte";
	import { toolErrorSummary } from "$lib/components/chat/state/toolError";
	import {
		describeTool,
		toolDepth,
		toolName,
		toolNoun,
		toolStatus,
		type ToolPart,
	} from "$lib/components/chat/state/toolPresentation";
	import type { AgentModeId } from "$lib/config/agentModes";

	interface Props {
		/** This turn is in flight (TurnPhaseController: Send to stream end). */
		isThinking: boolean;
		/** When the turn began, for the live clock. Owned by the view, so a
		 *  block that mounts mid-turn does not restart it at zero. */
		startedAt?: number | null;
		/** In flight with nothing arriving: the label says so. */
		stalled?: boolean;
		/** Tool call parts from the message */
		toolCalls: ToolPart[];
		/** Reasoning/thinking text from the model */
		reasoningContent?: string;
		/**
		 * What the model said on its way here — every text run in this turn
		 * except the last, which is the reply. These are the lines it writes
		 * before reaching for a tool; the newest one is the status label.
		 */
		narration?: string[];
		/**
		 * The narration line that introduced the call in flight, if one did
		 * (`splitTurn`). An older line is not a status: the model may make
		 * later calls without a word, and the label kept saying the first.
		 */
		intent?: string;
		/** How long the turn worked, in seconds; 0 when nobody measured it. */
		seconds?: number;
		/** Play the landing when the turn ends. Off for a turn that was
		 *  stopped or failed: nothing landed. */
		land?: boolean;
		/**
		 * Which mode the turn is running in. Deep Research is, by
		 * construction, a turn that goes out to the record — so the mark starts a
		 * dimension up rather than waiting for the first tool to prove it.
		 */
		agentMode?: AgentModeId;
	}

	let {
		isThinking,
		toolCalls = [],
		reasoningContent = "",
		narration = [],
		intent = "",
		startedAt = null,
		stalled = false,
		seconds = 0,
		land = true,
		agentMode = "chat",
	}: Props = $props();

	// Expansion state - always starts collapsed (user can expand manually)
	let expanded = $state(false);

	/** A turn this long is a long one, whatever it is doing. */
	const LONG_TURN_MS = 15_000;
	/** Past this, settle into the calmer form of whatever is playing. */
	const SUSTAINED_MS = 4_000;
	/** Enough calls that the turn is plainly working through something. */
	const MANY_TOOLS = 6;

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
	const toolRunning = $derived(
		toolCalls.findLast((t) => toolStatus(t, isThinking) === "running"),
	);
	const toolInFlight = $derived(toolRunning ?? toolCalls.at(-1));

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
	 * what `describeTool` already derives for free, and would not know WHY
	 * the call is being made, which is the one thing the narration does.
	 */
	const thinkingLabel = $derived.by(() => {
		const said = lastIntent(intent);
		if (said) return said;

		// Nothing said for this call — the call itself is the next best truth.
		// Only one still running: a finished call in the present tense was
		// the label while the reply was already being written.
		const current = toolRunning;
		if (current && toolName(current) !== "think") {
			return describeTool(current, true, true);
		}
		// Nothing has arrived for a while, and no call is running to explain
		// it: the wait is on the server, and saying so is the one true thing.
		if (stalled) return "Waiting on your server";
		// Long and silent: "Thinking" stops being informative somewhere around
		// the fifteen-second mark, and saying so is the one thing we know that
		// the model has not already said.
		return elapsedMs > LONG_TURN_MS ? "Sitting with this one" : "Thinking";
	});

	/**
	 * The last clause of the newest thing the model said.
	 *
	 * The prompt asks for the line to END with a clause naming what it is
	 * about to do, because what comes before is usually a finding — worth
	 * reading, but not a status. A clause is often joined by a dash or a
	 * semicolon rather than a full stop ("He wrote twice in August — checking
	 * September."), so those split too. Degrades to the whole line for a
	 * model that ignores the shape.
	 */
	function lastIntent(line: string): string {
		line = line.trim();
		if (!line) return "";
		const pieces = line.split(/(?<=[.!?])\s+|\s+[—–]\s+|;\s+/).filter((t) => t.trim());
		const last = (pieces.at(-1) ?? line).trim();
		// A whole paragraph is not a label; better to fall through to the tool.
		if (last.length > 90) return "";
		return last.charAt(0).toUpperCase() + last.slice(1);
	}

	/**
	 * HOW DEEP, in dots. See ThinkingMark for what the dots mean and why they
	 * are dots; this is the only place that decides which number is true. Each
	 * tool's own depth lives with the rest of how it reads (toolPresentation).
	 *
	 * The order of these tests is the order of confidence: what
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
		const base = toolInFlight ? toolDepth(toolName(toolInFlight)) : 3;
		if (base === 5) return 5;

		const floor = agentMode === "chat" ? 3 : 4;
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
	let wasThinking = false;
	$effect.pre(() => {
		if (isThinking && !wasThinking) {
			wasThinking = true;
			landing = false;
			if (landingTimer) {
				clearTimeout(landingTimer);
				landingTimer = null;
			}
		} else if (!isThinking && wasThinking) {
			wasThinking = false;
			// Three premises fall into one conclusion. This is the ONLY place
			// the single dot is used, which is what keeps it meaning something
			// — so a turn that was stopped or failed does not get it.
			if (!untrack(() => land)) return;
			landing = true;
			landingTimer = setTimeout(() => {
				landing = false;
				landingTimer = null;
			}, LAND_MS);
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
		const from = startedAt ?? Date.now();
		elapsedMs = Date.now() - from;
		const id = setInterval(() => {
			elapsedMs = Date.now() - from;
		}, 500);
		return () => clearInterval(id);
	});

	$effect(() => {
		return () => {
			if (landingTimer) clearTimeout(landingTimer);
		};
	});


	function formatDuration(total: number): string {
		if (total < 1) return "<1s";
		const s = Math.round(total);
		if (s < 60) return `${s}s`;
		const h = Math.floor(s / 3600);
		const m = Math.floor((s % 3600) / 60);
		const sec = s % 60;
		return h > 0 ? `${h}h ${m}m` : `${m}m ${sec}s`;
	}

	/** See the live region in the markup. */
	let announcement = $state("");
	let announced = false;
	$effect(() => {
		if (isThinking) {
			announced = true;
			announcement = "Working on a reply";
		} else if (announced) {
			announcement = seconds > 0 ? `Worked for ${formatDuration(seconds)}` : "Reply finished";
		}
	});

	const reduceMotion =
		typeof matchMedia !== "undefined" && matchMedia("(prefers-reduced-motion: reduce)").matches;

	// Check if we have content
	const hasContent = $derived(
		reasoningContent || toolCalls.length > 0 || narration.length > 0,
	);

	// Get unique tools for collapsed summary (filter out "think" — rendered inline, not as a tool)
	const uniqueToolNames = $derived.by(() => {
		const names = toolCalls
			.map(toolName)
			.filter((n) => n !== "think")
			.map(toolNoun);
		return [...new Set(names)];
	});
</script>

<div class="thinking-block">
	<!-- What a screen reader hears: that a turn started, then how long it
	     took. Outside the button, so the button's name is not a live region,
	     and present from mount but empty, so the first words are a change a
	     reader announces rather than text it skips. Not the label itself —
	     that changes with every call and would be read out every time. -->
	<span class="sr-only" role="status" aria-live="polite">{announcement}</span>
	<!-- Header. With nothing to open it is not a control: out of the tab
	     order rather than a button that does nothing. -->
	<button
		type="button"
		class="block-header"
		class:has-content={hasContent}
		tabindex={hasContent ? undefined : -1}
		onclick={() => {
			if (hasContent) {
				expanded = !expanded;
			}
		}}
		aria-expanded={hasContent ? expanded : undefined}
	>
		<span class="header-content">
			{#if isThinking || landing}
				<!-- How deep, in dots. It says nothing the label does not, and
				     that is the point: it is readable at a glance, from across
				     the desk, without reading a word. -->
				<ThinkingMark depth={thinkingDepth} {sustained} />
			{/if}

			{#if isThinking}
				<span class="thinking-text">{thinkingLabel}</span>
			{:else if seconds > 0}
				<!-- "Worked", not "Thought": the span covers the tools and the
				     writing too, and claiming it all as thought overstates the
				     one thing nobody can see. -->
				<span class="duration-text">
					Worked for {formatDuration(seconds)}
				</span>
			{:else}
				<span class="duration-text">Worked on this</span>
			{/if}

			<!-- The summary of a finished turn. While it runs, the label already
			     names the tool in flight, and the two side by side only took
			     width from the one that is live. -->
			{#if !isThinking && uniqueToolNames.length > 0}
				<span class="header-tools">
					{uniqueToolNames.slice(0, 3).join(", ")}
					{#if uniqueToolNames.length > 3}
						+{uniqueToolNames.length - 3} more
					{/if}
				</span>
			{/if}
		</span>

		<!-- The dots lead; the chevron turns up on hover, trailing, as the
		     affordance for opening the block rather than a permanent glyph.
		     A touch screen has no hover, so there it stays, faintly. -->
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
	</button>

	<!-- Expandable content with slide animation -->
	{#if expanded && hasContent}
		<div
			class="block-content markdown"
			transition:slide={{ duration: reduceMotion ? 0 : 200, easing: cubicOut }}
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
						{@const status = toolStatus(tool, isThinking)}
						{@const isPending = status === "running"}
						{@const isError = status === "failed"}
						{@const isThinkTool = toolName(tool) === "think"}
						{#if isThinkTool}
							<li class="think-item">
								<p class="think-text">{tool.input?.thought || ""}</p>
							</li>
						{:else}
							<li
								class="tool-item"
								class:pending={isPending}
								class:error={isError}
								class:unfinished={status === "unfinished"}
							>
								{#if isPending}
									<span class="tool-spinner"></span>
								{:else}
									<span class="tool-icon" class:error={isError}>·</span>
								{/if}
								<!-- A failed call says why, in one line: the Postgres
								     message, not the column list the model was handed.
								     The next item in this list is the retry, so a failure
								     the model recovered from reads as what it was — one
								     wrong guess on the way — and never leaves this block
								     for the transcript (ChatView keeps the body-level
								     error for a turn that ENDED on the failure). -->
								<span class="tool-description">
									{describeTool(tool, isPending || status === "unfinished")}{#if isError && tool.errorText}<span class="tool-reason"> · failed: {toolErrorSummary(tool.errorText)}</span>{:else if status === "unfinished"}<span class="tool-reason"> · didn't finish</span>{/if}
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
		/* One line, always. The header is status, not content: a long label
		   used to wrap, and with two flex children each wrapping on its own
		   the row became two ragged columns. Anything longer ends in an
		   ellipsis; the whole text is one click away in the list below. */
		max-width: 100%;
		/* At rest the words sit flush with the reply's own left edge: the
		   pill's padding hangs outside the column, where there is nothing to
		   see. On hover the pill slides in by its padding as its ground fades
		   up, so the badge forms around the words instead of the words
		   jumping inside a badge. A transform, so the reply below never
		   reflows. */
		transform: translateX(-12px);
		transition:
			transform 0.2s cubic-bezier(0.4, 0, 0.2, 1),
			background-color 0.15s ease,
			color 0.15s ease;
	}

	.block-header.has-content {
		cursor: pointer;
	}

	.block-header.has-content:hover,
	.block-header.has-content:focus-visible {
		transform: translateX(0);
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
		opacity: 0;
		transition:
			transform 0.2s cubic-bezier(0.4, 0, 0.2, 1),
			opacity 0.15s ease;
	}

	.chevron.rotated {
		transform: rotate(90deg);
	}

	@media (hover: none) {
		.block-header.has-content .chevron {
			opacity: 0.6;
		}
	}

	.block-header.has-content:hover .chevron,
	.block-header.has-content:focus-visible .chevron {
		opacity: 1;
	}

	.header-content {
		display: flex;
		align-items: baseline;
		gap: 8px;
		min-width: 0;
		white-space: nowrap;
	}

	.thinking-text,
	.header-tools {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.duration-text {
		flex-shrink: 0;
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

	/* Content area. Filled with the same wash the header takes on hover, so
	   the open block is one card: the pill above it used to hover in
	   --hover-bg while this sat in --color-surface-elevated, and on any theme
	   whose paper is tinted (Oxford: warm stone) the two were visibly
	   different colors stacked in the same column. */
	.block-content {
		margin-top: 8px;
		padding: 12px 16px;
		background: var(--hover-bg);
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

	.tool-item.unfinished {
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

	.tool-reason {
		opacity: 0.85;
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

	/* Reduced motion: the pill keeps its color fade and loses the slide;
	   the shimmer stops and the label reads as plain muted text. */
	@media (prefers-reduced-motion: reduce) {
		.chevron {
			transition: none;
		}

		.block-header {
			transition:
				background-color 0.15s ease,
				color 0.15s ease;
		}

		.thinking-text {
			animation: none;
			background: none;
			color: var(--color-foreground-muted);
		}

		.tool-spinner {
			animation: none;
		}
	}
</style>
