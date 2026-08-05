<!--
  Devices — the unified "+ paired things" list. Every browser tab, mobile app,
  Mac collector, and sensor that's paired with this box shows up here. The
  user can revoke any of them; the box invalidates the credential AND evicts
  the WG peer in a single transaction. There is no "log out of just one tab"
  here — that's signout. This is "this hardware/app no longer has any
  authority."
-->
<script lang="ts">
	import type { Tab } from "$lib/tabs/types";
	import { Page, Button, Badge, EmptyState, LoadingState, ErrorState } from "$lib";
	import Icon from "$lib/components/Icon.svelte";
	import { confirmAction } from "$lib/stores/dialog.svelte";
	import {
		listDevices,
		pairMint,
		pairDeny,
		pairStatus as pairStatusApi,
	} from "$lib/api/client";
	import { createResource } from "$lib/utils/resource.svelte";
	import { formatTimeAgo } from "$lib/utils/dateUtils";
	import { toast } from "svelte-sonner";
	import { isTauri } from "$lib/utils/platform";

	// Where to land after revoking THIS device — the one true "return to pairing"
	// flow. In the browser, pairing is the SPA's cookie-redeem `/pair` page. In
	// the Tauri app, pairing is the native shell's concern: reloading the webview
	// root re-runs the app's unpaired gate, which hands control back to the shell.
	// (The precise native handoff — a shell IPC that drops the proven iroh key —
	// is the one open seam; until it lands, the gate + re-pair covers it.)
	function returnToPairing() {
		if (isTauri) {
			window.location.href = "/";
			return;
		}
		window.location.href = "/pair";
	}

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	type Device = {
		id: string;
		permissions: {
			full_disk_access?: boolean;
			accessibility?: boolean;
			denied?: string[];
			checked_at?: string;
			stale?: boolean;
		} | null;
		// Mirrors the CHECK on `app_device.kind`. No "browser": the allowlisted
		// iroh key is the credential (middleware/auth.rs), and a bare browser
		// holds none — it cannot be a paired device, only the loopback console.
		kind: "mobile_app" | "desktop_app" | "sensor" | "cli";
		label: string;
		paired_at: string;
		last_seen_at: string | null;
		paired_from_ip: string | null;
		// Reported build identity (X-Virtues-Client header). Null until the
		// device has checked in on a build that reports it.
		version: string | null;
		sha: string | null;
		channel: string | null;
		is_current: boolean;
	};

	type DevicesResponse = { devices: Device[] };
	const res = createResource(() => listDevices<DevicesResponse>());
	const devices = $derived(res.data?.devices ?? []);

	// "+ Add device" modal state.
	let addOpen = $state(false);
	let mintLoading = $state(false);
	let mintError = $state<string | null>(null);
	let mintedToken = $state<string | null>(null);
	let mintedUrl = $state<string | null>(null);
	let mintedQrSvg = $state<string | null>(null);
	let mintedTokenId = $state<string | null>(null);
	let mintedExpiresAt = $state<string | null>(null);
	// No "pending": an authenticated mint is authorized on the spot, so the
	// confirm round-trip (and the /api/pair/confirm route it called, which no
	// longer exists) is gone.
	let pairStatus = $state<"authorized" | "consumed" | "expired" | "denied" | "idle">(
		"idle"
	);
	let consumedByLabel = $state<string | null>(null);
	let pollHandle: ReturnType<typeof setInterval> | null = null;

	async function revoke(device: Device) {
		const ok = await confirmAction({
			title: device.is_current ? 'Revoke this device?' : `Revoke "${device.label}"?`,
			body: device.is_current
				? `${device.label} is the device you're using. You'll be signed out immediately.`
				: 'It loses access to the box right away.',
			confirmLabel: 'Revoke',
			danger: true,
		});
		if (!ok) return;

		try {
			const resp = await fetch(`/api/devices/${device.id}`, { method: "DELETE" });
			if (resp.status === 409) {
				toast.error("Cannot revoke the only active device", {
					description:
						"Run `virtues sudo` on the box to confirm before deleting your last paired device.",
				});
				return;
			}
			if (!resp.ok) {
				const data = await resp.json().catch(() => ({}));
				toast.error("Revoke failed", { description: data.error ?? `HTTP ${resp.status}` });
				return;
			}
			toast.success("Device revoked");
			if (device.is_current) {
				// We just revoked our own access — hand back to pairing.
				returnToPairing();
				return;
			}
			await res.reload();
		} catch (e) {
			toast.error("Revoke failed", {
				description: e instanceof Error ? e.message : "Network error",
			});
		}
	}

	function stopPolling() {
		if (pollHandle) {
			clearInterval(pollHandle);
			pollHandle = null;
		}
	}

	async function startAdd() {
		addOpen = true;
		mintError = null;
		mintedToken = null;
		mintedUrl = null;
		mintedTokenId = null;
		mintedExpiresAt = null;
		pairStatus = "idle";
		consumedByLabel = null;
		mintLoading = true;
		try {
			// No intended_kind. This minted "Add device" without one is redeemed
			// by whatever scans it — a phone, a Mac, the CLI — and the device
			// declares its own kind at consume. Naming one here meant naming
			// "browser", which the token's CHECK constraint rejects, so this
			// button failed on every click with `mint_failed`.
			const data = await pairMint();
			mintedToken = data.token;
			mintedUrl = data.pair_url;
			mintedQrSvg = data.qr_svg;
			mintedTokenId = data.id;
			mintedExpiresAt = data.expires_at;
			// Authenticated mints are minted `authorized` (see mint_pair_token) —
			// there is no confirm round-trip to wait through.
			pairStatus = "authorized";
			pollHandle = setInterval(pollStatus, 2000);
		} catch (e) {
			mintError = e instanceof Error ? e.message : "Could not mint pair token";
		} finally {
			mintLoading = false;
		}
	}

	async function denyPair() {
		if (!mintedTokenId) return;
		await pairDeny(mintedTokenId);
		pairStatus = "denied";
		stopPolling();
	}

	async function pollStatus() {
		if (!mintedTokenId) return;
		try {
			const data = await pairStatusApi(mintedTokenId);
			pairStatus = data.status as typeof pairStatus;
			if (data.consumed_by_label) {
				consumedByLabel = data.consumed_by_label;
			}
			if (
				data.status === "consumed" ||
				data.status === "expired" ||
				data.status === "denied"
			) {
				stopPolling();
				if (data.status === "consumed") {
					toast.success("New device paired");
					await res.reload();
				}
			}
		} catch {
			/* swallow */
		}
	}

	function closeAdd() {
		stopPolling();
		addOpen = false;
	}

	function copyUrl() {
		if (mintedUrl) {
			navigator.clipboard.writeText(mintedUrl);
			toast.success("URL copied");
		}
	}

	/// A denied macOS permission, in the owner's terms: what it costs and how to
	/// fix it. The collector reports raw capability names; a name alone ("
	/// accessibility") tells you nothing about what stopped working.
	const PERMISSION_COPY: Record<string, { label: string; costs: string }> = {
		full_disk_access: {
			label: "Full Disk Access",
			costs: "iMessages and Safari history can't be read"
		},
		accessibility: {
			label: "Accessibility",
			costs: "app events are recorded without window titles"
		}
	};

	function deniedPermissions(device: Device) {
		return (device.permissions?.denied ?? []).map(
			(name) => PERMISSION_COPY[name] ?? { label: name, costs: "some data can't be read" }
		);
	}

	function kindLabel(k: Device["kind"]) {
		switch (k) {
			case "mobile_app":
				return "Mobile";
			case "desktop_app":
				return "Desktop";
			case "sensor":
				return "Sensor";
			case "cli":
				return "CLI";
		}
	}

	function kindIcon(k: Device["kind"]) {
		switch (k) {
			case "mobile_app":
				return "ri:smartphone-line";
			case "desktop_app":
				return "ri:macbook-line";
			case "sensor":
				return "ri:cpu-line";
			case "cli":
				return "ri:terminal-line";
		}
	}

