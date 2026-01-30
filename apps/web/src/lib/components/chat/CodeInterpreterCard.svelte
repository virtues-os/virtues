<script lang="ts">
	/**
	 * CodeInterpreterCard
	 *
	 * Displays code_interpreter tool results with expandable code and output.
	 * Matches EditDiffCard design pattern.
	 */
	import { slide } from 'svelte/transition';
	import Icon from '$lib/components/Icon.svelte';

	interface CodeOutput {
		stdout?: string;
		stderr?: string;
		success?: boolean;
		error?: string;
		/** Execution time in milliseconds (from backend) */
		execution_time_ms?: number;
	}

	interface Props {
		/** Status of the execution */
		status: 'running' | 'success' | 'error';
		/** The Python code that was executed */
		code: string;
		/** The execution output */
		output?: CodeOutput;
	}

	let { status, code, output }: Props = $props();

	let expanded = $state(false);
	let copySuccess = $state(false);

	const statusConfig = $derived({
		running: {
			icon: 'ri:loader-4-line',
			label: 'Running code',
			spinning: true
		},
		success: {
			icon: 'ri:check-line',
			label: 'Code executed',
			spinning: false
		},
		error: {
			icon: 'ri:error-warning-line',
			label: 'Execution failed',
			spinning: false
		}
	}[status]);

	// Format duration for display (converts ms to readable format)
	const durationText = $derived(() => {
		const ms = output?.execution_time_ms;
		if (!ms) return '';
		const seconds = ms / 1000;
		if (seconds < 1) return '< 1s';
		if (seconds < 60) return `${seconds.toFixed(1)}s`;
		const mins = Math.floor(seconds / 60);
		const secs = Math.round(seconds % 60);
		return `${mins}m ${secs}s`;
	});

	// Combined output text for display
	const outputText = $derived(() => {
		if (!output) return '';
		if (output.error) return output.error;
		const parts: string[] = [];
		if (output.stdout) parts.push(output.stdout);
		if (output.stderr) parts.push(output.stderr);
		return parts.join('\n').trim();
	});

	function toggleExpanded() {
		expanded = !expanded;
	}

	async function handleCopy(e: Event) {
		e.stopPropagation();
		const text = outputText();
		if (!text) return;
		
		try {
			await navigator.clipboard.writeText(text);
			copySuccess = true;
			setTimeout(() => {
				copySuccess = false;
			}, 2000);
		} catch (err) {
			console.error('Failed to copy:', err);
		}
	}
</script>

