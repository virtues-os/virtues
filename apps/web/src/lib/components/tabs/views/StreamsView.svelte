<script lang="ts">
	/**
	 * Sources · Streams: what your devices and accounts sent, archived
	 * exactly as it arrived, and the files those records point to.
	 *
	 * Your server builds everything else from these raw copies, so this page
	 * changes nothing. A record leaves its raw copy only when it is deleted
	 * (api::deletions, which Query and the assistant use), and recordings go
	 * when Keep recordings says. Each row opens its source's page.
	 */
	import { onMount } from 'svelte';
	import LoadError from '$lib/components/settings/LoadError.svelte';
	import { getLakeStreams, listSourceCatalog } from '$lib/api/client';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { openSetting } from '$lib/settings/focus';
	import { DEFAULT_ASSISTANT_NAME } from '$lib/settings';
	import { formatBytes } from '$lib/settings/serverStatus';
	import { covers, streamName, streamsLead, type LakeStream } from '$lib/sources/streams';

	let streams = $state<LakeStream[] | null>(null);
	let names = $state<Map<string, string>>(new Map());
	let failed = $state<string | null>(null);

	async function load() {
		failed = null;
		try {
			const [rows, catalog] = await Promise.all([
				getLakeStreams<LakeStream[]>(),
				// Names only: a catalog that doesn't load leaves the ids showing.
				listSourceCatalog().catch(() => []),
			]);
			streams = rows;
			names = new Map(catalog.map((s) => [s.id, s.name]));
		} catch (e) {
			failed = `Your server couldn’t list what it keeps: ${e instanceof Error ? e.message : String(e)}`;
		}
	}

	onMount(load);

	const shown = $derived((streams ?? []).filter((s) => s.source_type === 'raw_stream' || s.source_type === 'media'));
	const records = $derived(shown.filter((s) => s.source_type === 'raw_stream'));
	const files = $derived(shown.filter((s) => s.source_type === 'media'));
	const now = Date.now();

	const sourceName = (s: LakeStream) => names.get(s.source_id) ?? s.source_name;

	function openSource(s: LakeStream) {
		windowShellStore.navigate(`/sources/${s.source_id}`, { label: sourceName(s) });
	}
</script>

{#snippet list(rows: LakeStream[], countLabel: string, count: (s: LakeStream) => number)}
	<div class="head" aria-hidden="true">
		<span>Stream</span><span class="num">{countLabel}</span><span class="num">Size</span><span class="num">Covers</span><span></span>
	</div>
	<div role="list">
		{#each rows as s (`${s.source_id}/${s.stream_name}`)}
			<div role="listitem"><button type="button" class="row" onclick={() => openSource(s)}>
				<span class="words">
					<span class="name">{streamName(s.stream_name)}</span>
					<span class="source">{sourceName(s)}<span class="dates-inline">, {covers(s.earliest_at, s.latest_at, now)}</span></span>
				</span>
				<span class="num count">{count(s).toLocaleString()}</span>
				<span class="num size">{formatBytes(s.size_bytes)}</span>
				<span class="num dates">{covers(s.earliest_at, s.latest_at, now)}</span>
				<svg class="chevron" viewBox="0 0 24 24" aria-hidden="true" width="15" height="15">
					<path fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.8" d="m9 18l6-6l-6-6" />
				</svg>
			</button></div>
		{/each}
	</div>
{/snippet}

<div class="streams">
	{#if failed}
		<LoadError message={failed} onretry={load} />
	{:else if !streams}
		<p class="sub">Asking your server what it keeps…</p>
	{:else}
		<p class="lead">{streamsLead(shown)}</p>
		<p class="sub">
			{#if shown.length}
				This is what your devices and accounts sent, exactly as it arrived. Everything else your server knows is built from it, so
				nothing here can be changed. Deleting a record in Query deletes its raw copy too, and recordings go when
				<button type="button" class="link" onclick={() => openSetting('keep-recordings', DEFAULT_ASSISTANT_NAME)}>Keep recordings</button> says.
			{:else}
				When a device or account sends its first records, they appear here exactly as they arrived.
			{/if}
		</p>

		{#if records.length}
			<section class="group">
				<h2>Records</h2>
				<p class="group-sub">Each reading, message and event, as your server received it.</p>
				{@render list(records, 'Records', (s) => s.record_count)}
			</section>
		{/if}

		{#if files.length}
			<section class="group">
				<h2>Files</h2>
				<p class="group-sub">The recordings and attachments those records point to.</p>
				{@render list(files, 'Files', (s) => s.object_count)}
			</section>
		{/if}
	{/if}
</div>

<style>
	.streams {
		box-sizing: border-box;
		max-width: var(--page-medium);
		margin: 0 auto;
		padding: 32px 32px 96px;
	}

	.lead {
		margin: 0;
		font-family: var(--font-serif);
		font-size: 28px;
		font-weight: 400;
		line-height: 1.25;
		color: var(--color-foreground);
	}

	.sub {
		margin: 8px 0 0;
		max-width: 64ch;
		font-size: 14px;
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}

	.link {
		padding: 0;
		border: none;
		background: none;
		font: inherit;
		color: var(--color-primary);
		cursor: pointer;
	}

	.group {
		padding-top: 40px;
	}

	h2 {
		margin: 0 0 4px;
		font-family: var(--font-serif);
		font-size: var(--text-heading);
		font-weight: 400;
		line-height: 1.3;
	}

	.group-sub {
		margin: 0 0 16px;
		font-size: 13px;
		color: var(--color-foreground-muted);
	}

	.head,
	.row {
		display: grid;
		grid-template-columns: minmax(0, 1fr) 120px 88px 200px 15px;
		align-items: center;
		gap: 16px;
	}

	.head {
		padding-bottom: 8px;
		border-bottom: 1px solid var(--color-border);
		font-size: 13px;
		color: var(--color-foreground-muted);
	}

	.row {
		width: 100%;
		min-height: 56px;
		padding: 8px 0;
		border: none;
		border-bottom: 1px solid var(--color-border);
		background: none;
		font: inherit;
		text-align: left;
		color: inherit;
		cursor: pointer;
	}

	.num {
		text-align: right;
		font-variant-numeric: tabular-nums;
	}

	.name {
		display: block;
		font-size: 15px;
		color: var(--color-foreground);
	}

	.source {
		display: block;
		margin-top: 2px;
		font-size: 13px;
		color: var(--color-foreground-muted);
	}

	.count,
	.dates {
		font-size: 14px;
		color: var(--color-foreground-muted);
	}

	.size {
		font-size: 14px;
		color: var(--color-foreground);
	}

	.dates-inline {
		display: none;
	}

	.chevron {
		color: var(--color-foreground-muted);
		transition: color var(--motion-hover-out) var(--ease-premium);
	}

	.row:hover .chevron {
		color: var(--color-foreground);
		transition-duration: var(--motion-hover-in);
	}

	@media (max-width: 720px) {
		.streams {
			padding: 24px 16px 64px;
		}

		.head {
			display: none;
		}

		.row {
			grid-template-columns: minmax(0, 1fr) auto 15px;
		}

		.count,
		.dates {
			display: none;
		}

		.dates-inline {
			display: inline;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.chevron {
			transition: none;
		}
	}
</style>
