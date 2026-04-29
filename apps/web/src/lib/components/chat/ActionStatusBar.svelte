<script lang="ts">
	import Icon from '$lib/components/Icon.svelte';
	import { formatRelativeTimestamp } from '$lib/utils/dateUtils';

	let { chatId, onNewMessages, onEditCode }: { chatId: string; onNewMessages?: () => void; onEditCode?: () => void } = $props();

	interface ActionConfig {
		action_state: string | null;
		action_instruction: string | null;
		action_trigger: string | null;
		action_activation: string | null;
		action_last_run_at: string | null;
		action_trigger_token: string | null;
		next_run: string | null;
	}

	let config = $state<ActionConfig | null>(null);
	let loading = $state(true);
	let running = $state(false);
	let currentStep = $state<number>(0);

	let lastRunAt = $state<string | null>(null);

	// SSE connection for real-time action events
	let eventSource: EventSource | null = null;

	$effect(() => {
		if (chatId) {
			loadConfig();
		}
	});

	// Poll for cron-triggered runs (check every 30s if last_run_at changed)
	$effect(() => {
		if (!chatId || !config?.action_state) return;
		const interval = setInterval(async () => {
			try {
				const res = await fetch(`/api/chats/${chatId}/action`);
				if (res.ok) {
					const updated = await res.json();
					if (updated.action_last_run_at && updated.action_last_run_at !== lastRunAt) {
						lastRunAt = updated.action_last_run_at;
						config = updated;
						onNewMessages?.();
					}
				}
			} catch { /* ignore */ }
		}, 30000);
		return () => {
			clearInterval(interval);
			closeEventSource();
		};
	});

	async function loadConfig() {
		loading = true;
		try {
			const res = await fetch(`/api/chats/${chatId}/action`);
			if (res.ok) {
				config = await res.json();
				lastRunAt = config?.action_last_run_at ?? null;
				if (config?.action_state === 'working') {
					running = true;
					subscribeToEvents();
				}
			} else {
				config = null;
			}
		} catch {
			config = null;
		} finally {
			loading = false;
		}
	}

	async function togglePause() {
		if (!config) return;
		const newState = config.action_state === 'paused' ? 'scheduled' : 'paused';
		try {
			const res = await fetch(`/api/chats/${chatId}`, {
				method: 'PATCH',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ action_state: newState })
			});
			if (res.ok) {
				config = { ...config, action_state: newState };
			}
		} catch (e) {
			console.error('Failed to toggle action state:', e);
		}
	}

	function closeEventSource() {
		if (eventSource) {
			eventSource.close();
			eventSource = null;
		}
	}

	/** Subscribe to real-time action events via SSE */
	function subscribeToEvents() {
		closeEventSource();
		currentStep = 0;

		const es = new EventSource(`/api/chats/${chatId}/action/events`);
		eventSource = es;

		es.addEventListener('action_event', (e) => {
			try {
				const event = JSON.parse(e.data);
				switch (event.type) {
					case 'step_complete':
						currentStep = event.step;
						break;
					case 'done':
						running = false;
						currentStep = 0;
						closeEventSource();
						loadConfig();
						onNewMessages?.();
						break;
					case 'error':
						running = false;
						currentStep = 0;
						closeEventSource();
						loadConfig();
						break;
				}
			} catch { /* ignore parse errors */ }
		});

		es.onerror = () => {
			closeEventSource();
			if (running) {
				running = false;
				loadConfig();
			}
		};
	}

	async function runNow() {
		if (!config || running) return;
		running = true;
		config = { ...config, action_state: 'working' };

		subscribeToEvents();

		try {
			await fetch(`/api/chats/${chatId}/action/run`, { method: 'POST' });
		} catch (e) {
			console.error('Failed to trigger action run:', e);
			closeEventSource();
			running = false;
			loadConfig();
		}
	}

	const stateColor = $derived(
		config?.action_state === 'scheduled' || config?.action_state === 'listening'
			? 'var(--color-success)'
			: config?.action_state === 'working'
				? 'var(--color-warning)'
				: 'var(--color-foreground-subtle)'
	);

	const stateLabel = $derived(
		config?.action_state === 'scheduled'
			? 'Scheduled'
			: config?.action_state === 'listening'
				? 'Listening'
				: config?.action_state === 'working'
					? 'Running'
					: config?.action_state === 'paused'
						? 'Paused'
						: config?.action_state === 'complete'
							? 'Complete'
							: 'Unknown'
	);
</script>

{#if !loading && config}
	<div class="action-status-bar">
		<div class="status-left">
			<span class="status-dot" style:background-color={stateColor}></span>
			<span class="status-label">{stateLabel}{#if running && currentStep > 0} (step {currentStep}){/if}</span>
			{#if config.next_run}
				<span class="status-detail">
					Next: {formatRelativeTimestamp(config.next_run)}
				</span>
			{/if}
			{#if config.action_last_run_at}
				<span class="status-detail">
					Last: {formatRelativeTimestamp(config.action_last_run_at)}
				</span>
			{/if}
		</div>
		<div class="status-actions">
			{#if onEditCode}
				<button class="status-btn code-btn" onclick={onEditCode} title="Edit activation code">
					<Icon icon="ri:code-line" width={14} height={14} />
					<span>Code</span>
				</button>
			{/if}
			<button
				class="status-btn"
				onclick={runNow}
				title="Run now"
				disabled={running || config.action_state === 'working'}
			>
				<Icon icon="ri:rocket-line" width={14} height={14} />
			</button>
			<button class="status-btn" onclick={togglePause} title={config.action_state === 'paused' ? 'Resume' : 'Pause'}>
				<Icon icon={config.action_state === 'paused' ? 'ri:play-line' : 'ri:pause-line'} width={14} height={14} />
			</button>
		</div>
	</div>
{/if}

<style>
	.action-status-bar {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 6px 16px;
		background: var(--color-surface-elevated);
		border-bottom: 1px solid var(--color-border);
		font-size: 12px;
		color: var(--color-foreground-muted);
		gap: 12px;
	}

	.status-left {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.status-dot {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		flex-shrink: 0;
	}

	.status-label {
		font-weight: 500;
		color: var(--color-foreground);
	}

	.status-detail {
		color: var(--color-foreground-subtle);
	}

	.status-detail::before {
		content: '\00b7';
		margin-right: 8px;
	}

	.status-actions {
		display: flex;
		gap: 4px;
	}

	.status-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 24px;
		height: 24px;
		border-radius: 4px;
		border: none;
		background: transparent;
		color: var(--color-foreground-muted);
		cursor: pointer;
		transition: all 0.15s;
	}

	.code-btn {
		width: auto;
		gap: 4px;
		padding: 0 6px;
		font-size: 11px;
	}

	.status-btn:hover:not(:disabled) {
		background: var(--color-surface-hover);
		color: var(--color-foreground);
	}

	.status-btn:disabled {
		opacity: 0.4;
		cursor: default;
	}
</style>
