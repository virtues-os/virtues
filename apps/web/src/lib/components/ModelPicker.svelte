<script lang="ts">
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

	// Capture initial value (intentionally captures initial value only)
	// svelte-ignore state_referenced_locally
	const initialValue = value;
	
	// Local state for UniversalSelect binding
	let localValue = $state<ModelOption | undefined>(initialValue);

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

	// Helper function to get icon for provider
	function getProviderIcon(provider: string): string {
		switch (provider.toLowerCase()) {
			case 'anthropic':
				return 'ri:claude-fill';
			case 'openai':
				return 'simple-icons:openai';
			case 'google':
				return 'simple-icons:google';
			case 'xai':
				return 'ri:twitter-x-fill';
			case 'moonshot ai':
				return 'ri:moon-fill';
			default:
				return 'ri:robot-fill';
		}
	}

	// Helper function to get icon color for provider
	function getProviderIconColor(provider: string): string {
		switch (provider.toLowerCase()) {
			case 'anthropic':
				return 'text-orange-600';
			case 'openai':
				return 'text-green-600';
			case 'google':
				return 'text-blue-600';
			case 'xai':
				return 'text-foreground';
			case 'moonshot ai':
				return 'text-purple-600';
			default:
				return 'text-foreground-muted';
		}
	}

	function getModelKey(model: ModelOption) {
		return model.id;
	}
</script>

{#if loading}
	<!-- Loading state -->
	<div class="flex items-center gap-2 px-3 py-1.5 text-foreground-subtle">
		<iconify-icon icon="ri:loader-4-line" class="animate-spin" width="16"></iconify-icon>
		<span class="text-sm">Loading models...</span>
	</div>
{:else if error}
	<!-- Error state -->
	<div class="flex items-center gap-2 px-3 py-1.5 text-error">
		<iconify-icon icon="ri:error-warning-line" width="16"></iconify-icon>
		<span class="text-sm">Error loading models</span>
	</div>
{:else if models.length > 0}
	<!-- Model picker when loaded -->
	<UniversalSelect
		bind:value={localValue}
		items={models}
		disabled={disabled || loading}
		width="w-64"
		getKey={getModelKey}
		onSelect={handleSelect}
	>
		{#snippet trigger(model: ModelOption, isDisabled: boolean, open: boolean)}
			<div
				class="flex items-center gap-2"
				class:px-3={!isDisabled}
				class:py-1.5={!isDisabled}
				class:w-8={isDisabled}
				class:h-8={isDisabled}
				class:justify-center={isDisabled}
				aria-label="Select model"
			>
				{#if isDisabled}
					<iconify-icon
						icon={getProviderIcon(model.provider)}
						class="text-foreground-muted"
						width="16"
						in:fade={{ duration: 200 }}
					></iconify-icon>
				{:else}
					<span class="text-foreground-muted">{model.displayName}</span>
					<iconify-icon
						icon="ri:arrow-down-s-line"
						class="text-foreground-subtle transition-transform duration-200"
						class:rotate-180={open}
						width="16"
					></iconify-icon>
				{/if}
			</div>
		{/snippet}

	{#snippet item(model: ModelOption, isSelected: boolean)}
		<div class="px-4 py-2.5">
			<div class="flex items-center justify-between gap-2">
				<div class="flex items-center gap-2">
					<iconify-icon
						icon={getProviderIcon(model.provider)}
						class={getProviderIconColor(model.provider)}
						width="16"
					></iconify-icon>
					<span class="text-sm text-foreground">
						{model.displayName}
						{#if defaultModel && model.id === defaultModel.id}
							<span class="text-xs text-foreground-subtle ml-1">(recommended)</span>
						{/if}
					</span>
				</div>
				{#if isSelected}
					<iconify-icon
						icon="ri:check-line"
						class="text-primary flex-shrink-0"
						width="16"
					></iconify-icon>
				{/if}
			</div>
		</div>
	{/snippet}
	</UniversalSelect>
{/if}
