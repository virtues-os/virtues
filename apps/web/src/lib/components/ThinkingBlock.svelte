<script lang="ts">
	import { onMount } from "svelte";
	import { slide } from "svelte/transition";
	import { cubicOut } from "svelte/easing";
	import { getRandomThinkingLabel } from "$lib/utils/thinkingLabels";

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
		/** Whether response content is currently streaming */
		isStreaming: boolean;
		/** Duration in seconds spent thinking */
		duration?: number;
	}

	let {
		isThinking,
		toolCalls = [],
		reasoningContent = "",
		isStreaming,
		duration = 0,
	}: Props = $props();

	// Expansion state - starts expanded, collapses when content arrives
	let expanded = $state(true);
	let hasAutoCollapsed = $state(false);

	// Track thinking duration
	let thinkingStartTime = $state<number | null>(null);
	let calculatedDuration = $state(0);

	// Rotating thinking label
	let thinkingLabel = $state(getRandomThinkingLabel());

	// Animated ellipsis
	let dots = $state("");

	onMount(() => {
		// Animate dots
		const dotsInterval = setInterval(() => {
			dots = dots.length >= 3 ? "" : dots + ".";
		}, 400);

		// Rotate label every 4 seconds
		const labelInterval = setInterval(() => {
			thinkingLabel = getRandomThinkingLabel();
		}, 4000);

		return () => {
			clearInterval(dotsInterval);
			clearInterval(labelInterval);
		};
	});

	// Track thinking start time
	$effect(() => {
		if (isThinking && !thinkingStartTime) {
			thinkingStartTime = Date.now();
			thinkingLabel = getRandomThinkingLabel();
		} else if (!isThinking && thinkingStartTime) {
			calculatedDuration = (Date.now() - thinkingStartTime) / 1000;
			thinkingStartTime = null;
		}
	});

	// Auto-collapse when streaming starts
	$effect(() => {
		if (isStreaming && !hasAutoCollapsed && expanded) {
			setTimeout(() => {
				expanded = false;
				hasAutoCollapsed = true;
			}, 300);
		}
	});

	// Reset on new thinking session
	$effect(() => {
		if (isThinking && hasAutoCollapsed) {
			hasAutoCollapsed = false;
			expanded = true;
		}
	});

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

	// Get human-readable description of what the tool is doing
	function getToolDescription(tool: ToolCallPart): string {
		const name = getToolName(tool);
		const input = tool.input || {};

		switch (name) {
			case "web_search":
				return `Searched the web for "${input.query || "information"}"`;
			case "query_database":
			case "database_query":
				return "Queried your personal data";
			case "calendar":
			case "get_calendar":
				return "Checked your calendar";
			case "location":
			case "get_location":
				return "Reviewed your location history";
			case "contacts":
			case "get_contacts":
				return "Looked up your contacts";
			case "notes":
			case "get_notes":
				return "Searched your notes";
			case "memory":
			case "recall":
				return "Recalled from memory";
			default:
				return name.replace(/_/g, " ").replace(/\b\w/g, (c) => c);
		}
	}

	// Check if we have content
	const hasContent = $derived(reasoningContent || toolCalls.length > 0);

	// Get unique tools for collapsed summary
	const uniqueToolNames = $derived.by(() => {
		const names = toolCalls.map((t) => getToolName(t));
		return [...new Set(names)];
	});
</script>

<div class="thinking-block">
	<!-- Header -->
	<button
		type="button"
		class="block-header"
		onclick={() => (expanded = !expanded)}
		aria-expanded={expanded}
	>
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

		<span class="header-content">
			{#if isThinking}
				<span class="thinking-text"
					>{thinkingLabel}<span class="dots">{dots}</span></span
				>
			{:else}
				<span class="duration-text">
					Thought for {formatDuration(duration || calculatedDuration)}
				</span>
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

			{#if toolCalls.length > 0}
				<ul class="tool-list">
					{#each toolCalls as tool, index (tool.toolCallId || `tool-${index}`)}
						{@const isPending =
							tool.state === "pending" ||
							tool.state === "input-available" ||
							!tool.state}
						{@const isError = tool.state === "output-error"}
						<li
							class="tool-item"
							class:pending={isPending}
							class:error={isError}
						>
							{#if isPending}
								<span class="tool-spinner"></span>
							{:else if isError}
								<span class="tool-icon error">✕</span>
							{:else}
								<span class="tool-icon">✓</span>
							{/if}
							<span class="tool-description">
								{getToolDescription(tool)}
							</span>
						</li>
					{/each}
				</ul>
			{/if}
		</div>
	{/if}
</div>

<style>
	.thinking-block {
		margin-bottom: 16px;
	}

	/* Header with hover effect */
	.block-header {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 6px 10px;
		margin: -6px -10px;
		background: transparent;
		border: none;
		border-radius: 6px;
		cursor: pointer;
		color: var(--color-foreground-muted);
		font-size: 13px;
		line-height: 1.5;
		text-align: left;
		transition:
			background-color 0.15s ease,
			color 0.15s ease;
	}

	.block-header:hover {
		background-color: var(--color-surface-elevated);
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

	.block-header:hover .chevron {
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

	.dots {
		display: inline-block;
		width: 1.2em;
		text-align: left;
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
		max-height: 200px;
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
