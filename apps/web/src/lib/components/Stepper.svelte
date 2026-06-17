<!--
  Stepper — the one horizontal progress rail for the onboarding flow.

  Replaces the two hand-rolled inline rails (the required /setup gate and the
  guided /get-started stepper) that had no connecting track and tried to fit
  four full-length labels into a 448px row (they wrapped/overflowed — the
  "wildly messed up" rail). This renders a real track: numbered nodes joined
  by a line that fills as steps complete, and a SINGLE current-step label
  underneath so nothing can overflow regardless of width.

  Node state per step: done (filled, check) > current (ring) > pending (empty).
-->
<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";

	interface Step {
		id: string;
		/** Short label; only the current step's label is shown (under the rail). */
		label: string;
		done: boolean;
	}

	interface Props {
		steps: Step[];
		/** Index of the active step (the one the user is on right now). */
		current: number;
	}

	let { steps, current }: Props = $props();

	type NodeState = "done" | "current" | "pending";
	function nodeState(step: Step, i: number): NodeState {
		if (step.done) return "done";
		if (i === current) return "current";
		return "pending";
	}

	const currentLabel = $derived(steps[current]?.label ?? "");
</script>

<div class="w-full">
	<div class="flex items-center">
		{#each steps as step, i (step.id)}
			{@const s = nodeState(step, i)}
			<!-- Node -->
			<div
				class="flex h-7 w-7 shrink-0 items-center justify-center rounded-full border text-xs font-medium transition-colors
					{s === 'done'
					? 'border-success bg-success text-white'
					: s === 'current'
						? 'border-primary text-primary'
						: 'border-border text-foreground-subtle'}"
				title={step.label}
				aria-current={s === "current" ? "step" : undefined}
			>
				{#if s === "done"}
					<Icon icon="ri:check-line" width="16" />
				{:else}
					{i + 1}
				{/if}
			</div>
			<!-- Connector track (between nodes only). Fills once the node to its
			     left is done, so the line visibly advances with progress. -->
			{#if i < steps.length - 1}
				<div class="h-px flex-1 {step.done ? 'bg-success' : 'bg-border'}"></div>
			{/if}
		{/each}
	</div>

	<!-- One label, centered — never overflows however narrow the column is. -->
	{#if currentLabel}
		<p class="mt-3 text-center text-sm font-medium text-foreground">{currentLabel}</p>
		<p class="text-center text-xs text-foreground-subtle">
			Step {current + 1} of {steps.length}
		</p>
	{/if}
</div>