<div class="code-card" class:expanded class:error={status === 'error'}>
	<!-- Header -->
	<!-- svelte-ignore a11y_click_events_have_key_events -->
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="card-header" onclick={toggleExpanded}>
		<div class="header-left">
			<span class="status-icon" class:spinning={statusConfig.spinning}>
				<Icon icon={statusConfig.icon} width="16" />
			</span>
			<span class="status-label">{statusConfig.label}</span>
			{#if durationText()}
				<span class="duration">· {durationText()}</span>
			{/if}
		</div>
		<div class="header-right">
			{#if outputText()}
				<button 
					class="copy-btn" 
					onclick={handleCopy} 
					type="button" 
					title={copySuccess ? 'Copied!' : 'Copy output'}
				>
					<Icon icon={copySuccess ? 'ri:check-line' : 'ri:file-copy-line'} width="16" />
				</button>
			{/if}
			<Icon icon={expanded ? 'ri:arrow-up-s-line' : 'ri:arrow-down-s-line'} width="18" />
		</div>
	</div>

	<!-- Expanded content -->
	{#if expanded}
		<div class="card-content" transition:slide={{ duration: 200 }}>
			<!-- Code input section -->
			<div class="section">
				<div class="section-label">
					<Icon icon="ri:code-s-slash-line" width="14" />
					<span>Code</span>
				</div>
				<div class="code-view">
					<pre class="code-text">{code}</pre>
				</div>
			</div>

			<!-- Output section -->
			{#if outputText()}
				<div class="section">
					<div class="section-label" class:error={status === 'error' || output?.stderr}>
						<Icon icon={status === 'error' ? 'ri:error-warning-line' : 'ri:terminal-line'} width="14" />
						<span>{status === 'error' ? 'Error' : 'Output'}</span>
					</div>
					<div class="output-view" class:error={status === 'error'}>
						<pre class="output-text">{outputText()}</pre>
					</div>
				</div>
			{/if}
		</div>
	{/if}
</div>

<style>
	.code-card {
		margin: 0.5rem 0;
		border: 1px solid var(--color-border);
		border-radius: 0.5rem;
		background: var(--color-surface);
		overflow: hidden;
		font-size: 0.8125rem;
	}

	.code-card.error {
		border-color: var(--color-error);
	}

	/* Header */
	.card-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		width: 100%;
		padding: 0.625rem 0.875rem;
		background: var(--color-surface);
		border: none;
		cursor: pointer;
		color: var(--color-text);
		transition: background 0.15s ease;
	}

	.card-header:hover {
		background: var(--color-surface-hover);
	}

	.header-left {
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}

	.status-icon {
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.status-icon.spinning {
		animation: spin 1s linear infinite;
	}

	@keyframes spin {
		from { transform: rotate(0deg); }
		to { transform: rotate(360deg); }
	}

	.status-label {
		font-weight: 500;
	}

	.duration {
		color: var(--color-text-secondary);
		font-weight: 400;
	}

	.header-right {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		color: var(--color-text-secondary);
	}

	.copy-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 0.25rem;
		background: transparent;
		border: none;
		color: var(--color-text-secondary);
		cursor: pointer;
		border-radius: 0.25rem;
		transition: all 0.15s ease;
	}

	.copy-btn:hover {
		background: var(--color-surface-hover);
		color: var(--color-text);
	}

	/* Content */
	.card-content {
		border-top: 1px solid var(--color-border);
		background: var(--color-surface);
	}

	/* Sections */
	.section {
		border-bottom: 1px solid var(--color-border);
	}

	.section:last-child {
		border-bottom: none;
	}

	.section-label {
		display: flex;
		align-items: center;
		gap: 0.375rem;
		padding: 0.5rem 0.75rem;
		font-size: 0.6875rem;
		font-weight: 500;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--color-text-secondary);
		background: var(--color-surface-elevated);
	}

	.section-label.error {
		color: var(--color-error);
	}

	/* Code view */
	.code-view {
		font-family: var(--font-mono, ui-monospace, monospace);
		font-size: 0.75rem;
		line-height: 1.5;
		max-height: 12rem;
		overflow-y: auto;
		background: var(--color-surface);
	}

	.code-text {
		margin: 0;
		padding: 0.75rem;
		white-space: pre-wrap;
		word-break: break-word;
		color: var(--color-text);
	}

	/* Output view */
	.output-view {
		font-family: var(--font-mono, ui-monospace, monospace);
		font-size: 0.75rem;
		line-height: 1.5;
		max-height: 16rem;
		overflow-y: auto;
		background: var(--color-surface);
	}

	.output-view.error {
		background: rgba(var(--color-error-rgb, 239, 68, 68), 0.05);
	}

	.output-text {
		margin: 0;
		padding: 0.75rem;
		white-space: pre-wrap;
		word-break: break-word;
		color: var(--color-text);
	}

	.output-view.error .output-text {
		color: var(--color-error);
	}

	/* Scrollbar styling */
	.code-view::-webkit-scrollbar,
	.output-view::-webkit-scrollbar {
		width: 4px;
	}

	.code-view::-webkit-scrollbar-track,
	.output-view::-webkit-scrollbar-track {
		background: transparent;
	}

	.code-view::-webkit-scrollbar-thumb,
	.output-view::-webkit-scrollbar-thumb {
		background-color: var(--color-border);
		border-radius: 2px;
	}

	/* Reduced motion */
	@media (prefers-reduced-motion: reduce) {
		.status-icon.spinning {
			animation: none;
		}
	}
</style>
