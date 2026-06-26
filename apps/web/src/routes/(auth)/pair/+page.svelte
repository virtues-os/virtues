<!--
  Pair landing page.

  Two states:

  - WITH a fragment token (`/pair#t=<token>`): consume it. The token is in
    the URL fragment specifically because browsers never send fragments to
    servers — no proxy logs, no referer leakage. The page reads it via JS,
    POSTs to /api/pair/consume, gets a session cookie back, and navigates
    away. The fragment is wiped from history on the way out.

  - WITHOUT a token: render a "this device isn't paired" landing with
    instructions. No login form, no email field, nothing to brute-force or
    phish. The only way in is from `virtues link` on the box or a "+ Add
    device" QR scanned from a paired device.
-->
<script lang="ts">
	import { goto } from "$app/navigation";
	import Icon from "$lib/components/Icon.svelte";
	import { onMount } from "svelte";

	type Mode = "idle" | "exchanging" | "error";
	let mode = $state<Mode>("idle");
	let errorMessage = $state<string | null>(null);

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

	function buildDeviceInfo(): Record<string, string> {
		return {
			user_agent: navigator.userAgent,
			screen: `${window.screen.width}x${window.screen.height}`,
			lang: navigator.language,
			// IANA timezone — read by the box's home_timezone cross-check in
			// pair.rs. See docs/timezone-model.md.
			timezone: Intl.DateTimeFormat().resolvedOptions().timeZone ?? "",
		};
	}

	async function consume(token: string) {
		mode = "exchanging";
		try {
			const resp = await fetch("/api/pair/consume", {
				method: "POST",
				headers: { "Content-Type": "application/json" },
				body: JSON.stringify({
					token,
					kind: "browser",
					device_info: buildDeviceInfo(),
				}),
			});
			if (resp.ok) {
				const data = await resp.json();
				// Wipe the fragment so a back-button or copy/paste can't leak the
				// already-consumed token. `replaceState` keeps the navigation
				// stack clean.
				history.replaceState(null, "", "/pair");
				// Fresh box → the setup wizard owns the next steps (account,
				// name). Set-up box → wherever the server pointed us. The state
				// is derived server-side, so this is safe to probe every time.
				try {
					const s = await fetch("/api/setup/state");
					if (s.ok) {
						const setup = await s.json();
						if (!setup.setup_complete) {
							await goto("/setup", { replaceState: true });
							return;
						}
					}
				} catch (_e) {
					/* fall through to the normal redirect */
				}
				await goto(data.redirect ?? "/", { replaceState: true });
				return;
			}
			const data = await resp.json().catch(() => ({}));
			errorMessage =
				data.error === "invalid_or_expired_token"
					? "This link is invalid or already used. Run `virtues pair` on the box to get a new one."
					: "Could not complete pairing. Try again with a fresh link.";
			mode = "error";
		} catch (_e) {
			errorMessage = "Could not reach the box. Make sure you're on the same network.";
			mode = "error";
		}
	}

	onMount(() => {
		const token = readFragmentToken();
		if (token) {
			void consume(token);
		}
	});
</script>

<div class="w-full">
	{#if mode === "exchanging"}
		<div
			class="mb-4 p-3 rounded-lg bg-surface-alt border border-border text-foreground-muted text-sm flex items-center gap-2"
		>
			<Icon icon="ri:loader-4-line" class="animate-spin" />
			<span>Pairing this device…</span>
		</div>
	{:else if mode === "error" && errorMessage}
		<div
			class="mb-4 p-3 rounded-lg bg-error-subtle border border-error/20 text-error text-sm"
		>
			{errorMessage}
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
					<span class="text-foreground">From the box itself:</span>
					run
					<code
						class="px-1.5 py-0.5 rounded bg-surface-alt border border-border text-xs">virtues pair</code
					>
					and open the URL (or enter the code) it prints.
				</li>
				<li>
					<span class="text-foreground">From an already-paired device:</span>
					open Settings → Devices → <span class="text-foreground">Add device</span>,
					and scan the QR with this device's camera (or paste the URL).
				</li>
			</ol>

			<p class="pl-11 text-xs">
				No passwords, no email. The only way in is to prove you're at the box or
				you already have a paired device.
			</p>
		</div>
	{/if}
</div>
