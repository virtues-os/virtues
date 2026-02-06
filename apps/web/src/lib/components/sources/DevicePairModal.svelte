<script lang="ts">
	/**
	 * DevicePairModal - Handles device pairing for iOS and Mac.
	 * iOS: Shows QR code (primary) + manual device ID entry (fallback)
	 * Mac: Shows pairing code for entry in the Mac app
	 */
	import { onDestroy } from "svelte";
	import Modal from "$lib/components/Modal.svelte";
	import { Button, Input } from "$lib";
	import * as api from "$lib/api/client";
	import type { PairingInitResponse } from "$lib/types/device-pairing";
	import QRCode from "qrcode";

	interface Props {
		deviceType: "ios" | "mac";
		displayName: string;
		open: boolean;
		onClose: () => void;
		onSuccess: (sourceId: string) => void;
	}

	let { deviceType, displayName, open, onClose, onSuccess }: Props = $props();

	// Shared state
	let error = $state<string | null>(null);
	let apiEndpoint = $state("");
	let isLoadingEndpoint = $state(true);

	// iOS QR pairing state
	let qrDataUrl = $state<string>("");
	let qrSourceId = $state<string>("");
	let isGeneratingQR = $state(false);
	let showManualEntry = $state(false);

	// iOS manual fallback state
	let deviceId = $state("");
	let isLinking = $state(false);

	// Shared polling state (used by both iOS QR and Mac flows)
	let pairingData = $state<PairingInitResponse | null>(null);
	let isInitiating = $state(false);
	let isPolling = $state(false);
	let hasTimedOut = $state(false);
	let timeRemaining = $state(600);
	let timerInterval: ReturnType<typeof setInterval> | null = null;
	let pollInterval: ReturnType<typeof setInterval> | null = null;

	// Fetch server endpoint on mount
	$effect(() => {
		if (typeof window !== "undefined" && open) {
			fetch("/api/app/server-info")
				.then((r) => r.json())
				.then((data) => {
					apiEndpoint = data.apiEndpoint;
					isLoadingEndpoint = false;
				})
				.catch(() => {
					apiEndpoint = `${window.location.origin}/api`;
					isLoadingEndpoint = false;
				});
		}
	});

	// Start iOS QR pairing or Mac pairing when modal opens
	$effect(() => {
		if (open && deviceType === "ios" && !pairingData && !isInitiating && !qrSourceId) {
			initiateQRPairing();
		}
		if (open && deviceType === "mac" && !pairingData && !isInitiating) {
			initiateMacPairing();
		}
	});

	// Generate QR code once we have both endpoint and source_id
	$effect(() => {
		if (qrSourceId && apiEndpoint && !isLoadingEndpoint && !qrDataUrl) {
			generateQRCode(apiEndpoint, qrSourceId);
		}
	});

	// --- iOS QR Flow ---

	async function initiateQRPairing() {
		isInitiating = true;
		error = null;

		try {
			const response = await api.initiatePairing(deviceType, displayName);
			pairingData = response;
			qrSourceId = response.source_id;
			startPolling();
			// Client-side 10 minute timer (server enforces actual expiry)
			timeRemaining = 600;
			startTimer();
		} catch (err) {
			error = err instanceof Error ? err.message : "Failed to initiate pairing";
		} finally {
			isInitiating = false;
		}
	}

	async function generateQRCode(endpoint: string, sourceId: string) {
		isGeneratingQR = true;
		try {
			// Strip /api suffix — the QR payload should be the root server URL
			const root = endpoint.replace(/\/api\/?$/, "");
			const payload = JSON.stringify({ e: root, s: sourceId });
			qrDataUrl = await QRCode.toDataURL(payload, {
				width: 240,
				margin: 2,
				errorCorrectionLevel: "M",
				color: { dark: "#26251E", light: "#FFFFFF" },
			});
		} catch (err) {
			console.error("Failed to generate QR code:", err);
			error = "Failed to generate QR code";
		} finally {
			isGeneratingQR = false;
		}
	}

	function retryQRPairing() {
		hasTimedOut = false;
		pairingData = null;
		qrSourceId = "";
		qrDataUrl = "";
		error = null;
		timeRemaining = 600;
		initiateQRPairing();
	}

	// --- iOS Manual Fallback ---

	let isValidId = $derived(
		/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(
			deviceId.trim(),
		),
	);

	async function handleiOSLink() {
		if (!isValidId) {
			error = "Please enter a valid Device ID (UUID)";
			return;
		}

		isLinking = true;
		error = null;

		try {
			const res = await fetch("/api/devices/pairing/link", {
				method: "POST",
				headers: { "Content-Type": "application/json" },
				body: JSON.stringify({
					device_id: deviceId.trim(),
					name: displayName || "My Device",
					device_type: deviceType,
				}),
			});

			if (!res.ok) {
				const data = await res.json();
				throw new Error(data.error || "Linking failed");
			}

			const data = await res.json();
			onSuccess(data.source_id);
			onClose();
		} catch (err) {
			error = err instanceof Error ? err.message : "Linking failed";
		} finally {
			isLinking = false;
		}
	}

	// --- Mac Flow ---

	async function initiateMacPairing() {
		isInitiating = true;
		error = null;

		try {
			const response = await api.initiatePairing(deviceType, displayName);
			pairingData = response;
			startPolling();
			startTimer();
		} catch (err) {
			error = err instanceof Error ? err.message : "Failed to initiate pairing";
		} finally {
			isInitiating = false;
		}
	}

	async function copyEndpoint() {
		try {
			await navigator.clipboard.writeText(apiEndpoint);
		} catch (err) {
			console.error("Failed to copy endpoint:", err);
		}
	}

	async function copyCode() {
		if (!pairingData) return;
		try {
			await navigator.clipboard.writeText(pairingData.code);
		} catch (err) {
			console.error("Failed to copy code:", err);
		}
	}

	function retryMacPairing() {
		hasTimedOut = false;
		pairingData = null;
		error = null;
		timeRemaining = 600;
		initiateMacPairing();
	}

	// --- Shared Polling & Timer ---

	async function checkPairingStatus() {
		const sourceId = pairingData?.source_id || qrSourceId;
		if (!sourceId) return;

		try {
			const status = await api.getPairingStatus(sourceId);

			if (status.status === "active") {
				stopPolling();
				stopTimer();
				onSuccess(sourceId);
				onClose();
			} else if (status.status === "revoked") {
				error = "Pairing was cancelled";
				stopPolling();
				stopTimer();
			}
		} catch (err) {
			console.error("Failed to check pairing status:", err);
		}
	}

	function startPolling() {
		if (pollInterval) return;
		isPolling = true;
		checkPairingStatus();
		pollInterval = setInterval(checkPairingStatus, 2000);
	}

	function stopPolling() {
		if (pollInterval) {
			clearInterval(pollInterval);
			pollInterval = null;
		}
		isPolling = false;
	}

	function startTimer() {
		if (timerInterval) return;

		// For Mac flow, use server-provided expiry; for iOS QR, use client-side 10min
		if (deviceType === "mac" && pairingData?.expires_at) {
			const expiresAt = new Date(pairingData.expires_at);
			const now = new Date();
			const secondsRemaining = Math.floor(
				(expiresAt.getTime() - now.getTime()) / 1000,
			);
			timeRemaining = Math.max(0, secondsRemaining);
		}

		timerInterval = setInterval(() => {
			timeRemaining--;
			if (timeRemaining <= 0) {
				hasTimedOut = true;
				stopTimer();
				stopPolling();
			}
		}, 1000);
	}

	function stopTimer() {
		if (timerInterval) {
			clearInterval(timerInterval);
			timerInterval = null;
		}
	}

	function formatTime(seconds: number): string {
		const mins = Math.floor(seconds / 60);
		const secs = seconds % 60;
		return `${mins}:${secs.toString().padStart(2, "0")}`;
	}

	function handleClose() {
		stopPolling();
		stopTimer();
		onClose();
	}

	onDestroy(() => {
		stopPolling();
		stopTimer();
	});
