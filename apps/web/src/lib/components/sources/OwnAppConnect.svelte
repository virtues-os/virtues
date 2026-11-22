<!--
  Signing in with an app you make at the vendor yourself: Spotify's recent
  plays. Spotify lets one app serve only a handful of people, so each person
  makes their own and pastes its Client ID. The sign-in then goes to Spotify
  and comes back through auth.virtues.com, which passes the browser on to
  this server (`own_app` in virtues-helpers).
-->
<script lang="ts">
	import Modal from '$lib/components/Modal.svelte';
	import { Button, Input } from '$lib';
	import { startOAuth, reloadOnReturn } from './connectDispatch';
	import { isTauri } from '$lib/utils/platform';
	import { openExternal } from '$lib/tauri/bridge';

	let {
		open,
		credentialId,
		onClose,
		onReturn
	}: { open: boolean; credentialId?: string; onClose: () => void; onReturn: () => void } = $props();

	/** The address the person registers in their app. It is the OAuth proxy's
	 *  bounce route; a box that points at another proxy says so in its own
	 *  docs, and every released box uses this one. */
	const REDIRECT_URI = 'https://auth.virtues.com/byo/spotify/callback';
	const DASHBOARD = 'https://developer.spotify.com/dashboard';

	let clientId = $state('');
	let busy = $state(false);
	let error = $state<string | null>(null);

	// Each opening starts clean: no field or error left from the last try.
	$effect(() => {
		if (open) {
			clientId = '';
			busy = false;
			error = null;
		}
	});

	function openDashboard(e: MouseEvent) {
		if (!isTauri) return;
		e.preventDefault();
		void openExternal(DASHBOARD);
	}

	async function go() {
		const id = clientId.trim();
		if (!id) {
			error = 'Paste your app’s Client ID, then continue.';
			return;
		}
		busy = true;
		error = null;
		try {
			const { external } = await startOAuth('spotify', { clientId: id, credentialId });
			if (external) {
				reloadOnReturn(onReturn);
				onClose();
			}
		} catch (e) {
			error = e instanceof Error ? e.message : 'Spotify could not be opened. Try again.';
		} finally {
			busy = false;
		}
	}
</script>

<Modal {open} {onClose} title="Connect your Spotify app" width="md">
	<div class="own-app">
		<ol>
			<li>
				Open <a href={DASHBOARD} target="_blank" rel="noreferrer" onclick={openDashboard}>Spotify’s developer
				dashboard</a> and create an app. Any name works.
			</li>
			<li>Under Redirect URIs, add <code>{REDIRECT_URI}</code>, choose Web API, and save.</li>
			<li>In the app’s settings, open User Management and add the email of your Spotify account.</li>
			<li>Copy the app’s Client ID and paste it here.</li>
		</ol>
		<p class="muted">
			Spotify asks the app’s owner to have Premium, and asks you to sign in again every six months.
		</p>
		<Input bind:value={clientId} label="Client ID" disabled={busy} />
		{#if error}<p class="error">{error}</p>{/if}
		<div class="actions">
			<Button variant="ghost" onclick={onClose} disabled={busy}>Cancel</Button>
			<Button variant="primary" loading={busy} onclick={() => void go()}>Continue to Spotify</Button>
		</div>
	</div>
</Modal>

<style>
	.own-app {
		display: flex;
		flex-direction: column;
		gap: 12px;
	}
	ol {
		margin: 0;
		padding-left: 20px;
		display: flex;
		flex-direction: column;
		gap: 6px;
		font-size: var(--text-body);
	}
	code {
		font-size: var(--text-small);
		word-break: break-all;
	}
	.muted {
		color: var(--color-foreground-muted);
		font-size: var(--text-small);
		margin: 0;
	}
	.error {
		color: var(--color-error);
		font-size: var(--text-small);
		margin: 0;
	}
	.actions {
		display: flex;
		justify-content: flex-end;
		gap: 8px;
	}
</style>
