<!--
	Steps 1 and 2 — Server and Wi-Fi, as the web app shows them.

	Nothing reaches `/setup` in the web app without a paired device on a
	connected server, so here both are receipts: what is already true, and
	the way on. They become real screens when Setup runs before pairing, in
	the apps (setup-plan.md, slice 2).
-->
<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import StepFrame from "../StepFrame.svelte";

	let { which, onnext }: { which: "server" | "wifi"; onnext: () => void } = $props();
</script>

<StepFrame
	title={which === "server" ? "You paired this device with your server" : "Your server is online"}
	subtitle={which === "server"
		? "This device and your server have exchanged keys, so they know each other from here on."
		: "It's on your network, which is how this device reached it."}
>
	<span class="check" aria-hidden="true">
		<svg viewBox="0 0 48 48" width="56" height="56">
			<circle cx="24" cy="24" r="22" />
			<path d="M14.5 24.8l6.6 6.3 12.8-13.6" />
		</svg>
	</span>
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
		stroke: var(--color-success, #2f8f5b);
		stroke-width: 1.4;
		stroke-linecap: round;
		stroke-linejoin: round;
	}
</style>
