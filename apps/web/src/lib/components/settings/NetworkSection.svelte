<script lang="ts">
	/**
	 * Settings → Network. Its own door since 2026-08-17 — it used to be the
	 * third component down inside "Box", which is a poor place for the screen
	 * people reach for precisely when something is wrong.
	 *
	 * Exists because a claimed box had no way to change networks: setup's
	 * surfaces correctly close at claim time, and on 2026-08-11 that marooned
	 * an office box on a captive guest network with its owner holding a paired
	 * phone. This is the authed successor — same status/scan/join verbs, no
	 * expiry.
	 *
	 * Two honesty rules learned on hardware, both load-bearing:
	 *
	 * 1. `connectivity` is NetworkManager's verdict, and "portal" gets named in
	 *    the UI — "connected, but this network wants a sign-in" — because an
	 *    IP-without-internet is the single most misleading state a box can be
	 *    in, and it looks exactly like success from every other angle.
	 *
	 * 2. A successful join may sever this very connection (single radio, and
	 *    the new network may not reach the old one's LAN). A dead request is
	 *    not an error — it's "go and look": we poll status until the box
	 *    answers again, from whichever network we find it on.
	 */
	import { onDestroy } from 'svelte';

	import { apiGet, apiSend } from '$lib/api/client';

	type Net = { ssid: string; signal: number; secured: boolean; enterprise: boolean };
	type Status = { connectivity: string; ssid: string | null; ip: string | null };

	let status = $state<Status | null>(null);
	let networks = $state<Net[]>([]);
	let scanning = $state(false);
	let scanError = $state<string | null>(null);

	let chosen = $state<Net | null>(null);
	let psk = $state('');
	let identity = $state('');
	let joinError = $state<string | null>(null);
	/** null = idle; otherwise we're mid-switch and polling for the box. */
	let switching = $state<'sending' | 'watching' | null>(null);
	let watchTimer: ReturnType<typeof setTimeout> | null = null;

	onDestroy(() => {
		if (watchTimer) clearTimeout(watchTimer);
	});

	async function refresh() {
		try {
			status = await apiGet<Status>('/network/status');
		} catch {
			status = null;
		}
	}

	async function scan() {
		scanning = true;
		scanError = null;
		try {
			const r = await apiGet<{ networks: Net[] }>('/network/scan');
			networks = r.networks;
		} catch (e) {
			scanError = String(e);
		} finally {
			scanning = false;
		}
	}

	function choose(n: Net) {
		chosen = n;
		psk = '';
		identity = '';
		joinError = null;
		if (!n.secured) void join();
	}

	async function join() {
		if (!chosen) return;
		switching = 'sending';
		joinError = null;
		try {
			const r = await apiSend<{ ok: boolean; detail?: string }>('POST', '/network/join', {
				ssid: chosen.ssid,
				psk: psk || undefined,
				identity: identity || undefined
			});
			if (!r.ok) {
				// NetworkManager's own words — "Secrets were required, but not
				// provided" beats anything we'd write.
				joinError = r.detail || "Couldn't join that network.";
				switching = null;
				return;
			}
			watch();
		} catch {
			// The box switched networks under this request. Expected — go look.
			watch();
		}
	}

	function watch() {
		switching = 'watching';
		let tries = 0;
		const tick = async () => {
			tries += 1;
			try {
				status = await apiGet<Status>('/network/status');
				switching = null;
				chosen = null;
				return;
			} catch {
				/* still moving */
			}
			if (tries > 15) {
				switching = null;
				joinError =
					"Lost the box after the switch. If it joined a different network than this device is on, it's reachable again once you're both somewhere that connects them — or via remote access.";
				return;
			}
			watchTimer = setTimeout(tick, 3000);
		};
		watchTimer = setTimeout(tick, 4000);
	}

	function bars(signal: number): string {
		if (signal >= 70) return '▂▄▆';
		if (signal >= 40) return '▂▄';
		return '▂';
	}

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

	const connectivityLabel = $derived.by(() => {
		switch (status?.connectivity) {
			case 'full':
				return { text: 'Online', tone: 'ok' };
			case 'portal':
				return { text: 'Connected, but this network wants a sign-in (captive portal)', tone: 'warn' };
			case 'limited':
				return { text: 'Connected, but no internet is getting through', tone: 'warn' };
			case 'none':
				return { text: 'No network', tone: 'warn' };
			default:
				return { text: 'Unknown', tone: 'warn' };
		}
	});

	void refresh();
	void refreshRelay();
</script>

<!-- No <Page>: this renders INSIDE System as a chapter. Which network the
     machine is on is a reading about the machine, not a separate destination —
     it was a sidebar row, then briefly a page under System, and both made you
     leave the page you were already reading to learn one fact. -->
