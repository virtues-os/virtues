<script lang="ts">
	import { openExternal } from "$lib/tauri/bridge";

	/** Canonical app download page — see docs/deployment.md. */
	const DOWNLOADS_URL = "https://virtues.com/downloads";
	/**
	 * DevicePairModal - Handles device pairing for iOS and Mac.
	 * iOS: Shows QR code (primary) + manual device ID entry (fallback)
	 * Mac: Shows pairing code for entry in the Mac app
	 */
	import { onDestroy } from "svelte";
	import Modal from "$lib/components/Modal.svelte";
	import Icon from "$lib/components/Icon.svelte";
	import { Button } from "$lib";
	import * as api from "$lib/api/client";
	import type { PairingInitResponse } from "$lib/types/device-pairing";

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

	// iOS QR pairing: the phone scans a QR of the box's `/pair#t=<token>` URL
	// while on the same network and consumes it over the LAN — the same
	// mint→consume flow as the Mac code, just rendered as a QR instead of digits.
	// The QR carries no secrets (only the pair URL), so cancelling simply denies
	// the pending token — nothing to revoke.
	let qrSvg = $state<string>("");
	let qrSourceId = $state<string>("");
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


	// Start iOS QR pairing or Mac pairing when modal opens
	$effect(() => {
		if (open && deviceType === "ios" && !pairingData && !isInitiating && !qrSourceId) {
			initiateQRPairing();
		}
		if (open && deviceType === "mac" && !pairingData && !isInitiating) {
			initiateMacPairing();
		}
	});

	// --- iOS QR Flow ---

	async function initiateQRPairing() {
		isInitiating = true;
		error = null;

		try {
			const response = await api.initiatePairing("ios", displayName);
			pairingData = response;
			qrSourceId = response.source_id;
			qrSvg = response.qr_svg ?? "";
			startPolling();
			startTimer();
		} catch (err) {
			error = err instanceof Error ? err.message : "Failed to initiate pairing";
		} finally {
			isInitiating = false;
		}
	}

	function retryQRPairing() {
		hasTimedOut = false;
		pairingData = null;
		qrSourceId = "";
		qrSvg = "";
		error = null;
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

	function retryMacPairing() {
		hasTimedOut = false;
		pairingData = null;
		error = null;
		initiateMacPairing();
	}

	// --- Shared Polling & Timer ---

	async function checkPairingStatus() {
		try {
			// Both iOS (QR) and Mac (code) ride the same token → consume lifecycle.
			const sourceId = pairingData?.source_id || qrSourceId;
			if (!sourceId) return;
			const status = await api.getPairingStatus(sourceId);
			if (status.status === "active") {
				stopPolling();
				stopTimer();
				pairingSucceeded = true;
				resetLocalState();
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

	function startTimer(seconds = 600) {
		if (timerInterval) return;

		timeRemaining = seconds;

		timerInterval = setInterval(() => {
			timeRemaining--;
			if (timeRemaining <= 0) {
				hasTimedOut = true;
				stopTimer();
				stopPolling();
				// Don't leave the displayed secret/token live just because the
				// user walked away — revoke/deny it now (server TTL is the backstop).
				cleanupPending();
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

	/** Clear iOS/Mac flow state so the next open starts a fresh pairing. */
	function resetLocalState() {
		qrSourceId = "";
		qrSvg = "";
		pairingData = null;
		hasTimedOut = false;
		error = null;
	}

	/** Deny the outstanding (unclaimed) pair token. Called when the user cancels
	 *  AND when the code/QR times out — no credential exists until the new device
	 *  consumes, so both flows just deny the pending token. Idempotent +
	 *  best-effort; the server-side token TTL is the backstop. */
	function cleanupPending() {
		if (pairingSucceeded) return;
		const pendingId = pairingData?.source_id || qrSourceId;
		if (pendingId) void api.pairDeny(pendingId);
	}

	function handleClose() {
		stopPolling();
		stopTimer();
		cleanupPending();
		resetLocalState();
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

{#snippet getTheApp(device: string)}
	<!-- A code is meaningless without the app to type it into, so it leads. -->
	<p class="text-sm text-foreground-muted mb-4">
		<span class="step-n">1</span> Install Virtues on your {device} —
		<button type="button" class="dl" onclick={() => void openExternal(DOWNLOADS_URL)}>
			virtues.com/downloads
		</button>
	</p>
{/snippet}

<Modal open={open} onClose={handleClose} title="Connect {displayName}" width="md">
	{#if deviceType === "ios"}
		<!-- iOS Flow: QR Code Primary + Manual Fallback -->
		<div class="space-y-5">

			{#if hasTimedOut}
				<!-- Expired state -->
				<div class="text-center py-6">
					<p class="font-serif text-lg text-foreground mb-2">Code expired</p>
					<p class="text-sm text-foreground-muted mb-6">
						The pairing session timed out. No device connected.
					</p>
					<div class="flex justify-center gap-4">
						<Button variant="ghost" onclick={handleClose}>Cancel</Button>
						<Button variant="primary" onclick={retryQRPairing}>Generate a new code</Button>
					</div>
				</div>

			{:else}
				<!-- QR Code pairing -->
				<div class="flex flex-col items-center text-center">
					{@render getTheApp('iPhone')}
					<div class="mb-5">
						<p class="text-sm leading-relaxed text-foreground-muted">
							<span class="step-n">2</span> Open Virtues on your iPhone and tap
							<strong class="text-foreground">Scan QR Code</strong>
						</p>
					</div>

					<!-- QR Code (server-rendered SVG encoding /pair#t=<token>), framed
					     with hairline corner brackets so it reads like a scan target,
					     not a clip-art box. -->
					<div class="qr-frame mb-5">
						<span class="qr-corner qr-corner--tl"></span>
						<span class="qr-corner qr-corner--tr"></span>
						<span class="qr-corner qr-corner--bl"></span>
						<span class="qr-corner qr-corner--br"></span>
						<div class="rounded-xl bg-white p-4">
							{#if isInitiating}
								<div class="w-[232px] h-[232px] flex items-center justify-center">
									<Icon icon="ri:loader-4-line" width="22" class="animate-spin text-neutral-400" />
								</div>
							{:else if qrSvg}
								<!-- eslint-disable-next-line svelte/no-at-html-tags -->
								<div class="w-[232px] h-[232px] [&>svg]:w-full [&>svg]:h-full">
									{@html qrSvg}
								</div>
							{:else}
								<div class="w-[232px] h-[232px] flex items-center justify-center">
									<p class="text-sm text-error">Failed to generate QR</p>
								</div>
							{/if}
						</div>
					</div>

					<!-- Status -->
					{#if isPolling}
						<div class="flex items-center gap-2.5 rounded-full bg-surface-elevated px-3.5 py-1.5 text-sm text-foreground-muted">
							<span class="relative flex h-2 w-2">
								<span class="absolute inline-flex h-full w-full animate-ping rounded-full bg-primary opacity-60"></span>
								<span class="relative inline-flex h-2 w-2 rounded-full bg-primary"></span>
							</span>
							<span>Waiting for your device…</span>
							<span class="font-mono text-xs tabular-nums text-foreground-subtle">{formatTime(timeRemaining)}</span>
						</div>
					{/if}
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
					<p class="font-serif text-lg text-foreground mb-2">Code expired</p>
					<p class="text-sm text-foreground-muted mb-6">
						The pairing code expired. No device connected.
					</p>
					<div class="flex justify-center gap-4">
						<Button variant="ghost" onclick={handleClose}>Cancel</Button>
						<Button variant="primary" onclick={retryMacPairing}>Try again</Button>
					</div>
				</div>
			{:else if pairingData}
				<div class="space-y-6">
					{@render getTheApp('Mac')}
					<div>
						<p class="text-sm text-foreground-muted mb-4">
							<span class="step-n">2</span> Enter this code in Virtues on that Mac:
						</p>
						<div class="font-mono text-2xl font-medium tracking-[0.3em] text-foreground py-4 break-all">
							{pairingData.token ?? "…"}
						</div>
						<p class="text-xs text-foreground-subtle mb-2">
							Expires in {formatTime(timeRemaining)}
						</p>
					</div>

					{#if isPolling}
						<p class="text-sm text-foreground-muted">Waiting for your device…</p>
					{/if}

					<!-- The app switches collection on itself as part of pairing
					     (src-tauri/ui/pair.html). Kept as a quiet fallback rather
					     than a numbered step: it is what to do if that best-effort
					     install didn't take, not part of the two-beat flow. -->
					<p class="text-xs text-foreground-subtle">
						Collection switches on automatically. If it doesn't, open
						Sources → This Mac in that app.
					</p>

					<div class="pt-4">
						<Button variant="ghost" onclick={handleClose}>Cancel</Button>
					</div>
				</div>
			{/if}
		</div>
	{/if}
</Modal>

<style>
	/* Numbered steps, so the two-beat shape reads before the words do. */
	.step-n {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 1.125rem;
		height: 1.125rem;
		margin-right: 0.375rem;
		border-radius: 50%;
		background: color-mix(in srgb, var(--color-foreground) 8%, transparent);
		font-size: 0.6875rem;
		font-weight: 600;
		color: var(--color-foreground, #111827);
	}
	.dl {
		border: none;
		background: none;
		padding: 0;
		font: inherit;
		color: var(--color-primary);
		cursor: pointer;
	}
	.dl:hover {
		text-decoration: underline;
	}

	@reference "../../../app.css";

	/* Scan-target frame: white QR plate with four hairline corner brackets.
	   Reads as "aim here" rather than a bare clip-art square. */
	.qr-frame {
		position: relative;
		padding: 10px;
	}

	.qr-corner {
		position: absolute;
		width: 16px;
		height: 16px;
		border-color: var(--color-foreground);
		opacity: 0.85;
	}
	.qr-corner--tl {
		top: 0;
		left: 0;
		border-top: 2px solid;
		border-left: 2px solid;
		border-top-left-radius: 6px;
	}
	.qr-corner--tr {
		top: 0;
		right: 0;
		border-top: 2px solid;
		border-right: 2px solid;
		border-top-right-radius: 6px;
	}
	.qr-corner--bl {
		bottom: 0;
		left: 0;
		border-bottom: 2px solid;
		border-left: 2px solid;
		border-bottom-left-radius: 6px;
	}
	.qr-corner--br {
		bottom: 0;
		right: 0;
		border-bottom: 2px solid;
		border-right: 2px solid;
		border-bottom-right-radius: 6px;
	}
</style>
