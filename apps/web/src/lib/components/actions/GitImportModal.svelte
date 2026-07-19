<script lang="ts">
	import Modal from '$lib/components/Modal.svelte';
	import Button from '$lib/components/Button.svelte';
	import Icon from '$lib/components/Icon.svelte';
	import { importActionsFromGit } from '$lib/api/client';

	type Props = {
		open: boolean;
		onClose: () => void;
		onImported?: () => void;
	};

	let { open, onClose, onImported }: Props = $props();

	let url = $state('');
	let ref = $state('main');
	let importing = $state(false);
	let error = $state<string | null>(null);
	let result = $state<{ added: string[]; updated: string[]; removed: string[] } | null>(null);

	function reset() {
		url = '';
		ref = 'main';
		importing = false;
		error = null;
		result = null;
	}

	function close() {
		reset();
		onClose();
	}

	async function submit(e?: Event) {
		e?.preventDefault();
		if (!url.trim()) return;
		importing = true;
		error = null;
		result = null;
		try {
			result = await importActionsFromGit({
				url: url.trim(),
				ref: ref.trim() || 'main'
			});
			onImported?.();
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		} finally {
			importing = false;
		}
	}
</script>

<Modal {open} onClose={close} title="Import actions from Git" width="md">
	{#if result}
		<div class="result">
			<div class="result-row">
				<Icon icon="ri:check-line" width="16" />
				<span>Imported from <code>{url}</code></span>
			</div>
			<dl class="counts">
				<div><dt>Added</dt><dd>{result.added.length}</dd></div>
				<div><dt>Updated</dt><dd>{result.updated.length}</dd></div>
				<div><dt>Removed</dt><dd>{result.removed.length}</dd></div>
			</dl>
		</div>
	{:else}
		<form onsubmit={submit} class="form">
			<label>
				<span class="label">Repository URL</span>
				<input
					type="url"
					bind:value={url}
					placeholder="https://github.com/owner/repo.git"
					required
					disabled={importing}
				/>
			</label>
			<label>
				<span class="label">Ref</span>
				<input type="text" bind:value={ref} placeholder="main" disabled={importing} />
			</label>
			<p class="hint">
				The repo is cloned locally and any folder containing a
				<code>manifest.toml</code> becomes an action.
			</p>
			<p class="hint">
				<strong>Public</strong> HTTPS URLs work without auth.
				<strong>Private</strong> repos: use the SSH URL
				(<code>git@host:owner/repo.git</code>) and make sure your key
				is loaded in <code>ssh-agent</code> first.
			</p>
			{#if error}
				<div class="error">{error}</div>
			{/if}
		</form>
	{/if}

	{#snippet footer()}
		{#if result}
			<Button variant="primary" onclick={close}>Done</Button>
		{:else}
			<Button variant="secondary" onclick={close} disabled={importing}>Cancel</Button>
			<Button variant="primary" onclick={submit} disabled={importing || !url.trim()}>
				{importing ? 'Importing…' : 'Import'}
			</Button>
		{/if}
	{/snippet}
</Modal>

<style>
	.form {
		display: flex;
		flex-direction: column;
		gap: 1rem;
	}
	label {
		display: flex;
		flex-direction: column;
		gap: 0.375rem;
	}
	.label {
		font-size: 0.75rem;
		font-weight: 500;
		color: var(--color-foreground-muted, #6b7280);
	}
	input {
		font: inherit;
		font-size: 0.875rem;
		padding: 0.5rem 0.625rem;
		border: 1px solid var(--color-border, #e5e7eb);
		border-radius: 6px;
		background: var(--color-surface, #fff);
		color: var(--color-foreground, inherit);
	}
	input:focus {
		outline: 2px solid var(--color-accent, #3b82f6);
		outline-offset: -1px;
		border-color: transparent;
	}
	.hint {
		margin: 0;
		font-size: 0.75rem;
		color: var(--color-foreground-subtle, #9ca3af);
		line-height: 1.5;
	}
	.hint code {
		font-size: 0.7rem;
		padding: 0.05rem 0.3rem;
		background: var(--color-surface-elevated, #f3f4f6);
		border-radius: 3px;
	}
	.error {
		font-size: 0.8125rem;
		color: var(--color-error, #dc2626);
		padding: 0.5rem 0.625rem;
		background: var(--color-error-bg, rgba(220, 38, 38, 0.08));
		border-radius: 6px;
	}
	.result {
		display: flex;
		flex-direction: column;
		gap: 1rem;
	}
	.result-row {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		font-size: 0.875rem;
	}
	.result-row code {
		font-size: 0.75rem;
	}
	.counts {
		display: flex;
		gap: 1.5rem;
		margin: 0;
	}
	.counts > div {
		display: flex;
		flex-direction: column;
		gap: 0.125rem;
	}
	.counts dt {
		font-size: 0.7rem;
		color: var(--color-foreground-subtle, #9ca3af);
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}
	.counts dd {
		margin: 0;
		font-size: 1.25rem;
		font-weight: 600;
	}
</style>
