<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import { createEventDispatcher, onMount } from "svelte";
	import { Spring } from "svelte/motion";
	import type { ModelOption } from "$lib/config/models";
	import { type AgentModeId, getNextMode, getModeById } from "$lib/config/agentModes";
	import { createEntityBadgeElement } from "$lib/utils/entityBadge";
	import AgentModePicker from "./AgentModePicker.svelte";
	import ContextIndicator from "./ContextIndicator.svelte";
	import PageEditIndicator from "./chat/PageEditIndicator.svelte";
	import ToolbarSettingsMenu from "./ToolbarSettingsMenu.svelte";
	import EntityPicker, { type EntityResult } from "./EntityPicker.svelte";

	interface ContextUsage {
		percentage: number;
		tokens: number;
		window: number;
		status: 'healthy' | 'warning' | 'critical';
	}

	interface PageBinding {
		pageId: string;
		pageTitle: string;
	}

	interface EditableItem {
		type: 'page' | 'folder' | 'wiki_entry';
		id: string;
		title: string;
		icon?: string;
	}

	let {
		value = $bindable(""),
		disabled = false,
		sendDisabled = false,
		isStreaming = false,
		maxWidth = "max-w-3xl",
		focused = $bindable(false),
		selectedModel = $bindable<ModelOption | undefined>(undefined),
		selectedPersona = $bindable<string>('default'),
		selectedAgentMode = $bindable<AgentModeId>('agent'),
		showToolbar = true,
		conversationId = undefined as string | undefined,
		contextUsage = undefined as ContextUsage | undefined,
		onContextClick = (() => {}) as () => void,
		pageBinding = undefined as PageBinding | undefined,
		editableItems = [] as EditableItem[],
		onRemoveItem = ((_type: string, _id: string) => {}) as (type: string, id: string) => void,
		onSelectEntities = undefined as ((entities: EntityResult[]) => void) | undefined,
	}: {
		value?: string;
		disabled?: boolean;
		sendDisabled?: boolean;
		isStreaming?: boolean;
		maxWidth?: string;
		focused?: boolean;
		selectedModel?: ModelOption;
		selectedPersona?: string;
		selectedAgentMode?: AgentModeId;
		showToolbar?: boolean;
		conversationId?: string;
		contextUsage?: ContextUsage;
		onContextClick?: () => void;
		pageBinding?: PageBinding;
		editableItems?: EditableItem[];
		onRemoveItem?: (type: string, id: string) => void;
		onSelectEntities?: (entities: EntityResult[]) => void;
	} = $props();

	const dispatch = createEventDispatcher<{ submit: string; stop: null }>();

	let inputEl: HTMLDivElement;
	let isFocused = $state(false);
	let inputIsEmpty = $state(true);

	const MIN_HEIGHT = 56;
	const MAX_HEIGHT = 220;
	const inputHeight = new Spring(MIN_HEIGHT, { stiffness: 0.18, damping: 0.8 });

	// Only enable scrolling when at max height to prevent scrollbar flash during animation
	const shouldScroll = $derived(inputHeight.current >= MAX_HEIGHT - 1);

	// @ mention state - uses EntityPicker
	let showEntityPicker = $state(false);
	// Save the text node and cursor position when @ is typed (before picker steals focus)
	let savedTextNode: Text | null = $state(null);
	let savedCursorOffset: number = $state(0);

	// Store entity references by ID for expansion on submit
	let entityMentions = $state<Map<string, EntityResult>>(new Map());


	// Derive placeholder based on toolbar visibility
	const placeholderText = $derived(showToolbar ? "What can I do for you?" : "Message...");

	// Derive mode color for border/glow (null means use default primary)
	const modeColor = $derived(getModeById(selectedAgentMode)?.color || 'var(--color-primary)');

	// Derive whether editing is enabled in current mode (hide edit picker in chat mode)
	const canEdit = $derived(getModeById(selectedAgentMode)?.tools.edit ?? true);

	// Sync internal focus state with external bindable prop
	$effect(() => {
		focused = isFocused;
	});

	// Focus input when focused prop is set to true externally
	// Only focus if no modal/overlay is blocking and no other input is focused
	$effect(() => {
		if (focused && inputEl && !isFocused) {
			// Don't steal focus if a modal/overlay is open
			const hasModalOpen = document.querySelector('.modal-backdrop, .picker-backdrop, [role="dialog"]');
			if (hasModalOpen) return;

			const active = document.activeElement;
			const isOtherInputFocused = active && (
				active.tagName === 'INPUT' ||
				active.tagName === 'TEXTAREA' ||
				(active as HTMLElement).isContentEditable
			);
			// Don't steal focus from other inputs (like SearchModal)
			if (!isOtherInputFocused) {
				inputEl.focus();
			}
		}
	});

	// Get text content with mentions expanded to markdown format
	function getExpandedContent(): string {
		if (!inputEl) return "";

		let result = "";
		const walker = document.createTreeWalker(inputEl, NodeFilter.SHOW_ALL);
		let node: Node | null = walker.currentNode;

		while (node) {
			if (node.nodeType === Node.TEXT_NODE) {
				result += node.textContent || "";
			} else if (node.nodeType === Node.ELEMENT_NODE) {
				const el = node as HTMLElement;
				if (el.classList.contains("mention-chip")) {
					const entityUrl = el.dataset.entityUrl;
					const name = el.textContent?.replace(/^@/, "") || "";
					if (entityUrl) {
						result += `[${name}](${entityUrl})`;
					} else {
						result += el.textContent || "";
					}
					// Skip children of mention chip
					const next = walker.nextSibling();
					if (next) {
						node = next;
						continue;
					} else {
						// Go up and find next
						let parent = walker.parentNode();
						while (parent && !walker.nextSibling()) {
							parent = walker.parentNode();
						}
						node = walker.currentNode;
						continue;
					}
				}
			}
			node = walker.nextNode();
		}

		return result;
	}

	// Get plain text content (for value binding)
	function getPlainContent(): string {
		return inputEl?.textContent || "";
	}

	function updateHeight() {
		if (!inputEl) return;
		// Temporarily reset height to measure natural scrollHeight
		inputEl.style.height = 'auto';
		const newHeight = Math.min(Math.max(inputEl.scrollHeight, MIN_HEIGHT), MAX_HEIGHT);
		inputEl.style.height = `${inputHeight.current}px`;
		inputHeight.target = newHeight;
	}

	function handleInput() {
		// Sync value for external binding
		value = getPlainContent();

		// Update empty state for placeholder
		inputIsEmpty = !value.trim();

		// Animate height change
		updateHeight();

		// Check for @ trigger
		const selection = window.getSelection();
		if (!selection || selection.rangeCount === 0) return;

		const range = selection.getRangeAt(0);
		if (!range.collapsed) return;

		// Get text before cursor
		const textNode = range.startContainer;
		if (textNode.nodeType !== Node.TEXT_NODE) return;

		const text = textNode.textContent || "";
		const cursorPos = range.startOffset;
		const textBeforeCursor = text.slice(0, cursorPos);

		// Check if @ was just typed
		if (textBeforeCursor.endsWith("@")) {
			// Save the text node and cursor position before picker steals focus
			savedTextNode = textNode as Text;
			savedCursorOffset = cursorPos;
			showEntityPicker = true;
		}
	}

	function handleEntityPickerSelect(entity: EntityResult) {
		// Use saved text node and cursor position (saved when @ was typed)
		if (!savedTextNode || !savedTextNode.parentNode) {
			closeEntityPicker();
			return;
		}

		const text = savedTextNode.textContent || "";
		const cursorPos = savedCursorOffset;

		// Find @ before cursor
		const atIndex = text.lastIndexOf("@", cursorPos - 1);
		if (atIndex !== -1) {
			// Create mention chip element using shared utility (@name format)
			const chip = createEntityBadgeElement(entity.name, entity.url, {
				className: 'mention-chip',
			});

			// Create a space after
			const space = document.createTextNode(" ");

			// Split text node and insert chip
			const beforeText = text.slice(0, atIndex);
			const afterText = text.slice(cursorPos);

			savedTextNode.textContent = beforeText;

			const parent = savedTextNode.parentNode;
			const afterNode = document.createTextNode(afterText);
			parent.insertBefore(chip, savedTextNode.nextSibling);
			parent.insertBefore(space, chip.nextSibling);
			parent.insertBefore(afterNode, space.nextSibling);

			// Move cursor after space
			const selection = window.getSelection();
			if (selection) {
				const newRange = document.createRange();
				newRange.setStartAfter(space);
				newRange.collapse(true);
				selection.removeAllRanges();
				selection.addRange(newRange);
			}

			// Store entity reference
			entityMentions.set(entity.id, entity);
		}

		closeEntityPicker();
		value = getPlainContent();
		inputIsEmpty = !value.trim();
	}

	function closeEntityPicker() {
		showEntityPicker = false;
		savedTextNode = null;
		savedCursorOffset = 0;
		inputEl?.focus();
	}

	function handleKeydown(e: KeyboardEvent) {
		// When entity picker is open, let it handle keyboard events
		if (showEntityPicker) {
			if (e.key === "Escape") {
				e.preventDefault();
				closeEntityPicker();
			}
			return;
		}

		// Shift+Tab cycles through agent modes
		if (e.shiftKey && e.key === "Tab") {
			e.preventDefault();
			cycleAgentMode();
			return;
		}

		if (e.key === "Enter" && !e.shiftKey) {
			e.preventDefault();
			handleSubmit();
		}
	}

	function handleSubmit() {
		const content = getExpandedContent().trim();
		if (!content || disabled) return;

		dispatch("submit", content);

		// Clear input
		if (inputEl) {
			inputEl.innerHTML = "";
		}
		value = "";
		inputIsEmpty = true;
		entityMentions.clear();
		inputHeight.target = MIN_HEIGHT;
	}

	function handleStop() {
		dispatch("stop", null);
	}

	function handleWrapperClick(e: MouseEvent) {
		const target = e.target as HTMLElement;
		if (
			target.tagName === "BUTTON" ||
			target.closest("button") ||
			target.classList.contains("z-50") ||
			target.closest(".z-50") ||
			target.closest(".toolbar")
		) {
			return;
		}
		if (inputEl) {
			inputEl.focus();
		}
	}

	function handleModelSelect(model: ModelOption) {
		selectedModel = model;
	}

	function cycleAgentMode() {
		const nextMode = getNextMode(selectedAgentMode);
		selectedAgentMode = nextMode.id;
	}

	function handlePaste(e: ClipboardEvent) {
		e.preventDefault();
		const text = e.clipboardData?.getData("text/plain") || "";
		document.execCommand("insertText", false, text);
	}

	onMount(() => {
		// Set initial content if value is provided
		if (value && inputEl) {
			inputEl.textContent = value;
		}
	});
