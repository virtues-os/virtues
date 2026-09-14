<!--
	The room's table of contents, under the door.

	Four steps in the margin, as type: the one being asked at full strength,
	the settled ones quiet, the ones set aside struck through. No card, no
	rule, no bar, no tick — a checklist drawn as a widget was tried twice in
	the column and torn out twice, because in the column it competes with the
	conversation saying the same thing. Out here it does not compete; it is
	the one fixed place to look after coming back from another tab.

	It NAVIGATES, which is what makes it a contents rather than a progress
	bar: a step scrolls the thread to where the room spoke about it, so a
	half-read line can be found again once the thread is long.
-->
<script lang="ts">
	import { gettingStarted } from "$lib/stores/gettingStarted.svelte";

	let { onjump }: { onjump?: (stepId: string) => void } = $props();

	const steps = $derived(gettingStarted.steps);
	const currentId = $derived(gettingStarted.firstOpen);
	const settled = $derived(steps.filter((s) => s.status !== "open").length);
</script>

{#if !gettingStarted.graduated && steps.length > 0}
	<nav class="toc" aria-label="Setting up">
		<ol>
			{#each steps as step (step.id)}
				<li>
					<button
						type="button"
						class:current={step.id === currentId}
						class:skipped={step.status === "skipped"}
						aria-current={step.id === currentId ? "step" : undefined}
						title={step.status === "skipped" ? "Set aside — say the word to come back to it" : step.title}
						onclick={() => onjump?.(step.id)}
					>
						{step.title}
					</button>
				</li>
			{/each}
		</ol>
		<!-- No room for four labels beside a 48rem column: the same fact, as
		     one number. -->
		<p class="count">{settled} of {steps.length}</p>
	</nav>
{/if}

<style>
	.toc {
		font-size: 0.8125rem;
		line-height: 1.65;
		text-align: right;
		user-select: none;
	}
	ol {
		list-style: none;
		margin: 0;
		padding: 0;
	}
	button {
		font: inherit;
		border: 0;
		background: none;
		padding: 0;
		margin: 0;
		color: var(--color-foreground-subtle);
		cursor: pointer;
		transition: color 0.18s ease;
	}
	button:hover {
		color: var(--color-foreground);
	}
	/* The one thing being asked of you, and the only line at full strength. */
	button.current {
		color: var(--color-foreground);
	}
	/* Set aside is not the same as done, and must not read as done. */
	button.skipped {
		text-decoration: line-through;
		text-decoration-thickness: 1px;
		opacity: 0.75;
	}
	.count {
		display: none;
		margin: 0;
		color: var(--color-foreground-subtle);
		font-variant-numeric: tabular-nums;
	}

	@media (max-width: 900px) {
		ol {
			display: none;
		}
		.count {
			display: block;
		}
	}
</style>
