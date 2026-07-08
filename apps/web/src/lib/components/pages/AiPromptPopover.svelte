<script lang="ts">
	/**
	 * AiPromptPopover — the prompt entry surface for the live AI cursor.
	 *
	 * "Dumb display" like SlashMenu/RefPicker: floats at a position, collects
	 * a quick action or free-text instruction, and reports it via onSubmit. The
	 * host (CodeMirrorEditor) starts the AI session.
	 */
	import Icon from "$lib/components/Icon.svelte";
	import { onMount, tick } from "svelte";
	import { fade } from "svelte/transition";
	import { FloatingContent, useClickOutside } from "$lib/floating";
	import type { VirtualAnchor } from "$lib/floating";
	import type { AiIntent } from "$lib/ai/inlineComplete";

	interface Props {
		position: { x: number; y: number };
		/** "rewrite" when there's a selection, otherwise "continue". */
		intent: AiIntent;
		onSubmit: (instruction: string) => void;
		onClose: () => void;
	}

	let { position, intent, onSubmit, onClose }: Props = $props();

	let value = $state("");
	let inputEl: HTMLInputElement | null = $state(null);
	let menuEl: HTMLDivElement | null = $state(null);

	const quickActions = $derived(
		intent === "rewrite"
			? [
					{ label: "Improve writing", instruction: "Improve the writing." },
					{ label: "Make shorter", instruction: "Make this more concise." },
					{ label: "Fix grammar", instruction: "Fix spelling and grammar." },
				]
			: [
					{ label: "Continue writing", instruction: "Continue the writing." },
					{ label: "Summarize above", instruction: "Summarize the text above." },
					{ label: "Brainstorm ideas", instruction: "Brainstorm related ideas." },
				],
	);

	const virtualAnchor = $derived<VirtualAnchor>({
		x: position.x,
		y: position.y,
		width: 0,
		height: 0,
	});

	useClickOutside(
		() => [menuEl],
		() => onClose(),
		() => true,
	);

	function submit(instruction: string) {
		const trimmed = instruction.trim();
		if (!trimmed) return;
		onSubmit(trimmed);
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === "Escape") {
			e.preventDefault();
			onClose();
		} else if (e.key === "Enter") {
			e.preventDefault();
			submit(value);
		}
	}

	onMount(async () => {
		await tick();
		inputEl?.focus();
	});
</script>

<FloatingContent
	anchor={virtualAnchor}
	options={{ placement: "bottom-start", offset: 6, flip: true, shift: true, padding: 8 }}
	class="ai-prompt-container"
>
	<div bind:this={menuEl} class="ai-prompt" transition:fade={{ duration: 100 }}>
		<div class="ai-prompt-input-row">
			<Icon icon="ri:sparkling-2-line" width="15" class="ai-prompt-spark" />
			<input
				bind:this={inputEl}
				bind:value
				class="ai-prompt-input"
				placeholder={intent === "rewrite"
					? "Tell Virtues how to edit this…"
					: "Ask Virtues to write…"}
				onkeydown={handleKeydown}
			/>
		</div>
		<div class="ai-prompt-actions">
			{#each quickActions as action (action.label)}
				<button class="ai-prompt-action" onclick={() => submit(action.instruction)}>
					{action.label}
				</button>
			{/each}
		</div>
	</div>
</FloatingContent>

<style>
	:global(.ai-prompt-container) {
		--z-floating: 103;
		padding: 0;
		background: transparent;
		border: none;
		box-shadow: none;
	}

	.ai-prompt {
		width: 320px;
		background: var(--color-surface);
		border: 1px solid var(--color-border-subtle, var(--color-border));
		border-radius: 10px;
		box-shadow: 0 8px 30px rgba(0, 0, 0, 0.14);
		overflow: hidden;
	}

	.ai-prompt-input-row {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 10px 12px;
	}

	:global(.ai-prompt-spark) {
		color: var(--color-primary);
		flex-shrink: 0;
	}

	.ai-prompt-input {
		flex: 1;
		border: none;
		background: transparent;
		outline: none;
		font-size: 14px;
		color: var(--color-foreground);
	}

	.ai-prompt-input::placeholder {
		color: var(--color-foreground-subtle);
	}

	.ai-prompt-actions {
		display: flex;
		flex-wrap: wrap;
		gap: 4px;
		padding: 0 8px 8px;
	}

	.ai-prompt-action {
		padding: 5px 9px;
		font-size: 12px;
		border: none;
		background: var(--color-surface-elevated);
		color: var(--color-foreground-muted);
		border-radius: 6px;
		cursor: pointer;
		transition:
			color 0.12s ease,
			background-color 0.12s ease;
	}

	.ai-prompt-action:hover {
		background: var(--color-primary-subtle);
		color: var(--color-primary);
	}
</style>
