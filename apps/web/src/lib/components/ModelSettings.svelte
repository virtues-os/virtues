<script lang="ts">
	/**
	 * The four slot pickers at the top of Settings → Assistant.
	 *
	 * The full catalog sits at the bottom of the same page (ModelCatalog). Both
	 * read and write `modelSlots`, so a choice made in one shows in the other at
	 * once. There used to be a separate Models page with its own copy of the
	 * pin controls and its own words for them, which is how one setting came to
	 * have five names.
	 */
	import { onMount } from "svelte";
	import UniversalPicker from "./UniversalPicker.svelte";
	import TextAction from "./TextAction.svelte";
	import {
		modelSlots,
		SLOTS,
		type SlotConfig,
		type CatalogModel,
	} from "$lib/stores/modelSlots.svelte";

	/** The Recommended row's key. Empty, because the picker needs a string and
	 *  no model id is empty. It saves as a JSON `null`. */
	const RECOMMENDED = "";

	type Option = Pick<CatalogModel, "model_id" | "display_name"> &
		Partial<CatalogModel>;

	onMount(() => modelSlots.load());

	/** The list a slot shows: the Recommended row, then the suggested set,
	 *  plus whatever this slot is set to now (so a catalog choice still
	 *  displays here). Browsing all ~240 models is the table below. */
	function optionsFor(slot: SlotConfig): Option[] {
		const name = modelSlots.nameOf(modelSlots.recommended[slot.key]);
		const current = modelSlots.chosen[slot.key];
		return [
			{
				model_id: RECOMMENDED,
				display_name: name ? `Recommended · ${name}` : "Recommended",
			},
			...modelSlots.models.filter(
				(m) => m.recommended || m.model_id === current,
			),
		];
	}

	/** Section label for the dropdown. Empty = the ungrouped Recommended row.
	 *  The suggested set was grouped as "Recommended" too, the same word as the
	 *  row above it that means "follow Virtues", so it is "Suggested" now. */
	function groupOf(model: Option): string {
		if (model.model_id === RECOMMENDED) return "";
		return model.recommended ? "Suggested" : "Chosen from the catalog";
	}

	function searchTextOf(model: Option): string {
		return [model.display_name, model.provider ?? "", model.model_id].join(" ");
	}

	function scrollToCatalog() {
		document
			.getElementById("model-catalog")
			?.scrollIntoView({ behavior: "smooth", block: "start" });
	}
</script>

<div class="bg-surface border border-border rounded-lg">
	<div
		class="flex items-center justify-between px-4 py-3 border-b border-border"
	>
		<h2 class="text-sm font-medium text-foreground">AI Models</h2>
		{#if modelSlots.saveError}
			<span class="text-xs text-error">{modelSlots.saveError}</span>
		{/if}
	</div>

	{#if !modelSlots.loaded && modelSlots.loading}
		<div class="text-center py-6 text-sm text-foreground-muted">
			Loading models...
		</div>
	{:else if modelSlots.error}
		<div class="text-center py-6 text-sm text-error">{modelSlots.error}</div>
	{:else}
		<div class="grid grid-cols-2 gap-4 p-4">
			{#each SLOTS as slot}
				{@const chosen = modelSlots.chosen[slot.key]}
				{@const recommended = modelSlots.recommended[slot.key]}
				<div>
					<div class="text-sm font-medium text-foreground mb-2">
						{slot.label}
						<span class="font-normal text-foreground-subtle"
							>· {slot.description}</span
						>
					</div>
					<UniversalPicker
						items={optionsFor(slot)}
						value={chosen ?? RECOMMENDED}
						getKey={(m) => m.model_id || "__recommended__"}
						getValue={(m) => m.model_id}
						onSelect={(m) =>
							modelSlots.choose(
								slot,
								m.model_id === RECOMMENDED ? null : m.model_id,
							)}
						width="w-full"
						maxHeight="max-h-64"
						searchable={true}
						getSearchText={searchTextOf}
						getGroup={groupOf}
						searchPlaceholder="Search models…"
					>
						{#snippet trigger(currentModel, disabled, open)}
							<div
								class="w-full px-3 py-2 bg-background border border-border rounded-md text-sm flex items-center justify-between hover:border-border-strong transition-colors {chosen
									? 'text-foreground'
									: 'text-foreground-muted'}"
							>
								<span class="truncate"
									>{currentModel
										? currentModel.display_name
										: "Select model..."}</span
								>
								<svg
									class="w-4 h-4 text-foreground-subtle shrink-0 ml-2 transition-transform {open
										? 'rotate-180'
										: ''}"
									fill="none"
									stroke="currentColor"
									viewBox="0 0 24 24"
								>
									<path
										stroke-linecap="round"
										stroke-linejoin="round"
										stroke-width="2"
										d="M19 9l-7 7-7-7"
									/>
								</svg>
							</div>
						{/snippet}
						{#snippet item(model, isSelected)}
							{@const isRecommendedRow = model.model_id === RECOMMENDED}
							<div
								class="px-3 py-2 flex items-center justify-between gap-2 {isRecommendedRow
									? 'border-b border-border'
									: ''}"
							>
								<span
									class="text-sm truncate min-w-0 {isRecommendedRow
										? 'text-foreground-muted'
										: 'text-foreground'}"
									>{model.display_name}{#if model.provider && !isRecommendedRow}<span
											class="text-foreground-subtle"
										>
											· {model.provider}</span
										>{/if}</span
								>
								{#if isSelected}
									<svg
										class="w-4 h-4 text-primary shrink-0 ml-2"
										fill="none"
										stroke="currentColor"
										viewBox="0 0 24 24"
									>
										<path
											stroke-linecap="round"
											stroke-linejoin="round"
											stroke-width="2"
											d="M5 13l4 4L19 7"
										/>
									</svg>
								{/if}
							</div>
						{/snippet}
					</UniversalPicker>
					{#if chosen}
						<div class="mt-1.5 flex items-center gap-2 flex-wrap text-xs text-foreground-subtle">
							<span>
								{#if recommended && recommended === chosen}
									Your choice, same as recommended. It stays if the recommendation changes.
								{:else if recommended}
									Your choice. Recommended: {modelSlots.nameOf(recommended)}.
								{:else}
									Your choice.
								{/if}
							</span>
							<TextAction quiet onclick={() => modelSlots.choose(slot, null)}>
								Use recommended
							</TextAction>
						</div>
					{/if}
				</div>
			{/each}
		</div>
		<div class="px-4 pb-4 -mt-1 space-y-1.5">
			<p class="text-xs text-foreground-subtle">
				A slot on <span class="text-foreground-muted">Recommended</span>
				changes when Virtues recommends a different model. Choose a model to
				keep it until you change it.
			</p>
			<TextAction quiet onclick={scrollToCatalog}>
				Compare all {modelSlots.models.length} models below
			</TextAction>
		</div>
	{/if}
</div>
