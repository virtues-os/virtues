<script lang="ts">
	/**
	 * Drive → Passwords: the logins you saved on your server.
	 *
	 * The list carries site, label and username. A password and its notes are
	 * fetched when you open a login, and nothing the assistant can call reads
	 * them (`api/passwords.rs`). API keys for applets and the sources'
	 * sign-ins are in Settings → Keys (`KeysView`).
	 */
	import { onMount } from 'svelte';
	import IconButton from '$lib/components/IconButton.svelte';
	import Modal from '$lib/components/Modal.svelte';
	import { Button, Input, Page, Textarea } from '$lib';
	import UniversalDataGrid, { type Column } from '$lib/components/datagrid/UniversalDataGrid.svelte';
	import {
		createLogin,
		deleteLogin,
		getLogins,
		revealLogin,
		updateLogin,
		type LoginItem
	} from '$lib/api/client';
	import { formatRelativeTimestamp } from '$lib/utils/dateUtils';

	let { active: _active }: { tab?: unknown; active?: boolean } = $props();

	let logins = $state<LoginItem[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);

	/** The login being looked at, with its password once fetched. */
	let opened = $state<{ login: LoginItem; password: string; notes: string } | null>(null);
	let passwordShown = $state(false);
	let copied = $state<'username' | 'password' | null>(null);
	let openError = $state<string | null>(null);

	/** The form: a new login (`id` null) or an edit. */
	let form = $state<{
		id: string | null;
		site: string;
		username: string;
		password: string;
		notes: string;
	} | null>(null);
	let saveError = $state<string | null>(null);
	let saving = $state(false);

	let removing = $state<LoginItem | null>(null);
	let removeError = $state<string | null>(null);
	let busy = $state(false);

	async function load() {
		error = null;
		try {
			logins = await getLogins();
		} catch (e) {
			error = message(e);
		} finally {
			loading = false;
		}
	}

	onMount(load);

	function message(e: unknown): string {
		return e instanceof Error ? e.message : String(e);
	}

	/** `https://www.example.com` as `www.example.com`. */
	function host(site: string): string {
		try {
			return new URL(site).host;
		} catch {
			return site;
		}
	}

	async function open(login: LoginItem) {
		openError = null;
		passwordShown = false;
		copied = null;
		try {
			const secret = await revealLogin(login.id);
			opened = { login, ...secret };
		} catch (e) {
			openError = message(e);
		}
	}

	async function copy(what: 'username' | 'password') {
		if (!opened) return;
		try {
			await navigator.clipboard.writeText(what === 'username' ? opened.login.username : opened.password);
			copied = what;
		} catch {
			copied = null;
		}
	}

	function startAdd() {
		form = { id: null, site: '', username: '', password: '', notes: '' };
		saveError = null;
	}

	function startEdit() {
		if (!opened) return;
		const { login, notes } = opened;
		form = {
			id: login.id,
			site: login.site,
			username: login.username,
			password: '',
			notes
		};
		opened = null;
		saveError = null;
	}

	async function save() {
		if (!form) return;
		saving = true;
		saveError = null;
		const input = {
			site: form.site,
			username: form.username,
			password: form.password,
			notes: form.notes
		};
		try {
			if (form.id) await updateLogin(form.id, input);
			else await createLogin(input);
			form = null;
			await load();
		} catch (e) {
			saveError = message(e);
		} finally {
			saving = false;
		}
	}

	function startRemove() {
		if (!opened) return;
		removing = opened.login;
		removeError = null;
		opened = null;
	}

	async function remove() {
		if (!removing) return;
		busy = true;
		try {
			await deleteLogin(removing.id);
			removing = null;
			await load();
		} catch (e) {
			removeError = message(e);
		} finally {
			busy = false;
		}
	}

	const columns: Column<LoginItem>[] = [
		{ key: 'name', label: 'Site', icon: 'mynaui:globe', width: '40%', minWidth: '180px' },
		{ key: 'username', label: 'Username', icon: 'mynaui:user', width: '40%', minWidth: '160px' },
		{
			key: 'updated_at',
			label: 'Changed',
			icon: 'mynaui:clock-circle',
			width: '20%',
			minWidth: '110px',
			hideOnMobile: true,
			getValue: (l) => formatRelativeTimestamp(l.updated_at)
		}
	];
</script>

<Page
	title="Passwords"
	description="Saved on your server. Your assistant can't see them."
	maxWidth="wide"
