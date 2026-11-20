<script lang="ts">
	/**
	 * Settings → Keys: the named keys your server keeps for you. Saved
	 * passwords are in Drive → Passwords (`PasswordsView`), and your sources'
	 * sign-ins are reconnected in Sources.
	 *
	 * Keys are named values that applets declare they need (`OPENAI_API_KEY`),
	 * set and removed here. The list carries names
	 * only: a value is fetched when you ask to see it, and nothing the
	 * assistant can call reads one (`api/keys.rs`).
	 */
	import { onMount } from 'svelte';
	import IconButton from '$lib/components/IconButton.svelte';
	import Modal from '$lib/components/Modal.svelte';
	import { Button, Input } from '$lib';
	import SettingsPage from '$lib/components/settings/SettingsPage.svelte';
	import UniversalDataGrid, { type Column } from '$lib/components/datagrid/UniversalDataGrid.svelte';
	import {
		deleteKey,
		getKeys,
		revealKey,
		setKey,
		type KeyItem
	} from '$lib/api/client';

	/** The grid keys its rows by `id`; a key's name is unique, so it is the id. */
	type KeyRow = KeyItem & { id: string };
	let keys = $state<KeyRow[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);

	/** The key being added (name editable) or replaced (name fixed). */
	let editing = $state<{ name: string; isNew: boolean } | null>(null);
	let draftValue = $state('');
	let saveError = $state<string | null>(null);
	let saving = $state(false);

	let shown = $state<{ name: string; value: string } | null>(null);
	let copied = $state(false);
	let removing = $state<KeyItem | null>(null);
	/** A failed show or remove; `error` is for the list itself, which the grid replaces. */
	let actionError = $state<string | null>(null);
	let busy = $state(false);

	async function load() {
		error = null;
		try {
			const o = await getKeys();
			keys = o.keys.map((k) => ({ ...k, id: k.name }));
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		} finally {
			loading = false;
		}
	}

	onMount(load);

	function startAdd() {
		editing = { name: '', isNew: true };
		draftValue = '';
		saveError = null;
	}

	function startReplace(k: KeyItem) {
		editing = { name: k.name, isNew: false };
		draftValue = '';
		saveError = null;
	}

	async function save() {
		if (!editing) return;
		const name = editing.name.trim();
		saving = true;
		saveError = null;
		try {
			await setKey(name, draftValue);
			editing = null;
			draftValue = '';
			await load();
		} catch (e) {
			saveError = e instanceof Error ? e.message : String(e);
		} finally {
			saving = false;
		}
	}

	async function show(k: KeyItem) {
		actionError = null;
		copied = false;
		try {
			shown = { name: k.name, value: await revealKey(k.name) };
		} catch (e) {
			actionError = e instanceof Error ? e.message : String(e);
		}
	}

	async function copyShown() {
		if (!shown) return;
		try {
			await navigator.clipboard.writeText(shown.value);
			copied = true;
		} catch {
			copied = false;
		}
	}

	async function remove() {
		if (!removing) return;
		busy = true;
		actionError = null;
		try {
			await deleteKey(removing.name);
			removing = null;
			await load();
		} catch (e) {
			removing = null;
			actionError = e instanceof Error ? e.message : String(e);
		} finally {
			busy = false;
		}
	}

	function usedBy(k: KeyItem): string {
		return k.used_by.length ? k.used_by.join(', ') : 'No applet yet';
	}

	const columns: Column<KeyRow>[] = [
		{ key: 'name', label: 'Key', icon: 'mynaui:key', width: '45%', minWidth: '150px' },
		{
			key: 'is_set',
			label: 'Value',
			icon: 'mynaui:check-circle',
			width: '20%',
			minWidth: '70px',
			getValue: (k) => (k.is_set ? 'Set' : 'Not set')
		},
		{ key: 'used_by', label: 'Used by', icon: 'atlas:applets', width: '35%', minWidth: '120px', hideOnMobile: true, getValue: usedBy }
	];
</script>

<SettingsPage
	title="Keys"
	back={{ route: '/virtues/data', label: 'Your data' }}
	lede="What your applets sign in with, such as an API key. An applet gets only the keys it lists, and the assistant sees their names, never their values."
	width="medium"
