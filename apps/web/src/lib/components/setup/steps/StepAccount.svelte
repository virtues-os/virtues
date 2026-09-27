<!--
	Setup, before the server: your Virtues account.

	Sign-in first (2026-09-27): the account's grant has to reach the server
	over Bluetooth before pairing, so the account is settled before the server
	is looked for (prepair.svelte.ts). An email, then the six-digit code
	emailed to it; atlas makes the account for a new email. Paying is not
	here: the Subscription step, after pairing, passes itself over when the
	account already pays.

	"Continue without an account" is a real path, for someone running their
	own models: the server links nothing, and Subscription offers "Use my own
	AI" after pairing.
-->
<script lang="ts">
	import { tick } from "svelte";
	import StepFrame from "../StepFrame.svelte";
	import { prePair, AccountError } from "../prepair.svelte";
	import { rise } from "../motion";

	let { onnext }: { onnext: () => void } = $props();

	let phase = $state<"email" | "code" | "done">(prePair.account ? "done" : "email");
	let email = $state(prePair.account?.email ?? "");
	let code = $state("");
	let busy = $state(false);
	let error = $state<string | null>(null);
	let codeEl = $state<HTMLInputElement | null>(null);

	const emailOk = $derived(/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email.trim()));
	const digits = $derived(code.replace(/\D/g, ""));

	async function send() {
		if (!emailOk || busy) return;
		busy = true;
		error = null;
		try {
			await prePair.sendCode(email.trim());
			phase = "code";
			await tick();
			codeEl?.focus();
		} catch (e) {
			error = e instanceof AccountError ? e.message : "Couldn't send the code. Try again.";
		} finally {
			busy = false;
		}
	}

	async function verify() {
		if (digits.length < 6 || busy) return;
		busy = true;
		error = null;
		try {
			await prePair.verify(email.trim(), digits);
			onnext();
		} catch (e) {
			error = e instanceof AccountError ? e.message : "That code didn't work. Try again.";
			busy = false;
		}
	}

	function without() {
		prePair.goWithout();
		onnext();
	}

	// Six digits in, it checks itself.
	$effect(() => {
		if (phase === "code" && digits.length === 6 && !busy && !error) void verify();
	});
</script>

<StepFrame
	title={phase === "code" ? "Check your email" : "Your Virtues account"}
	subtitle={phase === "code"
		? `Enter the six-digit code sent to ${email.trim()}.`
		: phase === "done"
			? `You're signed in as ${prePair.account?.email}.`
			: "Sign in with your email, and your server links itself to your account once it's online. New here? The same code starts one."}
>
	{#if phase === "email"}
		<form class="one" onsubmit={(e) => (e.preventDefault(), send())} in:rise>
			<input
				class="setup-field"
				type="email"
				inputmode="email"
				autocomplete="email"
				autocapitalize="off"
				spellcheck="false"
				placeholder="you@example.com"
				aria-label="Email"
				bind:value={email}
				oninput={() => (error = null)}
			/>
		</form>
	{:else if phase === "code"}
		<form class="one" onsubmit={(e) => (e.preventDefault(), verify())} in:rise>
			<input
				class="setup-field code"
				type="text"
				inputmode="numeric"
				autocomplete="one-time-code"
				maxlength="7"
				placeholder="000000"
				aria-label="Six-digit code"
				bind:this={codeEl}
				bind:value={code}
				oninput={() => (error = null)}
			/>
		</form>
	{/if}
	<p class="note" class:error role={error ? "alert" : undefined}>{error ?? " "}</p>

	{#snippet actions()}
		{#if phase === "email"}
			<button type="button" class="setup-go" onclick={send} disabled={!emailOk || busy}>
				{busy ? "Sending…" : "Email me a code"}
			</button>
			<button type="button" class="setup-past" onclick={without}>Continue without an account</button>
		{:else if phase === "code"}
			<button type="button" class="setup-go" onclick={verify} disabled={digits.length < 6 || busy}>
				{busy ? "Checking…" : "Sign in"}
			</button>
			<button type="button" class="setup-past" onclick={send} disabled={busy}>Email me a new code</button>
			<button
				type="button"
				class="setup-past"
				onclick={() => {
					phase = "email";
					code = "";
					error = null;
				}}>Use a different email</button
			>
		{:else}
			<button type="button" class="setup-go" onclick={onnext}>Continue</button>
			<button
				type="button"
				class="setup-past"
				onclick={() => {
					phase = "email";
					error = null;
				}}>Use a different account</button
			>
		{/if}
	{/snippet}
</StepFrame>

<style>
	.one {
		display: flex;
		justify-content: center;
	}
	.code {
		letter-spacing: 0.3em;
		font-variant-numeric: tabular-nums;
		text-align: center;
		max-width: 9em;
	}
	.note {
		margin: 12px 0 0;
		min-height: 1.5em;
		text-align: center;
		font-size: 14px;
		color: var(--color-foreground-muted);
	}
	.note.error {
		color: var(--color-error, var(--color-foreground));
	}
</style>