>
	{#if openError}
		<p class="form-error">{openError}</p>
	{/if}
	<UniversalDataGrid
		items={logins}
		{columns}
		entityType="login"
		{loading}
		{error}
		emptyIcon="mynaui:lock-password"
		emptyMessage="No passwords yet. Add a login to keep it here."
		loadingMessage="Loading passwords…"
		searchPlaceholder="Search sites and usernames…"
		defaultViewMode="table"
		onItemClick={open}
		onRetry={load}
	>
		{#snippet toolbarActions()}
			<Button variant="secondary" size="sm" onclick={startAdd}>Add a login</Button>
		{/snippet}

		{#snippet tableRow(l: LoginItem)}
			<td class="cell">
				<span class="site-name">{l.name}</span>
				{#if l.name !== host(l.site)}
					<span class="site-host">{host(l.site)}</span>
				{/if}
			</td>
			<td class="cell dim">{l.username || 'No username'}</td>
			<td class="cell dim hide-mobile">{formatRelativeTimestamp(l.updated_at)}</td>
		{/snippet}
	</UniversalDataGrid>
</Page>

<Modal open={opened !== null} title={opened?.login.name ?? ''} onClose={() => (opened = null)}>
	{#if opened}
		<dl class="fields">
			<dt>Site</dt>
			<dd>
				<a href={opened.login.site} target="_blank" rel="noopener noreferrer">{host(opened.login.site)}</a>
			</dd>

			<dt>Username</dt>
			<dd class="row">
				<span class="value">{opened.login.username || 'No username'}</span>
				{#if opened.login.username}
					<Button variant="ghost" size="sm" onclick={() => copy('username')}>
						{copied === 'username' ? 'Copied' : 'Copy'}
					</Button>
				{/if}
			</dd>

			<dt>Password</dt>
			<dd class="row">
				<code class="value">{passwordShown ? opened.password : '••••••••••••'}</code>
				<IconButton
					icon={passwordShown ? 'mynaui:eye-off' : 'mynaui:eye'}
					label={passwordShown ? 'Hide password' : 'Show password'}
					size="sm"
					onclick={() => (passwordShown = !passwordShown)}
				/>
				<Button variant="ghost" size="sm" onclick={() => copy('password')}>
					{copied === 'password' ? 'Copied' : 'Copy'}
				</Button>
			</dd>

			{#if opened.notes}
				<dt>Notes</dt>
				<dd class="notes">{opened.notes}</dd>
			{/if}
		</dl>
		<div class="form-actions">
			<Button variant="ghost" onclick={startRemove}>Remove</Button>
			<Button variant="secondary" onclick={startEdit}>Edit</Button>
		</div>
	{/if}
</Modal>

<Modal
	open={form !== null}
	title={form?.id ? 'Edit login' : 'Add a login'}
	onClose={() => (form = null)}
>
	{#if form}
		<form
			class="form"
			onsubmit={(e) => {
				e.preventDefault();
				save();
			}}
		>
			<Input label="Site" bind:value={form.site} placeholder="example.com" autocomplete="off" />
			<Input label="Username" bind:value={form.username} placeholder="nick@example.com" autocomplete="off" />
			<Input
				label="Password"
				type="password"
				bind:value={form.password}
				placeholder={form.id ? 'Leave empty to keep the saved one' : ''}
				autocomplete="new-password"
			/>
			<Textarea label="Notes" bind:value={form.notes} rows={3} placeholder="Optional" />
			{#if saveError}
				<p class="form-error">{saveError}</p>
			{/if}
			<div class="form-actions">
				<Button variant="ghost" type="button" onclick={() => (form = null)}>Cancel</Button>
				<Button
					type="submit"
					disabled={saving || !form.site.trim() || (!form.id && !form.password)}
				>
					{saving ? 'Saving…' : 'Save'}
				</Button>
			</div>
		</form>
	{/if}
</Modal>

<Modal
	open={removing !== null}
	title={`Remove ${removing?.name ?? ''}?`}
	onClose={() => (removing = null)}
>
	{#if removing}
		<p class="body">
			Your server deletes the login for {removing.username || host(removing.site)}. This can't be
			undone.
		</p>
		{#if removeError}
			<p class="form-error">{removeError}</p>
		{/if}
		<div class="form-actions">
			<Button variant="ghost" onclick={() => (removing = null)}>Cancel</Button>
			<Button variant="danger" disabled={busy} onclick={remove}>Remove</Button>
		</div>
	{/if}
</Modal>

<style>
	.cell {
		padding: 0.625rem 0.75rem;
	}
	.dim {
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
	}
	.site-name {
		font-weight: 500;
	}
	.site-host {
		margin-left: 0.5rem;
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
	}
	@media (max-width: 768px) {
		.hide-mobile {
			display: none;
		}
	}
	.fields {
		display: grid;
		grid-template-columns: max-content 1fr;
		gap: 0.75rem 1rem;
		margin: 0;
		align-items: baseline;
	}
	.fields dt {
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
	}
	.fields dd {
		margin: 0;
		min-width: 0;
	}
	.row {
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}
	.value {
		flex: 1;
		min-width: 0;
		overflow-wrap: anywhere;
	}
	.notes {
		white-space: pre-wrap;
		font-size: 0.875rem;
	}
	.body {
		margin: 0;
		font-size: 0.875rem;
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}
	.form {
		display: flex;
		flex-direction: column;
		gap: 1rem;
	}
	.form-error {
		margin: 0 0 0.75rem;
		font-size: 0.8125rem;
		color: var(--color-error);
	}
	.form-actions {
		display: flex;
		justify-content: flex-end;
		gap: 0.5rem;
		margin-top: 1rem;
	}
</style>
