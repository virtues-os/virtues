<script lang="ts">
	import { createEventDispatcher } from "svelte";
	import type { ModelOption } from "$lib/config/models";
	import ModelPicker from "./ModelPicker.svelte";
	import ContextIndicator from "./ContextIndicator.svelte";

	interface ContextUsage {
		percentage: number;
		tokens: number;
		window: number;
		status: 'healthy' | 'warning' | 'critical';
	}

	let {
		value = $bindable(""),
		disabled = false,
		sendDisabled = false,
		maxWidth = "max-w-3xl",
		focused = $bindable(false),
		selectedModel = $bindable<ModelOption | undefined>(undefined),
		showToolbar = true,
		conversationId = undefined as string | undefined,
		contextUsage = undefined as ContextUsage | undefined,
		onContextClick = (() => {}) as () => void,
	} = $props();

	const dispatch = createEventDispatcher<{ submit: string }>();

	let textarea: HTMLTextAreaElement;
	let isFocused = $state(false);

	// Derive placeholder based on toolbar visibility
	const placeholder = $derived(showToolbar ? "What can I do for you?" : "Message...");

	// Sync internal focus state with external bindable prop
	$effect(() => {
		focused = isFocused;
	});

	// Simple auto-resize: measure scrollHeight and set height
	function syncSize() {
		if (!textarea) return;
		textarea.style.height = 'auto';
		textarea.style.height = `${Math.min(textarea.scrollHeight, 220)}px`;
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === "Enter" && !e.shiftKey) {
			e.preventDefault();
			handleSubmit();
		}
	}

	function handleSubmit() {
		if (!value.trim() || disabled) return;
		dispatch("submit", value);
		value = "";
		// Reset height after clearing
		if (textarea) {
			textarea.style.height = "56px";
		}
	}

	function handleWrapperClick(e: MouseEvent) {
		// Don't focus if clicking on a button, dropdown, or interactive element
		const target = e.target as HTMLElement;
		if (
			target.tagName === "BUTTON" ||
			target.closest("button") ||
			target.classList.contains("z-50") || // Don't focus when clicking dropdown menus
			target.closest(".z-50") || // Don't focus when clicking inside dropdown menus
			target.closest(".toolbar") // Don't focus when clicking toolbar
		) {
			return;
		}
		// Focus the textarea
		if (textarea) {
			textarea.focus();
		}
	}

	function handleModelSelect(model: ModelOption) {
		console.log('[ChatInput] handleModelSelect called:', model.id, model.displayName);
		selectedModel = model;
		console.log('[ChatInput] selectedModel after set:', selectedModel?.id, selectedModel?.displayName);
	}
</script>

