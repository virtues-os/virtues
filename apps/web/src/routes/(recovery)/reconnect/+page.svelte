<!--
	"Can't reach your server", with a way out. The logic, and what each state
	means, is in $lib/components/recovery/reconnect.svelte.ts; this page only
	draws it.
-->
<script lang="ts">
	import { onMount } from "svelte";
	import { fade } from "svelte/transition";
	import { isMacOS } from "$lib/utils/platform";
	import StepFrame from "$lib/components/setup/StepFrame.svelte";
	import { Reconnect, LISTEN_EVERY_MS, thisDevice } from "$lib/components/recovery/reconnect.svelte";

	const r = new Reconnect();
	const device = thisDevice();
	const Device = device[0].toUpperCase() + device.slice(1);

	/** "Forget this server" takes two presses; the first says what it does. */
	let forgetArmed = $state(false);

	onMount(() => {
		void r.check();
		// Keep looking while the answer is "not yet": a server that was
		// restarting comes back on its own.
		const every = setInterval(() => {
			if (document.hidden) return;
			if (r.verdict !== "rejected" && r.verdict !== "back") void r.check();
		}, LISTEN_EVERY_MS);
		// Coming back to the app, or back online: look now rather than at the
		// next tick. (`visibilitychange` also fires on the way out.)
		const back = () => {
			if (!document.hidden) void r.check();
		};
		window.addEventListener("online", back);
		document.addEventListener("visibilitychange", back);
		return () => {
			clearInterval(every);
			window.removeEventListener("online", back);
			document.removeEventListener("visibilitychange", back);
			r.dispose();
		};
	});

	async function forget() {
		if (!forgetArmed) {
			forgetArmed = true;
			return;
		}
		await r.pairAgain(true);
	}

	const title = $derived(
		r.problem
			? `${Device} can't use its pairing`
			: r.verdict === "checking"
				? "Looking for your server"
				: r.verdict === "back"
					? "Opening your server"
					: r.verdict === "rejected"
						? `Your server doesn't recognize ${device}`
						: r.verdict === "offline"
							? `${Device} is offline`
							: "Can't reach your server",
	);
	const subtitle = $derived(
		r.problem
			? r.problem.message
			: r.verdict === "checking"
				? "Checking the connection."
				: r.verdict === "rejected"
					? "It answered but refused this device. Someone may have reset it, or removed this device from its list."
					: r.verdict === "offline"
						? "Connect it to Wi-Fi or a cellular network, then try again."
						: `It may be off or restarting, or on a network ${device} can't reach.`,
	);
	/** The error line above the actions. A pairing problem's sentence is
	 *  already the subtitle, so it is not repeated here. */
	const note = $derived(r.problem && r.error === r.problem.message ? null : r.error);
	/** The re-check button. A pairing problem's sentence tells you to quit and
	 *  reopen Virtues to try again, which this button does not do, so under a
	 *  problem it says what it does. */
	const recheck = $derived(r.problem ? "Check again" : "Try again");
</script>

<svelte:head>
	<title>Reconnect</title>
</svelte:head>

<div class="setup-stage" aria-hidden="true"></div>
{#if isMacOS}<div class="drag" data-tauri-drag-region aria-hidden="true"></div>{/if}

<main class="flow">
	<div class="leaf" in:fade={{ duration: 200 }}>
		<StepFrame {title} {subtitle}>
			{#if r.verdict === "checking" || r.verdict === "back"}
				<div class="waiting" aria-hidden="true">
					<svg viewBox="0 0 24 24" width="40" height="40">
						<circle cx="4.5" cy="18" r="2.85" />
						<circle cx="19.5" cy="18" r="2.85" />
						<circle cx="12" cy="5" r="2.85" />
					</svg>
				</div>
			{:else if r.verdict === "rejected" || r.problem?.pairAgain}
				<p class="hint">Pairing again forgets the old connection on {device} only. Nothing on your server changes.</p>
			{/if}
			<p class="note" class:error={!!note} role={note ? "alert" : undefined}>{note ?? " "}</p>

			{#snippet actions()}
				{#if r.verdict === "rejected" || r.problem?.pairAgain}
					<button type="button" class="setup-go" onclick={() => void r.pairAgain(true)}>Pair again</button>
				{:else if r.verdict === "silent" || r.verdict === "offline"}
					<button type="button" class="setup-go" onclick={() => void r.check()} disabled={r.busy}>
						{r.busy ? "Looking…" : recheck}
					</button>
					{#if r.canOpenAnyway}
						<button type="button" class="setup-past" onclick={() => r.openApp()}>Open the app anyway</button>
					{/if}
					<button type="button" class="setup-past" onclick={forget}>
						{forgetArmed ? "Press again to forget. Nothing on your server changes" : `Forget this server on ${device}`}
					</button>
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
