<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import { onDestroy, onMount } from "svelte";
	import { Spring } from "svelte/motion";
	import RefPicker, { type EntityResult } from "./RefPicker.svelte";
	import { mobileLayout } from "$lib/stores/mobileLayout.svelte";
	import { closeOpenFence, createComposerEditor, type ComposerEditor } from "$lib/codemirror/composer";
	import { createRefPicker, insertRef } from "$lib/codemirror/extensions/ref-picker";
	import { ATTACH_ACCEPT } from "$lib/components/chat/state/attachments.svelte";
	import { AGENT_MODES, getModeById, nextMode, type AgentModeId } from "$lib/config/agentModes";
	import { contextMenu, type ContextMenuItem } from "$lib/stores/contextMenu.svelte";

	let {
		value = $bindable(""),
		disabled = false,
		sendDisabled = false,
		allowEmptySubmit = false,
		isStreaming = false,
		maxWidth = "max-w-3xl",
		focused = $bindable(false),
		placeholder = "Ask Virtues",
		onAttach = undefined as ((files: File[]) => void) | undefined,
		onSubmit = undefined as ((content: string) => void) | undefined,
		onStop = undefined as (() => void) | undefined,
		agentMode = undefined as AgentModeId | undefined,
		onModeChange = undefined as ((mode: AgentModeId) => void) | undefined,
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
		/** The chat's mode. The chip under the pill shows it when it is not plain chat. */
		agentMode?: AgentModeId;
		/** Chosen from the (+) menu or the chip, or cycled with Shift+Tab. */
		onModeChange?: (mode: AgentModeId) => void;
	} = $props();

	const activeMode = $derived(agentMode && agentMode !== "chat" ? getModeById(agentMode) : undefined);

	let fileInputEl: HTMLInputElement | null = $state(null);
	let cameraInputEl: HTMLInputElement | null = $state(null);

	function pickFiles() {
		fileInputEl?.click();
	}

	function cycleMode() {
		if (agentMode && onModeChange) onModeChange(nextMode(agentMode));
	}

	// The (+) menu: what you add to the message, then how it is handled.
	// Nothing else goes in here — a menu of every option is a junk drawer.
	function modeRows(): ContextMenuItem[] {
		return AGENT_MODES.map((m) => ({
			id: `mode-${m.id}`,
			label: m.name,
			description: m.description,
			icon: m.icon,
			checked: m.id === (agentMode ?? "chat"),
			action: () => onModeChange?.(m.id),
		}));
	}

	function openMenu(anchorEl: HTMLElement, onlyModes = false) {
		const items: ContextMenuItem[] = [];
		if (!onlyModes && onAttach) {
			items.push({ id: "attach", label: "Add files", icon: "ri:attachment-2", action: pickFiles });
			// A phone has a camera a tap away; a desktop's "take a photo" is
			// a webcam nobody means.
			if (mobileLayout.isMobile) {
				items.push({
					id: "camera",
					label: "Take a photo",
					icon: "ri:camera-line",
					action: () => cameraInputEl?.click(),
				});
			}
		}
		if (onModeChange) {
			const rows = modeRows();
			if (onlyModes) {
				items.push(...rows);
			} else if (mobileLayout.isMobile) {
				// Fly-out submenus are fiddly under a thumb: the modes sit in
				// the same sheet.
				items.push(...rows);
			} else {
				const current = getModeById(agentMode ?? "chat");
				items.push({
					id: "mode",
					label: "Mode",
					icon: current?.icon ?? "ri:chat-3-line",
					shortcut: current && current.id !== "chat" ? current.name : undefined,
					submenu: rows,
				});
			}
		}
		if (items.length === 0) return;
		const r = anchorEl.getBoundingClientRect();
		contextMenu.show({ x: r.left, y: r.top }, items, {
			anchor: { x: r.left, y: r.top, width: r.width, height: r.height },
			placement: "top-start",
		});
	}

	function onFilesPicked(e: Event) {
		const input = e.target as HTMLInputElement;
		if (input.files && input.files.length > 0) {
			onAttach?.(Array.from(input.files));
		}
		input.value = ""; // allow re-picking the same file
	}

	// The composer is a CodeMirror instance — the same kernel as Pages, in a
	// smaller configuration (see lib/codemirror/composer.ts). The document is
	// the message: a markdown string, no DOM to serialize.
	let inputEl: HTMLDivElement;
	let editor: ComposerEditor | null = null;
	let isFocused = $state(false);
	let inputIsEmpty = $state(true);
	// Mirror of the editor document, so an external `value` set can be told
	// apart from our own echo of a keystroke.
	let docText = "";

	// One line of 1rem × 1.5 plus the content padding; the cap matches the old
	// composer. The spring animates the mount box between the two.
	const MIN_HEIGHT = 28;
	const MAX_HEIGHT = 200;
	const inputHeight = new Spring(MIN_HEIGHT, { stiffness: 0.18, damping: 0.8 });

	// Only enable scrolling when at max height to prevent scrollbar flash during animation
	const shouldScroll = $derived(inputHeight.current >= MAX_HEIGHT - 1);
	// Bottom-align the controls once the field wraps past a single line.
	const isMultiline = $derived(inputHeight.current > MIN_HEIGHT + 6);

	// `@` picker — detection lives in the ref-picker extension; the panel is
	// the same RefPicker component, anchored above the pill as before.
	let showEntityPicker = $state(false);
	let entityPickerFrom = 0;

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
		if (focused && editor && !isFocused) {
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
				editor.view.focus();
			}
		}
	});

	// External `value` set (the host clears it after a send, or prefills it)
	// → replace the document. Our own onChange echo lands here equal to
	// docText and is a no-op.
	$effect(() => {
		if (editor && value !== docText) {
			docText = value;
			editor.setDoc(value);
		}
	});

	$effect(() => {
		editor?.setDisabled(disabled);
	});

	function setHeight(contentPx: number) {
		inputHeight.target = Math.min(Math.max(contentPx, MIN_HEIGHT), MAX_HEIGHT);
	}

	function handleEntityPickerSelect(entity: EntityResult) {
		if (!editor) return;
		// Always the inline form. Pages hand files to the media widgets as
		// `![label](url)`; a message attaches files through the attach path
		// instead, so a picked file is a link here like any other ref.
		insertRef(editor.view, entityPickerFrom, entity.name, entity.url);
		showEntityPicker = false;
	}

	function closeEntityPicker() {
		showEntityPicker = false;
		editor?.view.focus();
	}

	// What gets sent. The document is already markdown; three transforms only.
	// The ref label: the picker writes `[@Label](url)` (the Pages form) and
	// the thread and the model have always seen `[Label](url)`, so the `@`
	// comes off here. A trailing empty list marker: on the phone Return
	// continues the list, so the natural last keystroke before Send leaves a
	// bare `- ` on its own line, which is never intended content. And a fence
	// left open gets its closing line, so the message is well-formed markdown
	// for anything that renders it later.
	const TRAILING_EMPTY_MARKER = /\n\s*(?:[-*+]|\d+[.)])(?:\s+\[[ xX]\])?\s*$/;
	function outgoingContent(): string {
		const text = docText
			.replace(/\[@([^\]]+)\]\(/g, "[$1](")
			.replace(/\s+$/, "")
			.replace(TRAILING_EMPTY_MARKER, "")
			.trim();
		return closeOpenFence(text);
	}

	function handleSubmit() {
		const content = outgoingContent();
		if ((!content && !allowEmptySubmit) || disabled) return;

		onSubmit?.(content);

		docText = "";
		value = "";
		editor?.setDoc("");
		inputIsEmpty = true;
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
		editor?.view.focus();
	}

	onMount(() => {
		editor = createComposerEditor({
			parent: inputEl,
			doc: value,
			placeholder,
			disabled,
			isMobile: () => mobileLayout.isMobile,
			onSubmit: handleSubmit,
			onChange: (doc) => {
				docText = doc;
				value = doc;
				inputIsEmpty = !doc.trim();
			},
			onFocusChange: (f) => {
				isFocused = f;
			},
			onHeight: setHeight,
			onAttach,
			onShiftTab: cycleMode,
			onEscape: () => {
				if (showEntityPicker) {
					closeEntityPicker();
					return true;
				}
				return false;
			},
			extensions: [
				createRefPicker({
					onOpen: (_coords, from) => {
						entityPickerFrom = from;
						showEntityPicker = true;
					},
					onClose: () => {
						showEntityPicker = false;
					},
					onQueryChange: () => {
						// RefPicker has its own search input; nothing to relay.
					},
				}),
			],
		});
		docText = value;
		inputIsEmpty = !value.trim();
	});

	onDestroy(() => {
		editor?.destroy();
		editor = null;
	});