<section class="chapter">
	<h2 class="settings-label">Network</h2>
	{#if status}
		<div class="statusrow">
			<span class="dot" class:warn={connectivityLabel.tone === 'warn'}></span>
			<div>
				<div class="text-sm text-foreground">
					{status.ssid ?? (status.ip ? 'Wired' : 'Not connected')}
					{#if status.ip}<span class="ml-2 font-mono text-xs text-foreground-subtle">{status.ip}</span>{/if}
				</div>
				<div class="text-xs" class:text-foreground-subtle={connectivityLabel.tone === 'ok'} class:warntext={connectivityLabel.tone === 'warn'}>
					{connectivityLabel.text}
				</div>
			</div>
		</div>
	{/if}

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
						Off — your devices reach this server on the local network only.
					{:else if relay.homed}
						Your devices find this server from anywhere through this relay. It moves
						encrypted traffic it cannot read, and keeps no account of who connects.
					{:else}
						Connecting to the relay…
					{/if}
				</div>
				{#if relayError}<p class="mt-1 text-xs warntext">{relayError}</p>{/if}
				<button class="relaybtn" disabled={relayBusy} onclick={toggleRelay}>
					{relay.enabled ? 'Turn off' : 'Turn on'}
				</button>
			</div>
		</div>
	{/if}

	{#if switching}
		<p class="mt-4 text-sm text-foreground-muted">
			{switching === 'sending' ? 'Asking the box to switch…' : 'The box is switching networks — reconnecting…'}
		</p>
	{:else if chosen && chosen.secured}
		<div class="joinform">
			<div class="mb-2 text-sm text-foreground">{chosen.ssid}</div>
			{#if joinError}<p class="mb-2 text-xs warntext">{joinError}</p>{/if}
			{#if chosen.enterprise}
				<p class="mb-2 text-xs text-foreground-subtle">
					This network uses per-person sign-in — the account its operator gave you.
				</p>
				<input
					class="field"
					type="text"
					placeholder="Username"
					autocapitalize="off"
					autocorrect="off"
					bind:value={identity}
				/>
			{/if}
			<input
				class="field"
				type="password"
				placeholder={chosen.enterprise ? 'Password' : 'Wi-Fi password'}
				autocomplete="current-password"
				bind:value={psk}
				onkeydown={(e) => e.key === 'Enter' && join()}
			/>
			<div class="mt-2 flex gap-3">
				<button
					class="joinbtn"
					disabled={chosen.enterprise ? !(identity.trim() && psk) : psk.length < 8}
					onclick={join}>Join network</button
				>
				<button class="cancelbtn" onclick={() => (chosen = null)}>Cancel</button>
			</div>
		</div>
	{:else}
		{#if joinError}<p class="mt-3 text-xs warntext">{joinError}</p>{/if}
		{#if networks.length === 0}
			<button class="scanbtn" disabled={scanning} onclick={scan}>
				{scanning ? 'Scanning…' : 'Find Wi-Fi networks'}
			</button>
			{#if scanError}<p class="mt-2 text-xs warntext">{scanError}</p>{/if}
		{:else}
			<div class="mt-3">
				{#each networks as n (n.ssid)}
					<button class="netrow" onclick={() => choose(n)}>
						<span class="font-mono text-xs text-foreground-subtle">{bars(n.signal)}</span>
						<span class="text-sm text-foreground">{n.ssid}</span>
						<span class="ml-auto text-xs text-foreground-subtle">
							{n.enterprise ? 'work network' : n.secured ? 'locked' : 'open'}
						</span>
					</button>
				{/each}
				<button class="scanbtn mt-2" disabled={scanning} onclick={scan}>
					{scanning ? 'Scanning…' : 'Scan again'}
				</button>
			</div>
		{/if}
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
	.relaybtn {
		margin-top: 0.35rem;
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
		text-decoration: underline;
		text-underline-offset: 2px;
	}
	.relaybtn:disabled {
		opacity: 0.45;
	}
	.relaybtn:hover:not(:disabled) {
		color: var(--color-foreground);
	}
	.warntext {
		color: var(--color-warning, #c92);
	}
	.netrow {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		width: 100%;
		text-align: left;
		padding: 0.55rem 0.25rem;
		border-bottom: 1px solid var(--color-border);
	}
	.netrow:hover {
		background: var(--color-background-secondary);
	}
	.joinform {
		margin-top: 1rem;
		max-width: 22rem;
	}
	.field {
		width: 100%;
		margin-bottom: 0.5rem;
		padding: 0.5rem 0.65rem;
		font-size: 0.875rem;
		background: var(--color-background-secondary);
		border: 1px solid var(--color-border);
		border-radius: 6px;
		color: var(--color-foreground);
	}
	.joinbtn,
	.scanbtn {
		font-size: 0.8125rem;
		padding: 0.4rem 0.9rem;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		color: var(--color-foreground);
	}
	.joinbtn:disabled,
	.scanbtn:disabled {
		opacity: 0.45;
	}
	.joinbtn:hover:not(:disabled),
	.scanbtn:hover:not(:disabled) {
		background: var(--color-background-secondary);
	}
	.cancelbtn {
		font-size: 0.8125rem;
		color: var(--color-foreground-subtle);
	}
</style>