<div class="chat-input-container {maxWidth} w-full">
	<div
		aria-label="Chat input"
		class="chat-input-wrapper bg-surface border border-border-strong hover:border-primary/60 cursor-text"
		class:focused={isFocused}
		onclick={handleWrapperClick}
		role="button"
		tabindex="-1"
	>
		<label for="chat-input" class="sr-only">Message</label>
		<div class="input-row relative flex items-start w-full">
			<textarea
				id="chat-input"
				bind:this={textarea}
				bind:value
				oninput={syncSize}
				onkeydown={handleKeydown}
				onfocus={() => {
					isFocused = true;
				}}
				onblur={() => {
					isFocused = false;
				}}
				{placeholder}
				{disabled}
				rows="1"
				class="chat-textarea w-full resize-none outline-none text-foreground placeholder:text-foreground-subtle font-sans text-base bg-transparent px-4 pt-4 pb-2"
			></textarea>
			{#if !showToolbar}
				<button
					type="button"
					onclick={handleSubmit}
					disabled={!value.trim() || sendDisabled}
					class="send-button absolute right-3 top-3 w-8 h-8 btn-primary cursor-pointer rounded-lg disabled:opacity-50 disabled:cursor-not-allowed transition-all flex items-center justify-center group"
				>
					{#if sendDisabled}
						<iconify-icon
							icon="ri:loader-4-line"
							class="animate-spin"
							style="color: inherit"
							width="16"
						></iconify-icon>
					{:else}
						<iconify-icon
							icon="ri:arrow-up-line"
							width="16"
							class="transition-transform duration-300 group-hover:rotate-45"
							style="color: inherit"
						></iconify-icon>
					{/if}
				</button>
			{/if}
		</div>

		{#if showToolbar}
			<div class="toolbar flex items-center gap-1.5 px-2 pb-2 pt-1">
				<div>
					<ModelPicker
						value={selectedModel}
						onSelect={handleModelSelect}
					/>
				</div>
				{#if conversationId && contextUsage}
					<div>
						<ContextIndicator
							{conversationId}
							usagePercentage={contextUsage.percentage}
							totalTokens={contextUsage.tokens}
							contextWindow={contextUsage.window}
							status={contextUsage.status}
							onclick={onContextClick}
						/>
					</div>
				{/if}
				<div class="flex-1"></div>
				<button
					type="button"
					onclick={handleSubmit}
					disabled={!value.trim() || sendDisabled}
					class="send-button-toolbar w-7 h-7 btn-primary cursor-pointer rounded-md disabled:opacity-50 disabled:cursor-not-allowed transition-all flex items-center justify-center group"
				>
					{#if sendDisabled}
						<iconify-icon
							icon="ri:loader-4-line"
							class="animate-spin"
							style="color: inherit"
							width="14"
						></iconify-icon>
					{:else}
						<iconify-icon
							icon="ri:arrow-up-line"
							width="14"
							class="transition-transform duration-300 group-hover:rotate-45"
							style="color: inherit"
						></iconify-icon>
					{/if}
				</button>
			</div>
		{/if}
	</div>
</div>

<style>
	.sr-only {
		position: absolute;
		width: 1px;
		height: 1px;
		padding: 0;
		margin: -1px;
		overflow: hidden;
		clip: rect(0, 0, 0, 0);
		white-space: nowrap;
		border-width: 0;
	}

	textarea {
		font-family: var(--font-sans);
	}

	/* Custom scrollbar for textarea */
	textarea::-webkit-scrollbar {
		width: 6px;
	}

	textarea::-webkit-scrollbar-track {
		background: transparent;
	}

	textarea::-webkit-scrollbar-thumb {
		background: var(--color-border-subtle);
		border-radius: 3px;
	}

	textarea::-webkit-scrollbar-thumb:hover {
		background: var(--color-border-strong);
	}

	.chat-input-wrapper {
		border-radius: 8px;
		transition:
			border-color 0.3s cubic-bezier(0.4, 0, 0.2, 1),
			box-shadow 0.3s cubic-bezier(0.4, 0, 0.2, 1);
	}

	.chat-input-wrapper.focused {
		border-color: var(--color-primary) !important;
		box-shadow:
			0 1px 2px 0 rgb(0 0 0 / 0.05),
			0 0 0 3px color-mix(in srgb, var(--color-primary) 40%, transparent) !important;
	}

	.chat-textarea {
		min-height: 56px;
		max-height: 220px;
		line-height: 1.5;
		padding-right: 3.5rem;
		transition: height 0.15s cubic-bezier(0.4, 0, 0.2, 1);
	}

	.toolbar {
		/* No background - clean look */
	}

	.picker-wrapper {
		font-size: 0.8125rem;
		background: color-mix(in srgb, var(--color-foreground) 5%, transparent);
		border-radius: 6px;
	}

	.picker-wrapper :global(button),
	.picker-wrapper :global([role="button"]) {
		border-radius: 6px !important;
	}

	.picker-wrapper :global(.px-3) {
		padding-left: 0.5rem;
		padding-right: 0.5rem;
	}

	.picker-wrapper :global(.py-1\.5) {
		padding-top: 0.25rem;
		padding-bottom: 0.25rem;
	}
</style>
