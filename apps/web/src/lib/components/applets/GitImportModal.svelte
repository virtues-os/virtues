<script lang="ts">
	import Modal from '$lib/components/Modal.svelte';
	import Button from '$lib/components/Button.svelte';
	import Icon from '$lib/components/Icon.svelte';
	import SudoModal from '$lib/components/SudoModal.svelte';
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
	// Shape follows the client's return type rather than restating it — the
	// local copy is how `slug` and `commit` went unnoticed for so long.
	let result = $state<Awaited<ReturnType<typeof importActionsFromGit>> | null>(null);

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

	// Importing runs a stranger's code on the box, so it is sudo-gated the same
	// way changing an API key is: approval happens at the box itself, not in a
	// browser someone could have talked you into.
	let showSudo = $state(false);

	function submit(e?: Event) {
		e?.preventDefault();
		if (!url.trim()) return;
		error = null;
		result = null;
		showSudo = true;
	}

	async function runImport(sudoRequestId: string) {
		importing = true;
		error = null;
		try {
			result = await importActionsFromGit({
				url: url.trim(),
				ref: ref.trim() || 'main',
				sudo_request_id: sudoRequestId
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
			<!-- The exact commit now on disk. The server always resolved it; the
			     client used to discard it, so there was no record anywhere of
			     what code was actually running. -->
			<dl class="counts">
				<div><dt>Installed as</dt><dd><code>{result.slug}</code></dd></div>
				{#if result.commit}
					<div><dt>Commit</dt><dd><code>{result.commit.slice(0, 12)}</code></dd></div>
				{/if}
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
			<!-- This caveat lived only in a source comment. The one screen where
			     someone decides to run a stranger's code is where it belongs. -->
			<div class="warn">
				<Icon icon="ri:alert-line" width="16" />
				<div>
					<strong>This runs someone else's code on your box.</strong>
					Imported applets are sandboxed and cannot gain root or read your
					stored credentials, but they still run on a schedule and can read
					and write your data. Import repositories you would be willing to
					read yourself — you can, from the applet's Source pane.
				</div>
			</div>
			<p class="hint">
				The repo is cloned locally and any folder containing a
				<code>manifest.toml</code> becomes an applet; a
				<code>sources.toml</code> adds sources.
			</p>
			<p class="hint">
				Pin to a tag or a commit rather than a branch if you want the code to
				stay put — a branch moves under you on the next import.
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

<SudoModal
	action="import_applet_package"
	title="Install a package from Git"
	description="Installing runs code you did not write on this box. Approve at the box itself by running `virtues sudo` — the same confirmation used for changing an API key."
	actionPayload={{ url: url.trim(), ref: ref.trim() || 'main' }}
	bind:show={showSudo}
	onApproved={(id) => runImport(id)}
/>

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
	.warn {
		display: flex;
		align-items: flex-start;
		gap: 0.5rem;
		padding: 0.625rem 0.75rem;
		border-radius: 8px;
		border: 1px solid color-mix(in srgb, var(--color-warning, #d97706) 35%, transparent);
		background: color-mix(in srgb, var(--color-warning, #d97706) 10%, transparent);
		color: var(--color-foreground, #111827);
		font-size: 0.75rem;
		line-height: 1.5;
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
