<script lang="ts">
	/**
	 * StreamHealthPanel — whether your data is actually still flowing.
	 *
	 * Every stall we ever hit was invisible: messages went dark for three days,
	 * the calendar sync died for two weeks, finance dropped every batch. Nothing
	 * surfaced any of it. This is that signal, worst-first.
	 *
	 * Status comes from the box (`/api/streams/health`); the panel only renders.
	 */
	import { onMount } from 'svelte';
	import Icon from '$lib/components/Icon.svelte';
	import { getStreamHealth, type StreamHealth } from '$lib/api/client';
	import { relativeTime } from '$lib/actions/palette';

	let streams = $state<StreamHealth[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);

	onMount(load);

	async function load() {
		loading = true;
		error = null;
		try {
			streams = await getStreamHealth();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load stream health';
		} finally {
			loading = false;
		}
	}

	const counts = $derived.by(() => {
		const c = { stalled: 0, idle: 0, never: 0, live: 0 } as Record<string, number>;
		for (const s of streams) c[s.status] = (c[s.status] ?? 0) + 1;
		return c;
	});

	// `never` is not a fault — that source was simply never connected — so it
	// must not make the panel read as broken.
	const needsAttention = $derived(counts.stalled > 0);

	const COPY: Record<string, string> = {
		live: 'Flowing',
		stalled: 'Stopped',
		idle: 'Nothing this week',
		never: 'Not connected'
	};
</script>

<section class="health">
	<header>
		<div class="title">
			<h2>Data flow</h2>
			{#if !loading && !error && streams.length > 0}
				<span class="summary" class:warn={needsAttention}>
					{#if needsAttention}
						{counts.stalled} stopped
					{:else}
						{counts.live} flowing
					{/if}
					{#if counts.idle}· {counts.idle} quiet{/if}
				</span>
			{/if}
		</div>
		<button type="button" class="refresh" onclick={load} disabled={loading} title="Refresh">
			<Icon icon="ri:refresh-line" width="15" />
		</button>
	</header>

	{#if loading}
		<p class="note">Checking…</p>
	{:else if error}
		<p class="note err">{error}</p>
	{:else if streams.length === 0}
		<p class="note">No streams yet.</p>
	{:else}
		<ul>
			{#each streams as s (s.name)}
				<li>
					<span class="dot {s.status}" aria-hidden="true"></span>
					<span class="name" title={s.name}>{s.display_name}</span>
					<span class="state {s.status}">{COPY[s.status] ?? s.status}</span>
					<span class="when">
						{s.status === 'never' ? '—' : relativeTime(s.last_ingest)}
					</span>
					<span class="count">
						{s.status === 'never' ? '' : `${s.count_24h.toLocaleString()} today`}
					</span>
				</li>
			{/each}
		</ul>
	{/if}
</section>

<style>
	@reference "../../../app.css";

	.health {
		border: 1px solid var(--color-border);
		border-radius: 12px;
		background: var(--color-surface);
		overflow: hidden;
	}
	header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
		padding: 12px 16px;
		border-bottom: 1px solid var(--color-border);
	}
	.title {
		display: flex;
		align-items: baseline;
		gap: 10px;
		min-width: 0;
	}
	h2 {
		font-size: 14px;
		font-weight: 600;
		color: var(--color-foreground);
		margin: 0;
	}
	.summary {
		font-size: 12px;
		color: var(--color-foreground-subtle);
	}
	.summary.warn {
		color: var(--color-warning);
		font-weight: 500;
	}
	.refresh {
		display: inline-flex;
		align-items: center;
		color: var(--color-foreground-subtle);
		background: none;
		border: none;
		cursor: pointer;
		padding: 4px;
		border-radius: 6px;
	}
	.refresh:hover:not(:disabled) {
		color: var(--color-foreground);
		background: var(--color-surface-elevated);
	}
	.refresh:disabled {
		opacity: 0.5;
		cursor: default;
	}
	.note {
		padding: 14px 16px;
		font-size: 13px;
		color: var(--color-foreground-muted);
		margin: 0;
	}
	.note.err {
		color: var(--color-error);
	}
	ul {
		list-style: none;
		margin: 0;
		padding: 4px 0;
	}
	li {
		display: grid;
		grid-template-columns: 10px minmax(0, 1.4fr) minmax(0, 1fr) auto auto;
		align-items: center;
		gap: 10px;
		padding: 7px 16px;
		font-size: 13px;
	}
	li:hover {
		background: var(--color-surface-elevated);
	}
	.dot {
		width: 7px;
		height: 7px;
		border-radius: 50%;
		background: var(--color-foreground-subtle);
	}
	.dot.live {
		background: var(--color-success, #22c55e);
	}
	.dot.stalled {
		background: var(--color-error);
	}
	.dot.idle {
		background: var(--color-warning);
	}
	.dot.never {
		background: var(--color-border-strong, #9ca3af);
	}
	.name {
		color: var(--color-foreground);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.state {
		color: var(--color-foreground-muted);
		font-size: 12px;
	}
	.state.stalled {
		color: var(--color-error);
		font-weight: 500;
	}
	.state.idle {
		color: var(--color-warning);
	}
	.when,
	.count {
		color: var(--color-foreground-subtle);
		font-size: 12px;
		text-align: right;
		white-space: nowrap;
	}
	.count {
		min-width: 84px;
	}
</style>
