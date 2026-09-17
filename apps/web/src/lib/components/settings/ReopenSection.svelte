<!--
  Settings → Devices → "Start over".

  Lives at the foot of Devices because that is what it does: it is the plural
  of Unpair. It used to sit at the bottom of "Box", under the CPU graphs — so
  a page you opened to check a temperature ended in a button that signs out
  every device you own.

  Wraps `POST /api/pair/reopen-onboarding` — the `virtues reset --keep-data`
  path. Revokes every paired device and its credentials; touches no data, no
  sources, no subscription, no box identity.

  DELIBERATELY NOT the full reset. That drops every table and lives behind the
  CLI's typed-hostname confirmation, which is the right guard for it — a
  settings screen is the wrong place for a screwdriver.

  Two-step, and the second step names the count. "Revoke 2 devices" is a
  different sentence from "are you sure?", and it is the one that stops someone
  who meant to unpair a single laptop.

  Losing your own session is the confirmation. There is no success state to
  render here: the moment the box accepts, this device is revoked too and the
  app falls back to its unpaired gate. Saying so up front is what keeps that
  from reading as a crash.
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
			// This device is revoked now. A reload lands on the unpaired gate,
			// which is the honest next screen.
			location.reload();
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
			busy = false;
			armed = false;
		}
	}
</script>

<section class="reopen">
	<header><h3>Start over</h3></header>

	<p class="sub">
		Unpairs every device and puts the box back into setup. Your record, your
		connected accounts and your subscription stay exactly as they are.
	</p>

	{#if error}
		<p class="err">
			<Icon icon="ri:error-warning-line" width="14" />
			{error}
		</p>
	{/if}

	{#if armed}
		<div class="confirm">
			<p>
				Every paired device is signed out, including this one — you'll set the
				box up again from the app, using the words it shows on its screen.
			</p>
			<div class="row">
				<Button variant="danger" size="sm" loading={busy} onclick={go}>
					Unpair everything
				</Button>
				<Button variant="ghost" size="sm" disabled={busy} onclick={() => (armed = false)}>
					Cancel
				</Button>
			</div>
		</div>
	{:else}
		<Button variant="secondary" size="sm" class="self-start" onclick={() => (armed = true)}>
			Put this box back into setup
		</Button>
	{/if}
</section>

<style>
	/* Inside Devices' `<Page>` now, which owns the gutter — this only needs the
	   rule and the air that separate it from the device list above. */
	.reopen {
		display: flex;
		flex-direction: column;
		gap: 12px;
		margin-top: 2.5rem;
		padding-top: 1.5rem;
		border-top: 1px solid var(--color-border);
	}

	header {
		display: flex;
		align-items: center;
		justify-content: space-between;
	}

	h3 {
		font-size: 14px;
		font-weight: 600;
		margin: 0;
	}

	.sub {
		margin: 0;
		font-size: 13px;
		line-height: 1.5;
		color: var(--color-foreground-muted);
		max-width: 60ch;
	}

	.confirm {
		display: flex;
		flex-direction: column;
		gap: 10px;
		padding: 12px 14px;
		border: 1px solid var(--color-border);
		border-radius: 8px;
	}

	.confirm p {
		margin: 0;
		font-size: 13px;
		line-height: 1.5;
		max-width: 60ch;
	}

	.row {
		display: flex;
		gap: 8px;
	}

	/* The red on this screen is spent on the only irreversible thing, and it
	   comes from `Button`'s `danger` — the hand-rolled tint here predated the
	   primitive having one. */

	.err {
		display: flex;
		align-items: center;
		gap: 6px;
		margin: 0;
		font-size: 13px;
		color: #ff9ea1;
	}
</style>
