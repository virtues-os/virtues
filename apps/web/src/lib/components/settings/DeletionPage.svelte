<script lang="ts">
	/**
	 * One deletion (/virtues/data/deleted/:id): what it removed, what still
	 * has a copy, and the one thing you can undo, letting its records be
	 * collected again (the tombstones go; a source that still has the
	 * records sends them on its next sync).
	 *
	 * A deletion that stopped part way kept what it had already deleted, so
	 * its tombstones can be lifted here too, and Try again runs it again
	 * when your server can (`preview_error` says when it can't). While one
	 * is running the page checks again every 2 seconds.
	 *
	 * Everything here comes from the deletion's receipt, which holds counts
	 * and dates, never the records.
	 */
	import { onMount } from 'svelte';
	import SettingsPage from './SettingsPage.svelte';
	import LoadError from './LoadError.svelte';
	import ServerSaid from './ServerSaid.svelte';
	import Fold from './Fold.svelte';
	import Button from '$lib/components/Button.svelte';
	import { allowDeletion, getDeletion, liftTombstones, type Deletion } from '$lib/api/deletions';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { alsoRemoved, dayInWords, deletionLead, deletionLede, deletionTitle, stopped } from '$lib/settings/deletions';

	let { id }: { id: string } = $props();

	let deletion = $state<Deletion | null>(null);
	let failed = $state<string | null>(null);
	let working = $state(false);
	let actionError = $state<string | null>(null);

	async function load() {
		failed = null;
		try {
			deletion = await getDeletion(id);
		} catch (e) {
			failed = `Your server couldn’t find this deletion: ${e instanceof Error ? e.message : String(e)}`;
		}
	}

	onMount(load);

	$effect(() => {
		if (deletion?.status !== 'running') return;
		const timer = setInterval(() => void load(), 2000);
		return () => clearInterval(timer);
	});

	async function act(run: () => Promise<unknown>, what: string) {
		if (working) return;
		working = true;
		actionError = null;
		try {
			await run();
			await load();
		} catch (e) {
			actionError = `Your server didn’t ${what}: ${e instanceof Error ? e.message : String(e)}`;
		} finally {
			working = false;
		}
	}

	const receipt = $derived(deletion?.receipt ?? {});
	const records = $derived(receipt.records ?? 0);
	const lifted = $derived(receipt.tombstones_lifted !== undefined);
	const removed = $derived(alsoRemoved(receipt));
	const ranDay = $derived(
		deletion ? new Date(deletion.approved_at ?? deletion.created_at).toLocaleDateString('en-US', { month: 'long', day: 'numeric', year: 'numeric' }) : '',
	);
	// Lifting only means something once a run has written tombstones.
	const canLift = $derived(!!deletion && !lifted && ((deletion.status === 'done' && records > 0) || stopped(deletion)));

	function openChat() {
		if (!deletion?.chat_id) return;
		void windowShellStore.openTabFromRoute(`/chat/${deletion.chat_id}`, { label: 'Chat', focusExisting: true });
	}
</script>

{#snippet fact(label: string, value: string)}
	<div class="fact">
		<span class="fact-label">{label}</span>
		<span class="fact-value">{value}</span>
	</div>
{/snippet}

<SettingsPage
	title={deletion ? deletionTitle(deletion) : 'A deletion'}
	lede={deletion ? deletionLede(deletion) : undefined}
	back={{ route: '/virtues/data/deleted', label: 'Deleted records' }}
>
	{#if failed}
		<LoadError message={failed} onretry={load} />
	{:else if !deletion}
		<p class="settings-sub">Asking your server about this deletion…</p>
	{:else}
		<p class="settings-lead">{deletionLead(deletion)}</p>
		{#if deletion.status === 'running'}
			<p class="settings-sub">You can leave this page. It shows what went when your server finishes.</p>
		{:else if lifted}
			<p class="settings-sub">
				{receipt.lifted_at ? `On ${dayInWords(receipt.lifted_at)} you` : 'You'} let them be collected again. A source that still has
				them brings them back on its next sync, and your server keeps them like any other record.
			</p>
		{:else if stopped(deletion)}
			<p class="settings-sub">What it deleted before it stopped stays deleted, and no sync brings it back.</p>
		{:else if records > 0}
			<p class="settings-sub">No sync brings them back. If a source still has them and sends them again, your server refuses them.</p>
		{/if}

		{#if deletion.status === 'failed'}
			{#if deletion.error}<ServerSaid text={deletion.error} />{/if}
			{#if !deletion.preview_error}
				<div class="actions">
					<Button variant="secondary" loading={working} onclick={() => act(() => allowDeletion(id), 'start it again')}>Try again</Button>
				</div>
			{/if}
		{/if}
		{#if deletion.chat_id}
			<div class="actions"><Button variant="secondary" onclick={openChat}>Open the chat</Button></div>
		{/if}
		{#if actionError}<p class="action-error" role="alert">{actionError}</p>{/if}

		{#if records > 0}
			<section class="group">
				<h2 class="settings-label">What went</h2>
				{#each receipt.tables ?? [] as t (t.table)}
					{@render fact(t.label, t.records.toLocaleString())}
				{/each}
				{#if removed}{@render fact('Also removed', removed)}{/if}
			</section>
		{/if}

		{#if deletion.status === 'done' && records > 0 && receipt.kept?.length}
			<section class="group">
				<h2 class="settings-label">What still has a copy</h2>
				{#if receipt.kept.includes('backups')}
					{@render fact('Backups', `Any made before ${ranDay}. Restoring one brings these back.`)}
				{/if}
				{#if receipt.kept.includes('pages_and_chats')}
					{@render fact('Pages and chats', 'Any that already quoted these records')}
				{/if}
			</section>
		{/if}

		{#if canLift}
			<div class="fold">
				<Fold label="Let them be collected again" sub="If a source still has these records, it brings them back on its next sync.">
					<Button variant="secondary" loading={working} onclick={() => act(() => liftTombstones(id), 'let them back')}>
						Let them be collected again
					</Button>
				</Fold>
			</div>
		{/if}
	{/if}
</SettingsPage>

<style>
	.actions {
		margin-top: 16px;
	}

	.action-error {
		margin: 12px 0 0;
		font-size: 13px;
		color: var(--color-error);
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
		line-height: 1.45;
	}

	.fact-label {
		color: var(--color-foreground);
	}

	.fact-value {
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
	}

	.fold {
		margin-top: 16px;
	}

	@media (max-width: 560px) {
		.fact {
			grid-template-columns: minmax(0, 1fr);
			gap: 4px;
		}
	}
</style>
