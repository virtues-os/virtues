<script lang="ts">
	/**
	 * Settings · Deleted records (/virtues/data/deleted): every deletion of
	 * ingested records that ran, newest first, and how to make another.
	 *
	 * A deletion removes records at once and tombstones them, so no sync
	 * brings them back (api::deletions on the server). Each row opens the
	 * deletion's own page, which says what it reached, can let its records
	 * be collected again, and offers Try again for one that stopped. While
	 * one is running the list checks again every 2 seconds. Chats, pages and
	 * files are not records: they wait 30 days in Recently deleted instead.
	 */
	import { onMount } from 'svelte';
	import SettingsPage from './SettingsPage.svelte';
	import LoadError from './LoadError.svelte';
	import { getAssistantProfile } from '$lib/api/client';
	import { listDeletions, type Deletion } from '$lib/api/deletions';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { DEFAULT_ASSISTANT_NAME } from '$lib/settings';
	import { deletionLine, deletionTitle, stopped } from '$lib/settings/deletions';

	let deletions = $state<Deletion[] | null>(null);
	let failed = $state<string | null>(null);
	let name = $state(DEFAULT_ASSISTANT_NAME);

	async function load() {
		failed = null;
		try {
			deletions = await listDeletions();
		} catch (e) {
			failed = `Your server couldn’t list your deletions: ${e instanceof Error ? e.message : String(e)}`;
		}
	}

	onMount(() => {
		void load();
		getAssistantProfile<{ assistant_name?: string | null }>()
			.then((p) => {
				if (p.assistant_name) name = p.assistant_name.charAt(0).toUpperCase() + p.assistant_name.slice(1);
			})
			// absent-ok: the how-to line names the default assistant instead.
			.catch(() => {});
	});

	const now = Date.now();

	function open(d: Deletion) {
		windowShellStore.navigate(`/virtues/data/deleted/${d.id}`, { label: 'Settings · Deleted records' });
	}

	// A running deletion finishes on its own; check until none is.
	$effect(() => {
		if (!deletions?.some((d) => d.status === 'running')) return;
		const timer = setInterval(() => void load(), 2000);
		return () => clearInterval(timer);
	});

	const openQuery = () => void windowShellStore.openTabFromRoute('/sources/query', { label: 'Sources · Query', focusExisting: true });
	const openTrash = () => windowShellStore.navigate('/storage/trash', { label: 'Recently deleted' });
</script>

{#snippet howTo()}
	To delete a record such as a purchase or a visit, ask {name}, or type <code>/delete-data</code> in a chat. You can also delete rows in
	<button type="button" class="settings-link" onclick={openQuery}>Query</button>. Your server deletes a record at once, and no sync
	brings it back.
{/snippet}

<SettingsPage
	title="Deleted records"
	lede="Records you deleted for good, and what each deletion reached."
	back={{ route: '/virtues/data', label: 'Your data' }}
>
	{#if failed}
		<LoadError message={failed} onretry={load} />
	{:else if !deletions}
		<p class="settings-sub">Asking your server for your deletions…</p>
	{:else if deletions.length === 0}
		<p class="settings-lead">You haven’t deleted any records.</p>
		<p class="settings-sub">{@render howTo()}</p>
	{:else}
		<p class="settings-sub how">{@render howTo()}</p>
		<div class="list" role="list">
			{#each deletions as d (d.id)}
				<div role="listitem">
					<button type="button" class="open" onclick={() => open(d)}>
						<span class="words">
							<span class="name">{deletionTitle(d)}</span>
							<span class="line" class:warn={stopped(d)}>{deletionLine(d, now)}</span>
						</span>
						<svg class="chevron" viewBox="0 0 24 24" aria-hidden="true" width="15" height="15">
							<path fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.8" d="m9 18l6-6l-6-6" />
						</svg>
					</button>
				</div>
			{/each}
		</div>
	{/if}

	{#if deletions}
		<p class="settings-aside">
			Chats, pages, threads and files you delete wait in
			<button type="button" class="settings-link" onclick={openTrash}>Recently deleted</button> for 30 days instead.
		</p>
	{/if}
</SettingsPage>

<style>
	.how {
		margin-top: 24px;
	}

	code {
		/* design-ok: a command you type, as you type it. */
		font-family: var(--font-mono);
		font-size: 0.92em;
		white-space: nowrap;
	}

	.list {
		margin-top: 16px;
	}

	.open {
		display: grid;
		width: 100%;
		grid-template-columns: minmax(0, 1fr) auto;
		align-items: center;
		gap: 16px;
		min-height: 60px;
		padding: 8px 0;
		border: none;
		background: none;
		font: inherit;
		text-align: left;
		color: inherit;
		cursor: pointer;
	}

	.name {
		display: block;
		font-size: 15px;
		color: var(--color-foreground);
	}

	.line {
		display: block;
		margin-top: 4px;
		font-size: 13px;
		line-height: 1.45;
		color: var(--color-foreground-muted);
	}

	.line.warn {
		color: var(--color-warning);
	}

	.chevron {
		color: var(--color-foreground-muted);
		transition: color var(--motion-hover-out) var(--ease-premium);
	}

	.open:hover .chevron {
		color: var(--color-foreground);
		transition-duration: var(--motion-hover-in);
	}

	@media (prefers-reduced-motion: reduce) {
		.chevron {
			transition: none;
		}
	}
</style>