</script>

<div class="chat-input-container {maxWidth} w-full">
	<!-- svelte-ignore a11y_click_events_have_key_events -->
	<div
		aria-label="Chat input"
		class="chat-input-wrapper bg-surface border border-border-strong cursor-text"
		class:focused={isFocused}
		style="--mode-color: {modeColor}"
		onclick={handleWrapperClick}
		role="textbox"
		tabindex="-1"
	>
		<label for="chat-input" class="sr-only">Message</label>
		<div class="input-row relative flex items-start w-full">
			<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
			<div
				id="chat-input"
				bind:this={inputEl}
				contenteditable={!disabled}
				oninput={handleInput}
				onkeydown={handleKeydown}
				onpaste={handlePaste}
				onfocus={() => {
					isFocused = true;
				}}
				onblur={() => {
					isFocused = false;
				}}
				class="chat-input w-full resize-none outline-none text-foreground font-sans text-base bg-transparent px-4 pt-4 pb-2"
				class:empty={inputIsEmpty}
				data-placeholder={placeholderText}
				role="textbox"
				aria-multiline="true"
				tabindex="0"
				style:height="{inputHeight.current}px"
				style:overflow-y={shouldScroll ? 'auto' : 'hidden'}
			></div>
			{#if !showToolbar}
				{#if isStreaming}
					<button
						type="button"
						onclick={handleStop}
						class="stop-button absolute right-3 top-3 w-8 h-8 btn-primary cursor-pointer rounded-lg transition-all flex items-center justify-center"
					>
						<Icon icon="ri:stop-fill" width="16" style="color: inherit" />
					</button>
				{:else}
					<button
						type="button"
						onclick={handleSubmit}
						disabled={!value.trim() || sendDisabled}
						class="send-button absolute right-3 top-3 w-8 h-8 btn-primary cursor-pointer rounded-lg disabled:opacity-50 disabled:cursor-not-allowed transition-all flex items-center justify-center group"
					>
						{#if sendDisabled}
							<Icon
								icon="ri:loader-4-line"
								class="animate-spin"
								style="color: inherit"
								width="16"
							/>
						{:else}
							<Icon
								icon="ri:arrow-up-line"
								width="16"
								class="transition-transform duration-300 group-hover:rotate-45"
								style="color: inherit"
							/>
						{/if}
					</button>
				{/if}
			{/if}
		</div>

		{#if showToolbar}
			<div class="toolbar flex items-center gap-1.5 px-2 pb-2 pt-1">
				<div>
					<AgentModePicker
						bind:value={selectedAgentMode}
					/>
				</div>
				<div>
					<PageEditIndicator
						items={editableItems}
						boundPage={pageBinding ? { id: pageBinding.pageId, title: pageBinding.pageTitle } : null}
						onRemoveItem={onRemoveItem}
						onSelectEntities={onSelectEntities}
						visible={canEdit}
					/>
				</div>
				<div>
					<ToolbarSettingsMenu
						{selectedModel}
						{selectedPersona}
						onModelSelect={handleModelSelect}
						onPersonaSelect={(id) => selectedPersona = id}
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
				{#if isStreaming}
					<button
						type="button"
						onclick={handleStop}
						class="stop-button-toolbar w-6 h-6 btn-primary cursor-pointer rounded-full transition-all flex items-center justify-center"
					>
						<Icon icon="ri:stop-fill" width="12" style="color: inherit" />
					</button>
				{:else}
					<button
						type="button"
						onclick={handleSubmit}
						disabled={!value.trim() || sendDisabled}
						class="send-button-toolbar w-6 h-6 btn-primary cursor-pointer rounded-full disabled:opacity-50 disabled:cursor-not-allowed transition-all flex items-center justify-center group"
					>
						{#if sendDisabled}
							<Icon
								icon="ri:loader-4-line"
								class="animate-spin"
								style="color: inherit"
								width="12"
							/>
						{:else}
							<Icon
								icon="ri:arrow-up-line"
								width="12"
								class="transition-transform duration-300 group-hover:rotate-45"
								style="color: inherit"
							/>
						{/if}
					</button>
				{/if}
			</div>
		{/if}

		{#if showEntityPicker}
			<EntityPicker
				mode="single"
				placeholder="Search entities to mention..."
				onSelect={handleEntityPickerSelect}
				onClose={closeEntityPicker}
			/>
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

	.chat-input-wrapper {
		position: relative;
		border-radius: 8px;
		transition:
			border-color 0.3s cubic-bezier(0.4, 0, 0.2, 1),
			box-shadow 0.3s cubic-bezier(0.4, 0, 0.2, 1);
	}

	.chat-input-wrapper:hover {
		border-color: color-mix(in srgb, var(--mode-color) 60%, transparent);
	}

	.chat-input-wrapper.focused {
		border-color: var(--mode-color) !important;
		box-shadow:
			0 1px 2px 0 rgb(0 0 0 / 0.05),
			0 0 0 3px color-mix(in srgb, var(--mode-color) 40%, transparent) !important;
	}

	.chat-input {
		line-height: 1.5;
		padding-right: 3.5rem;
		white-space: pre-wrap;
		word-wrap: break-word;
		font-family: var(--font-sans);
	}

	/* Placeholder using ::before pseudo-element */
	.chat-input.empty::before {
		content: attr(data-placeholder);
		color: var(--color-foreground-subtle);
		pointer-events: none;
		position: absolute;
	}

	/* Custom scrollbar for input */
	.chat-input::-webkit-scrollbar {
		width: 6px;
	}

	.chat-input::-webkit-scrollbar-track {
		background: transparent;
	}

	.chat-input::-webkit-scrollbar-thumb {
		background: var(--color-border-subtle);
		border-radius: 3px;
	}

	.chat-input::-webkit-scrollbar-thumb:hover {
		background: var(--color-border-strong);
	}

	.toolbar {
		display: flex;
	}
</style>