>
	{#snippet actions()}
		<Button variant="primary" onclick={startAdd}>Add a key</Button>
	{/snippet}
	<section class="block">
		{#if actionError}
			<p class="form-error">{actionError}</p>
		{/if}
		<UniversalDataGrid
			items={keys}
			{columns}
			entityType="key"
			{loading}
			{error}
			emptyIcon="mynaui:key"
			emptyMessage="No keys yet. Add one when an applet asks for it."
			loadingMessage="Loading keys…"
			searchPlaceholder="Search keys…"
			defaultViewMode="table"
			showCounts={false}
			onItemClick={startReplace}
			onRetry={load}
		>
			{#snippet tableRow(k: KeyRow)}
				<td class="col-name"><code>{k.name}</code></td>
				<td class="col-dim" class:attention={!k.is_set}>{k.is_set ? 'Set' : 'Not set'}</td>
				<td class="col-dim hide-mobile">{usedBy(k)}</td>
			{/snippet}

			{#snippet rowActions(k: KeyRow)}
				<span class="actions">
					{#if k.is_set}
						<IconButton
							icon="mynaui:eye"
							label={`Show ${k.name}`}
							size="nested"
							onclick={(e) => {
								e.stopPropagation();
								show(k);
							}}
						/>
					{/if}
					<IconButton
						icon="mynaui:edit"
						label={k.is_set ? `Replace ${k.name}` : `Set ${k.name}`}
						size="nested"
						onclick={(e) => {
							e.stopPropagation();
							startReplace(k);
						}}
					/>
					{#if k.is_set}
						<IconButton
							icon="mynaui:trash"
							label={`Remove ${k.name}`}
							size="nested"
							onclick={(e) => {
								e.stopPropagation();
								removing = k;
							}}
						/>
					{/if}
				</span>
			{/snippet}
		</UniversalDataGrid>
	</section>
</SettingsPage>

<Modal
	open={editing !== null}
	title={editing?.isNew ? 'Add a key' : `Set ${editing?.name ?? ''}`}
	onClose={() => (editing = null)}
>
	{#if editing}
		<form
			class="form"
			onsubmit={(e) => {
				e.preventDefault();
				save();
			}}
		>
			{#if editing.isNew}
				<Input
					label="Name"
					bind:value={editing.name}
					placeholder="OPENAI_API_KEY"
					helperText="Capitals, digits and underscores, as the applet names it."
					autocomplete="off"
				/>
			{/if}
			<Input
				label="Value"
				type="password"
				bind:value={draftValue}
				placeholder="Paste the key"
				autocomplete="off"
			/>
			{#if saveError}
				<p class="form-error">{saveError}</p>
			{/if}
			<div class="form-actions">
				<Button variant="ghost" type="button" onclick={() => (editing = null)}>Cancel</Button>
				<Button type="submit" disabled={saving || !draftValue.trim() || !editing.name.trim()}>
					{saving ? 'Saving…' : 'Save'}
				</Button>
			</div>
		</form>
	{/if}
</Modal>

<Modal open={shown !== null} title={shown?.name ?? ''} onClose={() => (shown = null)}>
	{#if shown}
		<p class="value"><code>{shown.value}</code></p>
		<div class="form-actions">
			<Button variant="ghost" onclick={copyShown}>{copied ? 'Copied' : 'Copy'}</Button>
			<Button onclick={() => (shown = null)}>Hide</Button>
		</div>
	{/if}
</Modal>

<Modal open={removing !== null} title={`Remove ${removing?.name ?? ''}?`} onClose={() => (removing = null)}>
	{#if removing}
		<p class="block-body">
			{removing.used_by.length
				? `${removing.used_by.join(', ')} will stop running until you add it again.`
				: 'No applet uses it right now.'}
		</p>
		<div class="form-actions">
			<Button variant="ghost" onclick={() => (removing = null)}>Cancel</Button>
			<Button variant="danger" disabled={busy} onclick={remove}>Remove</Button>
		</div>
	{/if}
</Modal>

<style>
	.block {
		margin-bottom: 2.5rem;
	}
	.block-body {
		margin: 0 0 1rem;
		max-width: 60ch;
		font-size: 0.875rem;
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}
	.attention {
		color: var(--color-warning);
	}
	.col-name,
	.col-dim {
		padding: 0.625rem 0.75rem;
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
	.form {
		display: flex;
		flex-direction: column;
		gap: 1rem;
	}
	.form-error {
		margin: 0;
		font-size: 0.8125rem;
		color: var(--color-error);
	}
	.form-actions {
		display: flex;
		justify-content: flex-end;
		gap: 0.5rem;
		margin-top: 0.5rem;
	}
	.value {
		margin: 0;
		padding: 0.75rem;
		border-radius: var(--radius-sm);
		background: var(--color-surface-elevated);
		overflow-wrap: anywhere;
		font-size: 0.8125rem;
	}
</style>
