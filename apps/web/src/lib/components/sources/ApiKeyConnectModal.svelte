<script lang="ts">
	/**
	 * ApiKeyConnectModal — for sources whose auth.kind = "api_key".
	 *
	 * The user pastes one or more strings (token, key, etc.) in a form whose
	 * fields are declared by the source. Frontend POSTs to
	 * `/api/connect/:source_id/complete` with `{name, fields}`; backend
	 * encrypts via virtues_helpers::auth and writes a fully-active credential row.
	 */
	import Modal from '$lib/components/Modal.svelte';
	import { Button, Input } from '$lib';
	import { apikeyComplete, type SourceCatalogItem } from '$lib/api/client';

	interface Props {
		source: SourceCatalogItem | null;
		/**
		 * Override for the field names to collect. Normally left unset — the
		 * source declares them in its manifest and the catalog now carries the
		 * list, so the form asks for exactly what the backend will validate.
		 */
		fields?: string[];
		open: boolean;
		onClose: () => void;
		onSuccess: (credentialId: string) => void;
	}

	let { source, fields: fieldsProp, open, onClose, onSuccess }: Props = $props();

	// `["token"]` is the last resort for a catalog entry that declares nothing,
	// not the default — an empty form would collect no secret at all.
	const fields = $derived(
		fieldsProp ?? (source?.fields?.length ? source.fields : ['token'])
	);

	let name = $state('');
	let values = $state<Record<string, string>>({});
	let submitting = $state(false);
	let error = $state<string | null>(null);

	$effect(() => {
		if (open && source) {
			name = `${source.name} key`;
			values = Object.fromEntries(fields.map((f) => [f, '']));
			error = null;
		}
	});

	async function submit() {
		if (!source) return;
		const trimmedName = name.trim();
		if (!trimmedName) {
			error = 'Name is required';
			return;
		}
		for (const f of fields) {
			if (!values[f]?.trim()) {
				error = `Missing field: ${f}`;
				return;
			}
		}
		submitting = true;
		error = null;
		try {
			const { credential_id } = await apikeyComplete(source.id, trimmedName, values);
			onSuccess(credential_id);
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		} finally {
			submitting = false;
		}
	}
</script>

<Modal {open} {onClose} title={source ? `Connect ${source.name}` : 'Connect source'}>
	{#if source}
		<div class="apikey-form">
			{#if source.description}
				<p class="muted">{source.description}</p>
			{/if}

			<label>
				<span>Name</span>
				<Input bind:value={name} placeholder="A label for this credential" />
			</label>

			{#each fields as field (field)}
				<label>
					<span>{field}</span>
					<Input
						type="password"
						bind:value={values[field]}
						placeholder={`Paste your ${field}`}
					/>
				</label>
			{/each}

			{#if error}
				<div class="error">{error}</div>
			{/if}

			<div class="actions">
				<Button variant="ghost" onclick={onClose} disabled={submitting}>Cancel</Button>
				<Button variant="primary" onclick={submit} disabled={submitting}>
					{submitting ? 'Connecting…' : 'Connect'}
				</Button>
			</div>
		</div>
	{/if}
</Modal>

<style>
	.apikey-form {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
		min-width: 28rem;
	}
	label {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		font-size: 0.875rem;
	}
	label span {
		color: var(--text-muted);
	}
	.muted {
		color: var(--text-muted);
		font-size: 0.875rem;
	}
	.error {
		color: var(--danger);
		font-size: 0.875rem;
	}
	.actions {
		display: flex;
		justify-content: flex-end;
		gap: 0.5rem;
		margin-top: 0.5rem;
	}
</style>
