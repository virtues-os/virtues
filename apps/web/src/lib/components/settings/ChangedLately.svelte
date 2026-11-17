<script lang="ts">
	/**
	 * Changed lately, at the top of the Settings page: what needs you now,
	 * then the settings you changed and the devices you paired, newest first
	 * (`GET /api/settings/changes`). A change that can be put back has Undo;
	 * anything else opens where it lives.
	 *
	 * After an undo, `onundo` lets the page reload its rows so they show the
	 * value that came back.
	 */
	import { onMount } from 'svelte';
	import { toast } from 'svelte-sonner';
	import Button from '$lib/components/Button.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import {
		getSettingsChanges,
		undoSettingsChange,
		ApiError,
		type SettingsChange,
		type SettingsLately,
	} from '$lib/api/client';
	import { findSetting, titleOf } from '$lib/settings';
	import { changeRowId, describeChange, whenLabel } from '$lib/settings/changes';

	let { assistantName, onundo }: { assistantName: string; onundo?: () => void } = $props();

	let lately = $state<SettingsLately | null>(null);
	let loadError = $state(false);
	let undoing = $state<string | null>(null);

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
		const setting = change.setting_id ? findSetting(change.setting_id) : undefined;
		if (setting && setting.route !== '/virtues') {
			windowShellStore.navigate(setting.route, { label: `Settings · ${nameOf(change)}` });
		} else if (setting) {
			document.getElementById(`setting-${setting.id}`)?.scrollIntoView({ behavior: 'smooth', block: 'center' });
		}
	}

	async function undo(change: SettingsChange) {
		const id = changeRowId(change);
		if (id === null) return;
		undoing = change.id;
		try {
			await undoSettingsChange(id);
			await load();
			onundo?.();
		} catch (e) {
			toast.error(e instanceof ApiError ? e.message : "Your server couldn't undo that change. Try again.");
		} finally {
			undoing = null;
		}
	}

	const empty = $derived(!!lately && lately.needs_you.length === 0 && lately.changes.length === 0);
</script>

{#if loadError}
	<div class="lately">
		<p class="label">Changed lately</p>
		<div class="failed">
			<span>Your server couldn't load what changed lately.</span>
			<Button variant="ghost" size="sm" onclick={load}>Try again</Button>
		</div>
	</div>
{:else if lately && !empty}
	<section class="lately" aria-label="Changed lately">
		<p class="label">Changed lately</p>
		<ul>
			{#each lately.needs_you as item (item.key)}
				<li>
					<span class="what">
						<span class="name">{item.title}</span>
						<span class="sub">{item.body}</span>
					</span>
					<span class="when"></span>
					<button type="button" class="act" onclick={() => windowShellStore.navigate(item.route, { label: item.title })}>Open</button>
				</li>
			{/each}
			{#each lately.changes as change (change.id)}
				<li>
					<span class="what">
						<span class="name">{nameOf(change)}</span>
						<span class="sub">{describeChange(change)}</span>
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
	</section>
{/if}

<style>
	.lately {
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
		grid-template-columns: minmax(0, 1fr) 120px 72px;
		align-items: center;
		gap: 16px;
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

	.failed {
		display: flex;
		align-items: center;
		gap: 12px;
		font-size: 13px;
		color: var(--color-foreground-muted);
	}
</style>
