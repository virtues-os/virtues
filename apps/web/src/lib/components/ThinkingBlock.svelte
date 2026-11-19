<script lang="ts">
	import { slide } from "$lib/utils/transitions";
	import { reducedMotion } from "$lib/stores/motion.svelte";
	import { cubicOut } from "svelte/easing";
	import { untrack } from "svelte";
	import ThinkingMark, { LAND_MS } from "./ThinkingMark.svelte";
	import { toolErrorSummary } from "$lib/components/chat/state/toolError";
	import {
		clip,
		describeTool,
		toolDepth,
		toolName,
		toolNoun,
		toolStatus,
		type ToolPart,
		type ToolStatus,
	} from "$lib/components/chat/state/toolPresentation";
	import { thinkingDetail, type ThinkingDetail } from "$lib/stores/thinkingDetail.svelte";
	import type { WorkerStep } from "$lib/stores/chatInstances.svelte";

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
		 * except the last, which is the reply. Shown in the fold, never as
		 * the label.
		 */
		narration?: string[];
		/**
		 * The browsing agent's call in flight, while a `browse` call runs. A
		 * task of twenty steps is better described by the step happening now
		 * than by the one `browse` call that started it.
		 */
		browseStep?: WorkerStep;
		/** How long the turn worked, in seconds; 0 when nobody measured it. */
		seconds?: number;
		/** Play the landing when the turn ends. Off for a turn that was
		 *  stopped or failed: nothing landed. */
		land?: boolean;
		/**
		 * What the turn runs as: "chat", or the name of the applet `/name`
		 * picked. An applet turn goes out to the record by design, so the mark
		 * starts a dimension up rather than waiting for the first tool.
		 */
		agentMode?: string;
	}

	let {
		isThinking,
		toolCalls = [],
		reasoningContent = "",
		narration = [],
		browseStep = undefined,
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
	 * The tool call says what it is doing, so the label is built from the
	 * call's name and arguments (`describeTool`), and only then a plain
	 * fallback. The model is not asked to narrate for it: writing a status
	 * line before every call was one more instruction competing with the
	 * work, and no second model is asked either, because it could only
	 * paraphrase what `describeTool` derives for free.
	 */
	const thinkingLabel = $derived.by(() => {
		if (browseStep && toolRunning && toolName(toolRunning) === "browse") {
			return describeTool({ type: `tool-${browseStep.tool}`, input: browseStep.input }, true, true);
		}
		// Only a call still running: a finished call in the present tense was
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
		reducedMotion();

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

	/**
	 * THE RAIL. The turn as a line of steps, in the order they happened: the
	 * model's reasoning, what it said on the way, each call, and the reply.
	 */
	interface Step {
		key: string;
		status: ToolStatus;
		label: string;
		/** Why a call failed or stopped. */
		reason?: string;
		/** The whole text, shown at All. */
		text?: string;
		/** A line the model said on the way: shown only at All. */
		said?: boolean;
	}

	const steps = $derived.by((): Step[] => {
		const out: Step[] = [];
		if (reasoningContent) {
			out.push({
				key: "reasoning",
				status: isThinking && toolCalls.length === 0 ? "running" : "done",
				label: "Thought it through",
				text: reasoningContent,
			});
		}
		narration.forEach((line, i) => {
			out.push({ key: `said-${i}`, status: "done", label: clip(line, 90), text: line, said: true });
		});
		toolCalls.forEach((tool, i) => {
			const status = toolStatus(tool, isThinking);
			const name = toolName(tool);
			const key = tool.toolCallId || `tool-${i}`;
			if (name === "think") {
				const thought = String(tool.input?.thought ?? "").trim();
				out.push({ key, status, label: thought ? clip(thought, 90) : "Planned", text: thought });
				return;
			}
			out.push({
				key,
				status,
				label: describeTool(tool, status === "running" || status === "unfinished"),
				reason:
					status === "failed" && tool.errorText
						? `failed: ${toolErrorSummary(tool.errorText)}`
						: status === "unfinished"
							? "didn't finish"
							: undefined,
			});
		});
		// A turn that was stopped or failed wrote no reply.
		if (!isThinking && land) {
			out.push({ key: "reply", status: "done", label: "Wrote the reply" });
		}
		return out;
	});

	const detail = $derived(thinkingDetail.value);
	const shownSteps = $derived(detail === "all" ? steps : steps.filter((s) => !s.said));

	/** "a, b and c" */
	function listed(words: string[]): string {
		if (words.length < 2) return words.join("");
		return `${words.slice(0, -1).join(", ")} and ${words.at(-1)}`;
	}

	/**
	 * The folded turn says what it used, in words: the first three things,
	 * then "and more". Never how many: a number of steps is a count, and a
	 * row of identical dots is the same count drawn.
	 */
	const HEADER_TOOLS = 3;
	const headerTools = $derived(
		uniqueToolNames.length > HEADER_TOOLS
			? `${uniqueToolNames.slice(0, HEADER_TOOLS).join(", ")} and more`
			: listed(uniqueToolNames),
	);

	/** Brief: what the turn used, in one sentence. */
	const brief = $derived(
		uniqueToolNames.length > 0
			? `Used ${listed(uniqueToolNames)}.`
			: "Thought it through and wrote the reply.",
	);

	const DETAILS: { key: ThinkingDetail; label: string }[] = [
		{ key: "brief", label: "Brief" },
		{ key: "steps", label: "Steps" },
		{ key: "all", label: "All" },
	];
</script>

{#snippet marker(step: Step)}
	{#if step.status === "failed" || step.status === "unfinished"}
		<span class="mark-ring" class:failed={step.status === "failed"}></span>
	{:else}
		<span class="mark-dot" class:running={step.status === "running"}></span>
	{/if}
{/snippet}

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

			<!-- The summary of a finished turn: what it used. While it runs,
			     the label already names the call in flight. -->
			{#if !isThinking && headerTools}
				<span class="header-tools">{headerTools}</span>
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
			<div class="detail-row">
				<div class="detail-switch" role="group" aria-label="How much to show">
					{#each DETAILS as d (d.key)}
						<button
							type="button"
							aria-pressed={detail === d.key}
							class:on={detail === d.key}
							onclick={() => thinkingDetail.set(d.key)}>{d.label}</button
						>
					{/each}
				</div>
			</div>

			{#if detail === "brief"}
				<p class="brief">{brief}</p>
			{:else}
				<ol class="rail">
					{#each shownSteps as step (step.key)}
						<li
							class="step"
							class:running={step.status === "running"}
							class:failed={step.status === "failed"}
							class:unfinished={step.status === "unfinished"}
						>
							<span class="step-mark">
								{@render marker(step)}
							</span>
							<div class="step-body">
								<!-- A failed call says why, in one line: the Postgres
								     message, not the column list the model was handed.
								     The next step is the retry, so a failure the model
								     recovered from reads as what it was, one wrong guess
								     on the way, and never leaves this block for the
								     transcript (ChatView keeps the body-level error for a
								     turn that ENDED on the failure). -->
								<span class="step-label"
									>{step.label}{#if step.reason}<span class="tool-reason"> · {step.reason}</span>{/if}</span
								>
								{#if detail === "all" && step.text && step.text !== step.label}
									<p class="step-text">{step.text}</p>
								{/if}
							</div>
						</li>
					{/each}
				</ol>
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

	/* Content area. The resting fill, the same one a sent message sits in,
	   so the open block and the turn above it are one color. The header pill
	   hovers one step darker in the same pigment. */
	.block-content {
		margin-top: 8px;
		padding: 12px 16px;
		background: var(--fill-bg);
		border-radius: var(--radius-lg);
		max-height: 500px;
		overflow-y: auto;
		color: var(--color-foreground);
		font-size: 13px;
		line-height: 1.6;
	}

	.detail-row {
		display: flex;
		justify-content: flex-end;
		margin-bottom: 8px;
	}

	.detail-switch {
		display: flex;
		gap: 4px;
	}

	.detail-switch button {
		height: 22px;
		padding: 0 8px;
		border: 0;
		border-radius: var(--radius-sm);
		font: inherit;
		font-size: 12px;
		line-height: 1;
		color: var(--color-foreground-muted);
		background: transparent;
		cursor: pointer;
	}

	.detail-switch button:hover {
		color: var(--color-foreground);
	}

	.detail-switch button.on {
		color: var(--color-foreground);
		background: var(--color-surface);
		box-shadow:
			0 0 0 1px color-mix(in srgb, var(--color-foreground) 6%, transparent),
			0 1px 2px rgb(0 0 0 / 0.08);
	}

	.brief {
		margin: 0;
		color: var(--color-foreground);
	}

	/* One step per row, its mark on a hairline that joins it to the next. */
	.rail {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.step {
		position: relative;
		display: grid;
		grid-template-columns: 20px 1fr;
		column-gap: 8px;
		color: var(--color-foreground);
	}

	.step:not(:last-child)::before {
		content: "";
		position: absolute;
		left: 9.5px;
		top: 19px;
		bottom: -8px;
		width: 1px;
		background: var(--color-border);
	}

	.step.running,
	.step.unfinished {
		color: var(--color-foreground-muted);
	}

	.step.failed {
		color: var(--color-error);
	}

	.step-mark {
		height: 21px;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.step-body {
		min-width: 0;
		line-height: 1.6;
	}

	.step-text {
		margin: 4px 0 0;
		padding-left: 12px;
		border-left: 2px solid var(--color-border);
		color: var(--color-foreground-muted);
		line-height: 1.5;
		white-space: pre-wrap;
	}

	.tool-reason {
		opacity: 0.85;
	}

	.mark-dot {
		width: 7px;
		height: 7px;
		border-radius: 50%;
		background: var(--color-foreground-muted);
	}

	.mark-ring {
		position: relative;
		width: 9px;
		height: 9px;
		box-sizing: border-box;
		border-radius: 50%;
		border: 1.5px solid var(--color-foreground-subtle);
	}

	/* A failed step: the ring struck through. */
	.mark-ring.failed {
		border-color: var(--color-error);
	}

	.mark-ring.failed::after {
		content: "";
		position: absolute;
		left: calc(50% - 0.75px);
		top: -2px;
		bottom: -2px;
		width: 1.5px;
		background: var(--color-error);
		transform: rotate(45deg);
	}

	/* The step in flight takes the accent and breathes. */
	.mark-dot.running {
		background: var(--color-primary);
		animation: step-breathes 1.6s ease-in-out infinite;
	}

	@keyframes step-breathes {
		50% {
			opacity: 0.35;
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

		.mark-dot.running {
			animation: none;
		}
	}
</style>
