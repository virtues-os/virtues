<script lang="ts">
	import 'iconify-icon';
	import ExpandableContent from './ExpandableContent.svelte';
	import { getDisplayInfo } from '$lib/citations/mapping';

	interface ToolCallPart {
		type: string;
		toolCallId: string;
		toolName: string;
		input: Record<string, unknown>;
		state: 'pending' | 'output-available' | 'output-error';
		output?: unknown;
		errorText?: string;
	}

	let {
		part,
		index
	} = $props<{
		part: ToolCallPart;
		index: number;
	}>();

	let expanded = $state(true);

	// Get display info (icon, color, label) for the tool
	const displayInfo = $derived(getDisplayInfo(part.toolName, part.input));

	// Format the input/output as JSON string
	const inputStr = $derived(JSON.stringify(part.input, null, 2));
	const outputStr = $derived(
		part.output ? JSON.stringify(part.output, null, 2) : ''
	);

	// Status indicator
	const statusIcon = $derived.by(() => {
		switch (part.state) {
			case 'pending':
				return 'ri:loader-4-line';
			case 'output-available':
				return 'ri:check-line';
			case 'output-error':
				return 'ri:close-line';
			default:
				return 'ri:question-line';
		}
	});

	const statusColor = $derived.by(() => {
		switch (part.state) {
			case 'pending':
				return 'text-amber-500';
			case 'output-available':
				return 'text-green-500';
			case 'output-error':
				return 'text-red-500';
			default:
				return 'text-gray-500';
		}
	});
</script>

<div class="trace-item" class:expanded>
	<button class="trace-header" onclick={() => expanded = !expanded}>
		<div class="header-left">
			<iconify-icon
				icon={expanded ? 'ri:arrow-down-s-line' : 'ri:arrow-right-s-line'}
				width="14"
				height="14"
				class="expand-icon"
			></iconify-icon>
			<span class="trace-index">[{index}]</span>
			<span class="tool-icon {displayInfo.color}">
				<iconify-icon icon={displayInfo.icon} width="16" height="16"></iconify-icon>
			</span>
			<span class="tool-name">{part.toolName}</span>
		</div>
		<div class="header-right">
			<span class="status-icon {statusColor}">
				<iconify-icon icon={statusIcon} width="14" height="14" class:spinning={part.state === 'pending'}></iconify-icon>
			</span>
		</div>
	</button>

	{#if expanded}
		<div class="trace-content">
			<!-- Input -->
			<div class="trace-section">
				<div class="section-label">
					<iconify-icon icon="ri:arrow-right-line" width="12" height="12"></iconify-icon>
					Input
				</div>
				<ExpandableContent content={inputStr} maxLines={5} />
			</div>

			<!-- Output (if available) -->
			{#if part.state === 'output-available' && outputStr}
				<div class="trace-section">
					<div class="section-label">
						<iconify-icon icon="ri:arrow-left-line" width="12" height="12"></iconify-icon>
						Output
					</div>
					<ExpandableContent content={outputStr} maxLines={5} />
				</div>
			{:else if part.state === 'output-error'}
				<div class="trace-section error">
					<div class="section-label">
						<iconify-icon icon="ri:error-warning-line" width="12" height="12"></iconify-icon>
						Error
					</div>
					<div class="error-text">{part.errorText || 'Unknown error'}</div>
				</div>
			{:else if part.state === 'pending'}
				<div class="trace-section pending">
					<div class="pending-indicator">
						<iconify-icon icon="ri:loader-4-line" width="14" height="14" class="spinning"></iconify-icon>
						<span>Executing...</span>
					</div>
				</div>
			{/if}
		</div>
	{/if}
</div>

<style>
	.trace-item {
		border: 1px solid var(--color-border, #e5e5e5);
		border-radius: 8px;
		overflow: hidden;
		background: var(--color-surface, #ffffff);
	}

	.trace-header {
		width: 100%;
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 10px 12px;
		background: var(--color-surface-elevated, #fafafa);
		border: none;
		cursor: pointer;
		transition: background 0.15s;
	}

	.trace-header:hover {
		background: var(--color-border, #f0f0f0);
	}

	.header-left {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.expand-icon {
		color: var(--color-foreground-muted, #737373);
	}

	.trace-index {
		font-size: 0.6875rem;
		font-family: ui-monospace, 'SF Mono', Menlo, Consolas, monospace;
		color: var(--color-foreground-muted, #a3a3a3);
	}

	.tool-icon {
		display: flex;
		align-items: center;
	}

	.tool-name {
		font-size: 0.8125rem;
		font-weight: 500;
		color: var(--color-foreground, #171717);
		font-family: ui-monospace, 'SF Mono', Menlo, Consolas, monospace;
	}

	.header-right {
		display: flex;
		align-items: center;
	}

	.status-icon {
		display: flex;
		align-items: center;
	}

	.trace-content {
		padding: 12px;
		border-top: 1px solid var(--color-border, #e5e5e5);
		display: flex;
		flex-direction: column;
		gap: 12px;
	}

	.trace-section {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.section-label {
		display: flex;
		align-items: center;
		gap: 4px;
		font-size: 0.6875rem;
		font-weight: 500;
		text-transform: uppercase;
		letter-spacing: 0.025em;
		color: var(--color-foreground-muted, #737373);
	}

	.trace-section.error .section-label {
		color: var(--color-error, #ef4444);
	}

	.error-text {
		font-size: 0.8125rem;
		color: var(--color-error, #ef4444);
		padding: 8px;
		background: rgba(239, 68, 68, 0.1);
		border-radius: 6px;
	}

	.pending-indicator {
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: 0.8125rem;
		color: var(--color-foreground-muted, #737373);
	}

	.spinning {
		animation: spin 1s linear infinite;
	}

	@keyframes spin {
		from {
			transform: rotate(0deg);
		}
		to {
			transform: rotate(360deg);
		}
	}

	/* Color classes from mapping */
	:global(.text-red-500) { color: #ef4444; }
	:global(.text-blue-500) { color: #3b82f6; }
	:global(.text-green-500) { color: #22c55e; }
	:global(.text-amber-500) { color: #f59e0b; }
	:global(.text-purple-500) { color: #a855f7; }
	:global(.text-indigo-500) { color: #6366f1; }
	:global(.text-violet-500) { color: #8b5cf6; }
	:global(.text-rose-500) { color: #f43f5e; }
	:global(.text-gray-500) { color: #6b7280; }
	:global(.text-cyan-500) { color: #06b6d4; }
	:global(.text-orange-500) { color: #f97316; }
	:global(.text-pink-500) { color: #ec4899; }
	:global(.text-emerald-500) { color: #10b981; }
</style>
