<script lang="ts">
	/**
	 * ApiKeyConnectModal — for sources whose auth.kind = "api_key".
	 *
	 * The user pastes one or more strings (token, key, etc.) in a form whose
	 * fields are declared by the source. Frontend POSTs to
	 * `/api/connect/:source_id/complete` with `{name, fields}`; backend
	 * encrypts via virtues_helpers::auth and writes a fully-active credential row.
	 *
	 * A source that declares a `login` (X, Instagram) is connected by logging in
	 * instead, where the app can open a window of its own: the shell returns the
	 * site's cookie jar and the fields are filled from it, then posted the same
	 * way. The paste fields stay one click away, and are all a browser tab gets.
	 */
	import Modal from '$lib/components/Modal.svelte';
	import { Button, Input } from '$lib';
	import { apikeyComplete, type SourceCatalogItem } from '$lib/api/client';
	import { browserLogin, canBrowserLogin, canBrowserPane, type JarCookie } from '$lib/tauri/bridge';

	interface Props {
		source: SourceCatalogItem | null;
		/**
		 * Override for the field names to collect. Normally left unset — the
		 * source declares them in its manifest and the catalog now carries the
		 * list, so the form asks for exactly what the backend will validate.
		 */
		fields?: string[];
		/** Reconnecting this credential: the new secrets replace its old ones. */
		credentialId?: string;
		open: boolean;
		onClose: () => void;
		onSuccess: (credentialId: string) => void;
	}

	let { source, fields: fieldsProp, credentialId, open, onClose, onSuccess }: Props = $props();

	// `["token"]` is the last resort for a catalog entry that declares nothing,
	// not the default — an empty form would collect no secret at all.
	const fields = $derived(
		fieldsProp ?? (source?.fields?.length ? source.fields : ['token'])
	);

	let name = $state('');
	let values = $state<Record<string, string>>({});
	let submitting = $state(false);
	let error = $state<string | null>(null);
	/** This shell can open a login window, and the source has a login page. */
	let loginAvailable = $state(false);
	/** Showing the paste fields rather than the login button. */
	let pasting = $state(false);
	let waitingForLogin = $state(false);
	/** The login opens in the Browser pane beside the app (Mac), not a window.
	 *  This modal steps aside while it does: a modal would cover the pane. */
	let inPane = $state(false);

	$effect(() => {
		if (open && source) {
			name = `${source.name} key`;
			values = Object.fromEntries(fields.map((f) => [f, '']));
			error = null;
			pasting = false;
			waitingForLogin = false;
			loginAvailable = false;
			if (source.login) {
				const forSource = source.id;
				void canBrowserLogin().then((ok) => {
					if (source?.id === forSource) loginAvailable = ok;
				});
				void canBrowserPane().then((ok) => (inPane = ok));
			}
		}
	});

	/** The api_key fields a login fills, from the site's cookie jar. */
	function fieldsFromJar(jar: JarCookie[]): Record<string, string> {
		const login = source!.login!;
		if (login.jar_field) {
			return { [login.jar_field]: jar.map((c) => `${c.name}=${c.value}`).join('; ') };
		}
		return Object.fromEntries(
			login.cookies.map((n) => [n, jar.find((c) => c.name === n)?.value ?? ''])
		);
	}

	async function logIn() {
		if (!source?.login) return;
		error = null;
		waitingForLogin = true;
		try {
			const jar = await browserLogin(
				source.id,
				source.login.url,
				source.login.cookies,
				`Log in to ${source.name}`
			);
			waitingForLogin = false;
			submitting = true;
			const { credential_id } = await apikeyComplete(
				source.id,
				source.name,
				fieldsFromJar(jar),
				credentialId
			);
			onSuccess(credential_id);
		} catch (e) {
			const why = e instanceof Error ? e.message : String(e);
			if (why === 'closed') {
				error = 'The login window closed before you finished. Log in again to connect.';
			} else if (why === 'timeout') {
				error = 'The login window timed out after 15 minutes. Log in again to connect.';
			} else if (why === 'already open') {
				error = 'The login window is already open. Finish logging in there.';
			} else {
				error = `Couldn't connect ${source.name}: ${why}. Try again, or paste the cookies instead.`;
			}
		} finally {
			waitingForLogin = false;
			submitting = false;
		}
	}

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
			const { credential_id } = await apikeyComplete(source.id, trimmedName, values, credentialId);
			onSuccess(credential_id);
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		} finally {
			submitting = false;
		}
	}
</script>

<Modal open={open && !(waitingForLogin && inPane)} {onClose} title={source ? `${credentialId ? 'Reconnect' : 'Connect'} ${source.name}` : 'Connect source'}>
	{#if source}
		<div class="apikey-form">
			{#if source.description}
				<p class="muted">{source.description}</p>
			{/if}

			{#if loginAvailable && !pasting}
				<p class="muted">
					Log in to {source.name} {inPane ? 'in the browser beside this view' : 'in a window here'}.
					Your server keeps the session in its vault, and nothing is copied by hand.
				</p>
				{#if waitingForLogin}
					<p class="muted">Finish logging in in the window that opened.</p>
				{/if}

				{#if error}
					<div class="error">{error}</div>
				{/if}

				<div class="actions">
					<Button variant="ghost" onclick={() => (pasting = true)} disabled={waitingForLogin || submitting}>
						Paste cookies instead
					</Button>
					<Button variant="primary" onclick={logIn} disabled={waitingForLogin || submitting}>
						{waitingForLogin ? 'Waiting for login…' : submitting ? 'Connecting…' : `Log in to ${source.name}`}
					</Button>
				</div>
			{:else}
			<label>
				<span>Name</span>
				<Input bind:value={name} placeholder="A label for this credential" />
			</label>

			{#each fields as field (field)}
				<label>
					<span>{field}</span>
					<!-- A getter, not `values[field]`: the effect that seeds `values`
					     runs after this first renders, and binding `undefined` into
					     Input's `$bindable("")` throws — the modal never mounted, so
					     every api_key Connect did nothing. -->
					<Input
						type="password"
						bind:value={() => values[field] ?? '', (v) => (values[field] = v)}
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
			{/if}
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
		color: var(--color-foreground-muted);
	}
	.muted {
		color: var(--color-foreground-muted);
		font-size: 0.875rem;
	}
	.error {
		color: var(--color-error);
		font-size: 0.875rem;
	}
	.actions {
		display: flex;
		justify-content: flex-end;
		gap: 0.5rem;
		margin-top: 0.5rem;
	}
</style>
