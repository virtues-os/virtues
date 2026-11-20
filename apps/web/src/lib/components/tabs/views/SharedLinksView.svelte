<script lang="ts">
	/**
	 * Settings · Your data · Shared links (/virtues/data/shared): every link
	 * this server has shared, in one place.
	 *
	 * Working links are rows with copy and turn off; turned-off and expired
	 * ones fold underneath. A link's title opens the page, chat or applet it
	 * came from, where its Share modal updates it. The links themselves are
	 * served by the door (`api::publications`), and only while this server is
	 * on. A link that asks the server for data each time it opens says so in
	 * its line (`is_live`); one that doesn't says nothing, because a snapshot
	 * and a page that reads nothing look the same from here.
	 *
	 * "Opened" stays: whether anyone opened what you shared is a fact about
	 * where your data went, not a score.
	 */
	import { onMount } from 'svelte';
	import Icon from '$lib/components/Icon.svelte';
	import IconButton from '$lib/components/IconButton.svelte';
	import Modal from '$lib/components/Modal.svelte';
	import { Button } from '$lib';
	import SettingsPage from '$lib/components/settings/SettingsPage.svelte';
	import Fold from '$lib/components/settings/Fold.svelte';
	import LoadError from '$lib/components/settings/LoadError.svelte';
	import { listPublications, revokePublication, type Publication } from '$lib/api/client';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { homeRoute } from '$lib/applets/routes';

	let all = $state<Publication[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let copied = $state<string | null>(null);
	let revoking = $state<Publication | null>(null);
	let busy = $state(false);

	const isLive = (p: Publication) =>
		!p.revoked_at && (!p.expires_at || new Date(p.expires_at).getTime() > Date.now());
	const live = $derived(all.filter(isLive));
	const ended = $derived(all.filter((p) => !isLive(p)));

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

	const shortDate = (iso: string) =>
		new Date(iso).toLocaleDateString('en-US', { month: 'short', day: 'numeric', year: 'numeric' });

	function expires(p: Publication): string {
		return p.expires_at ? shortDate(p.expires_at) : 'Never';
	}

	/** "today", "yesterday", "3 days ago", "last week", or a date. */
	function when(iso: string): string {
		const day = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
		const days = Math.round((day(new Date()) - day(new Date(iso))) / 86_400_000);
		if (days <= 0) return 'today';
		if (days === 1) return 'yesterday';
		if (days < 7) return `${days} days ago`;
		if (days < 14) return 'last week';
		return `on ${shortDate(iso)}`;
	}

	function line(p: Publication): string {
		const what = p.producer_kind === 'applet' ? 'A site, shared' : p.producer_kind === 'chat' ? 'A chat, shared' : 'Shared';
		return `${what} ${when(p.created_at)}.${p.is_live ? ' Shows current data each time it opens.' : ''}`;
	}

	function endedLine(p: Publication): string {
		const how = p.revoked_at ? `Turned off ${when(p.revoked_at)}` : `Expired ${when(p.expires_at ?? p.created_at)}`;
		const seen = p.open_count === 0 ? 'Never opened' : `Opened ${opens(p).toLowerCase()}`;
		return `${how}. ${seen}.`;
	}

	function openSource(p: Publication) {
		const route =
			p.producer_kind === 'page'
				? `/page/${p.producer_id}`
				: p.producer_kind === 'chat'
					? `/chat/${p.producer_id}`
					: homeRoute(p.producer_id);
		windowShellStore.openTabFromRoute(route, { label: p.title });
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
</script>

<SettingsPage
	title="Shared links"
	lede="Every page, chat or site you've shared. Turn a link off and it stops opening at once."
	back={{ route: '/virtues/data', label: 'Your data' }}
>
	{#if error}
		<LoadError message={`Your server couldn't list your shared links. ${error}`} onretry={load} />
	{:else if loading}
		<p class="quiet">Loading your shared links…</p>
	{:else if all.length === 0}
		<div class="empty">
			<span class="empty-glyph"><Icon icon="mynaui:share" width="28" /></span>
			<p class="empty-title">Nothing shared yet</p>
			<p class="empty-body">Share a page, a chat or a site, and the link appears here.</p>
		</div>
	{:else}
		{#if live.length > 0}
			<div class="cols head" aria-hidden="true">
				<span>Page</span><span>Opened</span><span>Expires</span><span></span>
			</div>
			<ul class="links">
				{#each live as p (p.id)}
					<li class="cols link">
						<span class="what">
							<button type="button" class="title" onclick={() => openSource(p)}>{p.title}</button>
							<span class="line">{line(p)}</span>
						</span>
						<span class="dim"><span class="tag">Opened </span>{opens(p)}</span>
						<span class="dim"><span class="tag">Expires </span>{expires(p)}</span>
						<span class="actions">
							{#if p.link}
								<IconButton
									icon={copied === p.id ? 'mynaui:check' : 'mynaui:copy'}
									label={copied === p.id ? 'Copied' : `Copy the link to ${p.title}`}
									size="nested"
									onclick={() => copy(p)}
								/>
							{/if}
							<IconButton icon="mynaui:unlink" label={`Turn off the link to ${p.title}`} size="nested" onclick={() => (revoking = p)} />
						</span>
					</li>
				{/each}
			</ul>
		{:else}
			<p class="quiet">None of your links open right now.</p>
		{/if}

		{#if ended.length > 0}
			<div class="ended">
				<Fold label="Turned off or expired" sub="These no longer open. Sharing again makes a new link.">
					<ul class="ended-list">
						{#each ended as p (p.id)}
							<li>
								<button type="button" class="title" onclick={() => openSource(p)}>{p.title}</button>
								<span class="line">{endedLine(p)}</span>
							</li>
						{/each}
					</ul>
				</Fold>
			</div>
		{/if}
	{/if}
</SettingsPage>

<Modal open={revoking !== null} onClose={() => (revoking = null)} title="Turn off this link?" width="sm">
	{#if revoking}
		<p class="confirm">
			The link to <strong>{revoking.title}</strong> stops working for everyone who has it, and your server deletes its copy of
			the page. You can't turn the same link back on; sharing again makes a new one.
		</p>
		<div class="confirm-actions">
			<Button variant="secondary" onclick={() => (revoking = null)}>Keep it</Button>
			<Button variant="danger" disabled={busy} onclick={revoke}>Turn off the link</Button>
		</div>
	{/if}
</Modal>

<style>
	.cols {
		display: grid;
		grid-template-columns: minmax(0, 1fr) 112px 104px 64px;
		align-items: center;
		gap: 12px;
	}

	.head {
		margin-top: 32px;
		padding-bottom: 8px;
		border-bottom: 1px solid var(--color-border);
		font-size: 12px;
		color: var(--color-foreground-muted);
	}

	.links {
		list-style: none;
		margin: 0;
		padding: 0;
	}

	.link {
		min-height: 56px;
		padding: 8px 0;
	}

	.what {
		min-width: 0;
	}

	.title {
		display: block;
		max-width: 100%;
		padding: 0;
		border: none;
		background: none;
		font: inherit;
		font-size: 15px;
		text-align: left;
		color: var(--color-foreground);
		cursor: pointer;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.line {
		display: block;
		margin-top: 4px;
		font-size: 13px;
		line-height: 1.45;
		color: var(--color-foreground-muted);
	}

	.dim {
		font-size: 14px;
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
	}

	/* The column names, read before each value when the header is hidden. */
	.tag {
		display: none;
	}

	.actions {
		display: flex;
		justify-content: flex-end;
		gap: 4px;
	}

	.ended {
		margin-top: 24px;
	}

	.ended-list {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 12px;
	}

	.quiet {
		margin: 32px 0 0;
		font-size: 14px;
		color: var(--color-foreground-muted);
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

	@media (max-width: 560px) {
		.head {
			display: none;
		}

		.link {
			grid-template-columns: minmax(0, 1fr) auto;
			row-gap: 4px;
			padding: 12px 0;
		}

		.what {
			grid-column: 1;
		}

		.actions {
			grid-column: 2;
			grid-row: 1;
		}

		.dim {
			grid-column: 1 / -1;
			font-size: 13px;
		}

		.tag {
			display: inline;
		}
	}
</style>
