<script lang="ts">
	/**
	 * SourceConnector - "Manifest" list design
	 * Text-forward list with modal for auth flows
	 */
	import DevicePairing from "./DevicePairing.svelte";
	import PlaidLink from "./PlaidLink.svelte";
	import Modal from "./Modal.svelte";
	import Button from "./Button.svelte";
	import * as api from "$lib/api/client";
	import type { DeviceInfo } from "$lib/types/device-pairing";

	interface CatalogSource {
		name: string;
		display_name: string;
		description: string;
		auth_type: string;
		stream_count: number;
		icon?: string;
	}

	interface ConnectedSource {
		id: string;
		source: string; // Source type: "google", "ios", "notion", etc.
		name: string; // User-given instance name
	}

	interface Stream {
		stream_name: string;
		display_name: string;
		description: string;
		is_enabled: boolean;
		supports_incremental: boolean;
		default_cron_schedule: string | null;
	}

	// Props
	let {
		catalog = [],
		connectedSources = [],
		onSourceConnected,
		variant = "default",
	}: {
		catalog: CatalogSource[];
		connectedSources: ConnectedSource[];
		onSourceConnected?: (sourceId: string, sourceName: string) => void;
		variant?: "default" | "manifest";
	} = $props();

	const isManifest = variant === "manifest";

	// Connection type badge helper
	function getConnectionBadge(authType: string): { label: string; type: "cloud" | "device" } | null {
		if (authType === "oauth2") return { label: "CLOUD", type: "cloud" };
		if (authType === "device") return { label: "DEVICE", type: "device" };
		return null;
	}

	// Modal state
	let activeSource = $state<CatalogSource | null>(null);
	let showModal = $state(false);
	let modalStep = $state<"download" | "connect" | "streams">("connect");

	// Connection state
	let isLoading = $state(false);
	let error = $state<string | null>(null);
	let connectedSourceId = $state<string | null>(null);
	let sourceName = $state("");

	// Streams state (for after connection)
	let availableStreams = $state<Stream[]>([]);
	let selectedStreams = $state<Set<string>>(new Set());

	function isConnected(source: CatalogSource): boolean {
		return connectedSources.some((c) => c.source === source.name);
	}

	function openConnectModal(source: CatalogSource) {
		activeSource = source;
		sourceName = `${source.display_name} Account`;
		modalStep = source.name === "mac" ? "download" : "connect";
		error = null;
		showModal = true;
	}

	function closeModal() {
		showModal = false;
		activeSource = null;
		connectedSourceId = null;
		sourceName = "";
		availableStreams = [];
		selectedStreams = new Set();
		modalStep = "connect";
		isLoading = false;
		error = null;
	}

	// OAuth flow
	async function handleOAuthAuthorize() {
		if (!activeSource) return;

		isLoading = true;
		error = null;

		try {
			const callbackUrl = `${window.location.origin}/oauth/callback`;
			// Encode return URL in state for redirect after OAuth
			const returnUrl = window.location.pathname;
			const oauthResponse = await api.initiateOAuth(activeSource.name, callbackUrl, returnUrl);
			window.location.href = oauthResponse.authorization_url;
		} catch (e) {
			error = e instanceof Error ? e.message : "Authorization failed";
			isLoading = false;
		}
	}

	// Device pairing success
	async function handleDevicePairingSuccess(sourceId: string, deviceInfo: DeviceInfo) {
		connectedSourceId = sourceId;
		sourceName = deviceInfo.device_name;

		try {
			const streams = await api.listStreams(sourceId);
			availableStreams = streams;
			selectedStreams = new Set(streams.map((s: Stream) => s.stream_name));
			modalStep = "streams";
		} catch (e) {
			error = e instanceof Error ? e.message : "Failed to load streams";
		}
	}

	// Plaid success
	async function handlePlaidSuccess(sourceId: string, institutionName?: string) {
		connectedSourceId = sourceId;
		sourceName = institutionName || "Bank Account";

		try {
			const streams = await api.listStreams(sourceId);
			availableStreams = streams;
			selectedStreams = new Set(streams.map((s: Stream) => s.stream_name));
			modalStep = "streams";
		} catch (e) {
			error = e instanceof Error ? e.message : "Failed to load streams";
		}
	}

	function toggleStream(streamName: string) {
		const newSet = new Set(selectedStreams);
		if (newSet.has(streamName)) {
			newSet.delete(streamName);
		} else {
			newSet.add(streamName);
		}
		selectedStreams = newSet;
	}

	async function handleEnableStreams() {
		if (!connectedSourceId) return;

		isLoading = true;
		error = null;

		try {
			const allStreamNames = availableStreams.map((s) => s.stream_name);

			for (const streamName of selectedStreams) {
				await api.enableStream(connectedSourceId, streamName);
			}

			for (const streamName of allStreamNames) {
				if (!selectedStreams.has(streamName)) {
					await api.disableStream(connectedSourceId, streamName);
				}
			}

			onSourceConnected?.(connectedSourceId, sourceName);
			closeModal();
		} catch (e) {
			error = e instanceof Error ? e.message : "Failed to enable streams";
			isLoading = false;
		}
	}
