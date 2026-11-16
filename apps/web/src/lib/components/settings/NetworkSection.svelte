<script lang="ts">
	/**
	 * Settings → System → Network: the relay switch. Whether this server keeps
	 * a rendezvous on the Virtues relay, so devices find it away from home, and
	 * which relay that is. The server's own network connection is the owner's
	 * to manage on that machine.
	 */
	import { apiGet, apiSend } from '$lib/api/client';
	import TextAction from '$lib/components/TextAction.svelte';

	// ── The rendezvous, named out loud ────────────────────────────────────
	// The relay is a baked default (relay.rs::DEFAULT_RELAY_URL) — disclosure
	// plus a real off switch is what separates a default from a secret.
	type Relay = { enabled: boolean; relay_url: string | null; default_url: string; homed: boolean };
	let relay = $state<Relay | null>(null);
	let relayBusy = $state(false);
	let relayError = $state<string | null>(null);

	async function refreshRelay() {
		try {
			relay = await apiGet<Relay>('/network/relay');
		} catch {
			relay = null;
		}
	}

	async function toggleRelay() {
		if (!relay || relayBusy) return;
		relayBusy = true;
		relayError = null;
		try {
			await apiSend('PUT', '/network/relay', { enabled: !relay.enabled });
			// The rebind takes a moment; read back after it settles.
			setTimeout(() => void refreshRelay(), 2500);
			await refreshRelay();
		} catch (e) {
			relayError = String(e);
		} finally {
			relayBusy = false;
		}
	}

	void refreshRelay();
</script>

<!-- No <Page>: this renders INSIDE System as a chapter. -->
<section class="chapter">
	<h2 class="settings-label">Network</h2>
	{#if relay}
		<div class="statusrow">
			<span class="dot" class:warn={relay.enabled && !relay.homed} class:off={!relay.enabled}></span>
			<div>
				<div class="text-sm text-foreground">
					Reach
					{#if relay.enabled}
						<span class="ml-2 font-mono text-xs text-foreground-subtle"
							>{(relay.relay_url ?? relay.default_url).replace('https://', '')}</span
						>
					{/if}
				</div>
				<div class="text-xs text-foreground-subtle">
					{#if !relay.enabled}
						Off. Your devices reach this server on the local network only.
					{:else if relay.homed}
						Your devices find this server from anywhere through this relay. It moves
						encrypted traffic it cannot read, and keeps no account of who connects.
					{:else}
						Connecting to the relay…
					{/if}
				</div>
				{#if relayError}<p class="mt-1 text-xs warntext">{relayError}</p>{/if}
				<div class="relayact">
					<TextAction quiet loading={relayBusy} onclick={toggleRelay}>
						{relay.enabled ? 'Turn off' : 'Turn on'}
					</TextAction>
				</div>
			</div>
		</div>
	{/if}

</section>

<style>
	.statusrow {
		display: flex;
		align-items: flex-start;
		gap: 0.6rem;
		margin-top: 0.75rem;
	}
	.dot {
		width: 8px;
		height: 8px;
		border-radius: 50%;
		background: var(--color-success, #4a9);
		margin-top: 0.35rem;
		flex: none;
	}
	.dot.warn {
		background: var(--color-warning, #c92);
	}
	.dot.off {
		background: var(--color-foreground-subtle, #999);
	}
	/* Placement only. The action itself is TextAction — the margin lives on a
	   wrapper because the primitive's own `margin: 0` is a scoped rule and
	   outranks anything forwarded onto it. */
	.relayact {
		margin-top: 0.35rem;
	}
	.warntext {
		color: var(--color-warning, #c92);
	}
</style>
