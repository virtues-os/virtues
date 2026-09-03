<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import { onMount } from "svelte";
	import { Spring } from "svelte/motion";
	import { createEntityBadgeElement } from "$lib/utils/refBadge";
	import RefPicker, { type EntityResult } from "./RefPicker.svelte";
	import { mobileLayout } from "$lib/stores/mobileLayout.svelte";

	let {
		value = $bindable(""),
		disabled = false,
		sendDisabled = false,
		allowEmptySubmit = false,
		isStreaming = false,
		maxWidth = "max-w-3xl",
		focused = $bindable(false),
		placeholder = "Write a message...",
		onAttach = undefined as ((files: File[]) => void) | undefined,
		onSubmit = undefined as ((content: string) => void) | undefined,
		onStop = undefined as (() => void) | undefined,
	}: {
		value?: string;
		disabled?: boolean;
		sendDisabled?: boolean;
		allowEmptySubmit?: boolean;
		isStreaming?: boolean;
		maxWidth?: string;
		focused?: boolean;
		placeholder?: string;
		onAttach?: (files: File[]) => void;
		onSubmit?: (content: string) => void;
		onStop?: () => void;
	} = $props();

	let fileInputEl: HTMLInputElement | null = $state(null);

	function pickFiles() {
		fileInputEl?.click();
	}

	function onFilesPicked(e: Event) {
		const input = e.target as HTMLInputElement;
		if (input.files && input.files.length > 0) {
			onAttach?.(Array.from(input.files));
		}
		input.value = ""; // allow re-picking the same file
	}

	let inputEl: HTMLDivElement;
	let isFocused = $state(false);
	let inputIsEmpty = $state(true);

	const MIN_HEIGHT = 24;
	const MAX_HEIGHT = 200;
	const inputHeight = new Spring(MIN_HEIGHT, { stiffness: 0.18, damping: 0.8 });

	// Only enable scrolling when at max height to prevent scrollbar flash during animation
	const shouldScroll = $derived(inputHeight.current >= MAX_HEIGHT - 1);
	// Bottom-align the controls once the field wraps past a single line.
	const isMultiline = $derived(inputHeight.current > MIN_HEIGHT + 6);

	// @ mention state - uses RefPicker
	let showEntityPicker = $state(false);
	// Save the text node and cursor position when @ is typed (before picker steals focus)
	let savedTextNode: Text | null = $state(null);
	let savedCursorOffset: number = $state(0);

	// Store entity references by ID for expansion on submit
	let entityMentions = $state<Map<string, EntityResult>>(new Map());

	// Can we submit? (has content, or staged refs/attachments allow an empty send)
	const canSubmit = $derived((!inputIsEmpty || allowEmptySubmit) && !sendDisabled);
	// The turn is in flight (content queued/sending) — show the spinner.
	const isBusy = $derived(sendDisabled && (!inputIsEmpty || allowEmptySubmit));

	// Which action the single trailing button performs right now.
	const trailingMode = $derived(isStreaming ? "stop" : "send");
	const trailingLabel = $derived(trailingMode === "stop" ? "Stop" : "Send");
	function trailingAction() {
		if (trailingMode === "stop") handleStop();
		else handleSubmit();
	}

	// Sync internal focus state with external bindable prop
	$effect(() => {
		focused = isFocused;
	});

	// Focus input when focused prop is set to true externally
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
				if (el.classList.contains("ref-pill")) {
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

		const textNode = range.startContainer;
		if (textNode.nodeType !== Node.TEXT_NODE) return;

		const text = textNode.textContent || "";
		const cursorPos = range.startOffset;
		const textBeforeCursor = text.slice(0, cursorPos);

		if (textBeforeCursor.endsWith("@")) {
			savedTextNode = textNode as Text;
			savedCursorOffset = cursorPos;
			showEntityPicker = true;
		}
	}

	function handleEntityPickerSelect(entity: EntityResult) {
		if (!savedTextNode || !savedTextNode.parentNode) {
			closeEntityPicker();
			return;
		}

		const text = savedTextNode.textContent || "";
		const cursorPos = savedCursorOffset;

		const atIndex = text.lastIndexOf("@", cursorPos - 1);
		if (atIndex !== -1) {
			const chip = createEntityBadgeElement(entity.name, entity.url, {
				className: 'ref-pill',
			});

			const space = document.createTextNode(" ");

			const beforeText = text.slice(0, atIndex);
			const afterText = text.slice(cursorPos);

			savedTextNode.textContent = beforeText;

			const parent = savedTextNode.parentNode;
			const afterNode = document.createTextNode(afterText);
			parent.insertBefore(chip, savedTextNode.nextSibling);
			parent.insertBefore(space, chip.nextSibling);
			parent.insertBefore(afterNode, space.nextSibling);

			const selection = window.getSelection();
			if (selection) {
				const newRange = document.createRange();
				newRange.setStartAfter(space);
				newRange.collapse(true);
				selection.removeAllRanges();
				selection.addRange(newRange);
			}

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
		if (showEntityPicker) {
			if (e.key === "Escape") {
				e.preventDefault();
				closeEntityPicker();
			}
			return;
		}

		if (e.key === "Enter" && !e.shiftKey) {
			e.preventDefault();
			handleSubmit();
		}
	}

	function handleSubmit() {
		const content = getExpandedContent().trim();
		if ((!content && !allowEmptySubmit) || disabled) return;

		onSubmit?.(content);

		if (inputEl) {
			inputEl.innerHTML = "";
		}
		value = "";
		inputIsEmpty = true;
		entityMentions.clear();
		inputHeight.target = MIN_HEIGHT;
	}

	function handleStop() {
		onStop?.();
	}

	function handleWrapperClick(e: MouseEvent) {
		const target = e.target as HTMLElement;
		if (target.tagName === "BUTTON" || target.closest("button")) {
			return;
		}
		if (inputEl) {
			inputEl.focus();
		}
	}

	function handlePaste(e: ClipboardEvent) {
		const dt = e.clipboardData;
		if (!dt) return;

		const imgs: File[] = [];
		for (const it of Array.from(dt.items || [])) {
			if (it.kind === "file" && it.type.startsWith("image/")) {
				const f = it.getAsFile();
				if (f) imgs.push(f);
			}
		}
		if (imgs.length === 0) {
			for (const f of Array.from(dt.files || [])) {
				if (f.type.startsWith("image/")) imgs.push(f);
			}
		}
		if (imgs.length > 0 && onAttach) {
			e.preventDefault();
			onAttach(imgs);
			return;
		}

		const text = dt.getData("text/plain") || "";
		if (text.length > 1500 && onAttach) {
			e.preventDefault();
			onAttach([new File([text], "Pasted Text.txt", { type: "text/plain" })]);
			return;
		}

		e.preventDefault();
		document.execCommand("insertText", false, text);
	}

	onMount(() => {
		if (value && inputEl) {
			inputEl.textContent = value;
		}
		// Set imperatively: `autocorrect` is WebKit's own attribute (not in
		// Svelte's HTML typings) and iOS keys the QuickType prediction row off
		// it — with it off (plus spellcheck=false above), the keyboard drops
		// the suggested-words strip the way native chat apps do.
		if (mobileLayout.isMobile && inputEl) {
			inputEl.setAttribute("autocorrect", "off");
		}
	});
</script>

<div class="chat-input-container {maxWidth} w-full">
	<!-- svelte-ignore a11y_click_events_have_key_events -->
	<div
		aria-label="Chat input"
		class="chat-input-wrapper bg-surface border border-border-strong cursor-text"
		class:focused={isFocused}
		class:multiline={isMultiline}
		onclick={handleWrapperClick}
		role="textbox"
		tabindex="-1"
	>
		<label for="chat-input" class="sr-only">Message</label>

		{#if onAttach}
			<button
				type="button"
				onclick={pickFiles}
				class="pill-btn attach-button"
				aria-label="Attach files"
				title="Attach images, PDFs, or audio"
			>
				<Icon icon="ri:add-line" width="18" />
			</button>
			<input
				bind:this={fileInputEl}
				type="file"
				multiple
				accept="image/*,application/pdf,audio/*,text/*,.md,.markdown,.csv,.tsv,.json,.html,.htm,.xml,.yaml,.yml,.toml,.ini,.log,.ts,.tsx,.js,.jsx,.py,.rb,.rs,.go,.java,.c,.h,.cpp,.cs,.php,.swift,.kt,.sh,.sql,.css,.scss"
				class="sr-only"
				onchange={onFilesPicked}
			/>
		{/if}

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
			class="chat-input resize-none outline-none text-foreground font-sans text-base bg-transparent"
			class:empty={inputIsEmpty}
			data-placeholder={placeholder}
			role="textbox"
			aria-multiline="true"
			tabindex="0"
			spellcheck={mobileLayout.isMobile ? false : undefined}
			autocapitalize={mobileLayout.isMobile ? "sentences" : undefined}
			enterkeyhint={mobileLayout.isMobile ? "send" : undefined}
			style:height="{inputHeight.current}px"
			style:overflow-y={shouldScroll ? 'auto' : 'hidden'}
		></div>

		<!-- Trailing controls: send / stop -->
		<div class="composer-actions">
			<!-- One persistent trailing button — its icon flips between send and
			     stop so a turn starting animates rather than swaps. -->
			<button
				type="button"
				onclick={trailingAction}
				disabled={trailingMode === "send" && !canSubmit}
				class="pill-btn action-btn btn-primary"
				aria-label={trailingLabel}
			>
				<span class="icon-swap">
					<span class="swap-icon" class:active={trailingMode === "send" && !isBusy}>
						<Icon icon="ri:arrow-up-line" width="15" style="color: inherit" />
					</span>
					<span class="swap-icon" class:active={trailingMode === "send" && isBusy}>
						<Icon icon="ri:loader-4-line" class="animate-spin" width="15" style="color: inherit" />
					</span>
					<span class="swap-icon" class:active={trailingMode === "stop"}>
						<Icon icon="ri:stop-fill" width="15" style="color: inherit" />
					</span>
				</span>
			</button>
		</div>

		{#if showEntityPicker}
			<RefPicker
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
		display: flex;
		align-items: center;
		gap: 0.25rem;
		padding: 0.4375rem 0.5rem 0.4375rem 0.5625rem;
		border-radius: 1.75rem;
		transition:
			border-color 0.3s cubic-bezier(0.4, 0, 0.2, 1),
			box-shadow 0.3s cubic-bezier(0.4, 0, 0.2, 1);
	}

	/* Once the field wraps to multiple lines, anchor controls to the bottom and
	   ease off the full pill radius into a rounded box. */
	.chat-input-wrapper.multiline {
		align-items: flex-end;
		border-radius: 1.25rem;
	}

	.chat-input-wrapper:hover {
		border-color: color-mix(in srgb, var(--color-foreground) 28%, var(--color-border-strong));
	}

	.chat-input-wrapper.focused {
		border-color: var(--color-primary) !important;
	}

	.chat-input {
		flex: 1;
		min-width: 0;
		line-height: 1.5;
		padding: 0.125rem 0.25rem;
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

	/* Round icon buttons that sit inside the pill */
	.pill-btn {
		flex-shrink: 0;
		display: flex;
		align-items: center;
		justify-content: center;
		width: 2rem;
		height: 2rem;
		border-radius: var(--radius-full);
		cursor: pointer;
		transition:
			background-color 0.15s ease,
			opacity 0.15s ease;
	}

	/* The composer's buttons are 32px because the pill they sit in is tight, and
	   growing them would push it open. So the hit area grows instead of the
	   button: an invisible 44pt square centred on each, which is what a finger
	   actually aims at. They sit at opposite ends of the pill, so the two
	   expanded areas never meet. */
	@media (max-width: 768px), (pointer: coarse) {
		.pill-btn {
			position: relative;
		}

		.pill-btn::after {
			content: "";
			position: absolute;
			top: 50%;
			left: 50%;
			width: max(100%, 44px);
			height: max(100%, 44px);
			transform: translate(-50%, -50%);
		}
	}

	.attach-button {
		color: var(--color-foreground-muted);
	}
	.attach-button:hover {
		background: var(--hover-bg);
		color: var(--color-foreground);
	}

	.composer-actions {
		flex-shrink: 0;
		display: flex;
		align-items: center;
		gap: 0.25rem;
	}

	.action-btn:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	/* Icon crossfade/flip: all states share one stacked box; the active one
	   rotates+fades in while the outgoing one rotates+fades out. */
	.icon-swap {
		position: relative;
		width: 1.125rem;
		height: 1.125rem;
	}

	.swap-icon {
		position: absolute;
		inset: 0;
		display: flex;
		align-items: center;
		justify-content: center;
		opacity: 0;
		transform: rotate(-90deg) scale(0.5);
		transition:
			opacity 0.16s ease,
			transform 0.24s cubic-bezier(0.34, 1.35, 0.64, 1);
		pointer-events: none;
	}

	.swap-icon.active {
		opacity: 1;
		transform: rotate(0deg) scale(1);
	}

</style>
