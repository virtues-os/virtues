<!--
  Sudo confirmation modal.

  Usage from a gated-action handler:

      <SudoModal
        action="export_data"
        title="Export all data"
        description="This will produce a single archive of everything Virtues
                     has indexed about you. Confirm at the box CLI."
        bind:show={showSudo}
        onApproved={(requestId) => actuallyExport(requestId)}
      />

  Lifecycle:
    1. `show = true` → POST /api/sudo/request, start polling /api/sudo/status/:id.
    2. UI tells the user to run `virtues sudo` on the box.
    3. Status flips to `approved` → fire `onApproved(requestId)`. Caller
       submits the dangerous-action request with the id (handler calls
       `verify_and_consume` server-side).
    4. Status flips to `expired` or `denied` → render the bad-end state.

  Caller responsibility: when `onApproved` fires, the caller has up to 5
  minutes from request mint to actually use the approval. After that the
  request is consumed (one-shot) — re-prompt if the user dawdled.
-->
<script lang="ts">
	import { Button, ErrorState, LoadingState } from "$lib";
	import Icon from "$lib/components/Icon.svelte";
	import { requestSudo, getSudoStatus } from "$lib/api/client";
	import { toast } from "svelte-sonner";

	type Props = {
		action:
			| "export_data"
			| "change_byo_key"
			| "wipe_box"
			| "revoke_last_device"
			| "import_applet_package";
		title: string;
		description: string;
		actionPayload?: Record<string, unknown>;
		show: boolean;
		onApproved: (requestId: string) => void | Promise<void>;
		onCancel?: () => void;
	};

	let {
		action,
		title,
		description,
		actionPayload = {},
		show = $bindable(),
		onApproved,
		onCancel,
	}: Props = $props();

	type Phase = "minting" | "waiting" | "approved" | "expired" | "denied" | "error";
	let phase = $state<Phase>("minting");
	let errorMessage = $state<string | null>(null);
	let requestId = $state<string | null>(null);
	let expiresAt = $state<string | null>(null);
	let cliCommand = $state<string>("sudo -u virtues virtues sudo");
	let remainingSec = $state<number>(0);
	let pollHandle: ReturnType<typeof setInterval> | null = null;
	let countdownHandle: ReturnType<typeof setInterval> | null = null;

	// Re-run the request when the modal opens. `show` going false stops
	// everything.
	$effect(() => {
		if (show) {
			void start();
		} else {
			stop();
		}
		return stop;
	});

	function stop() {
		if (pollHandle) {
			clearInterval(pollHandle);
			pollHandle = null;
		}
		if (countdownHandle) {
			clearInterval(countdownHandle);
			countdownHandle = null;
		}
	}

	async function start() {
		phase = "minting";
		errorMessage = null;
		requestId = null;
		expiresAt = null;
		try {
			const data = await requestSudo<{
				id: string;
				expires_at: string;
				cli_command?: string;
			}>(action, actionPayload);
			requestId = data.id;
			expiresAt = data.expires_at;
			// Server-controlled CLI command — varies by deployment.
			if (typeof data.cli_command === "string" && data.cli_command.length > 0) {
				cliCommand = data.cli_command;
			}
			phase = "waiting";
			tickCountdown();
			countdownHandle = setInterval(tickCountdown, 1000);
			pollHandle = setInterval(pollStatus, 1500);
		} catch (e) {
			errorMessage = e instanceof Error ? e.message : "Network error";
			phase = "error";
		}
	}

	function tickCountdown() {
		if (!expiresAt) return;
		const exp = new Date(expiresAt).getTime();
		remainingSec = Math.max(0, Math.floor((exp - Date.now()) / 1000));
		if (remainingSec <= 0) {
			phase = "expired";
			stop();
		}
	}

	async function pollStatus() {
		if (!requestId) return;
		try {
			const data = await getSudoStatus<{ status: string }>(requestId);
			if (data.status === "approved") {
				phase = "approved";
				stop();
				try {
					await onApproved(requestId);
				} catch (e) {
					toast.error("Action failed", {
						description: e instanceof Error ? e.message : "Unknown error",
					});
				}
				show = false;
			} else if (data.status === "denied") {
				phase = "denied";
				stop();
			} else if (data.status === "expired") {
				phase = "expired";
				stop();
			}
		} catch {
			/* swallow */
		}
	}

	function cancel() {
		stop();
		show = false;
		onCancel?.();
	}

	function fmt(sec: number) {
		const m = Math.floor(sec / 60);
		const s = sec % 60;
		return `${m}:${s.toString().padStart(2, "0")}`;
	}
</script>

{#if show}
	<div
		class="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm px-4"
		onclick={cancel}
		onkeydown={(e) => e.key === "Escape" && cancel()}
		role="dialog"
		tabindex="-1"
	>
		<div
			class="w-full max-w-md rounded-xl bg-surface border border-border shadow-xl p-6"
			onclick={(e) => e.stopPropagation()}
			onkeydown={(e) => e.stopPropagation()}
			role="document"
		>
			<div class="flex items-center gap-3 mb-2">
				<div
					class="flex-shrink-0 w-10 h-10 rounded-lg bg-surface-alt border border-border flex items-center justify-center"
				>
					<Icon icon="ri:shield-keyhole-line" class="text-foreground-muted" />
				</div>
				<div>
					<h2 class="text-lg font-semibold leading-tight">{title}</h2>
					<p class="text-xs text-foreground-muted">
						Sensitive action — requires physical confirmation.
					</p>
				</div>
			</div>

			<p class="text-sm text-foreground-muted mt-3 mb-4">
				{description}
			</p>

			{#if phase === "minting"}
				<LoadingState />
			{:else if phase === "waiting"}
				<div class="rounded-lg bg-surface-alt border border-border p-4 text-sm space-y-3">
					<div>
						<div class="font-medium text-foreground mb-1">Run this on the box:</div>
						<code
							class="block px-3 py-2 rounded bg-surface border border-border font-mono text-xs"
						>
							{cliCommand}
						</code>
					</div>
					<div class="flex items-center gap-2 text-xs text-foreground-muted">
						<Icon icon="ri:loader-4-line" class="animate-spin" />
						<span>Waiting for confirmation… ({fmt(remainingSec)})</span>
					</div>
				</div>
				<div class="flex justify-end mt-4">
					<Button variant="ghost" onclick={cancel}>Cancel</Button>
				</div>
			{:else if phase === "approved"}
				<div class="rounded-lg bg-surface-alt border border-border p-4 text-sm flex items-start gap-3">
					<Icon icon="ri:check-line" class="text-success mt-0.5" />
					<div>
						<div class="font-medium">Confirmed</div>
						<div class="text-foreground-muted text-xs">Running the action…</div>
					</div>
				</div>
			{:else if phase === "expired"}
				<ErrorState
					title="Confirmation timed out"
					message="The request expired before being approved. Close and try again."
				/>
				<div class="flex justify-end mt-4">
					<Button variant="ghost" onclick={cancel}>Close</Button>
				</div>
			{:else if phase === "denied"}
				<ErrorState
					title="Denied at the box"
					message="The CLI confirmation was denied. The action was not performed."
				/>
				<div class="flex justify-end mt-4">
					<Button variant="ghost" onclick={cancel}>Close</Button>
				</div>
			{:else if phase === "error"}
				<ErrorState message={errorMessage ?? "Could not start sudo flow."} />
				<div class="flex justify-end mt-4">
					<Button variant="ghost" onclick={cancel}>Close</Button>
				</div>
			{/if}
		</div>
	</div>
{/if}
