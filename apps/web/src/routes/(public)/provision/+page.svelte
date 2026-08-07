<!--
  /provision — hand the box your home wifi, from a phone on its setup AP.

  Served BY THE BOX at 10.42.0.1:8000, not from the bundled app. When the
  phone is on the setup AP there is no pairing yet, so no iroh transport and
  nothing bundled can reach the box — this has to be plain HTTP from the box
  itself. It is also what a captive-portal redirect lands on.

  It is a PHONE page, unlike /display which is a fixed 585x329 panel. Real
  keyboard, real touch, one column, large targets.

  The network list is the BOX's scan, not the phone's — the box is what has to
  reach the network, and offering one it cannot see produces a failure with no
  explanation. Both the list and the join are gated server-side on
  AP-subnet-and-unclaimed; see api/provision.rs.

  The pair code is deliberately NOT here. It is loopback-only, so this page
  could not show it even if we wanted to, and that is the point: the code is
  read off the box's own screen, which is what proves the person is standing
  in front of it.
-->
<script lang="ts">
	import { onDestroy } from "svelte";

	type Network = { ssid: string; signal: number; secured: boolean };
	type Phase = "list" | "password" | "joining" | "done" | "closed";

	let phase = $state<Phase>("list");
	let networks = $state<Network[]>([]);
	let chosen = $state<Network | null>(null);
	let psk = $state("");
	let error = $state<string | null>(null);
	let loading = $state(true);
	let poll: ReturnType<typeof setInterval> | null = null;

	async function loadNetworks() {
		loading = true;
		error = null;
		try {
			const res = await fetch("/api/provision/networks");
			if (res.status === 404) {
				// Either this phone is not on the setup AP, or the box has already
				// been claimed. Both mean there is nothing to do here.
				phase = "closed";
				return;
			}
			if (!res.ok) throw new Error(String(res.status));
			networks = await res.json();
		} catch {
			// Deliberately not the status code. A transport error here means the
			// box is unreachable from this phone, and "502" tells the owner
			// nothing they can act on — unlike a join failure, where nmcli's own
			// wording ("Secrets were required") is the most useful thing we have.
			error = "Couldn't reach your box. Make sure you're still on its wifi network.";
		} finally {
			loading = false;
		}
	}

	function choose(n: Network) {
		chosen = n;
		psk = "";
		error = null;
		phase = n.secured ? "password" : "joining";
		if (!n.secured) void join();
	}

	async function join() {
		if (!chosen) return;
		phase = "joining";
		error = null;
		try {
			const res = await fetch("/api/provision/join", {
				method: "POST",
				headers: { "content-type": "application/json" },
				body: JSON.stringify({ ssid: chosen.ssid, psk: psk || undefined }),
			});
			const body = await res.json();
			if (!body.ok) {
				// nmcli's own words. "Secrets were required, but not provided"
				// tells someone their password was wrong far better than we could.
				error = body.detail || "Couldn't join that network.";
				phase = chosen.secured ? "password" : "list";
				return;
			}
			watchStatus();
		} catch {
			// The box may have just switched networks under us — that is the
			// success path as often as the failure one, so ask rather than guess.
			watchStatus();
		}
	}

	function watchStatus() {
		if (poll) clearInterval(poll);
		let tries = 0;
		poll = setInterval(async () => {
			tries += 1;
			try {
				const res = await fetch("/api/provision/status");
				const s = await res.json();
				if (s.online) {
					if (poll) clearInterval(poll);
					phase = "done";
					return;
				}
			} catch {
				/* transient while the radio reassociates */
			}
			if (tries > 20) {
				if (poll) clearInterval(poll);
				error = "The box didn't come online. Check the password and try again.";
				phase = "list";
				void loadNetworks();
			}
		}, 3000);
	}

	onDestroy(() => {
		if (poll) clearInterval(poll);
	});

	void loadNetworks();

	function bars(signal: number): string {
		if (signal >= 70) return "▂▄▆";
		if (signal >= 40) return "▂▄ ";
		return "▂  ";
	}
</script>

