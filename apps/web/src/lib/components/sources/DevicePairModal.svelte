<script lang="ts">
	/**
	 * DevicePairModal - Handles device pairing for iOS and Mac.
	 * iOS: Shows QR code (primary) + manual device ID entry (fallback)
	 * Mac: Shows pairing code for entry in the Mac app
	 */
	import { onDestroy } from "svelte";
	import Modal from "$lib/components/Modal.svelte";
	import { Button } from "$lib";
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
	// Shared polling state (used by both iOS QR and Mac flows)
	let pairingData = $state<PairingInitResponse | null>(null);
	let isInitiating = $state(false);
	let isPolling = $state(false);
	let hasTimedOut = $state(false);
	let timeRemaining = $state(600);
	/** True once the credential has been finalized as `active` server-side.
	 *  Distinguishes "modal closing because we paired" from "modal closing
	 *  because the user cancelled mid-flow". The latter triggers a hard-delete
	 *  of the still-pending credential row. */
	let pairingSucceeded = $state(false);
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
				pairingSucceeded = true;
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

		// Client-side 10 minute timer (server enforces actual expiry)
		timeRemaining = 600;

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

		// Cancel mid-flow: hard-delete the pending credential the server minted
		// at pair_initiate, otherwise it sits in the DB as a stale `pending`
		// row and surfaces in the credentials list. Backend's smart DELETE
		// dispatches by status, so this is a no-op for already-active rows.
		const pendingId = pairingData?.source_id || qrSourceId;
		if (pendingId && !pairingSucceeded) {
			void api.revokeCredential(pendingId).catch(() => {
				/* benign — row may have been finalized in a race */
			});
		}

		onClose();
	}

	// Reset success flag whenever the modal re-opens for a fresh pair.
	$effect(() => {
		if (open) pairingSucceeded = false;
	});

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
							Enter this device ID in the Virtues Mac app:
						</p>
						<div class="font-mono text-xl font-medium tracking-wide text-foreground py-4 break-all">
							{pairingData.source_id}
						</div>
						<p class="text-xs text-foreground-subtle mb-2">
							Expires in {formatTime(timeRemaining)}
						</p>
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
