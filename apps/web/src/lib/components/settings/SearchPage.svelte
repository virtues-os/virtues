<script lang="ts">
	/**
	 * Settings · Search (/virtues/search): whether search is answering, how
	 * much of your record it covers, and where it runs.
	 *
	 * It opens with one sentence (`searchLead` in lib/settings/searchStatus.ts).
	 * The page changes nothing itself. Moving search to another machine
	 * re-checks the model and can rebuild the index, so it is one command on
	 * the server (`virtues configure-inference --embed-url`, or
	 * `--recommended` to come back), which a fold names.
	 *
	 * When search runs on a host outside your network, the Runs on row says
	 * that host receives the text of everything your server indexes.
	 */
	import { onMount } from 'svelte';
	import SettingsPage from './SettingsPage.svelte';
	import Fold from './Fold.svelte';
	import LoadError from './LoadError.svelte';
	import ServerSaid from './ServerSaid.svelte';
	import Command from './Command.svelte';
	import Button from '$lib/components/Button.svelte';
	import { getSearchStatus, type SearchStatus } from '$lib/api/client';
	import { openExternal } from '$lib/tauri/bridge';
	import { changePercent, hostOf, modelName, runsOn, searchLead } from '$lib/settings/searchStatus';

	const GUIDE = 'https://virtues.com/docs/setup/accelerators';
	const EMBED_URL = 'virtues configure-inference --embed-url http://127.0.0.1:8080';
	const RECOMMENDED = 'sudo virtues configure-inference --recommended';

	let status = $state<SearchStatus | null>(null);
	let failed = $state(false);
	let checking = $state(false);

	async function load() {
		failed = false;
		try {
			status = await getSearchStatus();
		} catch {
			failed = true;
		}
	}

	async function recheck() {
		checking = true;
		await load();
		checking = false;
	}

	onMount(load);

	const lead = $derived(status ? searchLead(status) : null);
</script>

{#snippet fact(label: string, value: string, note?: string)}
	<div class="fact">
		<span class="fact-label">{label}</span>
		<span class="fact-value">
			{value}
			{#if note}<span class="note">{note}</span>{/if}
		</span>
	</div>
{/snippet}

<SettingsPage title="Search" lede="Where search runs, and how much of your record it covers." back={{ route: '/virtues/system', label: 'Your server' }}>
	{#if failed}
		<LoadError message="Your server couldn’t send the search status." onretry={load} />
	{:else if !status || !lead}
		<p class="settings-sub">Asking your server about search…</p>
	{:else}
		{@const host = hostOf(status.embed_url)}
		<p class="settings-lead">{lead.sentence}</p>
		<p class="settings-sub">{lead.sub}</p>
		{#if !status.reachable && !status.busy && status.probe_error}
			<ServerSaid text={status.probe_error} />
		{/if}
		{#if !status.reachable}
			<div class="recheck"><Button variant="secondary" loading={checking} onclick={recheck}>Check again</Button></div>
		{/if}

		<section class="group">
			<h2 class="settings-label">Where it runs</h2>
			{@render fact(
				'Runs on',
				runsOn(status),
				status.embed_locality === 'public'
					? 'Your server sends this host the text of everything it indexes for search. Whoever runs it can read that text.'
					: status.embed_locality === 'unknown'
						? `Your server couldn’t look up ${host}, so it can’t tell whether this host is on your network.`
						: status.accelerator && status.mode === 'bundled' && !status.model_change
							? `This server also has a ${status.accelerator}. Search is much faster on it.`
							: undefined,
			)}
			{@render fact(
				'Model',
				status.index_model ? modelName(status.index_model) : 'Nothing indexed yet',
				status.model_change
					? `Moving to ${modelName(status.model_change.model)}: ${changePercent(status)}% of the index is rebuilt.`
					: status.mode === 'manual'
						? 'A model you chose. Updates leave it alone.'
						: undefined,
			)}
			{#if status.reachable && status.probe_ms != null}
				{@render fact('Answers in', `About ${status.probe_ms} ms`)}
			{/if}
		</section>

		<div class="folds">
			{#if status.mode === 'manual'}
				<Fold label="Go back to the recommended setup" sub="Virtues runs its own model on this server’s processor and keeps it current.">
					<p class="fold-text">Search keeps using {host} until the new index is built. Run this on your server:</p>
					<Command command={RECOMMENDED} />
				</Fold>
			{:else}
				<Fold label="Run search on a graphics chip or another machine" sub="It’s faster on a GPU or an NPU. Moving it takes one command on your server.">
					<p class="fold-text">
						Start an embedding server there, then run this on your server. Your index stays if the new server runs the same model.
					</p>
					<Command command={EMBED_URL} />
					<div class="fold-action">
						<Button variant="secondary" onclick={() => openExternal(status?.accelerator_guide ?? GUIDE)}>Open the setup guide</Button>
					</div>
				</Fold>
			{/if}
		</div>

		<p class="settings-aside">
			The manual’s <a href={GUIDE} target="_blank" rel="noopener noreferrer">Accelerators</a> page explains which chips work and how
			to set one up.
		</p>
	{/if}
</SettingsPage>

<style>
	.recheck {
		margin-top: 16px;
	}

	.group {
		padding: 24px 0 8px;
	}

	.fact {
		display: grid;
		grid-template-columns: 180px minmax(0, 1fr);
		gap: 16px;
		padding: 8px 0;
		font-size: 15px;
		line-height: 1.4;
	}

	.fact-label {
		color: var(--color-foreground);
	}

	.fact-value {
		color: var(--color-foreground-muted);
		overflow-wrap: anywhere;
		font-variant-numeric: tabular-nums;
	}

	.note {
		display: block;
		margin-top: 4px;
		max-width: 60ch;
		font-size: 13px;
		line-height: 1.45;
		color: var(--color-foreground-muted);
	}

	.folds {
		margin-top: 16px;
	}

	.fold-text {
		margin: 0 0 8px;
		max-width: 60ch;
		font-size: 13px;
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}

	.fold-action {
		margin-top: 12px;
	}

	@media (max-width: 560px) {
		.fact {
			grid-template-columns: minmax(0, 1fr);
			gap: 4px;
		}
	}
</style>
