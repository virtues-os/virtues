<script lang="ts">
	import { onMount } from 'svelte';
	import Icon from '$lib/components/Icon.svelte';
	import { contextMenu, type ContextMenuItem } from '$lib/stores/contextMenu.svelte';
	import { getModels, isLoading } from '$lib/stores/models.svelte';
	import { getProviderIcon } from '$lib/config/providerIcons';
	import { personaStore } from '$lib/stores/personas.svelte';
	import type { ModelOption } from '$lib/config/models';

	interface Props {
		selectedModel?: ModelOption;
		selectedPersona: string;
		onModelSelect?: (model: ModelOption) => void;
		onPersonaSelect?: (personaId: string) => void;
	}

	let { selectedModel, selectedPersona, onModelSelect, onPersonaSelect }: Props = $props();

	const models = $derived(getModels());
	const loading = $derived(isLoading());
	const personas = $derived(personaStore.visiblePersonas);

	onMount(() => {
		personaStore.init();
	});

	function handleClick(event: MouseEvent) {
		const button = event.currentTarget as HTMLElement;
		const rect = button.getBoundingClientRect();

		const menuItems: ContextMenuItem[] = [];

		// Model submenu
		if (!loading && models.length > 0) {
			menuItems.push({
				id: 'model',
				label: 'Model',
				icon: selectedModel ? getProviderIcon(selectedModel.provider) : 'ri:robot-fill',
				submenu: models.map((model) => ({
					id: `model-${model.id}`,
					label: model.displayName,
					icon: getProviderIcon(model.provider),
					checked: selectedModel?.id === model.id,
					action: () => onModelSelect?.(model)
				}))
			});
		}

		// Persona submenu
		if (personas.length > 0) {
			menuItems.push({
				id: 'persona',
				label: 'Persona',
				icon: 'ri:user-line',
				submenu: personas.map((persona) => ({
					id: `persona-${persona.id}`,
					label: persona.title,
					icon: 'ri:user-line',
					checked: selectedPersona === persona.id,
					action: () => onPersonaSelect?.(persona.id)
				}))
			});
		}

		// Use anchor-based positioning with Floating UI
		const anchor = {
			x: rect.left,
			y: rect.top,
			width: rect.width,
			height: rect.height
		};

		contextMenu.show(
			{ x: rect.right, y: rect.top },
			menuItems,
			{ anchor, placement: 'top-end' }
		);
	}
</script>

<button
	class="toolbar-btn"
	onclick={handleClick}
	type="button"
	title="Settings"
>
	<Icon icon="ri:settings-3-line" width="16" />
</button>

<style>
	.toolbar-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 28px;
		height: 28px;
		background: none;
		border: none;
		border-radius: 50%;
		color: var(--color-foreground-muted);
		cursor: pointer;
		transition: all 0.15s ease;
	}

	.toolbar-btn:hover {
		background: var(--color-surface-elevated);
		color: var(--color-foreground);
	}
</style>
