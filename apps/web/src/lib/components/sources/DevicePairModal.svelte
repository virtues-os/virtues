<script lang="ts">
	/**
	 * DevicePairModal - Handles device pairing for iOS and Mac.
	 * iOS: Shows QR code + manual device ID entry
	 * Mac: Shows pairing code for entry in the Mac app
	 */
	import { onMount, onDestroy } from "svelte";
	import Modal from "$lib/components/Modal.svelte";
	import { Button, Input } from "$lib";
	import * as api from "$lib/api/client";
	import type { PairingInitResponse, DeviceInfo } from "$lib/types/device-pairing";

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

	// iOS-specific state
	let deviceId = $state("");
	let isLinking = $state(false);

	// Mac-specific state
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

	// Start Mac pairing when modal opens
	$effect(() => {
		if (open && deviceType === "mac" && !pairingData && !isInitiating) {
			initiateMacPairing();
		}
	});

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

	// iOS: Validate UUID format
	let isValidId = $derived(
		/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(
			deviceId.trim(),
		),
	);

	// iOS: Link device by ID
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

	// Mac: Initiate pairing with code
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

	async function checkPairingStatus() {
		if (!pairingData) return;

		try {
			const status = await api.getPairingStatus(pairingData.source_id);

			if (status.status === "active") {
				stopPolling();
				stopTimer();
				onSuccess(pairingData.source_id);
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
		if (timerInterval || !pairingData) return;

		const expiresAt = new Date(pairingData.expires_at);
		const now = new Date();
		const secondsRemaining = Math.floor(
			(expiresAt.getTime() - now.getTime()) / 1000,
		);

		timeRemaining = Math.max(0, secondsRemaining);

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

	function retryMacPairing() {
		hasTimedOut = false;
		pairingData = null;
		error = null;
		timeRemaining = 600;
		initiateMacPairing();
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
		<!-- iOS Flow: QR Code + Manual Device ID -->
		<div class="space-y-6">
			<div class="bg-surface-elevated p-4 rounded-lg border border-border">
				<div class="flex items-start gap-4 mb-4 pb-4 border-b border-border">
					<img 
						src="/images/app-store-qr.png" 
						alt="Download Virtues from App Store" 
						class="w-20 h-20"
					/>
					<div>
						<h4 class="text-sm font-medium text-foreground mb-1">
							Don't have the app yet?
						</h4>
						<p class="text-sm text-foreground-muted">
							Scan to download Virtues from the App Store, or visit
							<a 
								href="https://apps.apple.com/us/app/virtues-personal-ai/id6756082640" 
								target="_blank" 
								rel="noopener noreferrer"
								class="text-primary hover:underline"
							>apps.apple.com</a>
						</p>
					</div>
				</div>
				<h4 class="text-sm font-medium text-foreground mb-3">
					Setup Instructions:
				</h4>
				<ol class="list-decimal list-inside text-sm text-foreground-muted space-y-2">
					<li>Open <strong>Settings</strong> in the Virtues iOS app</li>
					<li>
						Set <strong>Server Endpoint</strong> to:
						<div class="flex items-center gap-2 mt-1 ml-4 bg-surface p-2 rounded border border-border w-fit max-w-full">
							<code class="text-xs font-mono text-foreground break-all">
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
					</li>
					<li>Copy your <strong>Device ID</strong> from the app</li>
					<li>Paste it below to link</li>
				</ol>
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

			{#if error}
				<div class="p-3 bg-error-subtle border border-error rounded-lg">
					<p class="text-sm text-error">{error}</p>
				</div>
			{/if}

			<div class="flex justify-end gap-3 pt-4 border-t border-border">
				<Button variant="ghost" onclick={handleClose} disabled={isLinking}>
					Cancel
				</Button>
				<Button
					variant="primary"
					onclick={handleiOSLink}
					disabled={!isValidId || isLinking}
				>
					{isLinking ? "Linking..." : "Link Device"}
				</Button>
			</div>
		</div>

	{:else}
		<!-- Mac Flow: Pairing Code -->
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
