<script lang="ts">
	/**
	 * Two lists on Settings' landing page (`GET /api/settings/changes`):
	 * Needs you, what wants an action now, and Changed lately, the settings
	 * you changed and the devices you paired, newest first. A change that can
	 * be put back has Undo; anything else opens where it lives.
	 */
	import { onMount } from 'svelte';
	import { toast } from 'svelte-sonner';
	import Button from '$lib/components/Button.svelte';
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

	// Each list opens on its first few; the rest are a click away, with no
	// count on the button (someone in recovery reads this page too).
	const FIRST = 4;
	let allNeeds = $state(false);
	let allChanges = $state(false);

	/** A failed applet's line would quote its raw error; the applet's own
	 *  page shows that, so here it only says where to look. */
	function subOf(item: { kind: string; body: string }): string {
		return item.kind === 'applet' ? 'Open it to see what went wrong.' : item.body;
	}

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

{#if loadError}
	<div class="list">
		<p class="label">Changed lately</p>
		<div class="failed">
			<span>Your server couldn't load what changed lately.</span>
			<Button variant="ghost" onclick={load}>Try again</Button>
		</div>
	</div>
{:else if lately}
	{#if lately.needs_you.length}
		<section class="list" aria-label="Needs you">
			<p class="label">Needs you</p>
			<ul>
				{#each allNeeds ? lately.needs_you : lately.needs_you.slice(0, FIRST) as item (item.key)}
					<li>
						<span class="what">
							<span class="name">{item.title}</span>
							<span class="sub">{subOf(item)}</span>
						</span>
						<span class="when"></span>
						<button type="button" class="act" onclick={() => windowShellStore.navigate(item.route, { label: item.title })}>Open</button>
					</li>
				{/each}
			</ul>
			{#if lately.needs_you.length > FIRST}
				<button type="button" class="more" onclick={() => (allNeeds = !allNeeds)}>{allNeeds ? 'Show fewer' : 'Show all'}</button>
			{/if}
		</section>
	{/if}
	{#if lately.changes.length}
		<section class="list" aria-label="Changed lately">
			<p class="label">Changed lately</p>
			<ul>
				{#each allChanges ? lately.changes : lately.changes.slice(0, FIRST) as change (change.id)}
					<li>
						<span class="what">
							<span class="name">{nameOf(change)}</span>
							<span class="sub">{describeChange(change)}{change.actor === 'assistant' ? `, with ${assistantName}` : ''}</span>
						</span>
						<span class="when">{whenLabel(change.occurred_at)}</span>
						{#if change.can_undo}
							<button type="button" class="act" disabled={undoing === change.id} onclick={() => undo(change)}>
								{undoing === change.id ? 'Undoing…' : 'Undo'}
							</button>
						{:else}
							<button type="button" class="act" onclick={() => open(change)}>Open</button>
						{/if}
					</li>
				{/each}
			</ul>
			{#if lately.changes.length > FIRST}
				<button type="button" class="more" onclick={() => (allChanges = !allChanges)}>{allChanges ? 'Show fewer' : 'Show all'}</button>
			{/if}
		</section>
	{/if}
{/if}

<style>
	.list {
		margin-top: 32px;
	}

	.label {
		margin: 0 0 8px;
		font-size: 12px;
		color: var(--color-foreground-muted);
	}

	ul {
		list-style: none;
		margin: 0;
		padding: 0;
		border-top: 1px solid var(--color-border-subtle);
	}

	li {
		display: grid;
		grid-template-columns: minmax(0, 1fr) auto 64px;
		align-items: center;
		gap: 24px;
		min-height: 52px;
		padding: 8px 0;
		border-bottom: 1px solid var(--color-border-subtle);
		box-sizing: border-box;
	}

	.what {
		min-width: 0;
	}

	.name {
		display: block;
		font-size: 14px;
		color: var(--color-foreground);
	}

	.sub {
		display: block;
		font-size: 13px;
		color: var(--color-foreground-muted);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.when {
		font-size: 13px;
		text-align: right;
		white-space: nowrap;
		color: var(--color-foreground-muted);
	}

	.act {
		justify-self: end;
		border: none;
		background: none;
		padding: 4px 0;
		font: inherit;
		font-size: 13px;
		color: var(--color-primary);
		cursor: pointer;
	}

	.act:disabled {
		color: var(--color-foreground-muted);
		cursor: default;
	}

	.more {
		margin-top: 8px;
		padding: 4px 0;
		border: none;
		background: none;
		font: inherit;
		font-size: 13px;
		color: var(--color-primary);
		cursor: pointer;
	}

	.failed {
		display: flex;
		align-items: center;
		gap: 12px;
		font-size: 13px;
		color: var(--color-foreground-muted);
	}
</style>
