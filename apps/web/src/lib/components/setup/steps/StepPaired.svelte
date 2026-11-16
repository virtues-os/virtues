<!--
	Server, once this device has one: a receipt. Nothing reaches this screen
	without a paired device on a connected server, so it says what is true
	and the way on. Only what this device can know: it reached the server,
	not how.
-->
<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import StepFrame from "../StepFrame.svelte";

	let { onnext }: { onnext: () => void } = $props();
</script>

<StepFrame
	title="You paired this device with your server"
	subtitle="This device and your server have exchanged keys, so they know each other from here on."
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
			<Icon icon="mynaui:arrow-right" width="16" />
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
