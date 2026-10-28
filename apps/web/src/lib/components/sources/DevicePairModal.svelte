<script lang="ts">
	import { openExternal } from "$lib/tauri/bridge";

	/** Canonical app download page — see agents/build/deployment.md. */
	const DOWNLOADS_URL = "https://virtues.com/downloads";
	/**
	 * DevicePairModal - Handles device pairing for iOS and Mac.
	 * iOS: a handoff QR when this computer's app can make one, and an address
	 * and pair code to type.
	 * Mac: a pair code to type into the app on that computer.
	 * Both mint at `/api/pair/mint`, poll `/api/pair/status/:id` until the new
	 * device redeems the code, and cancel at `/api/pair/deny/:id`.
	 */
	import { onDestroy } from "svelte";
	import Modal from "$lib/components/Modal.svelte";
	import Icon from "$lib/components/Icon.svelte";
	import { Button, TextAction } from "$lib";
	import * as api from "$lib/api/client";
	import { openPairDoor, closePairDoor, createPairHandoff } from "$lib/tauri/bridge";
	import type { HandoffOutcome } from "$lib/tauri/bridge";
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

	// iOS pairing: the phone types an address and the pair code, the same
	// mint→consume flow as the Mac code, and cancelling denies the token. The
	// handoff QR below enrolls the phone directly instead, so cancelling also
	// revokes that device.
	let qrSourceId = $state<string>("");
	// Shared polling state (used by both the iOS and Mac flows)
	let pairingData = $state<PairingInitResponse | null>(null);
	let isInitiating = $state(false);
	let isPolling = $state(false);
	let hasTimedOut = $state(false);
	let timeRemaining = $state(600);
	/** True once the new device has paired: the token was redeemed, or the
	 *  handoff device dialed the box. Distinguishes "modal closing because we
	 *  paired" from "modal closing because the user cancelled mid-flow". The
	 *  latter denies the token and revokes a handoff device (`cleanupPending`). */
	let pairingSucceeded = $state(false);
	let timerInterval: ReturnType<typeof setInterval> | null = null;
	let pollInterval: ReturnType<typeof setInterval> | null = null;


	// Start iOS or Mac pairing when the modal opens
	$effect(() => {
		if (open && deviceType === "ios" && !pairingData && !isInitiating && !qrSourceId) {
			initiateQRPairing();
		}
		if (open && deviceType === "mac" && !pairingData && !isInitiating) {
			initiateMacPairing();
		}
	});

	// --- iOS Flow ---

	/**
	 * The pairing door's address, when this computer could open one.
	 *
	 * Pairing is plain HTTP to the server, so a phone that is not on the
	 * server's network cannot reach it. This computer holds a door open on its
	 * own LAN address for the length of this window, and the phone types that
	 * address and the code instead (`openPairDoor`). Null in a browser, on a
	 * phone, or when this machine can't reach the box; the sheet then shows the
	 * server's own address and the code from the mint.
	 */
	let doorOrigin = $state<string | null>(null);

	/**
	 * The handoff QR — the path that works on any network, including one that
	 * won't let these two devices see each other.
	 *
	 * This machine mints the phone's identity, enrols its public half with the
	 * box over the relay it already has, and puts the result in this code. The
	 * phone scans it and is paired; nothing passes between the two devices but
	 * light. It leads because it is the only option that doesn't depend on the
	 * network being cooperative — the address below is the fallback, for a
	 * phone that can reach this machine and would rather type.
	 *
	 * The code carries a private key: on screen for this window only, never
	 * stored, revocable in Devices the moment it's used.
	 */
	let handoffQr = $state<string | null>(null);
	/**
	 * Why there is no handoff QR, when there ought to be one.
	 *
	 * Null means "no explanation is owed" — a browser or the phone, where the
	 * option was never on offer. Anything else must be SAID. A released Mac app
	 * older than 1.0.26 has no `pair_handoff_create` at all. Without a reason,
	 * the person who most needs the scan code, on a network that keeps the two
	 * devices apart, reads its absence as the feature not existing and is left
	 * with an address their phone cannot reach.
	 */
	let handoffUnavailable = $state<Exclude<HandoffOutcome, { kind: "ok" | "no-shell" }> | null>(
		null
	);
	/** The device the handoff enrolled. Success is this row reporting a
	 *  `last_seen_at` — i.e. the phone has actually dialled the box. The
	 *  sheet's usual signal (the pair token being consumed) never fires here,
	 *  because the handoff enrols directly and leaves that token untouched.
	 *
	 *  That only works because enrollment leaves `last_seen_at` NULL. It used
	 *  to stamp `now()` at insert, so the first poll — which runs immediately —
	 *  found its own proof and closed the sheet green a half-second after it
	 *  opened, with the phone untouched. Re-opening the sheet then enrolled
	 *  another one. If this ever flashes shut again, check that column first. */
	let handoffDeviceId = $state<string | null>(null);
	/** The typed-address fallback, unfolded. Only meaningful while a handoff
	 *  QR is up — with no QR the door always shows. */
	let showManual = $state(false);

	async function initiateQRPairing() {
		isInitiating = true;
		error = null;

		try {
			const response = await api.initiatePairing("ios", displayName);
			pairingData = response;
			qrSourceId = response.source_id;
			// Same window as the countdown this modal already runs.
			const door = await openPairDoor(timeRemaining);
			doorOrigin = door?.origin ?? null;
			const outcome = await createPairHandoff(displayName);
			handoffUnavailable =
				outcome.kind === "ok" || outcome.kind === "no-shell" ? null : outcome;
			handoffQr = outcome.kind === "ok" ? outcome.handoff.qrSvg : null;
			handoffDeviceId = outcome.kind === "ok" ? outcome.handoff.deviceId : null;
			startPolling();
			startTimer();
		} catch (err) {
			error = err instanceof Error ? err.message : "Failed to initiate pairing";
		} finally {
			isInitiating = false;
		}
	}

	/** Where the phone should point: the server's origin, from the pair URL. */
	function serverOrigin(pairUrl: string | undefined): string {
		if (!pairUrl) return "…";
		try {
			return new URL(pairUrl).origin;
		} catch {
			return pairUrl;
		}
	}

	function retryQRPairing() {
		hasTimedOut = false;
		pairingData = null;
		qrSourceId = "";
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
			// The handoff's own completion signal, checked first: the enrolled
			// device reporting a last_seen_at means the phone dialled the box.
			if (handoffDeviceId) {
				type DeviceRow = { id: string; last_seen_at?: string | null };
				const devices = await api
					.listDevices<{ devices?: DeviceRow[] }>()
					.catch(() => null);
				const row = devices?.devices?.find((d) => d.id === handoffDeviceId);
				if (row?.last_seen_at) {
					stopPolling();
					stopTimer();
					pairingSucceeded = true;
					resetLocalState();
					onSuccess(handoffDeviceId);
					onClose();
					return;
				}
			}
			// Both the iOS and Mac flows ride the same token → consume lifecycle.
			// The handoff above enrolls directly and leaves the token alone.
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
		pairingData = null;
		hasTimedOut = false;
		error = null;
		showManual = false;
	}

	/** Undo everything this sheet created that the user never completed. Called
	 *  when they cancel AND when the code/QR times out. Idempotent +
	 *  best-effort; the server-side token TTL is the backstop.
	 *
	 *  Two things to undo, not one. The token is merely denied — nothing exists
	 *  until a device consumes it. The HANDOFF is different: it enrolls a live,
	 *  allowlisted device up front, because the whole point is that the laptop
	 *  vouches for a phone it cannot reach. So an abandoned sheet leaves a real
	 *  credential on the box, whose private half was on screen. Revoke it. */
	function cleanupPending() {
		if (pairingSucceeded) return;
		const pendingId = pairingData?.source_id || qrSourceId;
		if (pendingId) void api.pairDeny(pendingId);
		if (handoffDeviceId) void api.revokeDevice(handoffDeviceId);
	}

	function handleClose() {
		stopPolling();
		stopTimer();
		cleanupPending();
		// The door is bound to the LAN, so it closes with the sheet rather than
		// waiting out its own timer. (The timer is the backstop for a window
		// that never gets closed properly — a crash, a force-quit.)
		doorOrigin = null;
		handoffQr = null;
		handoffUnavailable = null;
		handoffDeviceId = null;
		void closePairDoor();
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
		// Not just handleClose's job: a parent unmounting this modal (route
		// change, window close) must not leave a listener bound to the LAN —
		// nor an enrolled device for a phone that never arrived.
		cleanupPending();
		void closePairDoor();
	});