</script>

<Modal open={open} onClose={handleClose} title="Connect {displayName}" width="md">
	{#if deviceType === "ios"}
		<!-- iOS Flow: QR Code Primary + Manual Fallback -->
		<div class="space-y-5">

			{#if hasTimedOut}
				<!-- Expired state -->
				<div class="text-center py-6">
					<p class="font-serif text-lg text-foreground mb-2">QR Code Expired</p>
					<p class="text-sm text-foreground-muted mb-6">
						The pairing session timed out. No device connected.
					</p>
					<div class="flex justify-center gap-4">
						<Button variant="ghost" onclick={handleClose}>Cancel</Button>
						<Button variant="primary" onclick={retryQRPairing}>Generate New Code</Button>
					</div>
				</div>

			{:else}
				<!-- QR Code pairing -->
				<div class="flex flex-col items-center text-center">
					<div class="mb-4">
						<p class="text-sm text-foreground-muted mb-1">
							Open the Virtues app on your iPhone and tap <strong>Scan QR Code</strong>
						</p>
					</div>

					<!-- QR Code -->
					<div class="bg-white rounded-xl p-4 shadow-sm border border-border mb-3">
						{#if isGeneratingQR || isInitiating || isLoadingEndpoint}
							<div class="w-[240px] h-[240px] flex items-center justify-center">
								<p class="text-sm text-foreground-muted">Generating...</p>
							</div>
						{:else if qrDataUrl}
							<img src={qrDataUrl} alt="Pairing QR Code" class="w-[240px] h-[240px]" />
						{:else}
							<div class="w-[240px] h-[240px] flex items-center justify-center">
								<p class="text-sm text-error">Failed to generate QR</p>
							</div>
						{/if}
					</div>

					<!-- Status -->
					<div class="flex items-center gap-2 text-sm text-foreground-muted">
						{#if isPolling}
							<span class="inline-block w-2 h-2 bg-primary rounded-full animate-pulse"></span>
							<span>Waiting for device...</span>
							<span class="text-foreground-subtle">{formatTime(timeRemaining)}</span>
						{/if}
					</div>
				</div>

				{#if error}
					<div class="p-3 bg-error-subtle border border-error rounded-lg">
						<p class="text-sm text-error">{error}</p>
					</div>
				{/if}

				<!-- Manual entry fallback (collapsed) -->
				<details bind:open={showManualEntry}>
					<summary class="text-sm text-foreground-muted cursor-pointer hover:text-foreground select-none py-1">
						Enter manually
					</summary>

					<div class="mt-4 space-y-4 pt-4 border-t border-border">
						<div>
							<p class="text-xs text-foreground-subtle mb-2">Server endpoint:</p>
							<div class="flex items-center gap-2 bg-surface p-2 rounded border border-border">
								<code class="text-xs font-mono text-foreground break-all flex-1">
									{isLoadingEndpoint ? "Loading..." : apiEndpoint}
								</code>
								<button
									class="text-xs text-primary hover:underline whitespace-nowrap"
									onclick={copyEndpoint}
									type="button"
								>
									Copy
								</button>
							</div>
						</div>

						<div>
							<label for="device-id" class="block text-sm text-foreground-muted mb-2">
								Device ID (UUID)
							</label>
							<Input
								id="device-id"
								type="text"
								bind:value={deviceId}
								placeholder="e.g. 123e4567-e89b-..."
								class="font-mono"
								disabled={isLinking}
							/>
						</div>

						<div class="flex justify-end gap-3">
							<Button
								variant="primary"
								onclick={handleiOSLink}
								disabled={!isValidId || isLinking}
							>
								{isLinking ? "Linking..." : "Link Device"}
							</Button>
						</div>
					</div>
				</details>

				<!-- Cancel -->
				<div class="flex justify-end pt-2 border-t border-border">
					<Button variant="ghost" onclick={handleClose}>Cancel</Button>
				</div>
			{/if}
		</div>

	{:else}
		<!-- Mac Flow: Pairing Code (unchanged) -->
		<div class="text-center py-4">
			{#if error}
				<p class="text-sm text-error mb-4">{error}</p>
			{/if}

			{#if isInitiating}
				<p class="text-foreground-muted py-8">Generating pairing code...</p>
			{:else if hasTimedOut}
				<div class="py-4">
					<p class="font-serif text-lg text-foreground mb-2">Code Expired</p>
					<p class="text-sm text-foreground-muted mb-6">
						The pairing code expired. No device connected.
					</p>
					<div class="flex justify-center gap-4">
						<Button variant="ghost" onclick={handleClose}>Cancel</Button>
						<Button variant="primary" onclick={retryMacPairing}>Try Again</Button>
					</div>
				</div>
			{:else if pairingData}
				<div class="space-y-6">
					<div>
						<p class="text-sm text-foreground-muted mb-4">
							Enter this code in the Virtues Mac app:
						</p>
						<div class="font-mono text-3xl font-medium tracking-widest text-foreground py-4">
							{pairingData.code}
						</div>
						<p class="text-xs text-foreground-subtle mb-2">
							Code expires in {formatTime(timeRemaining)}
						</p>
						<button
							class="text-sm text-foreground-muted hover:text-foreground underline"
							onclick={copyCode}
						>
							Copy code
						</button>
					</div>

					<div class="pt-4 border-t border-border">
						<p class="text-xs text-foreground-subtle mb-2">Server endpoint:</p>
						<div class="flex items-center justify-center gap-2">
							<code class="text-xs font-mono text-foreground">
								{isLoadingEndpoint ? "Loading..." : apiEndpoint}
							</code>
							<button
								class="text-xs text-primary hover:underline"
								onclick={copyEndpoint}
								disabled={isLoadingEndpoint}
							>
								Copy
							</button>
						</div>
					</div>

					{#if isPolling}
						<p class="text-sm text-foreground-muted">Waiting for device...</p>
					{/if}

					<div class="pt-4">
						<Button variant="ghost" onclick={handleClose}>Cancel</Button>
					</div>
				</div>
			{/if}
		</div>
	{/if}
</Modal>
