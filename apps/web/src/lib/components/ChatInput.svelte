<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import { onDestroy, onMount } from "svelte";
	import { Spring, prefersReducedMotion } from "svelte/motion";
	import { fly } from "svelte/transition";
	import { quintOut } from "svelte/easing";
	import ContextPanel from "$lib/components/chat/ContextPanel.svelte";
	import SkillsPanel from "$lib/components/chat/SkillsPanel.svelte";
	import type { RefSearchResult, SkillSummary } from "$lib/api/client";
	import { mobileLayout } from "$lib/stores/mobileLayout.svelte";
	import { createComposer, type Composer } from "$lib/document/composer";
	import { outgoingMessage } from "$lib/document/message";
	import { ATTACH_ACCEPT } from "$lib/components/chat/state/attachments.svelte";
	import {
		AGENT_MODES,
		DEPTH_NAMES,
		depthModes,
		getModeById,
		nextMode,
		type AgentMode,
		type AgentModeId,
	} from "$lib/config/agentModes";
	import { Popover } from "$lib/floating";
	import { contextMenu, type ContextMenuItem } from "$lib/stores/contextMenu.svelte";
	import { canDictate, startDictation } from "$lib/tauri/bridge";
	import { dndManager, MENTION_DROP_ATTR, MENTION_DROP_EVENT } from "$lib/stores/dndManager.svelte";
	import { droppedMention, isMentionDrag } from "$lib/utils/projectActions";

	let {
		value = $bindable(""),
		disabled = false,
		sendDisabled = false,
		allowEmptySubmit = false,
		isStreaming = false,
		maxWidth = "max-w-3xl",
		focused = $bindable(false),
		placeholder = "Ask anything, or type @ to add context",
		onAttach = undefined as ((files: File[]) => void) | undefined,
		onSubmit = undefined as ((content: string) => void) | undefined,
		onStop = undefined as (() => void) | undefined,
		agentMode = undefined as AgentModeId | undefined,
		onModeChange = undefined as ((mode: AgentModeId) => void) | undefined,
		modes = AGENT_MODES,
		skill = $bindable(null),
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
		/** The chat's mode, shown as the depth and access chips. */
		agentMode?: AgentModeId;
		/** Chosen from the depth or access chip, or cycled with Shift+Tab. */
		onModeChange?: (mode: AgentModeId) => void;
		/** The modes this chat may switch between (see `availableModes`). */
		modes?: AgentMode[];
		/** The skill the next message runs as, shown as a chip before the
		 *  text. It stays after a send, so a skill that asks questions gets
		 *  the answers, until the person takes it off. */
		skill?: SkillSummary | null;
	} = $props();

	// The one mode, shown as two chips (see DEPTH_ORDER). Full access runs at
	// the balanced depth, so that is where the depth chip sits while it is on.
	const depths = $derived(depthModes(modes));
	const depthId = $derived<AgentModeId>(
		agentMode && depths.some((m) => m.id === agentMode) ? agentMode : "chat",
	);
	const depthIndex = $derived(Math.max(0, depths.findIndex((m) => m.id === depthId)));
	const depthMode = $derived(getModeById(depthId));
	const hasAccess = $derived(modes.some((m) => m.id === "sudo"));
	const fullAccess = $derived(agentMode === "sudo");
	let depthOpen = $state(false);

	let fileInputEl: HTMLInputElement | null = $state(null);
	let photoInputEl: HTMLInputElement | null = $state(null);

	function pickFiles() {
		fileInputEl?.click();
	}

	// Whether focus is anywhere in the composer, the tray included.
	let focusWithin = $state(false);

	// Focus leaving the composer closes its menus too, as a click outside a
	// menu does. The date menu opens outside the composer and belongs to it.
	function onShellFocusOut(e: FocusEvent) {
		const shell = e.currentTarget as HTMLElement;
		if (shell.contains(e.relatedTarget as Node | null)) return;
		focusWithin = false;
		if (mention !== null && !contextMenu.visible) closeContext({ refocus: false });
		if (slash !== null) closeSkills({ refocus: false });
	}

	function cycleMode() {
		if (agentMode && onModeChange) onModeChange(nextMode(agentMode, modes));
	}

	function setDepth(i: number) {
		const m = depths[Math.min(Math.max(i, 0), depths.length - 1)];
		if (m && m.id !== agentMode) onModeChange?.(m.id);
	}

	function onDepthKey(e: KeyboardEvent) {
		const step: Record<string, number> = {
			ArrowRight: depthIndex + 1,
			ArrowUp: depthIndex + 1,
			ArrowLeft: depthIndex - 1,
			ArrowDown: depthIndex - 1,
			Home: 0,
			End: depths.length - 1,
		};
		if (!(e.key in step)) return;
		e.preventDefault();
		setDepth(step[e.key]);
	}

	// The track snaps to the nearest stop, on press and while dragging.
	function onTrackPointer(e: PointerEvent) {
		const el = e.currentTarget as HTMLElement;
		if (e.type === "pointerdown") el.setPointerCapture(e.pointerId);
		else if (!el.hasPointerCapture(e.pointerId)) return;
		const r = el.getBoundingClientRect();
		setDepth(Math.round(((e.clientX - r.left) / r.width) * (depths.length - 1)));
	}

	// (+) turns into a close mark while its menu is open.
	let attachOpen = $state(false);
	let attachWasOpen = false;
	$effect(() => {
		if (!contextMenu.visible) attachOpen = false;
	});

	// The date menu closed without handing the focus back (Escape, a click
	// elsewhere): the @ menu closes with it.
	$effect(() => {
		if (!contextMenu.visible && mention !== null && !focusWithin) closeContext({ refocus: false });
	});

	function openAttach(anchorEl: HTMLElement) {
		if (attachOpen) {
			contextMenu.hide();
			return;
		}
		const r = anchorEl.getBoundingClientRect();
		contextMenu.show(
			{ x: r.left, y: r.top },
			[
				{ id: "attach", label: "Add files", icon: "ri:attachment-2", action: pickFiles },
				{ id: "photos", label: "Add photos", icon: "ri:image-line", action: () => photoInputEl?.click() },
			],
			{
				anchor: { x: r.left, y: r.top, width: r.width, height: r.height },
				placement: "top-start",
			},
		);
		attachOpen = true;
	}

	function toggleAccess() {
		onModeChange?.(fullAccess ? "chat" : "sudo");
	}

	function onFilesPicked(e: Event) {
		const input = e.target as HTMLInputElement;
		if (input.files && input.files.length > 0) {
			onAttach?.(Array.from(input.files));
		}
		input.value = ""; // allow re-picking the same file
	}

	// The composer runs on the block editor's kernel, holding part of the
	// page contract (lib/document/composer.ts). `value` is the message's
	// markdown, as it is sent and as its draft is kept.
	let inputEl: HTMLDivElement;
	let editor: Composer | null = null;
	let isFocused = $state(false);
	let inputIsEmpty = $state(true);
	// The `@` query while the editor's `@` trigger is open, else null.
	let mention = $state<string | null>(null);
	// The `/` query while the skills menu is open, else null.
	let slash = $state<string | null>(null);
	// The tray above the card shows one view at a time.
	type TrayView = "context" | "skills";
	const trayView = $derived<TrayView | null>(
		mention !== null ? "context" : slash !== null ? "skills" : null,
	);
	const trayOpen = $derived(trayView !== null);
	// What the tray draws. While it closes it keeps the last view and query,
	// so the content folds away with it instead of switching first.
	let lastView: TrayView | null = null;
	let lastQuery = "";
	const shownView = $derived.by(() => (lastView = trayView ?? lastView));
	const shownQuery = $derived.by(() => (lastQuery = mention ?? slash ?? lastQuery));
	// Each opening starts the view fresh, so the @ panel comes back on All
	// with any date, and its entrance plays again.
	let wasOpen = false;
	let opens = 0;
	const viewKey = $derived.by(() => {
		if (trayOpen && !wasOpen) opens++;
		wasOpen = trayOpen;
		return `${shownView}:${opens}`;
	});
	// The tray's height follows its content, so opening, closing and moving
	// between views are all one height transition.
	let trayContentHeight = $state(0);
	const viewIn = $derived({ y: 6, duration: prefersReducedMotion.current ? 0 : 280, easing: quintOut });
	// Mirror of the editor's markdown, so an external `value` set can be told
	// apart from our own echo of a keystroke.
	let docText = "";

	// One line of 1rem × 1.5 plus the content padding; the cap matches the old
	// composer. The spring animates the mount box between the two.
	const MIN_HEIGHT = 28;
	const MAX_HEIGHT = 200;
	const inputHeight = new Spring(MIN_HEIGHT, { stiffness: 0.18, damping: 0.8 });

	// Only enable scrolling when at max height to prevent scrollbar flash during animation
	const shouldScroll = $derived(inputHeight.current >= MAX_HEIGHT - 1);

	// Can we submit? (has content, or staged refs/attachments allow an empty send)
	const canSubmit = $derived((!inputIsEmpty || allowEmptySubmit || skill !== null) && !sendDisabled);
	// The turn is in flight (content queued/sending) — show the spinner.
	const isBusy = $derived(sendDisabled && (!inputIsEmpty || allowEmptySubmit));

	// Text typed during a running turn is a follow-up for the parent to queue,
	// so the button offers Send while there is a draft and Stop once it is
	// empty. A phone has no Enter to reach the queue, so this is its only door.
	const canQueue = $derived(isStreaming && (!inputIsEmpty || allowEmptySubmit));

	// Which action the single trailing button performs right now.
	const trailingMode = $derived(isStreaming && !canQueue ? "stop" : "send");
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
				editor.focus();
			}
		}
	});

	// External `value` set (the host clears it after a send, or prefills it)
	// → replace the document. Our own onChange echo lands here equal to
	// docText and is a no-op.
	$effect(() => {
		if (editor && value !== docText) {
			docText = value;
			editor.setMarkdown(value);
		}
	});

	// Read the prop before the `?.`: while `editor` is still null the call
	// short-circuits, its argument is never read, and the effect would never
	// learn that it depends on it.
	$effect(() => {
		const next = disabled;
		editor?.setDisabled(next);
	});

	$effect(() => {
		const text = placeholder;
		editor?.setPlaceholder(text);
	});

	function setHeight(contentPx: number) {
		inputHeight.target = Math.min(Math.max(contentPx, MIN_HEIGHT), MAX_HEIGHT);
	}

	function pickContext(result: RefSearchResult) {
		// Always inline: a message attaches files through the attach path, so
		// a picked file is a ref here like any other.
		editor?.insertRef(result.name, result.url);
		mention = null;
	}

	/** Escape: the menu closes and what was typed after the `@` stays. */
	function closeContext(opts?: { refocus?: boolean }) {
		mention = null;
		editor?.closeRef(opts);
	}

	function pickSkill(picked: SkillSummary) {
		editor?.takeSlash();
		skill = picked;
		slash = null;
	}

	function closeSkills(opts?: { refocus?: boolean }) {
		slash = null;
		editor?.closeSlash(opts);
	}

	// A sidebar row or a window tab dropped on the composer is mentioned at
	// the caret. Rows arrive as an HTML drag, caught before the editor would
	// paste their title as text; tabs arrive from the tab drag
	// (`dndManager`) as MENTION_DROP_EVENT.
	let shellEl: HTMLDivElement | null = $state(null);
	let rowOver = $state(false);
	const mentionOver = $derived(rowOver || (shellEl !== null && dndManager.mentionTarget === shellEl));

	function mentionDropped(url: string, label: string) {
		editor?.insertRefHere(label, url);
	}

	function onDragOverCapture(e: DragEvent) {
		if (!isMentionDrag(e)) return;
		e.preventDefault();
		e.stopPropagation();
		if (e.dataTransfer) e.dataTransfer.dropEffect = "link";
		rowOver = true;
	}

	function onDropCapture(e: DragEvent) {
		rowOver = false;
		const dropped = droppedMention(e);
		if (!dropped) return;
		e.preventDefault();
		e.stopPropagation();
		mentionDropped(dropped.url, dropped.label);
	}

	$effect(() => {
		const el = shellEl;
		if (!el) return;
		const onTab = (e: Event) => {
			const { url, label } = (e as CustomEvent<{ url: string; label: string }>).detail;
			mentionDropped(url, label);
		};
		el.addEventListener(MENTION_DROP_EVENT, onTab);
		return () => el.removeEventListener(MENTION_DROP_EVENT, onTab);
	});

	// Backspace in an empty field takes the skill chip off, as it would
	// delete a character before the caret.
	function onShellKeydown(e: KeyboardEvent) {
		if (e.key === "Backspace" && skill && inputIsEmpty && !trayOpen) {
			e.preventDefault();
			skill = null;
		}
	}

	function handleSubmit() {
		const typed = outgoingMessage(docText);
		// A skill rides as the message's leading `/name`, which ChatView
		// reads to run the turn as it and which the transcript shows.
		const content = skill ? `/${skill.name}${typed ? ` ${typed}` : ""}` : typed;
		if ((!content && !allowEmptySubmit) || disabled) return;

		onSubmit?.(content);

		docText = "";
		value = "";
		editor?.setMarkdown("");
		inputIsEmpty = true;
		inputHeight.target = MIN_HEIGHT;
	}

	function handleStop() {
		onStop?.();
	}

	// The Mac app's mic: Apple's dictation, into this field. A phone has the
	// keyboard's own mic and a browser has nothing to ask, so only the Mac
	// shell new enough to answer shows it.
	let showMic = $state(false);
	function dictate() {
		editor?.focus();
		void startDictation();
	}

	function handleWrapperClick(e: MouseEvent) {
		const target = e.target as HTMLElement;
		// The depth popover renders inside the card, and a press on its
		// slider must not hand the focus back to the field.
		if (target.closest("button, .popover-content")) {
			return;
		}
		editor?.focus();
	}

	onMount(() => {
		editor = createComposer({
			element: inputEl,
			markdown: value,
			placeholder,
			disabled,
			isMobile: () => mobileLayout.isMobile,
			onSubmit: handleSubmit,
			onChange: (markdown) => {
				docText = markdown;
				value = markdown;
				inputIsEmpty = !markdown.trim();
			},
			onFocusChange: (f) => {
				isFocused = f;
			},
			onHeight: setHeight,
			onAttach,
			onShiftTab: cycleMode,
			onMentionOpen: (q) => (mention = q),
			onMentionQuery: (q) => {
				if (mention !== null) mention = q;
			},
			onMentionClose: () => (mention = null),
			onSlashOpen: (q) => (slash = q),
			onSlashQuery: (q) => {
				if (slash !== null) slash = q;
			},
			onSlashClose: () => (slash = null),
		});
		docText = value;
		inputIsEmpty = !value.trim();
		void canDictate().then((yes) => (showMic = yes));
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
		class="chat-input-wrapper border border-border-strong cursor-text"
		class:focused={isFocused}
		class:tray-open={trayOpen}
		class:sudo={fullAccess}
		class:mention-over={mentionOver}
		bind:this={shellEl}
		{...{ [MENTION_DROP_ATTR]: "" }}
		ondragovercapture={onDragOverCapture}
		ondragleave={(e) => {
			if (!shellEl?.contains(e.relatedTarget as Node | null)) rowOver = false;
		}}
		ondropcapture={onDropCapture}
		onclick={handleWrapperClick}
		onfocusin={() => (focusWithin = true)}
		onfocusout={onShellFocusOut}
		onkeydown={onShellKeydown}
		role="textbox"
		tabindex="-1"
	>
		<div
			class="tray"
			class:open={trayOpen}
			inert={!trayOpen}
			style:height="{trayOpen ? trayContentHeight : 0}px"
		>
			<div bind:offsetHeight={trayContentHeight}>
				{#key viewKey}
					<div class="tray-view panel" in:fly={viewIn}>
						{#if shownView === "context"}
							<ContextPanel
								query={shownQuery}
								active={mention !== null}
								onSelect={pickContext}
								onClose={() => closeContext()}
								onRangePicked={() => editor?.focus()}
							/>
						{:else if shownView === "skills"}
							<SkillsPanel
								query={shownQuery}
								active={slash !== null}
								onSelect={pickSkill}
								onClose={() => closeSkills()}
							/>
						{/if}
					</div>
				{/key}
			</div>
		</div>

		<div class="composer-card bg-surface">
			{#if onAttach}
				<input
					bind:this={fileInputEl}
					type="file"
					multiple
					accept={ATTACH_ACCEPT}
					class="sr-only"
					onchange={onFilesPicked}
				/>
				<!-- No `capture`: a phone then offers its library and its camera. -->
				<input
					bind:this={photoInputEl}
					type="file"
					multiple
					accept="image/*"
					class="sr-only"
					onchange={onFilesPicked}
				/>
			{/if}

			<div class="field-row">
				{#if skill}
					<span class="skill-chip" title={skill.description}>
						<Icon icon={skill.icon ?? "ri:flashlight-line"} width="13" style="color: inherit" />
						<span>/{skill.name}</span>
						<button
							type="button"
							class="skill-chip-remove"
							aria-label="Remove skill"
							onclick={(e) => {
								e.stopPropagation();
								skill = null;
								editor?.focus();
							}}
						>
							<Icon icon="ri:close-line" width="12" />
						</button>
					</span>
				{/if}
				<!-- The editor mounts here. The box's height is the spring; this box
				     scrolls once the content passes the cap, so the caret scrolls
				     into view through the nearest scrollable ancestor, as
				     ProseMirror expects. -->
				<div
					id="chat-input"
					bind:this={inputEl}
					class="chat-input text-foreground font-sans bg-transparent"
					class:empty={inputIsEmpty}
					style:height="{inputHeight.current}px"
					style:overflow-y={shouldScroll ? 'auto' : 'hidden'}
				></div>
			</div>

			<div class="composer-toolbar">
				{#if onAttach}
					<!-- The menu closes itself on the press outside it, so the
					     press records whether this one was meant to close it. -->
					<button
						type="button"
						onpointerdown={() => (attachWasOpen = attachOpen)}
						onclick={(e) => {
							e.stopPropagation();
							if (attachWasOpen) {
								attachWasOpen = false;
								return;
							}
							openAttach(e.currentTarget);
						}}
						class="pill-btn attach-button"
						class:open={attachOpen}
						aria-label={attachOpen ? "Close" : "Add files or photos"}
						aria-haspopup="menu"
						aria-expanded={attachOpen}
						title="Add files or photos"
					>
						<span class="plus-mark"><Icon icon="ri:add-line" width="18" /></span>
					</button>
				{/if}

				{#if onModeChange && depths.length > 1 && depthMode}
					<Popover bind:open={depthOpen} placement="top-start" offset={6}>
						{#snippet trigger({ toggle }: { toggle: () => void })}
							<button
								type="button"
								class="chip"
								class:open={depthOpen}
								onclick={(e) => {
									e.stopPropagation();
									toggle();
								}}
								aria-haspopup="dialog"
								aria-expanded={depthOpen}
								title="Depth (Shift+Tab to switch)"
							>
								<Icon icon={depthMode.icon} width="14" style="color: inherit" />
								<span>{DEPTH_NAMES[depthMode.id] ?? depthMode.name}</span>
							</button>
						{/snippet}
						{#snippet children()}
							<div class="depth-panel">
								<div class="depth-head">
									<span class="depth-label">Depth</span>
									<span class="depth-current">
										<Icon icon={depthMode.icon} width="14" style="color: inherit" />
										{DEPTH_NAMES[depthMode.id] ?? depthMode.name}
									</span>
								</div>
								<div
									class="depth-track"
									role="slider"
									tabindex="0"
									aria-label="Depth"
									aria-valuemin={0}
									aria-valuemax={depths.length - 1}
									aria-valuenow={depthIndex}
									aria-valuetext={DEPTH_NAMES[depthMode.id] ?? depthMode.name}
									style:--stops={depths.length - 1}
									style:--at={depthIndex}
									onkeydown={onDepthKey}
									onpointerdown={onTrackPointer}
									onpointermove={onTrackPointer}
								>
									{#each depths as m, i (m.id)}
										<span class="depth-dot" style:--i={i} title={DEPTH_NAMES[m.id] ?? m.name}></span>
									{/each}
									<span class="depth-thumb"></span>
								</div>
							</div>
						{/snippet}
					</Popover>
				{/if}

				{#if onModeChange && hasAccess}
					<span class="chip-sep" aria-hidden="true">&middot;</span>
					<button
						type="button"
						class="chip"
						class:danger={fullAccess}
						onclick={(e) => {
							e.stopPropagation();
							toggleAccess();
						}}
						aria-pressed={fullAccess}
						title={fullAccess
							? "Full access: runs commands on your server and asks before deleting anything. Click to ask first."
							: "Ask first: asks before changing anything. Click for full access."}
					>
						<Icon icon={fullAccess ? "ri:lock-unlock-line" : "ri:lock-line"} width="14" style="color: inherit" />
						<span>{fullAccess ? "Full access" : "Ask first"}</span>
					</button>
				{/if}

				<span class="toolbar-spacer"></span>

				{#if showMic}
					<!-- mousedown is held back so the field keeps the focus the
					     dictation writes into. -->
					<button
						type="button"
						onmousedown={(e) => e.preventDefault()}
						onclick={dictate}
						{disabled}
						class="pill-btn mic-button"
						aria-label="Dictate"
						title="Dictate"
					>
						<Icon icon="ri:mic-line" width="17" />
					</button>
				{/if}
				<!-- One persistent trailing button — its icon flips between send and
				     stop so a turn starting animates rather than swaps. -->
				<button
					type="button"
					onclick={trailingAction}
					disabled={trailingMode === "send" && !canSubmit && !canQueue}
					class="pill-btn action-btn"
					aria-label={trailingLabel}
				>
					<span class="icon-swap">
						<span class="swap-icon" class:active={trailingMode === "send" && (!isBusy || canQueue)}>
							<Icon icon="ri:arrow-up-line" width="15" style="color: inherit" />
						</span>
						<span class="swap-icon" class:active={trailingMode === "send" && isBusy && !canQueue}>
							<Icon icon="ri:loader-4-line" class="animate-spin" width="15" style="color: inherit" />
						</span>
						<span class="swap-icon" class:active={trailingMode === "stop"}>
							<Icon icon="ri:stop-fill" width="15" style="color: inherit" />
						</span>
					</span>
				</button>
			</div>
		</div>
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

	/* The shell: a tinted frame around the white card. At rest the card
	   fills it; the tray opens inside it, above the card. */
	.chat-input-wrapper {
		--shell-radius: 1rem;
		position: relative;
		display: flex;
		flex-direction: column;
		/* design-ok: the shell's corners are the card's plus the frame, so the two curves stay concentric. */
		border-radius: var(--shell-radius);
		background: color-mix(in srgb, var(--color-foreground) 3%, var(--color-surface));
		/* design-ok: the composer floats over the page (§6 elevation), so it takes a soft lift. */
		box-shadow: 0 6px 18px -6px color-mix(in srgb, var(--color-foreground) 12%, transparent);
		/* Out at hover-out. A state that comes on (ChatView's drop ring) sets
		   its own hover-in duration. */
		transition:
			border-color var(--motion-hover-out) var(--ease-premium),
			box-shadow var(--motion-hover-out) var(--ease-premium);
	}

	/* The card: the message on top, the toolbar under it. */
	.composer-card {
		position: relative;
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		padding: 0.75rem 0.625rem 0.5rem;
		border-top: 1px solid transparent;
		/* design-ok: one pixel inside the shell's radius, so the card sits flush in it. */
		border-radius: calc(var(--shell-radius) - 1px);
		transition: border-color var(--motion-enter) var(--ease-premium);
	}

	.tray-open .composer-card {
		border-top-color: var(--color-border);
	}

	/* Closing is quicker than opening and the content fades out before the
	   height has finished, so nothing is left hanging over the card. */
	.tray {
		height: 0;
		overflow: hidden;
		opacity: 0;
		transition:
			height 200ms var(--ease-premium),
			opacity 120ms var(--ease-premium);
	}

	/* Opening ends on a long ease-out (quint), so the top edge settles
	   slowly instead of stopping. */
	.tray.open {
		opacity: 1;
		transition:
			height 340ms cubic-bezier(0.22, 1, 0.36, 1),
			opacity 200ms var(--ease-premium) 40ms;
	}

	.tray-view {
		display: flex;
		align-items: center;
		gap: 0.25rem;
		padding: 0.4375rem 0.5rem;
	}

	.tray-view.panel {
		padding: 0.5rem 0.5rem 0.375rem;
	}

	@media (prefers-reduced-motion: reduce) {
		.tray,
		.tray.open,
		.composer-card {
			transition: none;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.chat-input-wrapper {
			transition: none;
		}
	}

	.chat-input-wrapper:hover {
		border-color: color-mix(in srgb, var(--color-foreground) 28%, var(--color-border-strong));
	}

	/* A row or tab held over the composer, which would mention it. */
	.chat-input-wrapper.mention-over {
		border-color: var(--color-primary);
	}

	.chat-input-wrapper.focused {
		border-color: color-mix(in srgb, var(--color-foreground) 28%, var(--color-border-strong));
	}

	/* Sudo mode holds the border in the error color, focused or not: the
	   state has to be impossible to miss while typing into it. */
	.chat-input-wrapper.sudo,
	.chat-input-wrapper.sudo.focused {
		border-color: var(--color-error) !important;
	}

	.field-row {
		display: flex;
		align-items: flex-start;
		gap: 0.25rem;
		min-width: 0;
	}

	.skill-chip {
		display: inline-flex;
		align-items: center;
		gap: 0.25rem;
		flex-shrink: 0;
		height: 1.5rem;
		margin: 0.125rem 0 0 0.25rem;
		padding: 0 0.25rem 0 0.5rem;
		border-radius: var(--radius-md);
		background: var(--hover-bg);
		font-size: 0.8125rem;
		color: var(--color-foreground);
		white-space: nowrap;
	}

	.skill-chip-remove {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 1.125rem;
		height: 1.125rem;
		border-radius: var(--radius-sm);
		color: var(--color-foreground-muted);
		cursor: pointer;
	}

	.skill-chip-remove:hover {
		color: var(--color-foreground);
	}

	.chat-input {
		flex: 1;
		min-width: 0;
		padding: 0 0.375rem;
		font-family: var(--font-sans);
	}

	/* (+) turns 45 degrees into a close mark. */
	.plus-mark {
		display: flex;
		transition: transform 0.2s var(--ease-premium);
	}

	.attach-button.open .plus-mark {
		transform: rotate(45deg);
	}

	@media (prefers-reduced-motion: reduce) {
		.plus-mark {
			transition: none;
		}
	}

	.composer-toolbar {
		display: flex;
		align-items: center;
		gap: 0.125rem;
		min-width: 0;
	}

	.toolbar-spacer {
		flex: 1;
	}

	.chip {
		display: inline-flex;
		align-items: center;
		gap: 0.3125rem;
		height: 1.875rem;
		padding: 0 0.5rem;
		border-radius: var(--radius-md);
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
		white-space: nowrap;
		cursor: pointer;
		transition:
			background-color 0.15s ease,
			color 0.15s ease;
	}

	.chip:hover,
	.chip.open {
		background: var(--hover-bg);
		color: var(--color-foreground);
	}

	.chip.danger {
		color: var(--color-error);
	}

	.chip-sep {
		color: var(--color-foreground-subtle);
		font-size: 0.75rem;
		user-select: none;
	}

	.depth-panel {
		display: flex;
		flex-direction: column;
		gap: 0.625rem;
		width: 14rem;
		padding: 0.625rem 0.75rem 0.75rem;
	}

	.depth-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		font-size: 0.8125rem;
	}

	.depth-label {
		color: var(--color-foreground-muted);
	}

	.depth-current {
		display: inline-flex;
		align-items: center;
		gap: 0.3125rem;
		color: var(--color-foreground);
	}

	/* Stops sit at the centers of equal slots, so the thumb, the dots and
	   the pointer math in onTrackPointer agree. */
	.depth-track {
		--pad: 0.1875rem;
		--thumb: 1.375rem;
		--span: calc(100% - 2 * var(--pad) - var(--thumb));
		position: relative;
		height: 1.625rem;
		border-radius: var(--radius-md);
		background: color-mix(in srgb, var(--color-foreground) 8%, transparent);
		cursor: pointer;
		touch-action: none;
		outline: none;
	}

	.depth-track:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: 1px;
	}

	.depth-dot,
	.depth-thumb {
		position: absolute;
		top: 50%;
		left: calc(var(--pad) + var(--span) * var(--pos) / max(var(--stops), 1));
	}

	.depth-dot {
		--pos: var(--i);
		width: var(--thumb);
		height: 0.25rem;
		margin-top: -0.125rem;
		display: flex;
		justify-content: center;
	}

	.depth-dot::before {
		content: "";
		width: 0.25rem;
		height: 0.25rem;
		border-radius: var(--radius-full);
		background: var(--color-foreground-subtle);
	}

	.depth-thumb {
		--pos: var(--at);
		width: var(--thumb);
		height: calc(100% - 2 * var(--pad));
		transform: translateY(-50%);
		border-radius: var(--radius-sm);
		background: var(--color-surface);
		/* design-ok: the thumb is a physical knob on the track. */
		box-shadow:
			0 0 0 1px color-mix(in srgb, var(--color-foreground) 10%, transparent),
			0 1px 3px color-mix(in srgb, var(--color-foreground) 15%, transparent);
		transition: left 0.22s var(--ease-premium);
	}

	@media (prefers-reduced-motion: reduce) {
		.depth-thumb {
			transition: none;
		}
	}


	/* The field around the message as typed. The text itself is prose.css's
	   (`prose prose--compact`), so it reads as it will in the thread. */
	.chat-input :global(.composer-field) {
		padding: 0.125rem 0.25rem;
		outline: none;
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
	.chat-input :global(.composer-field p.is-editor-empty:first-child::before) {
		content: attr(data-placeholder);
		float: left;
		height: 0;
		pointer-events: none;
		color: var(--color-foreground-subtle);
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

	.attach-button,
	.mic-button {
		color: var(--color-foreground-muted);
	}
	.attach-button:hover,
	.attach-button.open,
	.mic-button:hover:not(:disabled) {
		background: var(--hover-bg);
		color: var(--color-foreground);
	}

	/* Send is a soft square: grey while there is nothing to send, the navy
	   primary once there is. */
	.action-btn {
		width: 1.875rem;
		height: 1.875rem;
		margin-left: 0.25rem;
		border-radius: var(--radius-md);
		background-color: var(--color-primary);
		color: var(--color-surface);
	}
	.action-btn:hover:not(:disabled) {
		background-color: var(--color-primary-hover);
	}
	.action-btn:active:not(:disabled) {
		background-color: var(--color-primary-active);
	}

	.action-btn:disabled {
		background-color: color-mix(in srgb, var(--color-foreground) 8%, transparent);
		color: var(--color-foreground-subtle);
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
