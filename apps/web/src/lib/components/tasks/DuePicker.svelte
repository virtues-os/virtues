<script lang="ts">
	/**
	 * Choosing a task's due date: type it the way the add row reads it
	 * ("fri", "tomorrow 5pm"), or pick a day on the month below. A time
	 * typed with a day is kept; picking a day on the grid keeps the time the
	 * task already has.
	 */
	import { onMount, untrack } from "svelte";
	import { fromIsoDay, isoDay, readDue } from "$lib/tasks/due";

	let {
		due_on,
		due_at,
		onPick,
	}: {
		due_on: string | null;
		due_at: string | null;
		/** The new due date, or nulls to clear it. */
		onPick: (due_on: string | null, due_at: string | null) => void;
	} = $props();

	let phrase = $state("");
	let field: HTMLInputElement | undefined = $state();

	// The picker opens in a floating layer, where `autofocus` does not take.
	onMount(() => field?.focus());
	const typed = $derived(readDue(phrase));

	const today = isoDay(new Date());
	// Opens on the due month; the arrows move it from there.
	let month = $state(untrack(() => startOfMonth(due_on ? fromIsoDay(due_on) : new Date())));

	function startOfMonth(d: Date): Date {
		return new Date(d.getFullYear(), d.getMonth(), 1);
	}

	/** The month's days, padded with blanks so the first lands on its weekday. */
	const cells = $derived.by(() => {
		const out: (string | null)[] = Array(month.getDay()).fill(null);
		const days = new Date(month.getFullYear(), month.getMonth() + 1, 0).getDate();
		for (let d = 1; d <= days; d++) out.push(isoDay(new Date(month.getFullYear(), month.getMonth(), d)));
		return out;
	});

	function shift(by: number) {
		month = new Date(month.getFullYear(), month.getMonth() + by, 1);
	}

	/** A day from the grid, at the time the task already has. */
	function pickDay(day: string) {
		if (!due_at) return onPick(day, null);
		const at = new Date(due_at);
		const d = fromIsoDay(day);
		d.setHours(at.getHours(), at.getMinutes());
		onPick(day, d.toISOString());
	}

	function submit(e: SubmitEvent) {
		e.preventDefault();
		if (typed) onPick(typed.due_on, typed.due_at);
	}
</script>

<div class="due-picker">
	<form onsubmit={submit}>
		<input
			bind:this={field}
			bind:value={phrase}
			class="phrase"
			placeholder="Fri, tomorrow 5pm, in 3 days"
			aria-label="Due date"
		/>
	</form>
	{#if phrase.trim()}
		<p class="reading">{typed ? "Press Return to set it" : "That isn't a date this reads"}</p>
	{/if}

	<div class="month-head">
		<button type="button" class="step" aria-label="Previous month" onclick={() => shift(-1)}>‹</button>
		<span>{month.toLocaleDateString("en-US", { month: "long", year: "numeric" })}</span>
		<button type="button" class="step" aria-label="Next month" onclick={() => shift(1)}>›</button>
	</div>
	<div class="grid" role="grid" aria-label="Days">
		{#each ["S", "M", "T", "W", "T", "F", "S"] as d, i (i)}
			<span class="weekday" aria-hidden="true">{d}</span>
		{/each}
		{#each cells as day, i (i)}
			{#if day}
				<button
					type="button"
					class="day"
					class:today={day === today}
					class:chosen={day === due_on}
					aria-pressed={day === due_on}
					aria-label={fromIsoDay(day).toLocaleDateString("en-US", { weekday: "long", month: "long", day: "numeric" })}
					onclick={() => pickDay(day)}
				>
					{fromIsoDay(day).getDate()}
				</button>
			{:else}
				<span></span>
			{/if}
		{/each}
	</div>
	{#if due_on}
		<button type="button" class="clear" onclick={() => onPick(null, null)}>No date</button>
	{/if}
</div>

<style>
	.due-picker {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		width: 15rem;
		padding: 0.625rem;
		background: var(--color-background);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-lg);
		font-size: 0.8125rem;
	}
	.phrase {
		width: 100%;
		background: transparent;
		border: none;
		border-bottom: 1px solid var(--color-border);
		padding: 0.25rem 0;
		color: var(--color-foreground);
		font: inherit;
		outline: none;
	}
	.phrase::placeholder {
		color: var(--color-foreground-subtle);
	}
	.reading {
		margin: 0;
		color: var(--color-foreground-muted);
	}
	.month-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		color: var(--color-foreground);
	}
	.step {
		background: none;
		border: none;
		padding: 0 0.375rem;
		font-size: 1rem;
		color: var(--color-foreground-muted);
		cursor: pointer;
	}
	.grid {
		display: grid;
		grid-template-columns: repeat(7, 1fr);
		gap: 0;
		text-align: center;
	}
	.weekday {
		color: var(--color-foreground-subtle);
		font-size: 0.6875rem;
	}
	.day {
		aspect-ratio: 1;
		background: none;
		border: none;
		border-radius: var(--radius-md);
		color: var(--color-foreground);
		font: inherit;
		font-variant-numeric: tabular-nums;
		cursor: pointer;
	}
	.day:hover {
		background: color-mix(in srgb, var(--wash-ink) 6%, transparent);
	}
	.day.today {
		color: var(--color-primary);
	}
	.day.chosen {
		background: var(--color-foreground);
		color: var(--color-background);
	}
	.clear {
		align-self: flex-start;
		background: none;
		border: none;
		padding: 0;
		color: var(--color-foreground-muted);
		font: inherit;
		cursor: pointer;
	}
	.clear:hover {
		color: var(--color-foreground);
	}
</style>
