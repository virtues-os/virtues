<script lang="ts">
	import Icon from '$lib/components/Icon.svelte';
	import UniversalPicker from './UniversalPicker.svelte';
	import { AGENT_MODES, type AgentMode, type AgentModeId } from '$lib/config/agentModes';

	interface Props {
		value: AgentModeId;
		disabled?: boolean;
		onSelect?: (mode: AgentMode) => void;
	}

	let { value = $bindable<AgentModeId>('chat'), disabled = false, onSelect }: Props = $props();

	function getModeKey(m: AgentMode) {
		return m.id;
	}

	function getModeValue(m: AgentMode): AgentModeId {
		return m.id;
	}

	function getCurrentMode(): AgentMode | undefined {
		return AGENT_MODES.find((m) => m.id === value);
	}
</script>

<UniversalPicker
	bind:value
	items={AGENT_MODES}
	{disabled}
	width="w-80"
	getKey={getModeKey}
	getValue={getModeValue}
	{onSelect}
>
	{#snippet trigger(mode: AgentMode | undefined, isDisabled: boolean, open: boolean)}
		{@const currentMode = getCurrentMode()}
		{@const hasColor = currentMode?.color != null}
		<div
			class="mode-trigger"
			class:open
			class:has-color={hasColor}
			style={hasColor ? `--mode-color: ${currentMode?.color}` : ''}
			title="Mode: {currentMode?.name || 'Agent'}"
		>
			<Icon icon={currentMode?.icon || 'ri:infinity-line'} width="14" />
			<span class="mode-name">{currentMode?.name || 'Agent'}</span>
		</div>
	{/snippet}

	{#snippet item(mode: AgentMode, isSelected: boolean)}
		<div class="px-3 py-2 flex items-center gap-2.5 whitespace-nowrap">
			<Icon
				icon={mode.icon}
				width="16"
				class="shrink-0 {mode.color ? '' : 'text-foreground-muted'}"
				style={mode.color ? `color: ${mode.color}` : ''}
			/>
			<span class="text-sm font-medium shrink-0 {isSelected ? 'text-primary' : 'text-foreground'}">
				{mode.name}
			</span>
			<span class="text-xs text-foreground-subtle truncate">{mode.description}</span>
			<span class="flex-1"></span>
			{#if isSelected}
				<Icon icon="ri:check-line" class="text-primary shrink-0" width="14" />
			{/if}
		</div>
	{/snippet}
</UniversalPicker>

<style>
	.mode-trigger {
		display: flex;
		align-items: center;
		gap: 6px;
		height: 28px;
		padding: 0 10px;
		border-radius: 100px;
		cursor: pointer;
		transition: all 0.15s ease;
		/* Default: no background, muted color */
		background: var(--color-surface-elevated);
		color: var(--color-foreground-muted);
	}

	.mode-trigger:hover {
		background: var(--color-border);
		color: var(--color-foreground);
	}

	.mode-trigger.open {
		background: var(--color-border);
		color: var(--color-foreground);
	}

	/* When mode has a color (chat, research) */
	.mode-trigger.has-color {
		background: color-mix(in srgb, var(--mode-color) 15%, transparent);
		color: var(--mode-color);
	}

	.mode-trigger.has-color:hover {
		background: color-mix(in srgb, var(--mode-color) 25%, transparent);
	}

	.mode-trigger.has-color.open {
		background: color-mix(in srgb, var(--mode-color) 25%, transparent);
	}

	.mode-name {
		font-size: 12px;
		font-weight: 500;
	}
</style>