<main>
	<header>
		<span class="mark">∴</span>
		<h1>Connect your box</h1>
	</header>

	{#if phase === "closed"}
		<p class="body">
			This page only works while your box is being set up, from a phone joined to its own network.
		</p>
	{:else if phase === "done"}
		<div class="done">
			<div class="tick">✓</div>
			<h2>Your box is online</h2>
			<p class="body">
				Now open the Virtues app and enter the code shown on your box's screen. Your phone will drop
				back to your normal wifi on its own.
			</p>
		</div>
	{:else if phase === "joining"}
		<p class="body">Joining {chosen?.ssid}…</p>
		<p class="hint">This takes a few seconds. Your phone may briefly lose its connection.</p>
	{:else if phase === "password"}
		<button class="back" onclick={() => (phase = "list")}>← Networks</button>
		<h2 class="ssid">{chosen?.ssid}</h2>
		{#if error}<p class="err">{error}</p>{/if}
		<!-- svelte-ignore a11y_autofocus -->
		<input
			type="password"
			bind:value={psk}
			placeholder="Wi-Fi password"
			autocomplete="current-password"
			autocapitalize="off"
			autocorrect="off"
			autofocus
			onkeydown={(e) => e.key === "Enter" && join()}
		/>
		<button class="primary" onclick={join} disabled={psk.length < 8}>Join network</button>
		<p class="hint">
			On iOS you can copy this from Settings → Wi-Fi → your network → the password field.
		</p>
	{:else if loading}
		<p class="body">Looking for networks…</p>
	{:else if error && networks.length === 0}
		<!-- One message, not two: an error AND "no networks found" are
		     contradictory explanations of the same blank list. -->
		<p class="err">{error}</p>
		<button class="primary" onclick={loadNetworks}>Try again</button>
	{:else if networks.length === 0}
		<p class="body">Your box can't see any wifi networks from where it is.</p>
		<button class="primary" onclick={loadNetworks}>Scan again</button>
	{:else}
		<!-- A retained error here is a failed join, so it belongs above the list
		     the owner is about to pick from again. -->
		{#if error}<p class="err">{error}</p>{/if}
		<p class="hint list-hint">These are the networks your box can see.</p>
			<ul>
				{#each networks as n (n.ssid)}
					<li>
						<button onclick={() => choose(n)}>
							<span class="name">{n.ssid}</span>
							<span class="meta">{n.secured ? "🔒" : ""} <span class="bars">{bars(n.signal)}</span></span>
						</button>
					</li>
				{/each}
			</ul>
	{/if}
</main>

<style>
	main {
		--ink: #f4f1ea;
		--dim: #93a0ad;
		--faint: #54606c;
		--bg: #0b0f14;
		--line: #1b242e;
		--accent: #3e9fd4;
		min-height: 100vh;
		background: var(--bg);
		color: var(--ink);
		font-family: system-ui, -apple-system, sans-serif;
		padding: 2rem 1.25rem 3rem;
		max-width: 30rem;
		margin-inline: auto;
	}
	header {
		display: flex;
		align-items: center;
		gap: 0.6rem;
		margin-bottom: 1.75rem;
	}
	.mark {
		color: var(--faint);
	}
	h1 {
		font-size: 1.25rem;
		font-weight: 600;
		margin: 0;
	}
	h2 {
		font-size: 1.1rem;
		font-weight: 600;
		margin: 0 0 0.75rem;
	}
	.ssid {
		margin-top: 0.5rem;
	}
	.body {
		color: var(--dim);
		line-height: 1.55;
		margin: 0 0 1rem;
	}
	.hint {
		color: var(--faint);
		font-size: 0.8rem;
		line-height: 1.5;
		margin: 0.9rem 0 0;
	}
	.list-hint {
		margin: 0 0 0.75rem;
	}
	.err {
		background: rgba(201, 60, 60, 0.12);
		border: 1px solid rgba(201, 60, 60, 0.3);
		color: #e2807c;
		padding: 0.7rem 0.85rem;
		border-radius: 0.5rem;
		font-size: 0.875rem;
		margin: 0 0 1rem;
	}
	ul {
		list-style: none;
		padding: 0;
		margin: 0;
		border-top: 1px solid var(--line);
	}
	li button {
		width: 100%;
		display: flex;
		justify-content: space-between;
		align-items: center;
		gap: 1rem;
		/* Comfortably above the 44px touch minimum — this is the one screen
		   where a mis-tap costs a full retry through a network switch. */
		min-height: 3.4rem;
		padding: 0.5rem 0.25rem;
		background: none;
		border: 0;
		border-bottom: 1px solid var(--line);
		color: var(--ink);
		font: inherit;
		text-align: left;
		cursor: pointer;
	}
	.name {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.meta {
		color: var(--faint);
		font-size: 0.8rem;
		flex: none;
	}
	.bars {
		font-family: ui-monospace, Menlo, monospace;
		letter-spacing: -1px;
	}
	input {
		width: 100%;
		font-size: 1rem; /* < 16px makes iOS zoom the viewport on focus */
		padding: 0.85rem;
		border-radius: 0.55rem;
		border: 1px solid var(--line);
		background: #111823;
		color: var(--ink);
		margin-bottom: 1rem;
	}
	button.primary {
		width: 100%;
		min-height: 3rem;
		border: 0;
		border-radius: 0.55rem;
		background: var(--accent);
		color: #fff;
		font-size: 1rem;
		font-weight: 600;
		cursor: pointer;
	}
	button.primary:disabled {
		opacity: 0.4;
	}
	button.back {
		background: none;
		border: 0;
		color: var(--dim);
		font: inherit;
		padding: 0;
		cursor: pointer;
	}
	.done {
		text-align: center;
		padding-top: 2rem;
	}
	.tick {
		font-size: 2rem;
		color: #5fb07e;
		margin-bottom: 0.75rem;
	}
</style>
