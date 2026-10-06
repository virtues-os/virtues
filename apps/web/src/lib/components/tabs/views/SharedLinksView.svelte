<script lang="ts">
	/**
	 * Settings → Shared links: every link this server has shared, in one place.
	 *
	 * Live links sit in the grid with copy and revoke; revoked and expired ones
	 * fold underneath, as archived projects do. A link opens the applet it came
	 * from, where its Share sheet updates it. The links themselves are served by
	 * the door (`api::publications`), and only while this server is on.
	 */
	import { onMount } from 'svelte';
	import Icon from '$lib/components/Icon.svelte';
	import IconButton from '$lib/components/IconButton.svelte';
	import Modal from '$lib/components/Modal.svelte';
	import { Button, Page } from '$lib';
	import UniversalDataGrid, { type Column } from '$lib/components/datagrid/UniversalDataGrid.svelte';
	import { listPublications, revokePublication, type Publication } from '$lib/api/client';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { formatRelativeTimestamp } from '$lib/utils/dateUtils';

	let all = $state<Publication[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let copied = $state<string | null>(null);
	let revoking = $state<Publication | null>(null);
	let busy = $state(false);
	let endedOpen = $state(false);

	const isLive = (p: Publication) =>
		!p.revoked_at && (!p.expires_at || new Date(p.expires_at).getTime() > Date.now());
	const live = $derived(all.filter(isLive));
	const ended = $derived(all.filter((p) => !isLive(p)));

	const description = $derived(
		live.length === 1 ? '1 link working' : `${live.length} links working`
	);

	async function load() {
		error = null;
		try {
			all = await listPublications();
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		} finally {
			loading = false;
		}
	}

	onMount(load);

	function opens(p: Publication): string {
		if (p.open_count === 0) return 'Not opened yet';
		return p.open_count === 1 ? 'Once' : `${p.open_count} times`;
	}

	function expires(p: Publication): string {
		if (!p.expires_at) return 'Never';
		return new Date(p.expires_at).toLocaleDateString(undefined, {
			month: 'short',
			day: 'numeric',
			year: 'numeric'
		});
	}

	// A link records only whether it asks the server for data; a snapshot and
	// a page that reads nothing look the same from here, so neither is named.
	function kind(p: Publication): string {
		return p.is_live ? 'Live' : '—';
	}

	function openApplet(p: Publication) {
		windowShellStore.navigate(`/applet/${p.producer_id}`, { label: p.title });
	}

	async function copy(p: Publication) {
		if (!p.link) return;
		try {
			await navigator.clipboard.writeText(p.link);
			copied = p.id;
		} catch {
			copied = null;
		}
	}

	async function revoke() {
		if (!revoking) return;
		busy = true;
		try {
			await revokePublication(revoking.id);
			revoking = null;
			await load();
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		} finally {
			busy = false;
		}
	}

	const columns: Column<Publication>[] = [
		{ key: 'title', label: 'Page', icon: 'ri:pages-line', width: '34%', minWidth: '180px' },
		{ key: 'is_live', label: 'Data', icon: 'ri:pulse-line', width: '14%', minWidth: '90px', hideOnMobile: true, getValue: kind },
		{ key: 'open_count', label: 'Opened', icon: 'ri:eye-line', width: '16%', minWidth: '100px', getValue: opens },
		{ key: 'expires_at', label: 'Expires', icon: 'ri:timer-line', width: '16%', minWidth: '110px', hideOnMobile: true, getValue: expires },
		{
			key: 'created_at',
			label: 'Shared',
			icon: 'ri:time-line',
			width: '20%',
			minWidth: '110px',
			hideOnMobile: true,
			getValue: (p) => formatRelativeTimestamp(p.created_at)
		}
	];
</script>

<Page title="Shared links" {description} maxWidth="wide">
	{#if !loading && all.length === 0 && !error}
		<div class="empty">
			<span class="empty-glyph"><Icon icon="ri:share-forward-line" width="28" /></span>
			<p class="empty-title">Nothing shared yet</p>
			<p class="empty-body">
				Open an applet that shows a page and choose Share. Every link you make appears here,
				and you can turn any of them off.
			</p>
		</div>
	{:else}
		<UniversalDataGrid
			items={live}
			{columns}
			entityType="publication"
			{loading}
			{error}
			emptyIcon="ri:share-forward-line"
			emptyMessage={live.length === 0 ? 'No links are working right now' : 'No links match'}
			loadingMessage="Loading shared links…"
			searchPlaceholder="Search shared links…"
			defaultViewMode="table"
			onItemClick={openApplet}
			onRetry={load}
		>
			{#snippet tableRow(p: Publication)}
				<td class="col-name"><span class="name-text">{p.title}</span></td>
				<td class="col-dim hide-mobile">{kind(p)}</td>
				<td class="col-dim">{opens(p)}</td>
				<td class="col-dim hide-mobile">{expires(p)}</td>
				<td class="col-dim hide-mobile">{formatRelativeTimestamp(p.created_at)}</td>
			{/snippet}

			{#snippet rowActions(p: Publication)}
				<span class="actions">
					{#if p.link}
						<IconButton
							icon={copied === p.id ? 'ri:check-line' : 'ri:file-copy-line'}
							label={copied === p.id ? 'Copied' : `Copy the link to ${p.title}`}
							size="sm"
							onclick={(e) => {
								e.stopPropagation();
								copy(p);
							}}
						/>
					{/if}
					<IconButton
						icon="ri:link-unlink"
						label={`Turn off the link to ${p.title}`}
						size="sm"
						onclick={(e) => {
							e.stopPropagation();
							revoking = p;
						}}
					/>
				</span>
			{/snippet}
		</UniversalDataGrid>
	{/if}

	{#if ended.length > 0}
		<section class="ended">
			<button
				type="button"
				class="ended-head"
				aria-expanded={endedOpen}
				onclick={() => (endedOpen = !endedOpen)}
			>
				<Icon icon={endedOpen ? 'ri:arrow-down-s-line' : 'ri:arrow-right-s-line'} width="14" />
				<span>Turned off or expired</span>
				<span class="ended-count">{ended.length}</span>
			</button>
			{#if endedOpen}
				<ul class="ended-list">
					{#each ended as p (p.id)}
						<li class="ended-row">
							<button type="button" class="ended-name" onclick={() => openApplet(p)}>{p.title}</button>
							<span class="ended-when">
								{p.revoked_at
									? `Turned off ${formatRelativeTimestamp(p.revoked_at)}`
									: `Expired ${formatRelativeTimestamp(p.expires_at ?? p.created_at)}`}
							</span>
							<span class="ended-when">{opens(p)}</span>
						</li>
					{/each}
				</ul>
			{/if}
		</section>
	{/if}
</Page>

<Modal open={revoking !== null} onClose={() => (revoking = null)} title="Turn off this link?" width="sm">
	{#if revoking}
		<p class="confirm">
			The link to <strong>{revoking.title}</strong> stops working for everyone who has it, and your
			server deletes its copy of the page. You can't turn the same link back on; sharing again makes
			a new one.
		</p>
		<div class="confirm-actions">
			<Button variant="ghost" onclick={() => (revoking = null)}>Keep it</Button>
			<Button variant="danger" disabled={busy} onclick={revoke}>Turn off the link</Button>
		</div>
	{/if}
</Modal>

<style>
	.col-name,
	.col-dim {
		padding: 0.625rem 0.75rem;
	}
	.name-text {
		font-weight: 500;
		color: var(--color-foreground);
	}
	.col-dim {
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
	}
	.actions {
		display: inline-flex;
		gap: 0.25rem;
	}
	@media (max-width: 768px) {
		.hide-mobile {
			display: none;
		}
	}
	.empty {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.5rem;
		padding: 3rem 1rem;
		text-align: center;
	}
	.empty-glyph {
		color: var(--color-foreground-muted);
	}
	.empty-title {
		margin: 0;
		font-weight: 600;
		color: var(--color-foreground);
	}
	.empty-body {
		margin: 0;
		max-width: 40ch;
		font-size: 0.875rem;
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}
	.ended {
		margin-top: 1.5rem;
	}
	.ended-head {
		display: inline-flex;
		align-items: center;
		gap: 0.375rem;
		padding: 0.25rem 0;
		border: 0;
		background: none;
		font: inherit;
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
		cursor: pointer;
	}
	.ended-count {
		font-variant-numeric: tabular-nums;
	}
	.ended-list {
		list-style: none;
		margin: 0.5rem 0 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
	}
	.ended-row {
		display: flex;
		align-items: center;
		gap: 1rem;
		padding: 0.375rem 0;
		font-size: 0.8125rem;
	}
	.ended-name {
		border: 0;
		background: none;
		padding: 0;
		font: inherit;
		color: var(--color-foreground);
		cursor: pointer;
		text-align: left;
	}
	.ended-when {
		color: var(--color-foreground-muted);
	}
	.confirm {
		margin: 0 0 1rem;
		font-size: 0.875rem;
		line-height: 1.5;
	}
	.confirm-actions {
		display: flex;
		justify-content: flex-end;
		gap: 0.5rem;
	}
</style>
