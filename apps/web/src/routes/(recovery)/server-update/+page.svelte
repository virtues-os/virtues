<!--
	"Your server needs an update": this copy of the app needs a newer server
	than the one it reached ($lib/boxApi.ts, MIN_BOX_API). The app's gate sends
	people here instead of opening half-working. The update itself is the same
	one Settings runs (POST /api/system/update/apply, on every box since
	2026-07-28), so the screen can fix what it reports.
-->
<script lang="ts">
	import { onMount } from "svelte";
	import { fade } from "svelte/transition";
	import { isMacOS } from "$lib/utils/platform";
	import StepFrame from "$lib/components/setup/StepFrame.svelte";
	import { applyUpdate, getUpdateStatus, type UpdateStatus } from "$lib/api/client";
	import { boxApiVersion, MIN_BOX_API } from "$lib/boxApi";

	type Phase = "checking" | "available" | "none" | "updating" | "unreachable";
	let phase = $state<Phase>("checking");
	let status = $state<UpdateStatus | null>(null);
	let error = $state<string | null>(null);

	/** A box downloading, migrating and restarting can take a while. */
	const UPDATE_WAIT_MS = 20 * 60_000;

	async function check() {
		phase = "checking";
		error = null;
		// Arrived here but the box is already new enough (or the floor is 0):
		// nothing to do, go back to the app.
		const v = await boxApiVersion(fetch, true);
		if (v !== null && v >= MIN_BOX_API) {
			window.location.replace("/");
			return;
		}
		try {
			status = await getUpdateStatus();
			phase = status.update_available ? "available" : "none";
		} catch {
			phase = "unreachable";
		}
	}

	async function update() {
		error = null;
		try {
			await applyUpdate();
		} catch (e) {
			error = e instanceof Error ? e.message : "Your server couldn't start the update. Try again.";
			return;
		}
		phase = "updating";
		const until = Date.now() + UPDATE_WAIT_MS;
		// The server goes away while it restarts. Once it has gone and come
		// back, its answer is final: new enough opens the app, and still too
		// old stops waiting instead of spinning out the full 20 minutes.
		let wentAway = false;
		while (Date.now() < until) {
			await new Promise((r) => setTimeout(r, 5000));
			const v = await boxApiVersion(fetch, true);
			if (v !== null && v >= MIN_BOX_API) {
				window.location.replace("/");
				return;
			}
			if (v === null) wentAway = true;
			else if (wentAway) break;
		}
		await check();
		// After `check`, which clears the note, so this one is seen.
		error = "Your server finished updating, but it's still not new enough for this app. Check for another update.";
	}

	onMount(() => void check());

	const title = $derived(phase === "updating" ? "Updating your server" : "Your server needs an update");
	const subtitle = $derived(
		phase === "checking"
			? "Checking for an update."
			: phase === "available"
				? `This version of the app needs a newer server. Updating to ${status?.latest ?? "the latest version"} takes a few minutes, and your server restarts once.`
				: phase === "updating"
					? "Your server is downloading and installing the update, then restarting. This page opens the app when it's done."
					: phase === "none"
						? "This version of the app needs a newer server, and your server's update channel doesn't have one yet. Check again later."
						: "Couldn't reach your server to check for an update. Make sure it's on, then try again.",
	);
</script>

<svelte:head>
	<title>Update your server</title>
</svelte:head>

<div class="setup-stage" aria-hidden="true"></div>
{#if isMacOS}<div class="drag" data-tauri-drag-region aria-hidden="true"></div>{/if}

<main class="flow">
	<div class="leaf" in:fade={{ duration: 200 }}>
		<StepFrame {title} {subtitle}>
			{#if phase === "checking" || phase === "updating"}
				<div class="waiting" aria-hidden="true">
					<svg viewBox="0 0 24 24" width="40" height="40">
						<circle cx="4.5" cy="18" r="2.85" />
						<circle cx="19.5" cy="18" r="2.85" />
						<circle cx="12" cy="5" r="2.85" />
					</svg>
				</div>
			{/if}
			{#if status && phase !== "checking"}
				<p class="hint">Your server is running {status.running_version}.</p>
			{/if}
			<p class="note" class:error={!!error} role={error ? "alert" : undefined}>{error ?? " "}</p>

			{#snippet actions()}
				{#if phase === "available"}
					<button type="button" class="setup-go" onclick={() => void update()}>Update your server</button>
				{:else if phase === "none" || phase === "unreachable"}
					<button type="button" class="setup-go" onclick={() => void check()}>Check again</button>
				{/if}
			{/snippet}
		</StepFrame>
	</div>
</main>

<style>
	.drag {
		position: fixed;
		z-index: 5;
		inset: 0 0 auto 0;
		height: 28px;
	}
	.flow {
		position: relative;
		z-index: 1;
		min-height: 100vh;
		padding-top: 64px;
		display: grid;
	}
	.leaf {
		grid-area: 1 / 1;
		display: flex;
		flex-direction: column;
	}
	.waiting {
		display: flex;
		justify-content: center;
		padding: 24px 0;
	}
	.waiting circle {
		fill: var(--color-foreground);
		animation: seek 1.4s var(--m-ease) infinite;
	}
	.waiting circle:nth-child(2) {
		animation-delay: 0.18s;
	}
	.waiting circle:nth-child(3) {
		animation-delay: 0.36s;
	}
	@keyframes seek {
		0%,
		100% {
			opacity: 0.25;
		}
		40% {
			opacity: 1;
		}
	}
	.hint {
		max-width: 28rem;
		margin: 0 auto;
		text-align: center;
		font-size: 15px;
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}
	.note {
		margin: 12px auto 0;
		max-width: 28rem;
		min-height: 1.5em;
		text-align: center;
		font-size: 14px;
		color: var(--color-foreground-muted);
	}
	.note.error {
		color: var(--color-error, var(--color-foreground));
	}
	@media (prefers-reduced-motion: reduce) {
		.waiting circle {
			animation: none;
			opacity: 0.6;
		}
	}
</style>
