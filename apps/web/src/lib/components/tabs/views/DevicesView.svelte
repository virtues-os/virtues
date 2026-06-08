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
	import { onMount } from "svelte";
	import { toast } from "svelte-sonner";

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	type Device = {
		id: string;
		kind: "browser" | "mobile_app" | "desktop_app" | "sensor" | "cli";
		label: string;
		paired_at: string;
		last_seen_at: string | null;
		paired_from_ip: string | null;
		is_current: boolean;
	};

	let devices = $state<Device[]>([]);
	let loading = $state(true);
	let errorMessage = $state<string | null>(null);

	// "+ Add device" modal state.
	let addOpen = $state(false);
	let mintLoading = $state(false);
	let mintError = $state<string | null>(null);
	let mintedToken = $state<string | null>(null);
	let mintedUrl = $state<string | null>(null);
	let mintedQrSvg = $state<string | null>(null);
	let mintedTokenId = $state<string | null>(null);
	let mintedExpiresAt = $state<string | null>(null);
	let pairStatus = $state<"pending" | "authorized" | "consumed" | "expired" | "denied" | "idle">(
		"idle"
	);
	let consumedByLabel = $state<string | null>(null);
	let pollHandle: ReturnType<typeof setInterval> | null = null;

	onMount(load);

	async function load() {
		loading = true;
		errorMessage = null;
		try {
			const resp = await fetch("/api/devices");
			if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
			const data = await resp.json();
			devices = data.devices ?? [];
		} catch (e) {
			errorMessage = e instanceof Error ? e.message : "Failed to load devices";
		} finally {
			loading = false;
		}
	}

	async function revoke(device: Device) {
		const confirmText = device.is_current
			? `Revoke THIS device? You will be signed out immediately.\n\n${device.label}`
			: `Revoke "${device.label}"? It will lose access to the box right away.`;
		if (!window.confirm(confirmText)) return;

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
				// We just nuked our own session — go to /pair.
				window.location.href = "/pair";
				return;
			}
			await load();
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
			const resp = await fetch("/api/pair/mint", {
				method: "POST",
				headers: { "Content-Type": "application/json" },
				body: JSON.stringify({ intended_kind: "browser" }),
			});
			if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
			const data = await resp.json();
			mintedToken = data.token;
			mintedUrl = data.pair_url;
			mintedQrSvg = data.qr_svg;
			mintedTokenId = data.id;
			mintedExpiresAt = data.expires_at;
			pairStatus = "pending";
			pollHandle = setInterval(pollStatus, 2000);
		} catch (e) {
			mintError = e instanceof Error ? e.message : "Could not mint pair token";
		} finally {
			mintLoading = false;
		}
	}

	async function confirmPair() {
		if (!mintedTokenId) return;
		try {
			const resp = await fetch(`/api/pair/confirm/${mintedTokenId}`, { method: "POST" });
			if (!resp.ok) {
				const data = await resp.json().catch(() => ({}));
				toast.error("Confirmation failed", {
					description: data.error ?? `HTTP ${resp.status}`,
				});
				return;
			}
			pairStatus = "authorized";
		} catch (e) {
			toast.error("Confirmation failed", {
				description: e instanceof Error ? e.message : "Network error",
			});
		}
	}

	async function denyPair() {
		if (!mintedTokenId) return;
		try {
			await fetch(`/api/pair/deny/${mintedTokenId}`, { method: "POST" });
		} catch {
			/* best effort */
		}
		pairStatus = "denied";
		stopPolling();
	}

	async function pollStatus() {
		if (!mintedTokenId) return;
		try {
			const resp = await fetch(`/api/pair/status/${mintedTokenId}`);
			if (!resp.ok) return;
			const data = await resp.json();
			pairStatus = data.status;
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
					await load();
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

	function kindLabel(k: Device["kind"]) {
		switch (k) {
			case "browser":
				return "Browser";
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
			case "browser":
				return "ri:window-line";
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

	function timeAgo(iso: string | null) {
		if (!iso) return "—";
		const then = new Date(iso).getTime();
		const now = Date.now();
		const sec = Math.max(0, Math.floor((now - then) / 1000));
		if (sec < 60) return "just now";
		const min = Math.floor(sec / 60);
		if (min < 60) return `${min}m ago`;
		const hr = Math.floor(min / 60);
		if (hr < 24) return `${hr}h ago`;
		const d = Math.floor(hr / 24);
		return `${d}d ago`;
	}
</script>

<Page>
	<div class="px-6 py-6 max-w-3xl mx-auto w-full">
		<div class="flex items-baseline justify-between mb-6">
			<div>
				<h1 class="text-2xl font-semibold tracking-tight">Devices</h1>
				<p class="text-sm text-foreground-muted mt-1">
					Every browser, app, and sensor paired with this box.
				</p>
			</div>
			<Button variant="primary" onclick={startAdd}>
				<Icon icon="ri:add-line" />
				Add device
			</Button>
		</div>

		{#if loading}
			<LoadingState />
		{:else if errorMessage}
			<ErrorState message={errorMessage} />
		{:else if devices.length === 0}
			<EmptyState
				icon="ri:device-line"
				title="No paired devices"
				message="Run `virtues link` on the box to pair this browser, or click Add device above."
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
								<span>Last seen {timeAgo(device.last_seen_at)}</span>
								<span>Paired {timeAgo(device.paired_at)}</span>
								{#if device.paired_from_ip}
									<span>from {device.paired_from_ip}</span>
								{/if}
							</div>
						</div>
						<Button variant="ghost" onclick={() => revoke(device)}>
							<Icon icon="ri:close-circle-line" />
							Revoke
						</Button>
					</li>
				{/each}
			</ul>
		{/if}
	</div>
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
					{#if pairStatus === "pending"}
						<div class="rounded-lg bg-surface-alt border border-border p-3 text-sm">
							<div class="font-medium mb-1">Before we hand out access…</div>
							<p class="text-foreground-muted text-xs">
								You're about to pair a new device. Confirm to authorize the QR
								below, then open the URL on the new device.
							</p>
							<div class="flex gap-2 mt-3">
								<Button variant="primary" onclick={confirmPair}>
									<Icon icon="ri:check-line" /> Confirm
								</Button>
								<Button variant="ghost" onclick={denyPair}>Cancel</Button>
							</div>
						</div>
					{:else if pairStatus === "authorized"}
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
