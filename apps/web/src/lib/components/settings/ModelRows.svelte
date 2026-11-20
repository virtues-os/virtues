<script lang="ts">
	/**
	 * Models, on Models and spending: one row per kind of work, with the
	 * model it runs on and what a call to that model has cost this month on
	 * average (`/api/usage/summary`, by model). A row opens the model list
	 * with that role picked (`?role=`), where you compare and choose; the
	 * list and these rows share one store, `modelSlots`.
	 *
	 * Pictures is a system role, so it shows only while a model is pinned to
	 * it, with a way back to the recommended one (the list has no picture
	 * models to choose from).
	 */
	import { onMount } from 'svelte';
	import Button from '$lib/components/Button.svelte';
	import LoadError from './LoadError.svelte';
	import { modelSlots, type SlotConfig } from '$lib/stores/modelSlots.svelte';
	import { getUsageSummary } from '$lib/api/client';
	import { formatMicrosPrecise } from '$lib/utils/currency';
	import { openSettingsPage } from '$lib/settings/focus';

	let { assistantName }: { assistantName: string } = $props();

	type Bucket = { label: string; calls: number; cost_micros: number; byo_calls: number };
	let byModel = $state<Bucket[]>([]);

	onMount(() => {
		void modelSlots.load();
		getUsageSummary<{ by_model: Bucket[] }>()
			.then((s) => (byModel = s.by_model ?? []))
			.catch(() => {
				// The rows stand without a cost line.
				byModel = [];
			});
	});

	const roles = $derived(modelSlots.slots.filter((s) => s.language));
	const pinnedPicture = $derived(modelSlots.slots.find((s) => !s.language && modelSlots.chosen[s.key]));

	/** The model a role runs on now: the one you chose, else the recommendation. */
	function inUse(slot: SlotConfig): string | undefined {
		return modelSlots.chosen[slot.key] ?? modelSlots.recommended[slot.key];
	}

	function nameOf(slot: SlotConfig): string {
		const id = inUse(slot);
		return id ? modelSlots.nameOf(id) : 'The recommended model';
	}

	/** "About $0.0030 a call this month", from calls paid through Virtues. */
	function costLine(slot: SlotConfig): string | null {
		const id = inUse(slot);
		const b = byModel.find((m) => m.label === id);
		const paid = b ? b.calls - b.byo_calls : 0;
		if (!b || paid <= 0) return null;
		const each = formatMicrosPrecise(Math.round(b.cost_micros / paid));
		// "<$0.0001" is already a bound, so it takes no "about".
		return `${each.startsWith('<') ? each : `about ${each}`} a call this month`;
	}

	function open(slot: SlotConfig) {
		openSettingsPage(`/virtues/assistant/spending/models?role=${slot.key}`, assistantName);
	}
</script>

<section class="models">
	<h2 class="settings-label">Models</h2>
	<p class="sec-hint">Each kind of work runs on its own model. Change one on the model list, where you can compare what each costs.</p>
	{#if !modelSlots.loaded && modelSlots.error}
		<LoadError message={modelSlots.error} onretry={() => modelSlots.load(true)} />
	{:else}
		<div class="rows" aria-busy={!modelSlots.loaded}>
			{#each roles as slot (slot.key)}
				{@const cost = costLine(slot)}
				<button type="button" class="row" onclick={() => open(slot)}>
					<span class="role">
						<span class="role-name">{slot.label}</span>
						{#if slot.caption}<span class="role-caption">{slot.caption}</span>{/if}
					</span>
					<span class="model">
						<span class="model-name">{modelSlots.loaded ? nameOf(slot) : ' '}</span>
						<span class="model-sub">{modelSlots.chosen[slot.key] ? 'Your choice' : 'Recommended'}{cost ? ` · ${cost}` : ''}</span>
					</span>
					<svg class="chev" viewBox="0 0 24 24" width="15" height="15" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.8" d="m9 18l6-6l-6-6" /></svg>
				</button>
			{/each}
			{#if pinnedPicture}
				<div class="row still">
					<span class="role"><span class="role-name">{pinnedPicture.label}</span></span>
					<span class="model">
						<span class="model-name">{modelSlots.nameOf(modelSlots.chosen[pinnedPicture.key]!)}</span>
						<span class="model-sub">Your choice</span>
					</span>
					<Button variant="secondary" onclick={() => modelSlots.choose(pinnedPicture, null)}>Use the recommended</Button>
				</div>
			{/if}
		</div>
	{/if}
</section>

<style>
	.models {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}

	.sec-hint {
		margin: 0;
		font-size: 13px;
		line-height: 1.5;
		color: var(--color-foreground-subtle);
	}

	.rows {
		margin-top: 4px;
	}

	.row {
		display: grid;
		grid-template-columns: minmax(0, 1fr) auto 15px;
		align-items: center;
		gap: 16px;
		width: calc(100% + 24px);
		min-height: 56px;
		margin: 0 -12px;
		padding: 6px 12px;
		box-sizing: border-box;
		border: none;
		border-radius: var(--radius-md);
		background: none;
		font: inherit;
		text-align: left;
		color: var(--color-foreground);
		cursor: pointer;
		transition: background var(--motion-hover-out) var(--ease-premium);
	}

	button.row:hover {
		background: var(--color-surface-elevated);
		transition-duration: var(--motion-hover-in);
	}

	.row.still {
		grid-template-columns: minmax(0, 1fr) auto auto;
		cursor: default;
	}

	.role,
	.model {
		display: flex;
		flex-direction: column;
		min-width: 0;
	}

	.model {
		align-items: flex-end;
		text-align: right;
	}

	.role-name,
	.model-name {
		font-size: 15px;
	}

	.role-caption,
	.model-sub {
		font-size: 12px;
		color: var(--color-foreground-subtle);
	}

	.chev {
		color: var(--color-foreground-muted);
	}

	@media (prefers-reduced-motion: reduce) {
		.row {
			transition: none;
		}
	}
</style>