</script>

<!-- The Manifest List -->
<div class="source-list" class:manifest={isManifest}>
	{#each catalog as source}
		{@const badge = getConnectionBadge(source.auth_type)}
		<div class="source-row" class:manifest={isManifest}>
			<div class="source-info">
				<div class="source-name-row">
					<span class="source-name" class:manifest={isManifest}>{source.display_name}</span>
					{#if isManifest && badge}
						<span class="connection-badge {badge.type}">{badge.label}</span>
					{/if}
				</div>
				{#if isManifest && source.description}
					<span class="source-description">{source.description}</span>
				{/if}
			</div>
			{#if isConnected(source)}
				<span class="source-status connected">Connected</span>
			{:else}
				<Button variant={isManifest ? "manuscript-ghost" : "secondary"} size="sm" onclick={() => openConnectModal(source)}>
					Connect
				</Button>
			{/if}
		</div>
	{/each}
</div>

<!-- Connection Modal -->
<Modal
	open={showModal && !!activeSource}
	onClose={closeModal}
	title={modalStep === "download"
		? `Download ${activeSource?.display_name}`
		: modalStep === "connect"
			? `Connect ${activeSource?.display_name}`
			: "Enable streams"}
	subtitle={modalStep === "streams" ? "Choose which data to sync:" : undefined}
	variant={isManifest ? "manuscript" : "default"}
>
	{#if modalStep === "download" && activeSource}
		<div class="download-step">
			<p class="modal-description">
				Download and install the Virtues Mac app to connect your computer.
			</p>

			<a
				href="https://github.com/virtues-os/virtues/releases/latest/download/virtues-mac-universal.zip"
				target="_blank"
				rel="noopener noreferrer"
				class="download-link"
			>
				<Button variant={isManifest ? "manuscript" : "primary"}>
					Download for macOS
				</Button>
			</a>

			<p class="download-hint">
				Universal build for Intel and Apple Silicon
			</p>
		</div>

		<div class="modal-actions">
			<Button variant={isManifest ? "manuscript-ghost" : "ghost"} onclick={closeModal}>
				Cancel
			</Button>
			<Button variant={isManifest ? "manuscript-ghost" : "ghost"} onclick={() => modalStep = "connect"}>
				I have it installed
			</Button>
		</div>
	{:else if modalStep === "connect" && activeSource}
		{#if error}
			<p class="error-text">{error}</p>
		{/if}

		{#if activeSource.auth_type === "device"}
			<!-- Device Pairing -->
			<DevicePairing
				deviceType={activeSource.name}
				deviceName={sourceName}
				onSuccess={handleDevicePairingSuccess}
				onCancel={closeModal}
			/>
		{:else if activeSource.name === "plaid"}
			<!-- Plaid Link -->
			<PlaidLink onSuccess={handlePlaidSuccess} onCancel={closeModal} />
		{:else}
			<!-- OAuth -->
			<p class="modal-description">
				You'll be redirected to {activeSource.display_name} to authorize access.
			</p>
			<div class="modal-actions">
				<Button variant={isManifest ? "manuscript-ghost" : "ghost"} onclick={closeModal}>Cancel</Button>
				<Button
					variant={isManifest ? "manuscript" : "primary"}
					onclick={handleOAuthAuthorize}
					disabled={isLoading}
				>
					{isLoading ? "Authorizing..." : "Authorize"}
				</Button>
			</div>
		{/if}
	{:else if modalStep === "streams"}
		<div class="streams-list">
			{#each availableStreams as stream}
				<label class="stream-row">
					<input
						type="checkbox"
						checked={selectedStreams.has(stream.stream_name)}
						onchange={() => toggleStream(stream.stream_name)}
					/>
					<span class="stream-name">{stream.display_name}</span>
				</label>
			{/each}
		</div>

		<div class="modal-actions">
			<Button variant={isManifest ? "manuscript-ghost" : "ghost"} onclick={closeModal}>Cancel</Button>
			<Button
				variant={isManifest ? "manuscript" : "primary"}
				onclick={handleEnableStreams}
				disabled={isLoading || selectedStreams.size === 0}
			>
				{isLoading ? "Enabling..." : `Enable ${selectedStreams.size}`}
			</Button>
		</div>
	{/if}
</Modal>

<style>
	/* Source List */
	.source-list {
		display: flex;
		flex-direction: column;
	}

	.source-row {
		display: flex;
		justify-content: space-between;
		align-items: center;
		padding: 16px 0;
		border-bottom: 1px solid var(--border);
	}

	.source-row:last-child {
		border-bottom: none;
	}

	.source-name {
		font-family: var(--font-serif);
		font-size: 16px;
		font-weight: 500;
		color: var(--foreground);
	}

	.source-status {
		font-family: var(--font-sans);
		font-size: 13px;
		color: var(--foreground-muted);
	}

	.source-status.connected {
		color: var(--success);
	}

	/* Modal content styles (used inside Modal component) */
	.modal-description {
		font-size: 14px;
		color: var(--foreground-muted);
		line-height: 1.5;
		margin-bottom: 24px;
	}

	.error-text {
		font-size: 14px;
		color: var(--error);
		margin-bottom: 16px;
	}

	.modal-actions {
		display: flex;
		justify-content: flex-end;
		gap: 16px;
		margin-top: 24px;
	}

	/* Streams */
	.streams-list {
		display: flex;
		flex-direction: column;
		gap: 12px;
		margin-bottom: 8px;
	}

	.stream-row {
		display: flex;
		align-items: center;
		gap: 12px;
		cursor: pointer;
	}

	.stream-row input[type="checkbox"] {
		width: 16px;
		height: 16px;
		accent-color: var(--foreground);
	}

	.stream-name {
		font-size: 14px;
		color: var(--foreground);
	}

	/* ===================================
	   MANIFEST VARIANT STYLES
	   Digital Manuscript Aesthetic
	   =================================== */

	/* Source List - Manifest */
	.source-list.manifest {
		border-top: 1px solid var(--border);
	}

	.source-row.manifest {
		padding: 20px 0;
		align-items: flex-start;
	}

	.source-info {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	.source-name-row {
		display: flex;
		align-items: center;
		gap: 10px;
	}

	.connection-badge {
		font-family: var(--font-mono);
		font-size: 10px;
		letter-spacing: 0.08em;
		padding: 2px 10px;
		text-transform: uppercase;
		border-radius: 9999px;
		color: var(--foreground-muted);
	}

	.connection-badge.cloud {
		color: var(--info);
		background: var(--info-subtle);
	}

	.connection-badge.device {
		color: var(--success);
		background: var(--success-subtle);
	}

	.source-name.manifest {
		font-family: var(--font-serif);
		font-size: 17px;
		font-weight: 400;
	}

	.source-description {
		font-family: var(--font-sans);
		font-size: 13px;
		color: var(--foreground-muted);
		line-height: 1.4;
		max-width: 280px;
	}

	/* Download Step */
	.download-step {
		text-align: center;
		padding: 16px 0;
	}

	.download-link {
		display: inline-block;
		margin: 24px 0 16px;
		text-decoration: none;
	}

	.download-hint {
		font-family: var(--font-mono);
		font-size: 11px;
		color: var(--foreground-subtle);
		letter-spacing: 0.03em;
	}
</style>