</script>

<div class="chat-input-container {maxWidth} w-full">
	<!-- svelte-ignore a11y_click_events_have_key_events -->
	<div
		aria-label="Chat input"
		class="chat-input-wrapper bg-surface border border-border-strong cursor-text"
		class:focused={isFocused}
		class:multiline={isMultiline}
		class:sudo={agentMode === "sudo"}
		onclick={handleWrapperClick}
		role="textbox"
		tabindex="-1"
	>
		{#if onAttach || onModeChange}
			<button
				type="button"
				onclick={(e) => {
					e.stopPropagation();
					// No modes here (another composer): a one-row menu is just
					// an extra click, so (+) picks files straight away.
					if (onModeChange) openMenu(e.currentTarget);
					else pickFiles();
				}}
				class="pill-btn attach-button"
				aria-label="Add files or change mode"
				aria-haspopup="menu"
				title="Add files or change mode"
			>
				<Icon icon="ri:add-line" width="18" />
			</button>
		{/if}
		{#if onAttach}
			<input
				bind:this={fileInputEl}
				type="file"
				multiple
				accept={ATTACH_ACCEPT}
				class="sr-only"
				onchange={onFilesPicked}
			/>
			<input
				bind:this={cameraInputEl}
				type="file"
				accept="image/*"
				capture="environment"
				class="sr-only"
				onchange={onFilesPicked}
			/>
		{/if}

		<!-- The editor mounts here. The box's height is the spring; the
		     editor's own scroller stays unclipped and this box scrolls once
		     the content passes the cap, so the caret scrolls into view
		     through the nearest scrollable ancestor as CodeMirror expects. -->
		<div
			id="chat-input"
			bind:this={inputEl}
			class="chat-input text-foreground font-sans bg-transparent"
			class:empty={inputIsEmpty}
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

	{#if activeMode}
		<div class="mode-row">
			<button
				type="button"
				class="mode-chip"
				style:color={activeMode.color}
				onclick={(e) => {
					e.stopPropagation();
					openMenu(e.currentTarget, true);
				}}
				aria-haspopup="menu"
				title="Change mode (Shift+Tab)"
			>
				<Icon icon={activeMode.icon} width="13" style="color: inherit" />
				<span class="mode-name">{activeMode.name}</span>
				<span class="mode-description">{activeMode.description}</span>
			</button>
			{#if !mobileLayout.isMobile}
				<span class="mode-hint">Shift+Tab to switch</span>
			{/if}
		</div>
	{/if}
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

	/* Sudo mode holds the border in the error color, focused or not: the
	   state has to be impossible to miss while typing into it. */
	.chat-input-wrapper.sudo,
	.chat-input-wrapper.sudo.focused {
		border-color: var(--color-error) !important;
	}

	.mode-row {
		display: flex;
		align-items: center;
		gap: 0.625rem;
		padding: 0.375rem 0.875rem 0;
		font-size: 0.75rem;
		line-height: 1rem;
	}

	.mode-chip {
		display: inline-flex;
		align-items: center;
		gap: 0.3125rem;
		min-width: 0;
		cursor: pointer;
	}

	.mode-name {
		font-weight: 500;
	}

	.mode-description {
		color: var(--color-foreground-muted);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.mode-hint {
		margin-left: auto;
		flex-shrink: 0;
		color: var(--color-foreground-subtle);
	}

	.chat-input {
		flex: 1;
		min-width: 0;
		font-family: var(--font-sans);
	}

	/* The editor fills the animated box; the box, not the editor, clips. */
	.chat-input :global(.cm-editor) {
		height: auto;
		background: transparent;
	}
	.chat-input :global(.cm-scroller) {
		overflow: visible;
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
