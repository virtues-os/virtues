<script lang="ts">
	import { onMount } from 'svelte';
	import Icon from '$lib/components/Icon.svelte';
	import UniversalSelect from './UniversalSelect.svelte';
	import { personaStore, type Persona } from '$lib/stores/personas.svelte';

	interface Props {
		value: string;
		disabled?: boolean;
		onSelect?: (persona: Persona) => void;
	}

	let { value = $bindable<string>('standard'), disabled = false, onSelect }: Props = $props();

	onMount(() => {
		personaStore.init();
	});

	function getPersonaKey(p: Persona) {
		return p.id;
	}

	function getPersonaValue(p: Persona): string {
		return p.id;
	}

	const currentPersona = $derived(personaStore.personas.find((p) => p.id === value));

	// Use visible personas (excluding hidden)
	const personas = $derived(personaStore.visiblePersonas);
</script>

<UniversalSelect
	bind:value
	items={personas}
	{disabled}
	width="min-w-[200px]"
	getKey={getPersonaKey}
	getValue={getPersonaValue}
	{onSelect}
>
	{#snippet trigger(persona: Persona | undefined, isDisabled: boolean, open: boolean)}
		<div
			class="flex h-7 w-7 items-center justify-center rounded-md text-foreground-muted hover:bg-surface-elevated hover:text-foreground transition-all duration-150 cursor-pointer"
			class:bg-surface-elevated={open}
			class:text-foreground={open}
			title="AI Persona: {currentPersona?.title || 'Select'}"
		>
			<Icon icon="ri:user-settings-line" width="16" />
		</div>
	{/snippet}

	{#snippet item(persona: Persona, isSelected: boolean)}
		<div class="px-3 py-2 flex items-center justify-between gap-3">
			<div class="flex items-center gap-2">
				<Icon icon="ri:user-line" width="14" class="text-foreground-muted" />
				<div class="flex flex-col">
					<span class="text-xs font-medium {isSelected ? 'text-primary' : 'text-foreground'}">
						{persona.title}
					</span>
					{#if persona.is_system}
						<span class="text-[9px] text-foreground-muted">System</span>
					{:else}
						<span class="text-[9px] text-foreground-muted">Custom</span>
					{/if}
				</div>
			</div>
			{#if isSelected}
				<Icon icon="ri:check-line" class="text-primary shrink-0" width="14" />
			{/if}
		</div>
	{/snippet}
</UniversalSelect>
