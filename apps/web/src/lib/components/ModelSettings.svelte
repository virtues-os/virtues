<script lang="ts">
	import { onMount } from "svelte";
	import UniversalPicker from "./UniversalPicker.svelte";

	interface Model {
		id?: string;
		model_id?: string;
		display_name?: string;
		displayName?: string;
		provider?: string;
		slot?: string;
	}

	interface SlotConfig {
		key: string;
		label: string;
		description: string;
		dbField: string;
	}

	const SLOTS: SlotConfig[] = [
		{
			key: "chat",
			label: "Chat",
			description: "Default for conversations",
			dbField: "chat_model_id",
		},
		{
			key: "lite",
			label: "Lite",
			description: "Titles & summaries",
			dbField: "lite_model_id",
		},
		{
			key: "reasoning",
			label: "Reasoning",
			description: "Complex analysis",
			dbField: "reasoning_model_id",
		},
		{
			key: "coding",
			label: "Coding",
			description: "Code generation",
			dbField: "coding_model_id",
		},
	];

	let loading = $state(true);
	let error = $state<string | null>(null);
	let models = $state<Model[]>([]);
	let slotValues = $state<Record<string, string>>({
		chat: "",
		lite: "",
		reasoning: "",
		coding: "",
	});

	onMount(async () => {
		await loadData();
	});

	async function loadData() {
		loading = true;
		error = null;
		try {
			const [modelsRes, profileRes] = await Promise.all([
				fetch("/api/models"),
				fetch("/api/assistant-profile"),
			]);

			if (!modelsRes.ok) {
				throw new Error("Failed to load models");
			}

			const data = await modelsRes.json();
			models = Array.isArray(data) ? data : data.data || [];

			if (profileRes.ok) {
				const profile = await profileRes.json();
				slotValues = {
					chat:
						profile.chat_model_id || profile.default_model_id || "",
					lite:
						profile.lite_model_id ||
						profile.background_model_id ||
						"",
					reasoning: profile.reasoning_model_id || "",
					coding: profile.coding_model_id || "",
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

	async function saveSlot(slot: SlotConfig, model: Model) {
		const modelId = getModelId(model);
		slotValues[slot.key] = modelId;
		try {
			await fetch("/api/assistant-profile", {
				method: "PUT",
				headers: { "Content-Type": "application/json" },
				body: JSON.stringify({ [slot.dbField]: modelId }),
			});
		} catch (error) {
			console.error(`Failed to save ${slot.key} model:`, error);
		}
	}

	function getModelId(model: Model): string {
		return model.model_id || model.id || "";
	}

	function getModelName(model: Model): string {
		return model.display_name || model.displayName || getModelId(model);
	}

	function getSelectedModel(slotKey: string): Model | undefined {
		const selectedId = slotValues[slotKey];
		return models.find((m) => getModelId(m) === selectedId);
	}
</script>

<div class="bg-surface border border-border rounded-lg">
	<div
		class="flex items-center justify-between px-4 py-3 border-b border-border"
	>
		<h2 class="text-sm font-medium text-foreground">AI Models</h2>
	</div>

	{#if loading}
		<div class="text-center py-6 text-sm text-foreground-muted">
			Loading models...
		</div>
	{:else if error}
		<div class="text-center py-6 text-sm text-red-500">{error}</div>
	{:else}
		<div class="grid grid-cols-2 gap-4 p-4">
			{#each SLOTS as slot}
				<div>
					<div class="text-sm font-medium text-foreground mb-2">
						{slot.label}
						<span class="font-normal text-foreground-subtle"
							>· {slot.description}</span
						>
					</div>
					<UniversalPicker
						items={models}
						value={slotValues[slot.key]}
						getKey={(m) => getModelId(m)}
						getValue={(m) => getModelId(m)}
						onSelect={(m) => saveSlot(slot, m)}
						width="w-full"
						maxHeight="max-h-64"
					>
						{#snippet trigger(currentModel, disabled, open)}
							<div
								class="w-full px-3 py-2 bg-background border border-border rounded-md text-sm text-foreground flex items-center justify-between hover:border-border-strong transition-colors"
							>
								<span class="truncate"
									>{currentModel
										? getModelName(currentModel)
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
							<div
								class="px-3 py-2 flex items-center justify-between"
							>
								<span class="text-sm text-foreground truncate"
									>{getModelName(model)}</span
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
	{/if}
</div>
