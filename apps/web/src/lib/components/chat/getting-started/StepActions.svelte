<!--
	The one row of controls under a step's ask. A button appears only where
	the step needs one: the subscription's doors, the source and phone
	doors, the interview's door, the first day's page. Introductions has
	none — the person just types below. Skip is the quiet verb at the end.
-->
<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { mobileLayout } from "$lib/stores/mobileLayout.svelte";
	import { gettingStarted } from "$lib/stores/gettingStarted.svelte";
	import { INTERVIEW_CHAT_ID } from "$lib/components/chat/interview/interview";
	import ConnectAiActions from "./ConnectAiActions.svelte";
	import type { GettingStartedStepId } from "$lib/api/client";

	interface Props {
		step: GettingStartedStepId | "promise";
	}
	let { step }: Props = $props();

	const status = $derived(step === "promise" ? "open" : (gettingStarted.step(step)?.status ?? "open"));
	const underway = $derived(gettingStarted.step("interview")?.underway ?? false);
	const firstDay = $derived(gettingStarted.state?.first_day ?? null);

	function pretty(d: string): string {
		const [y, m, day] = d.split("-").map(Number);
		return new Date(y, m - 1, day).toLocaleDateString(undefined, { month: "long", day: "numeric" });
	}
	function beside(route: string, label: string) {
		windowShellStore.openRouteBeside(route, label);
	}
</script>

{#if status === "open"}
	<div class="actions">
		{#if step === "connect_ai"}
			<ConnectAiActions />
		{:else if step === "connect_world"}
			<button type="button" class="btn" onclick={() => beside("/sources", "Sources")}>Connect a source</button>
			{#if mobileLayout.isMobile}
				<button type="button" class="btn quiet" onclick={() => mobileLayout.openOnboarding()}>Set up this phone</button>
			{:else}
				<button type="button" class="btn quiet" onclick={() => beside("/sources", "Sources")}>Pair your phone</button>
			{/if}
		{:else if step === "interview"}
			<button
				type="button"
				class="btn"
				onclick={() => windowShellStore.openTabFromRoute(`/chat/${INTERVIEW_CHAT_ID}`, { label: "In your own words" })}
			>
				{underway ? "Continue the interview" : "Start the interview"}
				<Icon icon="ri:arrow-right-line" width="14" />
			</button>
		{:else if step === "promise" && firstDay}
			<button type="button" class="btn" onclick={() => firstDay && beside(`/day/day_${firstDay}`, "Your first day")}>
				Read {pretty(firstDay)}
				<Icon icon="ri:arrow-right-line" width="14" />
			</button>
		{/if}
	</div>
{/if}

<style>
	.actions {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.5rem 0.75rem;
		margin: 0.5rem 0 0.25rem;
	}
	.btn {
		display: inline-flex;
		align-items: center;
		gap: 0.35rem;
		font: inherit;
		font-size: 0.875rem;
		padding: 0.4rem 0.9rem;
		border-radius: 6px;
		border: 1px solid var(--color-foreground);
		background: var(--color-foreground);
		color: var(--color-background);
		cursor: pointer;
	}
	.btn.quiet {
		background: transparent;
		color: var(--color-foreground);
		border-color: var(--color-border);
	}
	.btn.quiet:hover {
		border-color: var(--color-foreground);
	}
</style>
