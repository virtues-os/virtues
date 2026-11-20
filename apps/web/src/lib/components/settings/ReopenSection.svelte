<!--
  Settings · Devices · Start over, at the foot of the Devices page, because it
  unpairs every device you own.

  Wraps `POST /api/pair/reopen-onboarding`, the `virtues reset --keep-data`
  path. Revokes every paired device and its credentials; touches no data, no
  sources, no subscription, no server identity.

  It is not the full reset. That drops every table and lives behind the CLI's
  typed-hostname confirmation, which is the right guard for it.

  Two steps: the second says it signs out every device, this one included,
  which is the sentence that stops someone who meant to unpair one laptop.

  Losing your own session is the confirmation. There is no success state to
  render here: the moment the server accepts, this device is revoked too and
  the app falls back to its unpaired gate. Saying so up front keeps that from
  reading as a crash.
-->
<script lang="ts">
	import Icon from '@iconify/svelte';
	import Button from '$lib/components/Button.svelte';
	import { reopenOnboarding } from '$lib/api/client';

	let armed = $state(false);
	let busy = $state(false);
	let error = $state<string | null>(null);

	async function go() {
		if (busy) return;
		busy = true;
		error = null;
		try {
			await reopenOnboarding();
			// This device is revoked now. A reload lands on the unpaired gate.
			// Not `reloadApp`: no file a page has on its way can reach the
			// server from a revoked device.
			location.reload();
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
			busy = false;
			armed = false;
		}
	}
</script>

<section class="group" id="setting-start-over" data-setting="start-over">
	<h2 class="settings-label">Start over</h2>

	<p class="sub">
		Unpairs every device, this one included, and puts your server back into setup. Your record, your connected accounts and your
		subscription stay as they are.
	</p>

	{#if error}
		<p class="err">
			<Icon icon="mynaui:danger-circle" width="14" />
			{error}
		</p>
	{/if}

	{#if armed}
		<div class="confirm">
			<p>
				You'll sign out every paired device, including this one. To pair again, run virtues pair on your server and
				enter the code in the app.
			</p>
			<div class="actions">
				<Button variant="danger" loading={busy} onclick={go}>Unpair everything</Button>
				<Button variant="secondary" disabled={busy} onclick={() => (armed = false)}>Cancel</Button>
			</div>
		</div>
	{:else}
		<div class="actions">
			<Button variant="secondary" onclick={() => (armed = true)}>Put this server back into setup</Button>
		</div>
	{/if}
</section>

<style>
	.group {
		padding: 24px 0 8px;
	}

	.sub {
		margin: 0;
		max-width: 60ch;
		font-size: 14px;
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}

	.actions {
		display: flex;
		gap: 8px;
		margin-top: 12px;
	}

	.confirm {
		margin-top: 12px;
		padding: 12px 16px;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-lg);
	}

	.confirm p {
		margin: 0;
		max-width: 60ch;
		font-size: 13px;
		line-height: 1.5;
	}

	.err {
		display: flex;
		align-items: center;
		gap: 8px;
		margin: 8px 0 0;
		font-size: 13px;
		color: var(--color-error);
	}
</style>
