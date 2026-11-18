<script lang="ts">
	/**
	 * "Where it came from" on a task's page: the record the task was made
	 * from, in its own words (a voicemail's transcript, a message as it was
	 * sent), with when it happened. The card opens the record beside the
	 * task. A source that is not a record shows as its name.
	 */
	import { getRecord, type OntologyRecord } from '$lib/api/client';
	import Icon from '$lib/components/Icon.svelte';
	import RecordCardBody, { KIND_LABEL, recordKind } from '$lib/components/RecordCardBody.svelte';
	import { identityOf } from '$lib/refs/identity.svelte';
	import { parseRecordRef } from '$lib/utils/refRoutes';
	import { formatRelativeTimestamp } from '$lib/utils/dateUtils';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';

	let { url }: { url: string } = $props();

	const ref = $derived(parseRecordRef(url));
	const identity = $derived(identityOf(url));
	const kind = $derived(ref ? recordKind(ref.ontology) : 'other');

	let record = $state<OntologyRecord | null>(null);
	let failed = $state(false);

	$effect(() => {
		const r = ref;
		record = null;
		failed = false;
		if (!r) return;
		getRecord(r.ontology, r.id)
			.then((got) => (record = got))
			.catch(() => (failed = true));
	});

	const row = $derived((record?.row ?? {}) as Record<string, unknown>);

	/** "Yesterday 4:12 PM · 0:38": when it happened, and how long a recording runs. */
	const meta = $derived.by(() => {
		const parts: string[] = [];
		const at = record?.timestamp_column ? row[record.timestamp_column] : null;
		if (typeof at === 'string') {
			const day = formatRelativeTimestamp(at);
			const time = new Date(at).toLocaleTimeString('en-US', { hour: 'numeric', minute: '2-digit' });
			parts.push(day.includes(':') ? day : `${day} ${time}`);
		}
		const seconds = typeof row.duration_seconds === 'number' ? Math.round(row.duration_seconds) : null;
		if (seconds !== null) parts.push(`${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`);
		return parts.join(' · ');
	});

	const label = $derived(KIND_LABEL[kind] ?? record?.display_name ?? 'Source');
</script>

{#if ref}
	<button type="button" class="src" onclick={() => windowShellStore.openRouteBeside(url)}>
		<span class="kicker">
			<Icon icon={identity?.icon || 'mynaui:file-text'} width="14" />
			<span>{label}</span>
			{#if meta}<span class="meta">{meta}</span>{/if}
		</span>
		{#if failed}
			<span class="quiet">Your server couldn't load this record. It may have been removed.</span>
		{:else if !record}
			<span class="quiet">Loading…</span>
		{:else}
			<RecordCardBody {kind} {record} title={identity?.title} />
		{/if}
	</button>
{:else}
	<button type="button" class="link" onclick={() => windowShellStore.openRouteBeside(url)}>
		<Icon icon={identity?.icon || 'mynaui:link'} width="16" />
		<span>{identity?.title ?? url}</span>
	</button>
{/if}

<style>
	.src {
		display: flex;
		flex-direction: column;
		gap: 8px;
		width: 100%;
		padding: 16px;
		border: 0;
		border-radius: var(--radius-md);
		background: var(--hover-bg);
		font: inherit;
		text-align: left;
		color: var(--color-foreground);
		cursor: pointer;
		transition: background-color var(--motion-hover-out) var(--ease-premium);
	}
	.src:hover {
		background: var(--active-bg);
		transition-duration: var(--motion-hover-in);
	}
	.kicker {
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: 13px;
		line-height: 20px;
		color: var(--color-foreground-muted);
	}
	.meta {
		margin-left: auto;
		font-variant-numeric: tabular-nums;
	}
	.quiet {
		font-size: 14px;
		color: var(--color-foreground-muted);
	}
	.link {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		padding: 8px 12px;
		margin: 0 -12px;
		border: 0;
		border-radius: var(--radius-md);
		background: transparent;
		font: inherit;
		font-size: 15px;
		color: var(--color-foreground);
		cursor: pointer;
		transition: background-color var(--motion-hover-out) var(--ease-premium);
	}
	.link:hover {
		background: var(--hover-bg);
		transition-duration: var(--motion-hover-in);
	}
	.link :global(svg),
	.kicker :global(svg) {
		color: var(--color-foreground-muted);
	}
	@media (prefers-reduced-motion: reduce) {
		.src,
		.link {
			transition: none;
		}
	}
</style>
