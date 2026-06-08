<!--
	LogsPanel.svelte — per-app stdout/stderr ring buffer.

	Renders the captured log lines for an `app`-runtime action. Polls
	`/api/actions/<id>/logs` every 2s. Auto-scrolls to bottom on new lines
	unless the user has scrolled up (gives them a "tail mode").

	v1: JSON polling. v1.1 will switch to SSE for sub-second latency.
-->

<script lang="ts">
	import { getActionLogs, type LogLine } from '$lib/api/client';

	let { actionId }: { actionId: string } = $props();

	let lines = $state<LogLine[]>([]);
	let err = $state<string | null>(null);
	let loaded = $state(false);
	let containerEl = $state<HTMLElement | null>(null);
	let pinnedToBottom = $state(true);

	async function load() {
		try {
			const next = await getActionLogs(actionId);
			lines = next;
			err = null;
		} catch (e) {
			err = e instanceof Error ? e.message : String(e);
		} finally {
			loaded = true;
		}
	}

	$effect(() => {
		void load();
		const id = setInterval(load, 2000);
		return () => clearInterval(id);
	});

	$effect(() => {
		// On new lines, scroll to bottom if the user is still pinned there.
		void lines;
		if (pinnedToBottom && containerEl) {
			queueMicrotask(() => {
				if (containerEl) containerEl.scrollTop = containerEl.scrollHeight;
			});
		}
	});

	function onScroll() {
		if (!containerEl) return;
		const distanceFromBottom =
			containerEl.scrollHeight - containerEl.scrollTop - containerEl.clientHeight;
		pinnedToBottom = distanceFromBottom < 24;
	}

	function jumpToBottom() {
		if (!containerEl) return;
		containerEl.scrollTop = containerEl.scrollHeight;
		pinnedToBottom = true;
	}

	function formatTime(iso: string): string {
		try {
			return new Date(iso).toLocaleTimeString(undefined, {
				hour12: false,
				hour: '2-digit',
				minute: '2-digit',
				second: '2-digit'
			});
		} catch {
			return iso;
		}
	}
</script>

<section class="logs-panel">
	<header>
		<h2>Logs</h2>
		<div class="meta">
			<span class="dim small">{lines.length} line{lines.length === 1 ? '' : 's'}</span>
			{#if !pinnedToBottom}
				<button class="jump" type="button" onclick={jumpToBottom}>Jump to latest</button>
			{/if}
		</div>
	</header>

	{#if err}
		<div class="error">{err}</div>
	{/if}

	{#if !loaded}
		<p class="muted">Loading…</p>
	{:else if lines.length === 0}
		<p class="muted">No output yet.</p>
	{:else}
		<div class="stream" bind:this={containerEl} onscroll={onScroll}>
			{#each lines as ln, i (i)}
				<div class="line" class:err={ln.stream === 'stderr'}>
					<span class="t">{formatTime(ln.at)}</span>
					<span class="s {ln.stream}">{ln.stream === 'stderr' ? 'E' : ' '}</span>
					<span class="text">{ln.line}</span>
				</div>
			{/each}
		</div>
	{/if}
</section>

<style>
	.logs-panel {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}
	header {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 1rem;
	}
	header h2 {
		margin: 0;
		font-size: 0.6875rem;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--color-foreground-subtle, #9ca3af);
	}
	.meta {
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}
	.jump {
		font-size: 0.6875rem;
		padding: 0.125rem 0.375rem;
		border: 1px solid var(--color-border, #e5e7eb);
		border-radius: 4px;
		background: var(--color-surface, #fff);
		cursor: pointer;
	}

	.stream {
		max-height: 32rem;
		overflow-y: auto;
		padding: 0.5rem 0.75rem;
		border: 1px solid var(--color-border, #e5e7eb);
		border-radius: 8px;
		background: var(--color-surface-elevated, #f9fafb);
		font-family: var(--font-mono, ui-monospace, SFMono-Regular, Menlo, monospace);
		font-size: 0.75rem;
		line-height: 1.45;
	}
	.line {
		display: grid;
		grid-template-columns: 64px 12px 1fr;
		gap: 0.375rem;
		padding: 0.0625rem 0;
		white-space: pre-wrap;
		word-break: break-word;
	}
	.line.err {
		color: #b91c1c;
	}
	.line .t {
		color: var(--color-foreground-subtle, #9ca3af);
		font-size: 0.6875rem;
		padding-top: 0.0625rem;
	}
	.line .s {
		text-align: center;
		font-weight: 600;
	}
	.line .s.stderr {
		color: #b91c1c;
	}

	.dim {
		color: var(--color-foreground-subtle, #9ca3af);
	}
	.small {
		font-size: 0.6875rem;
	}
	.muted {
		margin: 0;
		font-size: 0.8125rem;
		font-style: italic;
		color: var(--color-foreground-subtle, #9ca3af);
	}
	.error {
		padding: 0.375rem 0.625rem;
		border-radius: 6px;
		background: #fee2e2;
		color: #991b1b;
		font-size: 0.75rem;
	}
</style>
