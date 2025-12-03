<script lang="ts">
	/**
	 * DevicePairing - Clean monospace code display
	 * Minimal design: white background, large code, simple timer
	 */
	import { onMount, onDestroy } from "svelte";
	import * as api from "$lib/api/client";
	import type { PairingInitResponse, DeviceInfo } from "$lib/types/device-pairing";

	interface Props {
		deviceType: string;
		deviceName: string;
		onSuccess: (sourceId: string, deviceInfo: DeviceInfo) => void;
		onCancel?: () => void;
	}

	let { deviceType, deviceName, onSuccess, onCancel }: Props = $props();

	// Pairing state
	let isInitiating = $state(false);
	let pairingData: PairingInitResponse | null = $state(null);
	let deviceInfo: DeviceInfo | null = $state(null);
	let error = $state<string | null>(null);
	let isPolling = $state(false);
	let hasTimedOut = $state(false);

	// Timer state
	let timeRemaining = $state(600);
	let timerInterval: ReturnType<typeof setInterval> | null = null;
	let pollInterval: ReturnType<typeof setInterval> | null = null;

	// API endpoint state
	let apiEndpoint = $state("");
	let isLoadingEndpoint = $state(true);

	$effect(() => {
		if (typeof window !== "undefined") {
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

	function formatTime(seconds: number): string {
		const mins = Math.floor(seconds / 60);
		const secs = seconds % 60;
		return `${mins}:${secs.toString().padStart(2, "0")}`;
	}

	async function copyCode() {
		if (!pairingData) return;
		try {
			await navigator.clipboard.writeText(pairingData.code);
		} catch (err) {
			console.error("Failed to copy code:", err);
		}
	}

	async function copyEndpoint() {
		try {
			await navigator.clipboard.writeText(apiEndpoint);
		} catch (err) {
			console.error("Failed to copy endpoint:", err);
		}
	}

	async function checkPairingStatus() {
		if (!pairingData || deviceInfo) return;

		try {
			const status = await api.getPairingStatus(pairingData.source_id);

			if (status.status === "active") {
				deviceInfo = status.device_info;
				stopPolling();
				stopTimer();
				onSuccess(pairingData.source_id, status.device_info);
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
		const secondsRemaining = Math.floor((expiresAt.getTime() - now.getTime()) / 1000);

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

	async function initiate() {
		isInitiating = true;
		error = null;

		try {
			const response = await api.initiatePairing(deviceType, deviceName);
			pairingData = response;
			startPolling();
			startTimer();
		} catch (err) {
			error = err instanceof Error ? err.message : "Failed to initiate pairing";
		} finally {
			isInitiating = false;
		}
	}

	function retry() {
		hasTimedOut = false;
		pairingData = null;
		deviceInfo = null;
		error = null;
		timeRemaining = 600;
		initiate();
	}

	function handleCancel() {
		stopPolling();
		stopTimer();
		onCancel?.();
	}

	onMount(() => {
		initiate();
	});

	onDestroy(() => {
		stopPolling();
		stopTimer();
	});
</script>

<div class="pairing-container">
	{#if error}
		<p class="error-text">{error}</p>
	{/if}

	{#if isInitiating}
		<p class="status-text">Generating pairing code...</p>
	{:else if hasTimedOut}
		<div class="timeout-state">
			<p class="timeout-title">Code Expired</p>
			<p class="timeout-description">The pairing code expired. No device connected.</p>
			<div class="actions">
				<button class="text-btn" onclick={handleCancel}>Cancel</button>
				<button class="primary-btn" onclick={retry}>Try Again</button>
			</div>
		</div>
	{:else if pairingData}
		<div class="code-display">
			<p class="instruction">Enter this code in the Virtues app:</p>

			<div class="pairing-code">{pairingData.code}</div>

			<p class="expiry">Code expires in {formatTime(timeRemaining)}</p>

			<button class="copy-btn" onclick={copyCode}>Copy code</button>
		</div>

		<div class="endpoint-section">
			<p class="endpoint-label">Server endpoint:</p>
			<p class="endpoint-value">
				{#if isLoadingEndpoint}
					Loading...
				{:else}
					{apiEndpoint}
				{/if}
			</p>
			<button class="copy-btn" onclick={copyEndpoint} disabled={isLoadingEndpoint}>
				Copy endpoint
			</button>
		</div>

		<div class="actions">
			<button class="text-btn" onclick={handleCancel}>Cancel</button>
		</div>

		{#if isPolling}
			<p class="polling-status">Waiting for device...</p>
		{/if}
	{/if}
</div>

<style>
	.pairing-container {
		text-align: center;
	}

	.error-text {
		font-size: 14px;
		color: var(--error);
		margin-bottom: 16px;
	}

	.status-text {
		font-size: 14px;
		color: var(--foreground-muted);
		padding: 24px 0;
	}

	/* Timeout State */
	.timeout-state {
		padding: 16px 0;
	}

	.timeout-title {
		font-family: var(--font-serif);
		font-size: 18px;
		font-weight: 500;
		color: var(--foreground);
		margin-bottom: 8px;
	}

	.timeout-description {
		font-size: 14px;
		color: var(--foreground-muted);
		margin-bottom: 24px;
	}

	/* Code Display */
	.code-display {
		margin-bottom: 32px;
	}

	.instruction {
		font-size: 14px;
		color: var(--foreground-muted);
		margin-bottom: 16px;
	}

	.pairing-code {
		font-family: var(--font-mono);
		font-size: 32px;
		font-weight: 500;
		letter-spacing: 0.1em;
		color: var(--foreground);
		padding: 24px 0;
	}

	.expiry {
		font-size: 13px;
		color: var(--foreground-muted);
		margin-bottom: 12px;
	}

	.copy-btn {
		font-family: var(--font-sans);
		font-size: 13px;
		color: var(--foreground-muted);
		background: none;
		border: none;
		cursor: pointer;
		text-decoration: underline;
		padding: 4px 0;
	}

	.copy-btn:hover {
		color: var(--foreground);
	}

	.copy-btn:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	/* Endpoint Section */
	.endpoint-section {
		padding-top: 24px;
		border-top: 1px solid var(--border);
		margin-bottom: 24px;
	}

	.endpoint-label {
		font-size: 13px;
		color: var(--foreground-muted);
		margin-bottom: 8px;
	}

	.endpoint-value {
		font-family: var(--font-mono);
		font-size: 14px;
		color: var(--foreground);
		word-break: break-all;
		margin-bottom: 8px;
	}

	/* Actions */
	.actions {
		display: flex;
		justify-content: center;
		gap: 16px;
		padding-top: 16px;
	}

	.text-btn {
		font-family: var(--font-sans);
		font-size: 14px;
		color: var(--foreground-muted);
		background: none;
		border: none;
		cursor: pointer;
		padding: 8px 0;
	}

	.text-btn:hover {
		color: var(--foreground);
	}

	.primary-btn {
		font-family: var(--font-sans);
		font-size: 14px;
		font-weight: 500;
		padding: 12px 24px;
		background: var(--foreground);
		color: var(--surface);
		border: none;
		cursor: pointer;
	}

	/* Polling Status */
	.polling-status {
		font-size: 13px;
		color: var(--foreground-muted);
		margin-top: 24px;
	}
</style>
