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
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import ReopenSection from "$lib/components/settings/ReopenSection.svelte";
	import {
		kindLabel,
		kindIcon,
		deniedPermissions,
		deviceHref,
		revokeDeviceFlow,
		type Device,
		type DevicesResponse,
	} from "$lib/devices/shared";

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	const res = createResource(() => listDevices<DevicesResponse>());

	// The device you are holding, first. The box orders by last-seen, which is
	// *usually* the same thing and reliably isn't when you have just opened the
	// app on a second machine — and the current device is both the one whose
	// warnings you can act on and the one people look for first.
	const devices = $derived(
		[...(res.data?.devices ?? [])].sort(
			(a, b) => Number(b.is_current) - Number(a.is_current),
		),
	);

	function openDevice(device: Device) {
		windowShellStore.navigate(deviceHref(device), { label: "Settings" });
	}

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
		if (await revokeDeviceFlow(device)) await res.reload();
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

	// Shown whenever we ARE a Mac app, not only on the `is_current` row.
	//
	// One Mac appears as TWO devices — the app ("Virtues Desktop", is_current)
	// and the collector (the .local hostname, is_current false) — and it is the
	// COLLECTOR that reports permissions. Gating on is_current therefore hid the
	// button behind a row that never carries a denial, so the fix shipped inert
	// (caught on live data 2026-08-13).
	//
	// The cost of relaxing it: someone with two Macs sees the button on the
	// other Mac's row too, where it opens the wrong machine's settings. Rare,
	// recoverable, and better than a control that cannot appear at all — but it
	// is why the label names this Mac rather than the device in the row.
	const canFix = $derived(isTauri);
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
						<!--
							The identity block is the drill-down; the warning banners and
							Revoke below stay their own controls. Deliberately NOT a
							click handler on the whole <li> — a row that also contains
							buttons cannot be a button, and the banner's "Open Full Disk
							Access" would then fire a navigation behind itself.
						-->
						<button class="open-btn" onclick={() => openDevice(device)}>
							<div class="flex items-center gap-2 flex-wrap">
								<span class="font-medium text-foreground truncate">{device.label}</span>
								<Badge>{kindLabel(device.kind)}</Badge>
								{#if device.is_current}
									<Badge>This device</Badge>
								{/if}
								<Icon icon="ri:arrow-right-s-line" class="chevron" />
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
						</button>
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
									{#if canFix && perm.open}
										<div class="text-foreground-muted mt-0.5">
											<!-- No "restart the collector". It re-checks on its own every
											     few minutes, so that instruction was jargon AND untrue —
											     it asked for work that was never needed. -->
											Turn on <span class="text-foreground">Virtues</span> in the list,
											then leave it — this Mac notices within a few minutes.
										</div>
										<button class="fix-btn mt-2" onclick={() => perm.open?.()}>
											<Icon icon="ri:external-link-line" width="13" />
											Open {perm.label} on this Mac
										</button>
									{:else}
										<!-- Was unconditional, so a browser on a phone got told to
										     "turn on Virtues in the list" and that "this Mac notices
										     within a few minutes" — instructions for a machine the
										     reader is not at, and with no button beneath them,
										     because macOS forbids granting these remotely. Same
										     conditional the device page uses: two screens showing
										     one fact must not disagree about whether it is
										     actionable from here. -->
										<div class="text-foreground-muted mt-0.5">
											Granting this needs someone at that machine — macOS has no
											remote path for it.
										</div>
									{/if}
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

	<!-- Revoke, in the plural. It lived at the foot of "Box" until 2026-08-17,
	     below the CPU graphs, which put the button that signs out every device
	     you own on a page you open to read a temperature. Its subject was
	     always this one: the same verb as the Revoke buttons above, applied to
	     all of them at once. -->
	<ReopenSection />
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

<style>
	/* Reads as text until you reach for it. The row is already dense with
	   badges, versions and timestamps; a button chrome around the name would
	   make the list look like a toolbar. */
	.open-btn {
		display: block;
		width: 100%;
		padding: 0;
		border: 0;
		background: none;
		font: inherit;
		text-align: left;
		cursor: pointer;
	}

	/* A real ring, not just the chevron. Making the row's identity block a
	   button is what put keyboard users in this list at all — fading in a 14px
	   glyph is not an indication of where you are. Matches the outline the rest
	   of the app uses (see NotebookDetailView's .ctrl-add). */
	.open-btn:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: 3px;
		border-radius: 4px;
	}

	.open-btn :global(.chevron) {
		opacity: 0;
		color: var(--color-foreground-subtle);
		transition: opacity 120ms ease;
	}

	.open-btn:hover :global(.chevron),
	.open-btn:focus-visible :global(.chevron) {
		opacity: 1;
	}

	.fix-btn {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		padding: 3px 9px;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		background: none;
		cursor: pointer;
		font-size: 12px;
		color: var(--color-foreground-muted);
	}

	.fix-btn:hover {
		background: color-mix(in srgb, var(--color-foreground) 8%, transparent);
		color: var(--color-foreground);
	}
</style>
