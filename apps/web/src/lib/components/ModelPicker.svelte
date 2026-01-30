<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import { fade } from "svelte/transition";
	import type { ModelOption } from "$lib/config/models";
	import { getModels, getDefaultModel, isLoading, getError } from "$lib/stores/models.svelte";
	import UniversalSelect from "./UniversalSelect.svelte";

	interface Props {
		value?: ModelOption;
		disabled?: boolean;
		onSelect?: (model: ModelOption) => void;
	}

	let {
		value,
		disabled = false,
		onSelect,
	}: Props = $props();

	// Get models from store (reactive)
	const models = $derived(getModels());
	const loading = $derived(isLoading());
	const error = $derived(getError());
	const defaultModel = $derived(getDefaultModel());

	// Local state for UniversalSelect binding
	// svelte-ignore state_referenced_locally
	let localValue = $state<ModelOption | undefined>(value);

	// Sync local value with prop value
	$effect(() => {
		if (value && value.id !== localValue?.id) {
			localValue = value;
		}
	});

	// Initialize localValue with defaultModel when models load
	$effect(() => {
		if (!localValue && defaultModel && models.length > 0) {
			localValue = defaultModel;
		}
	});

	// Handle selection from UniversalSelect
	function handleSelect(model: ModelOption) {
		localValue = model;
		onSelect?.(model);
	}

	function getProviderDisplayName(model: ModelOption): string {
		if (model.id.toLowerCase().includes('cerebras') || model.provider.toLowerCase() === 'cerebras') {
			return 'Cerebras';
		}
		return model.provider;
	}

	function getModelKey(model: ModelOption) {
		return model.id;
	}
</script>

{#if loading && !localValue}
	<div
		class="flex h-7 w-7 items-center justify-center rounded-md text-foreground-muted animate-pulse"
		title="Loading models..."
	>
		<Icon icon="ri:robot-fill" width="16" />
	</div>
{:else if error && !localValue}
	<!-- Error state -->
	<div class="flex h-7 w-7 items-center justify-center text-error" title="Error loading models">
		<Icon icon="ri:error-warning-line" width="16" />
	</div>
{:else}
	<!-- Model picker when loaded or has value -->
	<UniversalSelect
		bind:value={localValue}
		items={models}
		disabled={disabled || (loading && models.length === 0)}
		width="min-w-[200px]"
		getKey={getModelKey}
		onSelect={handleSelect}
	>
		{#snippet trigger(model: ModelOption | undefined, isDisabled: boolean, open: boolean)}
			<div
				class="flex h-7 w-7 items-center justify-center rounded-md text-foreground-muted hover:bg-surface-elevated hover:text-foreground transition-all duration-150 cursor-pointer"
				class:bg-surface-elevated={open}
				class:text-foreground={open}
				title="Select model: {model?.displayName || 'Select Model'}"
			>
				<Icon icon="ri:robot-fill" width="16" />
			</div>
		{/snippet}

		{#snippet item(model: ModelOption, isSelected: boolean)}
			<div class="px-3 py-2 flex items-center justify-between gap-3 group">
				<div class="flex flex-col">
					<span class="text-xs font-medium {isSelected ? 'text-primary' : 'text-foreground'}">
						{model.displayName}
					</span>
					<span class="text-[9px] text-foreground-muted uppercase tracking-wider">
						{getProviderDisplayName(model)}
					</span>
				</div>
				{#if isSelected}
					<Icon
						icon="ri:check-line"
						class="text-primary shrink-0"
						width="14"
					/>
				{/if}
			</div>
		{/snippet}
	</UniversalSelect>
{/if}
