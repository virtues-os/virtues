<!--
	Step 1 — Connect AI. Two doors: the Virtues subscription (AccountGate,
	the same component the old page used) and your own endpoint (the BYO
	form lives in Billing; this opens it — extracting that sudo-gated form
	into a card is its own change). Not skippable here: the door in the mast
	is the skip. Nothing typed in this card ever reaches the conversation.
-->
<script lang="ts">
	import AccountGate from "$lib/components/onboarding/document/AccountGate.svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { gettingStarted } from "$lib/stores/gettingStarted.svelte";
	import StepCard from "./StepCard.svelte";

	interface Props {
		forceOpen?: boolean;
	}
	let { forceOpen = false }: Props = $props();

	function openByo() {
		windowShellStore.openTabFromRoute("/virtues/billing", { label: "Billing" });
	}
</script>

<StepCard
	step="connect_ai"
	title="Connect AI"
	what="The models your server writes and answers with. A Virtues subscription, or an endpoint of your own."
	{forceOpen}
>
	<AccountGate done={false} onLinked={() => void gettingStarted.refresh()} />
	<p class="byo">
		Running your own models? <button type="button" class="link" onclick={openByo}>Connect an endpoint of your own</button>
		— any OpenAI-compatible URL. Keys are entered there, never in this conversation.
	</p>
</StepCard>

<style>
	.byo {
		margin: 0.875rem 0 0;
		font-size: 0.875rem;
		color: var(--color-foreground-muted);
	}
	.link {
		background: none;
		border: 0;
		padding: 0;
		font: inherit;
		color: var(--color-foreground);
		text-decoration: underline;
		cursor: pointer;
	}
</style>
