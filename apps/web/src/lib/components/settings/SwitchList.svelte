<script lang="ts" generics="Id extends string">
	/**
	 * A setting made of several switches under one name: What the assistant
	 * can do, What it may read. Loads them, saves one at a time, and shows
	 * what the server answers with, so a failed save puts the switch back.
	 */
	import { onMount } from 'svelte';
	import SettingRow from './SettingRow.svelte';
	import Switch from './Switch.svelte';
	import { ApiError } from '$lib/api/client';

	let {
		id,
		label,
		help,
		words,
		load: fetchAll,
		save,
	}: {
		/** The setting's id in lib/settings. */
		id: string;
		label: string;
		help: string;
		/** Each switch's name and one line of help, by its id. */
		words: Record<Id, { label: string; help?: string }>;
		load: () => Promise<{ id: Id; on: boolean }[]>;
		/** Saves one switch; resolves to every switch as stored. */
		save: (id: Id, on: boolean) => Promise<{ id: Id; on: boolean }[]>;
	} = $props();

	let items = $state<{ id: Id; on: boolean }[] | null>(null);
	let loadError = $state(false);
	let saving = $state<Id | null>(null);
	let error = $state<string | null>(null);

	async function load() {
		loadError = false;
		try {
			items = await fetchAll();
		} catch {
			loadError = true;
		}
	}

	onMount(load);

	async function toggle(which: Id, on: boolean) {
		saving = which;
		error = null;
		try {
			items = await save(which, on);
		} catch (e) {
			error = e instanceof ApiError ? e.message : "Your server couldn't change this. Try again.";
		} finally {
			saving = null;
		}
	}
</script>

<SettingRow {id} {label} {help} block>
	{#if loadError}
		<p class="note">
			Your server couldn't read these.
			<button type="button" class="again" onclick={load}>Try again</button>
		</p>
	{:else if items}
		<ul>
			{#each items as item (item.id)}
				{@const w = words[item.id]}
				{#if w}
					<li>
						<span class="words">
							<span class="name">{w.label}</span>
							{#if w.help}<span class="help">{w.help}</span>{/if}
						</span>
						<Switch checked={item.on} label={w.label} disabled={saving !== null} onchange={(on) => toggle(item.id, on)} />
					</li>
				{/if}
			{/each}
		</ul>
		{#if error}<p class="note error">{error}</p>{/if}
	{/if}
</SettingRow>

<style>
	ul {
		list-style: none;
		margin: 0;
		padding: 0;
	}

	li {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 24px;
		padding: 8px 0;
	}

	.words {
		min-width: 0;
	}

	.name {
		display: block;
		font-size: 14px;
		color: var(--color-foreground);
	}

	.help {
		display: block;
		font-size: 13px;
		color: var(--color-foreground-muted);
	}

	.note {
		margin: 0;
		font-size: 13px;
		color: var(--color-foreground-muted);
	}

	.note.error {
		margin-top: 8px;
		color: var(--color-error);
	}

	.again {
		border: none;
		background: none;
		padding: 0;
		font: inherit;
		color: var(--color-primary);
		cursor: pointer;
	}
</style>