</script>

<!--
	The page's own heading, not a hand-rolled one. This view used to draw its
	own `<h1 class="text-2xl font-semibold">` inside a bare `<Page>` with its own
	measure and padding — so Devices was a sans 24px heading in a 3xl column
	while every neighbouring room was a serif 30px heading in a 6xl one, for no
	reason anyone chose.
-->
<Page
	title="Devices"
	description="Every browser, app, and sensor paired with this box."
	maxWidth="wide"
>
	{#snippet actions()}
		<Button variant="primary" onclick={startAdd}>
			<Icon icon="ri:add-line" />
			Add device
		</Button>
	{/snippet}

	{#if res.loading}
		<LoadingState />
	{:else if res.error}
		<ErrorState message={res.error} onRetry={res.reload} />
	{:else if devices.length === 0}
		<EmptyState
			icon="ri:device-line"
			title="No paired devices"
			message="Run `virtues pair` on the box to pair this browser, or click Add device above."
		/>
	{:else}
		<ul class="divide-y divide-border rounded-lg border border-border bg-surface">
			{#each devices as device (device.id)}
				<li class="p-4 flex items-start gap-4">
					<div
						class="flex-shrink-0 w-10 h-10 rounded-lg bg-surface-alt border border-border flex items-center justify-center"
					>
						<Icon icon={kindIcon(device.kind)} class="text-foreground-muted text-lg" />
					</div>
					<div class="flex-1 min-w-0">
						<div class="flex items-center gap-2 flex-wrap">
							<span class="font-medium text-foreground truncate">{device.label}</span>
							<Badge>{kindLabel(device.kind)}</Badge>
							{#if device.is_current}
								<Badge>This device</Badge>
							{/if}
						</div>
						<div class="text-xs text-foreground-muted mt-1 flex flex-wrap gap-x-3 gap-y-1">
							{#if device.version}
								<span class="font-mono text-foreground"
									>{device.version}{device.sha && device.sha !== "dev"
										? ` · ${device.sha}`
										: ""}{device.channel ? ` · ${device.channel}` : ""}</span
								>
							{:else}
								<span class="italic">version unknown</span>
							{/if}
							<span>Last seen {formatTimeAgo(device.last_seen_at)}</span>
							<span>Paired {formatTimeAgo(device.paired_at)}</span>
							{#if device.paired_from_ip}
								<span>from {device.paired_from_ip}</span>
							{/if}
						</div>
						{#each deniedPermissions(device) as perm}
							<!-- A collector missing a permission isn't an error — nothing
							     crashed, and the rest of its streams are fine. It's a
							     capability the box has been quietly denied, so it reads as
							     a standing warning with the remedy attached, not a toast
							     that can be dismissed and forgotten. -->
							<div
								class="mt-2 flex items-start gap-2 rounded-md border border-warning/40 bg-warning/10 px-3 py-2 text-xs"
							>
								<Icon icon="ri:lock-line" class="text-warning mt-0.5 flex-shrink-0" />
								<div class="min-w-0">
									<span class="text-foreground font-medium">{perm.label} is off</span>
									<span class="text-foreground-muted"> — {perm.costs}.</span>
									<div class="text-foreground-muted mt-0.5">
										Grant it in System Settings → Privacy &amp; Security → {perm.label}, then
										restart the collector.
									</div>
								</div>
							</div>
						{/each}
						{#if device.permissions?.stale}
							<div class="text-xs text-foreground-muted mt-2 italic">
								Permission report is stale — the collector may not be running.
							</div>
						{/if}
					</div>
					<Button variant="ghost" onclick={() => revoke(device)}>
						<Icon icon="ri:close-circle-line" />
						Revoke
					</Button>
				</li>
			{/each}
		</ul>
	{/if}
</Page>

{#if addOpen}
	<!-- Modal backdrop. Click outside dismisses; the inner stops propagation. -->
	<div
		class="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm px-4"
		onclick={closeAdd}
		onkeydown={(e) => e.key === "Escape" && closeAdd()}
		role="dialog"
		tabindex="-1"
	>
		<div
			class="w-full max-w-md rounded-xl bg-surface border border-border shadow-xl p-6"
			onclick={(e) => e.stopPropagation()}
			onkeydown={(e) => e.stopPropagation()}
			role="document"
		>
			<div class="flex items-center justify-between mb-4">
				<h2 class="text-lg font-semibold">Add a device</h2>
				<button
					onclick={closeAdd}
					class="text-foreground-muted hover:text-foreground"
					aria-label="Close"
				>
					<Icon icon="ri:close-line" />
				</button>
			</div>

			{#if mintLoading}
				<LoadingState />
			{:else if mintError}
				<ErrorState message={mintError} />
			{:else if mintedUrl}
				<div class="space-y-4">
					{#if pairStatus === "authorized"}
						<div class="rounded-lg bg-surface-alt border border-border p-3 text-sm">
							<div class="flex items-center gap-2">
								<Icon icon="ri:loader-4-line" class="animate-spin text-foreground-muted" />
								<span class="text-foreground-muted">
									Waiting for the new device to open the link…
								</span>
							</div>
						</div>

						<div
							class="rounded-lg border border-border bg-white p-4 flex items-center justify-center"
						>
							{#if mintedQrSvg}
								<!-- Rendered server-side; the SVG is fully self-contained,
								     never touches a third party. -->
								<div class="w-56 h-56 [&_svg]:w-full [&_svg]:h-full">
									{@html mintedQrSvg}
								</div>
							{/if}
						</div>

						<div>
							<div class="text-xs text-foreground-muted mb-1">
								Or open this URL on the new device:
							</div>
							<div class="flex gap-2">
								<input
									readonly
									value={mintedUrl}
									class="flex-1 text-xs font-mono px-2 py-1.5 rounded border border-border bg-surface"
								/>
								<Button variant="ghost" onclick={copyUrl}>Copy</Button>
							</div>
						</div>
					{:else if pairStatus === "consumed"}
						<div class="rounded-lg bg-surface-alt border border-border p-4 text-sm flex items-start gap-3">
							<Icon icon="ri:check-line" class="text-success mt-0.5" />
							<div>
								<div class="font-medium">Device paired</div>
								{#if consumedByLabel}
									<div class="text-foreground-muted text-xs mt-0.5">
										{consumedByLabel}
									</div>
								{/if}
							</div>
						</div>
						<Button variant="primary" onclick={closeAdd} class="w-full">Done</Button>
					{:else if pairStatus === "expired"}
						<ErrorState message="This pair token expired. Close and try again." />
						<Button variant="ghost" onclick={closeAdd} class="w-full">Close</Button>
					{:else if pairStatus === "denied"}
						<div class="text-sm text-foreground-muted">Pair denied.</div>
						<Button variant="ghost" onclick={closeAdd} class="w-full">Close</Button>
					{/if}
				</div>
			{/if}
		</div>
	</div>
{/if}
