<script lang="ts">
	import { onMount } from "svelte";
	import UniversalPicker from "./UniversalPicker.svelte";

	interface Model {
		id?: string;
		model_id?: string;
		display_name?: string;
		displayName?: string;
		provider?: string;
		input_cost_per_1k?: number | null;
		output_cost_per_1k?: number | null;
	}

	interface SlotConfig {
		key: SlotKey;
		label: string;
		description: string;
		dbField: string;
		/** Pre-slot-system column. Cleared alongside dbField so an old pinned
		 *  value can't resurrect itself through the backend's `.or()` fallback. */
		legacyField?: string;
	}

	type SlotKey = "chat" | "lite" | "coding" | "image";

	/** Sentinel for "no pin — follow the Virtues default". Empty string, because
	 *  that is what an unset slot already reads as, and it round-trips to a JSON
	 *  `null` in saveSlot(). */
	const DEFAULT = "";

	const SLOTS: SlotConfig[] = [
		{
			key: "chat",
			label: "Chat",
			description: "Default for conversations",
			dbField: "chat_model_id",
			legacyField: "default_model_id",
		},
		{
			key: "lite",
			label: "Lite",
			description: "Titles, summaries, background work",
			dbField: "lite_model_id",
			legacyField: "background_model_id",
		},
		{
			key: "coding",
			label: "Coding",
			description: "Code generation",
			dbField: "coding_model_id",
		},
		{
			key: "image",
			label: "Image",
			description: "Text-to-image",
			dbField: "image_model_id",
		},
	];

	let loading = $state(true);
	let error = $state<string | null>(null);
	let saveError = $state<string | null>(null);
	let models = $state<Model[]>([]);
	/** What each slot resolves to when unpinned — served live by the box, which
	 *  gets it from virtues-api. Swapping a default is a cloud change, not a
	 *  release, so this is fetched rather than hardcoded. */
	let slotDefaults = $state<Record<string, string>>({});
	let slotValues = $state<Record<SlotKey, string>>({
		chat: DEFAULT,
		lite: DEFAULT,
		coding: DEFAULT,
		image: DEFAULT,
	});

	onMount(loadData);

	async function loadData() {
		loading = true;
		error = null;
		try {
			// /recommended carries both the picker list and the live slot map.
			const [modelsRes, profileRes] = await Promise.all([
				fetch("/api/models/recommended"),
				fetch("/api/assistant-profile"),
			]);

			if (!modelsRes.ok) throw new Error("Failed to load models");

			const data = await modelsRes.json();
			models = Array.isArray(data) ? data : data.data || [];
			slotDefaults = data.slots || {};

			if (profileRes.ok) {
				const profile = await profileRes.json();
				// NULL (or a legacy NULL) means unpinned → Virtues default.
				slotValues = {
					chat:
						profile.chat_model_id ||
						profile.default_model_id ||
						DEFAULT,
					lite:
						profile.lite_model_id ||
						profile.background_model_id ||
						DEFAULT,
					coding: profile.coding_model_id || DEFAULT,
					image: profile.image_model_id || DEFAULT,
				};
			}
		} catch (e) {
			error =
				e instanceof Error
					? e.message
					: "Failed to load model settings";
			console.error("Failed to load model settings:", e);
		} finally {
			loading = false;
		}
	}

	/** The list a slot shows: the "Virtues default" row, then the models. */
	function optionsFor(slot: SlotConfig): Model[] {
		const resolved = slotDefaults[slot.key];
		const name = resolved ? modelName(byId(resolved)) || resolved : null;
		return [
			{
				model_id: DEFAULT,
				display_name: name
					? `Virtues default · ${name}`
					: "Virtues default",
			},
			...models,
		];
	}

	function byId(id: string): Model | undefined {
		return models.find((m) => modelId(m) === id);
	}

	function modelId(model: Model): string {
		return model.model_id ?? model.id ?? "";
	}

	function modelName(model?: Model): string {
		if (!model) return "";
		return model.display_name || model.displayName || modelId(model);
	}

	async function saveSlot(slot: SlotConfig, model: Model) {
		const id = modelId(model);
		const previous = slotValues[slot.key];
		slotValues[slot.key] = id;
		saveError = null;

		// `null` clears the pin — the backend reads that as "follow the default"
		// (a plain omitted key would be a no-op; see assistant_profile.rs).
		const body: Record<string, string | null> = {
			[slot.dbField]: id === DEFAULT ? null : id,
		};
		if (slot.legacyField && id === DEFAULT) body[slot.legacyField] = null;

		try {
			const res = await fetch("/api/assistant-profile", {
				method: "PUT",
				headers: { "Content-Type": "application/json" },
				body: JSON.stringify(body),
			});
			if (!res.ok) throw new Error(`${res.status} ${res.statusText}`);
		} catch (e) {
			// Roll the optimistic update back — showing a value that was never
			// saved is worse than showing the old one.
			slotValues[slot.key] = previous;
			saveError = `Couldn't save ${slot.label}. ${e instanceof Error ? e.message : ""}`.trim();
			console.error(`Failed to save ${slot.key} model:`, e);
		}
	}
</script>

<div class="bg-surface border border-border rounded-lg">
	<div
		class="flex items-center justify-between px-4 py-3 border-b border-border"
	>
		<h2 class="text-sm font-medium text-foreground">AI Models</h2>
		{#if saveError}
			<span class="text-xs text-error">{saveError}</span>
		{/if}
	</div>

	{#if loading}
		<div class="text-center py-6 text-sm text-foreground-muted">
			Loading models...
		</div>
	{:else if error}
		<div class="text-center py-6 text-sm text-error">{error}</div>
	{:else}
		<div class="grid grid-cols-2 gap-4 p-4">
			{#each SLOTS as slot}
				{@const pinned = slotValues[slot.key] !== DEFAULT}
				<div>
					<div class="text-sm font-medium text-foreground mb-2">
						{slot.label}
						<span class="font-normal text-foreground-subtle"
							>· {slot.description}</span
						>
					</div>
					<UniversalPicker
						items={optionsFor(slot)}
						value={slotValues[slot.key]}
						getKey={(m) => modelId(m) || "__default__"}
						getValue={(m) => modelId(m)}
						onSelect={(m) => saveSlot(slot, m)}
						width="w-full"
						maxHeight="max-h-64"
					>
						{#snippet trigger(currentModel, disabled, open)}
							<div
								class="w-full px-3 py-2 bg-background border border-border rounded-md text-sm flex items-center justify-between hover:border-border-strong transition-colors {pinned
									? 'text-foreground'
									: 'text-foreground-muted'}"
							>
								<span class="truncate"
									>{currentModel
										? modelName(currentModel)
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
							{@const isDefaultRow = modelId(model) === DEFAULT}
							<div
								class="px-3 py-2 flex items-center justify-between {isDefaultRow
									? 'border-b border-border'
									: ''}"
							>
								<span
									class="text-sm truncate {isDefaultRow
										? 'text-foreground-muted'
										: 'text-foreground'}"
									>{modelName(model)}</span
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
				</div>
			{/each}
		</div>
		<div class="px-4 pb-4 -mt-1">
			<p class="text-xs text-foreground-subtle">
				A slot on <span class="text-foreground-muted"
					>Virtues default</span
				>
				follows whatever model we currently recommend, and moves when we move
				it. Pick a model to pin it — we won't change it.
			</p>
		</div>
	{/if}
</div>
