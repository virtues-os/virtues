<script lang="ts">
	/**
	 * Settings → Updates and network: the relay switch. Whether this server keeps
	 * a rendezvous on the Virtues relay, so devices find it away from home, and
	 * which relay that is. The relay is a baked default
	 * (relay.rs::DEFAULT_RELAY_URL), so the row names it and can turn it off.
	 * What the relay can and can't see is in agents/build/privacy-boundary.md.
	 */
	import { apiGet, apiSend } from '$lib/api/client';
	import SettingRow from './SettingRow.svelte';
	import Switch from './Switch.svelte';

	type Relay = { enabled: boolean; relay_url: string | null; default_url: string; homed: boolean };
	let relay = $state<Relay | null>(null);
	let busy = $state(false);
	let error = $state<string | null>(null);

	async function refresh() {
		try {
			relay = await apiGet<Relay>('/network/relay');
		} catch {
			relay = null;
		}
	}

	async function toggle(on: boolean) {
		if (!relay || busy) return;
		busy = true;
		error = null;
		try {
			await apiSend('PUT', '/network/relay', { enabled: on });
			// The rebind takes a moment; read back after it settles.
			setTimeout(() => void refresh(), 2500);
			await refresh();
		} catch {
			error = "Your server couldn't change this. Try again.";
		} finally {
			busy = false;
		}
	}

	void refresh();

	const host = $derived(relay ? (relay.relay_url ?? relay.default_url).replace(/^https?:\/\//, '') : '');
	const help = $derived(
		error ??
			(!relay
				? ''
				: !relay.enabled
					? 'Off. Your devices reach this server on your home network only.'
					: relay.homed
						? `Your devices find this server from anywhere through ${host}, which passes encrypted traffic it can't read.`
						: `Connecting to ${host}.`),
	);
</script>

{#if relay}
	<SettingRow id="reach" label="Reach your server away from home" {help}>
		<Switch checked={relay.enabled} label="Reach your server away from home" disabled={busy} onchange={toggle} />
	</SettingRow>
{/if}
