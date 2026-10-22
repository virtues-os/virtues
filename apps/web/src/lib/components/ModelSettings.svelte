<script lang="ts">
	/**
	 * Models, on the Assistant page: one native select per role.
	 *
	 * A select, not a searchable picker: each role offers a handful of
	 * models. Comparing the whole catalog is the page this section links to
	 * (ModelCatalogView), and both write through `modelSlots`.
	 */
	import { onMount } from "svelte";
	import { Button } from "$lib";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { modelSlots, type SlotConfig } from "$lib/stores/modelSlots.svelte";

	// Every section on the Assistant page takes the assistant's name. This
	// one's copy has no use for it yet.
	let { assistantName = "Ari" }: { assistantName?: string } = $props();

	/** The Recommended option's value. No model id is empty, and it saves
	 *  as a JSON `null`. */
	const RECOMMENDED = "";

	onMount(() => modelSlots.load());

	/** Pictures appear only while pinned: the image role is a system role,
	 *  so the row exists to let a stored choice go back to the
	 *  recommendation, not to invite one. */
	const roles = $derived(
		modelSlots.slots.filter((s) => s.language || modelSlots.chosen[s.key]),
	);

	/** Named only when the catalog lists the model. The catalog lists language
	 *  models, so a picture model may be missing, and a bare id in the menu
	 *  reads as a fault. */
	function recommendedLabel(slot: SlotConfig): string {
		const name = modelSlots.catalogName(modelSlots.recommended[slot.key]);
		return name ? `Recommended (${name})` : "Recommended";
	}

	/** The pinned model when neither the Recommended option nor a suggestion
	 *  already offers it, so the select can show what is stored. */
	function ownChoice(slot: SlotConfig): string | null {
		const id = modelSlots.chosen[slot.key];
		if (!id) return null;
		return modelSlots.suggestedFor(slot).some((m) => m.model_id === id)
			? null
			: id;
	}

	/** After the save settles, the select is set from the store. A save that
	 *  fails within the same tick rolls the store back to the value it had,
	 *  so Svelte sees no change and would leave the person's pick on screen. */
	async function onChange(slot: SlotConfig, select: HTMLSelectElement) {
		const value = select.value;
		await modelSlots.choose(slot, value === RECOMMENDED ? null : value);
		select.value = modelSlots.chosen[slot.key] ?? RECOMMENDED;
	}

	function openCatalog() {
		windowShellStore.navigate("/virtues/assistant/models", { label: "Settings" });
	}
</script>

<section class="models">
	<h2 class="settings-label">Models</h2>

	{#if !modelSlots.loaded && modelSlots.error}
		<p class="load-error" role="alert">{modelSlots.error}</p>
		<div>
			<Button variant="secondary" size="sm" onclick={() => modelSlots.load(true)}>
				Try again
			</Button>
		</div>
	{:else if !modelSlots.loaded}
		<!-- In the rows' own shape, so nothing moves when they land. -->
		<div class="roles" aria-busy="true">
			{#each [0, 1, 2] as i (i)}
				<div class="role">
					<span class="ph ph-label"></span>
					<span class="ph ph-select"></span>
				</div>
			{/each}
		</div>
	{:else}
		<div class="roles">
			{#each roles as slot (slot.key)}
				{@const own = ownChoice(slot)}
				{@const suggested = modelSlots.suggestedFor(slot)}
				{@const saveError = modelSlots.saveErrors[slot.key]}
				<div class="role">
					<div class="role-words">
						<label class="role-label" for="model-role-{slot.key}">{slot.label}</label>
						{#if slot.caption}
							<span class="role-caption" id="model-role-{slot.key}-caption">{slot.caption}</span>
						{/if}
					</div>
					<select
						id="model-role-{slot.key}"
						aria-describedby={slot.caption ? `model-role-${slot.key}-caption` : undefined}
						value={modelSlots.chosen[slot.key] ?? RECOMMENDED}
						onchange={(e) => onChange(slot, e.currentTarget)}
					>
						<option value={RECOMMENDED}>{recommendedLabel(slot)}</option>
						{#if suggested.length}
							<optgroup label="Suggested">
								{#each suggested as m (m.model_id)}
									<option value={m.model_id}>{m.display_name}</option>
								{/each}
							</optgroup>
						{/if}
						{#if own}
							<optgroup label="Your choice">
								<option value={own}>{modelSlots.nameOf(own)}</option>
							</optgroup>
						{/if}
					</select>
					{#if saveError}
						<p class="role-error" role="alert">{saveError.message}</p>
					{/if}
				</div>
			{/each}
		</div>

		<p class="sec-hint">
			Recommended follows Virtues' pick as it changes. A model you choose stays
			until you change it.
		</p>
		{#if modelSlots.byoKey}
			<!-- Billing's model map is left out on purpose: it re-addresses a
			     recommended model but passes a chosen one through unchanged
			     (apply_byo_model, virtues_api/client.rs). -->
			<p class="sec-hint">
				Your own AI key is on, so your provider runs every AI request, at its own
				prices and under its own retention terms.
			</p>
		{/if}

		{#if modelSlots.models.length}
			<div>
				<Button variant="secondary" size="sm" onclick={openCatalog}>
					Compare all {modelSlots.models.length} models
				</Button>
			</div>
		{/if}
	{/if}
</section>

<style>
	.models {
		display: flex;
		flex-direction: column;
		gap: 12px;
	}

	.roles {
		display: flex;
		flex-direction: column;
		gap: 12px;
		container-type: inline-size;
	}

	.role {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		justify-content: space-between;
		column-gap: 16px;
		row-gap: 8px;
	}

	.role-words {
		display: flex;
		flex-direction: column;
		min-width: 0;
	}

	.role-label {
		font-size: 15px;
		color: var(--color-foreground);
	}

	.role-caption {
		font-size: 12px;
		color: var(--color-foreground-subtle);
	}

	/* DisplayView's face picker, at 14px: a model name is read, not scanned. */
	select {
		font: inherit;
		font-size: 14px;
		height: 40px;
		padding: 0 12px;
		border-radius: var(--radius-md);
		border: 1px solid var(--color-border);
		background: var(--color-surface);
		color: var(--color-foreground);
		/* One width for every role, so the selects line up as a column. */
		width: 18rem;
		max-width: 100%;
		min-width: 0;
	}

	@media (pointer: coarse) {
		select {
			height: 44px;
		}
	}

	.role-error {
		flex-basis: 100%;
		margin: 0;
		font-size: 12px;
		color: var(--color-error);
	}

	@container (max-width: 479px) {
		.role {
			flex-direction: column;
			align-items: stretch;
		}
		select {
			width: 100%;
		}
		.role-error {
			flex-basis: auto;
		}
	}

	.sec-hint {
		margin: 0;
		font-size: 12px;
		color: var(--color-foreground-subtle);
		max-width: 60ch;
	}

	.load-error {
		margin: 0;
		font-size: 12px;
		color: var(--color-error);
	}

	/* Placeholder bars, sized like a label and a select. */
	.ph {
		display: block;
		border-radius: var(--radius-md);
		background: var(--hover-bg);
	}
	.ph-label {
		width: 8rem;
		height: 16px;
	}
	.ph-select {
		width: 16rem;
		max-width: 100%;
		height: 40px;
	}
</style>
