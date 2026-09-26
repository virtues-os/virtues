<!--
	The Setup panel — what is left, and the way back in.

	The rail's Setup tile stays while any step is not done, skipped included,
	and this is its panel: every step in order, one row each, a check once
	done, a dash once set aside, the step's number while open. Parts hang
	under Connections ("Computer · App ✓ · Permissions").

	The steps do not run here. Setup is one full-screen flow (/setup), so a
	row, or "Continue setup", goes back into it at that step; the flow ends
	with the app opening again. Forward is strict, backward is free: a step
	past the first one still open is drawn but cannot be entered yet.

	The founder's letter is a step like the others, so reading it again is
	its row.
-->
<script lang="ts">
	import { onMount } from "svelte";
	import { goto } from "$app/navigation";
	import Icon from "$lib/components/Icon.svelte";
	import { setup, type SetupStepId } from "./setup.svelte";

	onMount(() => {
		void setup.refresh();
	});

	const steps = $derived(setup.steps);
	const next = $derived(setup.resumeAt);

	function open(id: SetupStepId) {
		void goto(`/setup/${id}`);
	}
</script>

<div class="stepper">
	<p class="count" aria-live="polite">{setup.doneCount} of {steps.length} done</p>

	{#if next}
		<button type="button" class="continue" onclick={() => open(next)}>
			Continue setup
			<Icon icon="ri:arrow-right-line" width="14" />
		</button>
	{/if}

	<ol class="steps">
		{#each steps as step, i (step.id)}
			{@const settled = step.status !== "open"}
			{@const selected = next === step.id}
			<li class="step" class:settled class:selected class:locked={!step.reachable}>
				<button
					type="button"
					class="row"
					disabled={!step.reachable}
					aria-current={selected ? "step" : undefined}
					title={step.reachable ? undefined : "Finish the step above first"}
					onclick={() => open(step.id)}
				>
					<span class="mark" aria-hidden="true">
						{#if step.status === "done"}
							<svg viewBox="0 0 16 16" width="16" height="16">
								<circle cx="8" cy="8" r="7" />
								<path d="M4.9 8.2l2.1 2 4.1-4.4" />
							</svg>
						{:else if step.status === "skipped"}
							<svg viewBox="0 0 16 16" width="16" height="16">
								<circle cx="8" cy="8" r="7" />
								<path d="M5.3 8h5.4" />
							</svg>
						{:else}
							<span class="num">{i + 1}</span>
						{/if}
					</span>
					<span class="title">{step.label}</span>
					{#if step.status === "skipped"}<span class="later">Skipped</span>{/if}
				</button>

				{#if step.parts && step.status !== "done"}
					<div class="parts">
						{#each step.parts as part}
							<div class="part">
								{#if part.group}<span class="group">{part.group}</span>{/if}
								<span class="badges">
									{#each part.items as item}
										<span class="badge" class:on={item.done}>
											{#if item.done}
												<svg viewBox="0 0 12 12" width="10" height="10" aria-hidden="true">
													<path d="M2.6 6.2l2.1 2 4.7-4.8" />
												</svg>
											{/if}
											{item.label}
										</span>
									{/each}
								</span>
							</div>
						{/each}
					</div>
				{/if}
			</li>
		{/each}
	</ol>

</div>

<style>
	.stepper {
		display: flex;
		flex-direction: column;
		min-height: 100%;
		padding: 0 0 4px;
	}

	.count {
		margin: 0 12px 8px;
		font-size: 12px;
		color: var(--color-foreground-subtle);
		font-variant-numeric: tabular-nums;
	}

	/* The one call to act in the panel, in the tile's own primary. */
	.continue {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		align-self: flex-start;
		margin: 0 12px 12px;
		padding: 6px 16px;
		border: none;
		border-radius: 999px;
		background: var(--color-primary);
		color: var(--color-background);
		font: inherit;
		font-size: 13px;
		cursor: pointer;
		transition: opacity 0.15s ease;
	}
	.continue:hover {
		opacity: 0.86;
	}
	.continue:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: 3px;
	}
	.later {
		margin-left: auto;
		font-size: 11px;
		color: var(--color-foreground-subtle);
	}

	.steps {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 0;
		position: relative;
	}

	.row {
		display: flex;
		align-items: center;
		gap: 8px;
		width: 100%;
		min-height: var(--sidebar-interactive-height, 32px);
		padding: 6px 12px;
		border: none;
		border-radius: var(--sidebar-interactive-radius, 8px);
		background: none;
		text-align: left;
		cursor: pointer;
		font-size: var(--sidebar-interactive-font-size, 13px);
		color: var(--color-foreground);
		transition:
			background var(--sidebar-transition-duration, 120ms) ease,
			color var(--sidebar-transition-duration, 120ms) ease;
	}
	.row:hover:not(:disabled) {
		background: var(--sidebar-hover-bg);
	}
	.selected .row {
		background: var(--active-bg);
	}
	.row:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: -2px;
	}
	.row:disabled {
		cursor: default;
		color: var(--color-foreground-subtle);
	}
	.settled:not(.selected) .row {
		color: var(--color-foreground-muted);
	}

	.mark {
		flex: none;
		display: grid;
		place-items: center;
		width: 18px;
		height: 18px;
	}
	.mark svg {
		display: block;
		fill: none;
		stroke-linecap: round;
		stroke-linejoin: round;
	}
	.mark circle {
		stroke: currentColor;
		stroke-width: 1.1;
		opacity: 0.35;
	}
	.settled .mark svg path {
		stroke: var(--color-success);
		stroke-width: 1.6;
	}
	.step:not(.settled) .mark {
		border-radius: 50%;
		outline: 1px solid color-mix(in srgb, currentColor 35%, transparent);
		outline-offset: -1px;
	}
	/* The step to do now carries the one call to act in the panel: primary
	   ink on its number, and nothing else. */
	.selected:not(.settled) .mark {
		outline: 1.2px solid var(--color-primary);
		outline-offset: -1.2px;
		color: var(--color-primary);
	}
	.num {
		font-size: 11px;
		font-weight: 500;
		line-height: 1;
		font-variant-numeric: tabular-nums;
	}

	.title {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	/* The parts hang from the step's title, on its left edge. */
	.parts {
		display: flex;
		flex-direction: column;
		gap: 8px;
		margin: 4px 0 8px 40px;
		padding-right: 8px;
	}
	.part {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	.group {
		font-size: 12px;
		color: var(--color-foreground-muted);
	}
	.badges {
		display: flex;
		flex-wrap: wrap;
		gap: 4px;
	}
	.badge {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		padding: 0 6px;
		border-radius: 999px;
		font-size: 11px;
		line-height: 17px;
		color: var(--color-foreground-subtle);
		outline: 1px solid var(--color-border);
		outline-offset: -1px;
		transition:
			color 0.4s ease,
			background 0.4s ease;
	}
	.badge.on {
		color: var(--color-foreground);
		background: color-mix(in srgb, var(--color-success) 10%, transparent);
		outline: 1px solid color-mix(in srgb, var(--color-success) 30%, transparent);
		outline-offset: -1px;
	}
	.badge svg {
		fill: none;
		stroke: var(--color-success);
		stroke-width: 1.6;
		stroke-linecap: round;
		stroke-linejoin: round;
	}

</style>
