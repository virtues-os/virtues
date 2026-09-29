<!--
	Account, Server and Wi-Fi, as the web app shows them.

	Nothing reaches `/setup` in the web app without a paired device on a
	connected server, so here both are receipts: what is already true, and
	the way on. Only what this device can know: it reached the server. Not
	how (over the relay "it's on your network" was false), and not over what
	(an ethernet server has no Wi-Fi to report). They become real screens when Setup runs before pairing, in
	the apps (setup-plan.md, slice 2).
-->
<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import StepFrame from "../StepFrame.svelte";
	import { gettingStarted } from "$lib/stores/gettingStarted.svelte";

	let { which, onnext }: { which: "account" | "server" | "wifi"; onnext: () => void } = $props();

	/** Account, once there's a server: signing in here would reach no server
	 *  (that happens before pairing), so it says where things stand instead. */
	const via = $derived(gettingStarted.step("connect_ai")?.via ?? null);
	const signedIn = $derived(via === "subscription" || via === "linked");

	const title = $derived(
		which === "account" && via === "byo"
			? "Your server uses your own AI"
			: which === "account"
			? signedIn
				? "Your server is using your Virtues account"
				: "Your server isn't signed in"
			: which === "server"
				? "You paired this device with your server"
				: "Your server is online",
	);
	const subtitle = $derived(
		which === "account" && via === "byo"
			? "It isn't signed in to a Virtues account, and your assistant doesn't need one."
			: which === "account"
			? via === "subscription"
				? "Your subscription covers its AI, web search, place search, and bank connections."
				: signedIn
					? "Your account has no subscription yet. You can add one on the Subscription step."
					: "You can sign in on the Subscription step, or use your own AI there."
			: which === "server"
				? "This device and your server have exchanged keys, so they know each other from here on."
				: "This device just reached it, so it's connected and ready.",
	);
</script>

<StepFrame {title} {subtitle}>
	{#if which !== "account" || signedIn || via === "byo"}
	<span class="check" aria-hidden="true">
		<svg viewBox="0 0 48 48" width="56" height="56">
			<circle cx="24" cy="24" r="22" />
			<path d="M14.5 24.8l6.6 6.3 12.8-13.6" />
		</svg>
	</span>
	{/if}
	{#snippet actions()}
		<button type="button" class="setup-go" onclick={onnext}>
			Continue
			<Icon icon="ri:arrow-right-line" width="16" />
		</button>
	{/snippet}
</StepFrame>

<style>
	.check svg {
		display: block;
		fill: none;
		stroke: var(--color-success);
		stroke-width: 1.4;
		stroke-linecap: round;
		stroke-linejoin: round;
	}
</style>
