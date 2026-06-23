<!--
  Stepper — the one horizontal progress rail for the onboarding flow.

  A real track: numbered ink nodes joined by a line that *fills* as steps
  complete (animated), and a SINGLE current-step label underneath that
  crossfades on change — so nothing overflows however narrow the column is.

  Node state per step: done (filled ink + check) > current (ink ring) >
  pending (faint). Monochrome on purpose — the structure reads as quiet
  "academic ink"; saturated green is reserved for "this data source is live"
  confirmations in the step bodies.
-->
<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import { scale, fade } from "svelte/transition";
	import { backOut } from "svelte/easing";

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
	<div class="flex items-center px-1">
		{#each steps as step, i (step.id)}
			{@const s = nodeState(step, i)}
			<!-- Node -->
			<div
				class="relative flex h-8 w-8 shrink-0 items-center justify-center rounded-full border text-[13px] font-medium
					transition-[background-color,border-color,color,box-shadow,transform] duration-[var(--duration-slow)] ease-[var(--ease-premium)]
					{s === 'done'
						? 'border-foreground bg-foreground text-surface'
						: s === 'current'
							? 'border-foreground text-foreground shadow-[0_0_0_4px_color-mix(in_srgb,var(--color-foreground)_9%,transparent)]'
							: 'border-border text-foreground-subtle'}"
				title={step.label}
				aria-current={s === "current" ? "step" : undefined}
			>
				{#if s === "done"}
					<span in:scale={{ duration: 320, start: 0.4, easing: backOut }}>
						<Icon icon="ri:check-line" width="16" />
					</span>
				{:else}
					<span class:opacity-100={s === "current"}>{i + 1}</span>
				{/if}
			</div>

			<!-- Connector track. The inner fill grows 0 → 100% once the node to
			     its left completes, so the line visibly advances with progress. -->
			{#if i < steps.length - 1}
				<div class="relative mx-1.5 h-[2px] flex-1 overflow-hidden rounded-full bg-border">
					<div
						class="absolute inset-y-0 left-0 rounded-full bg-foreground transition-[width] duration-[600ms] ease-[var(--ease-out-expo)]"
						style="width: {step.done ? '100%' : '0%'}"
					></div>
				</div>
			{/if}
		{/each}
	</div>

	<!-- One label, centered — crossfades as the step changes. -->
	{#if currentLabel}
		<div class="mt-4 h-9 text-center">
			{#key current}
				<div in:fade={{ duration: 240 }}>
					<p class="text-sm font-medium tracking-tight text-foreground">{currentLabel}</p>
					<p class="text-[11px] uppercase tracking-[0.14em] text-foreground-subtle">
						Step {current + 1} of {steps.length}
					</p>
				</div>
			{/key}
		</div>
	{/if}
</div>
