<script lang="ts">
	/**
	 * Changed lately, on Settings' landing page (`GET /api/settings/changes`):
	 * the settings you changed and the devices you paired in the last four
	 * weeks, as a line of days (ChangesChart) over the newest few. A change
	 * that can be put back has Undo; anything else opens where it lives.
	 *
	 * What needs you is not here: it is on the new-chat page and in the Mac
	 * app's notifications (`api/attention.rs`), and it is fixed on the page
	 * of the source or applet that failed.
	 */
	import { onMount } from 'svelte';
	import { toast } from 'svelte-sonner';
	import Button from '$lib/components/Button.svelte';
	import ChangesChart from './ChangesChart.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { openSetting } from '$lib/settings/focus';
	import { loadThemeFromDB } from '$lib/utils/theme';
	import {
		getSettingsChanges,
		undoSettingsChange,
		ApiError,
		type SettingsChange,
		type SettingsLately,
	} from '$lib/api/client';
	import { findSetting, titleOf } from '$lib/settings';
	import { changeRowId, describeChange, whenLabel } from '$lib/settings/changes';

	let { assistantName }: { assistantName: string } = $props();

	let lately = $state<SettingsLately | null>(null);
	let loadError = $state(false);
	let undoing = $state<string | null>(null);
	/** The change under the pointer in the list, ringed on the chart. */
	let hot = $state<string | null>(null);

	// The list opens on its newest few, so the page fits on one screen; the
	// rest are a click away, with no count on the button (someone in
	// recovery reads this page too).
	const FIRST = 4;
	let showAll = $state(false);

	const shown = $derived(lately ? (showAll ? lately.changes : lately.changes.slice(0, FIRST)) : []);

	async function load() {
		loadError = false;
		try {
			lately = await getSettingsChanges();
		} catch {
			loadError = true;
		}
	}

	onMount(load);

	function nameOf(change: SettingsChange): string {
		if (change.kind === 'paired') return change.label ?? 'A device';
		const setting = change.setting_id ? findSetting(change.setting_id) : undefined;
		return setting ? titleOf(setting.title, assistantName) : 'A setting';
	}

	function open(change: SettingsChange) {
		if (change.kind === 'paired') {
			windowShellStore.navigate('/virtues/devices', { label: 'Settings · Devices' });
			return;
		}
		if (change.setting_id) openSetting(change.setting_id, assistantName);
	}

	async function undo(change: SettingsChange) {
		const id = changeRowId(change);
		if (id === null) return;
		undoing = change.id;
		try {
			await undoSettingsChange(id);
			// The theme is applied in this window, so an undone theme is put on now.
			if (change.setting_id === 'theme') await loadThemeFromDB();
			await load();
		} catch (e) {
			toast.error(e instanceof ApiError ? e.message : "Your server couldn't undo that change. Try again.");
		} finally {
			undoing = null;
		}
	}
</script>

<section class="lately" aria-labelledby="changed-lately">
	<div class="head">
		<h2 id="changed-lately">Changed lately</h2>
		{#if lately && lately.changes.length > FIRST}
			<button type="button" class="link" onclick={() => (showAll = !showAll)}>{showAll ? 'Show fewer' : 'See all'}</button>
		{/if}
	</div>

	{#if loadError}
		<div class="failed">
			<span>Your server couldn't load what changed lately.</span>
			<Button variant="ghost" onclick={load}>Try again</Button>
		</div>
	{:else if lately}
		<ChangesChart changes={lately.changes} listed={shown.map((c) => c.id)} {hot} />
		{#if lately.changes.length}
			<ul>
				{#each shown as change (change.id)}
					<!-- Pointer or focus on a row rings its dot; the row's button is what acts. -->
					<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
					<li
						onmouseenter={() => (hot = change.id)}
						onmouseleave={() => (hot = null)}
						onfocusin={() => (hot = change.id)}
						onfocusout={() => (hot = null)}
					>
						<span class="when">{whenLabel(change.occurred_at)}</span>
						<span class="what">
							<span class="name">{nameOf(change)}</span>
							<span class="sub">{describeChange(change)}{change.actor === 'assistant' ? `, with ${assistantName}` : ''}</span>
						</span>
						{#if change.can_undo}
							<button type="button" class="link" disabled={undoing === change.id} onclick={() => undo(change)}>
								{undoing === change.id ? 'Undoing…' : 'Undo'}
							</button>
						{:else}
							<button type="button" class="link" onclick={() => open(change)}>Open</button>
						{/if}
					</li>
				{/each}
			</ul>
		{:else}
			<p class="empty">Nothing in Settings has changed in the last four weeks.</p>
		{/if}
	{/if}
</section>

<style>
	.lately {
		margin-top: 40px;
	}

	.head {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 16px;
	}

	h2 {
		margin: 0;
		font-family: var(--font-sans);
		font-size: 13px;
		font-weight: 400;
		letter-spacing: normal;
		color: var(--color-foreground-muted);
	}

	ul {
		list-style: none;
		margin: 0;
		padding: 0;
	}

	li {
		display: grid;
		grid-template-columns: 88px minmax(0, 1fr) auto;
		align-items: baseline;
		gap: 16px;
		padding: 4px 0;
	}

	.when {
		font-size: 12px;
		color: var(--color-foreground-subtle);
		white-space: nowrap;
	}

	.what {
		display: flex;
		align-items: baseline;
		gap: 8px;
		min-width: 0;
		white-space: nowrap;
	}

	.name {
		flex: none;
		font-size: 14px;
		color: var(--color-foreground);
	}

	.sub {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		font-size: 13px;
		color: var(--color-foreground-muted);
	}

	.link {
		padding: 0;
		border: none;
		background: none;
		font: inherit;
		font-size: 13px;
		color: var(--color-primary);
		cursor: pointer;
	}

	.link:hover:not(:disabled) {
		text-decoration: underline;
	}

	.link:disabled {
		color: var(--color-foreground-muted);
		cursor: default;
	}

	.empty {
		margin: 0;
		font-size: 13px;
		color: var(--color-foreground-muted);
	}

	.failed {
		display: flex;
		align-items: center;
		gap: 12px;
		margin-top: 8px;
		font-size: 13px;
		color: var(--color-foreground-muted);
	}
</style>