</script>

{#snippet getTheApp(device: string)}
	<!-- A code is meaningless without the app to type it into, so it leads. -->
	<p class="text-sm text-foreground-muted mb-4">
		<span class="step-n">1</span> Install Virtues on your {device} from
		<TextAction inline onclick={() => void openExternal(DOWNLOADS_URL)}>
			virtues.com/downloads
		</TextAction>
	</p>
{/snippet}

<!-- "Computer", not "Mac": this sheet is served to any device, and a PC
     user can reach it. -->
<Modal open={open} onClose={handleClose} title={deviceType === "mac" ? "Add your computer" : `Add your ${displayName || "iPhone"}`} width="md">
	{#if deviceType === "ios"}
		<!-- iOS flow: the handoff QR when this computer can make one, and the
		     address and code to type -->
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
				<!-- Pairing -->
				<div class="flex flex-col items-center text-center">
					{@render getTheApp('iPhone')}
					<div class="mb-5">
						<p class="text-sm leading-relaxed text-foreground-muted">
							{#if handoffQr}
								<span class="step-n">2</span> Open it, choose
								<strong class="text-foreground">Connect to an existing server</strong>,
								and point the phone here
							{:else}
								<span class="step-n">2</span> Open it, choose
								<strong class="text-foreground">Connect to an existing server</strong>,
								then <strong class="text-foreground">Enter an address manually</strong>
							{/if}
						</p>
					</div>

					{#if handoffQr}
						<!-- The handoff code. Leads because it needs nothing of the
						     network: this machine already reached the box to mint it,
						     and the phone only has to see the screen. -->
						<div class="qr-frame mb-5">
							<span class="qr-corner qr-corner--tl"></span>
							<span class="qr-corner qr-corner--tr"></span>
							<span class="qr-corner qr-corner--bl"></span>
							<span class="qr-corner qr-corner--br"></span>
							<div class="rounded-xl bg-white p-4">
								<!-- eslint-disable-next-line svelte/no-at-html-tags -->
								<div class="w-[232px] h-[232px] [&>svg]:w-full [&>svg]:h-full">
									{@html handoffQr}
								</div>
							</div>
						</div>
					{/if}

					{#if handoffUnavailable}
						<!-- The scan-anywhere option is missing and the person is entitled
						     to know why. Silence here reads as "no such feature", and leaves
						     someone on a client-isolating network with only an address their
						     phone cannot reach. -->
						<div class="notice mb-5">
							{#if handoffUnavailable.kind === "too-old"}
								<p class="notice-title">Scanning needs a newer app on this Mac</p>
								<p class="notice-body">
									Update Virtues here, then reopen this window. Until then the phone
									has to reach this computer's network using the address below.
								</p>
							{:else}
								<p class="notice-title">Couldn't prepare a scan code</p>
								<p class="notice-body">{handoffUnavailable.error}</p>
							{/if}
						</div>
					{/if}

					{#if doorOrigin}
						<!-- The address is THIS computer, not the box. Pairing has to be
						     plain HTTP (a device can't use iroh until it's allowlisted, and
						     allowlisting is what pairing does), so a phone away from home
						     can't reach the box at all — but it can reach this machine,
						     which is already paired and holds a door open onto the box's
						     pair endpoint for as long as this sheet is up.

						     Folded behind a link while a QR is up: the scan is the whole
						     flow for almost everyone, and an address+code block sitting
						     beside it read as a third simultaneous instruction. With no
						     QR, typing IS the flow, so the door shows itself. -->
						{#if handoffQr && !showManual}
							<!-- The spacing sits on a wrapper, not on the action: the
							     primitive declares `margin: 0` from a scoped rule, which
							     outranks a forwarded Tailwind `mb-*` and would silently
							     eat it. -->
							<div class="mb-5">
								<TextAction quiet onclick={() => (showManual = true)}>
									Can't scan? Type an address instead
								</TextAction>
							</div>
						{:else}
							<div class="door mb-5">
								<div class="door-row">
									<span class="door-label">Address</span>
									<span class="door-value">{doorOrigin}</span>
								</div>
								<div class="door-row">
									<span class="door-label">Code</span>
									<span class="door-value">{pairingData?.token ?? "…"}</span>
								</div>
								<p class="door-note">
									{#if handoffQr}
										In the app, choose Enter an address manually and type these. Your
										iPhone needs to be on this computer's network.
									{:else}
										Works anywhere this computer and your iPhone share a network. Your
										server doesn't have to be on it.
									{/if}
								</p>
							</div>
						{/if}
					{/if}

					<!-- THE ADDRESS AND CODE, when there is neither a scan code nor a
					     door: a plain browser, or an app too old to make either. The
					     app's scanner reads handoff codes only, so the sheet shows what
					     the person types: the server's origin, taken from the mint's
					     `pair_url`, and the code. -->
					{#if !doorOrigin && !handoffQr}
						<div class="door mb-5">
							{#if isInitiating}
								<Icon icon="mynaui:spinner" width="20" class="animate-spin text-neutral-400" />
							{:else if pairingData}
								<div class="door-row">
									<span class="door-label">Address</span>
									<span class="door-value">{serverOrigin(pairingData.pair_url)}</span>
								</div>
								<div class="door-row">
									<span class="door-label">Code</span>
									<span class="door-value">{pairingData.token ?? "…"}</span>
								</div>
								<p class="door-note">Your iPhone needs to be on the same network as your server.</p>
							{:else}
								<p class="text-sm text-error">Your server couldn't make a pairing code. Close this and try again.</p>
							{/if}
						</div>
					{/if}

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
					{@render getTheApp('computer')}
					<div>
						<p class="text-sm text-foreground-muted mb-4">
							<span class="step-n">2</span> Enter this code in Virtues on that computer:
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
						The app turns on collecting by itself. If it doesn't, open
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
	/* The pairing door's address + code. Monospace and generously spaced
	   because these are read off one screen and typed into another — the
	   failure mode is a misread character, not a slow read. */
	.door {
		width: 100%;
		max-width: 20rem;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-xl);
		padding: 0.75rem 0.875rem;
		text-align: left;
	}

	/* Same frame as .door, deliberately: this is an explanation of what is
	   missing, sitting directly above the fallback it makes you use. Warning
	   color on the title only — the body is instruction, not alarm. */
	.notice {
		width: 100%;
		max-width: 20rem;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-xl);
		padding: 0.75rem 0.875rem;
		text-align: left;
	}

	.notice-title {
		font-size: 0.8125rem;
		font-weight: 500;
		color: var(--color-warning);
	}

	.notice-body {
		margin-top: 0.25rem;
		font-size: 0.75rem;
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}

	.door-row {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 1rem;
		padding: 0.3rem 0;
	}

	.door-label {
		font-size: 12px;
		color: var(--color-foreground-muted);
	}

	.door-value {
		font-family: var(--font-sans, system-ui);
		font-variant-numeric: tabular-nums;
		font-size: 15px;
		color: var(--color-foreground);
		user-select: all;
	}

	.door-note {
		margin-top: 0.5rem;
		padding-top: 0.5rem;
		border-top: 1px solid var(--color-border);
		font-size: 12px;
		line-height: 1.45;
		color: var(--color-foreground-muted);
	}

	/* Numbered steps, so the two-beat shape reads before the words do. */
	.step-n {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 1.125rem;
		height: 1.125rem;
		margin-right: 0.375rem;
		border-radius: 50%;
		background: color-mix(in srgb, var(--wash-ink) 8%, transparent);
		font-size: 0.6875rem;
		font-weight: 600;
		color: var(--color-foreground);
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
		border-top-left-radius: var(--radius-md);
	}
	.qr-corner--tr {
		top: 0;
		right: 0;
		border-top: 2px solid;
		border-right: 2px solid;
		border-top-right-radius: var(--radius-md);
	}
	.qr-corner--bl {
		bottom: 0;
		left: 0;
		border-bottom: 2px solid;
		border-left: 2px solid;
		border-bottom-left-radius: var(--radius-md);
	}
	.qr-corner--br {
		bottom: 0;
		right: 0;
		border-bottom: 2px solid;
		border-right: 2px solid;
		border-bottom-right-radius: var(--radius-md);
	}
</style>
