<!--
  Pair landing page.

  This page can explain pairing; it can never perform it. The allowlisted iroh
  key is the credential (virtues-core middleware/auth.rs) — a browser tab holds
  no key, so there is nothing for the box to allowlist and no session to hand
  back. It used to POST /api/pair/consume with `kind: "browser"`, which the
  handler and the `app_device.kind` CHECK constraint both reject; the "gets a
  session cookie back" it was written against is from the pre-iroh auth model.

  So both states are informational:

  - WITH a fragment token (`/pair#t=<token>`): the link was opened in a browser
    when it was meant to be scanned or typed *into the Virtues app*, which holds
    the key. Say that, and leave the token unused so it stays redeemable. The
    token lives in the fragment because browsers never send fragments to servers
    — no proxy logs, no referer leakage — and it is wiped from history here.

  - WITHOUT a token: a "this device isn't paired" landing. No login form, no
    email field, nothing to brute-force or phish. The only way in is `virtues
    pair` on the box or a QR scanned from the app on an already-paired device.
-->
<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import { onMount } from "svelte";

	type Mode = "idle" | "wrong-surface";
	let mode = $state<Mode>("idle");

	function readFragmentToken(): string | null {
		// `window.location.hash` is `#t=…`. Strip the leading `#` and parse as
		// a query string so we can support future params (e.g. `#t=…&kind=…`).
		const raw = window.location.hash.startsWith("#")
			? window.location.hash.slice(1)
			: window.location.hash;
		if (!raw) return null;
		const params = new URLSearchParams(raw);
		const token = params.get("t");
		return token && token.length > 0 ? token : null;
	}

	function checkFragment() {
		if (!readFragmentToken()) return;
		// Deliberately unused. Redeeming it here would burn a live token on a
		// surface that cannot hold the resulting key; leaving it lets the user
		// finish in the app with the same link. Wipe the fragment so a
		// back-button or a copied URL can't leak it.
		history.replaceState(null, "", "/pair");
		mode = "wrong-surface";
	}

	onMount(() => {
		checkFragment();
		// Pasting a pair URL while already on /pair changes only the fragment,
		// which SvelteKit resolves without remounting — so mount alone would
		// leave the page silently showing the unpaired landing.
		window.addEventListener("hashchange", checkFragment);
		return () => window.removeEventListener("hashchange", checkFragment);
	});
</script>

<div class="w-full">
	{#if mode === "wrong-surface"}
		<div class="space-y-4 text-sm text-foreground-muted">
			<div class="flex items-start gap-3">
				<div
					class="flex-shrink-0 w-8 h-8 rounded-full bg-surface-alt border border-border flex items-center justify-center"
				>
					<Icon icon="ri:smartphone-line" class="text-foreground-muted" />
				</div>
				<div>
					<div class="text-foreground font-medium mb-1">
						Finish this in the Virtues app
					</div>
					<p>
						This link pairs an app, not a browser tab. Virtues authenticates a
						device by a key the app holds, and a browser has none to offer.
					</p>
				</div>
			</div>

			<p class="pl-11">
				Open Virtues on the device you're adding and scan the same QR from its
				pairing screen, or type the code shown beside it. The link is still
				good — nothing here used it up.
			</p>
		</div>
	{:else}
		<div class="space-y-4 text-sm text-foreground-muted">
			<div class="flex items-start gap-3">
				<div
					class="flex-shrink-0 w-8 h-8 rounded-full bg-surface-alt border border-border flex items-center justify-center"
				>
					<Icon icon="ri:lock-line" class="text-foreground-muted" />
				</div>
				<div>
					<div class="text-foreground font-medium mb-1">
						This device isn't paired
					</div>
					<p>
						Virtues uses device pairing instead of passwords. To get in:
					</p>
				</div>
			</div>

			<ol class="space-y-3 pl-11">
				<li>
					<span class="text-foreground">From the server itself:</span>
					run
					<code
						class="px-1.5 py-0.5 rounded bg-surface-alt border border-border text-xs">virtues pair</code
					>
					and open the URL (or enter the code) it prints.
				</li>
				<li>
					<span class="text-foreground">From an already-paired device:</span>
					open Settings → Devices → <span class="text-foreground">Add device</span>,
					then scan that QR from the Virtues app on this device — the app holds
					the key, so pairing has to finish there rather than in a browser.
				</li>
			</ol>

			<p class="pl-11 text-xs">
				No passwords, no email. The only way in is to prove you're at the box or
				you already have a paired device.
			</p>
		</div>
	{/if}
</div>
