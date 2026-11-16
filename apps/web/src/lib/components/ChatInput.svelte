<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import { onDestroy, onMount } from "svelte";
	import { Spring, prefersReducedMotion } from "svelte/motion";
	import { fly } from "svelte/transition";
	import { quintOut } from "svelte/easing";
	import ContextPanel from "$lib/components/chat/ContextPanel.svelte";
	import SkillsPanel from "$lib/components/chat/SkillsPanel.svelte";
	import type { RefSearchResult, SkillSummary, SuggestedQuestion } from "$lib/api/client";
	import { mobileLayout } from "$lib/stores/mobileLayout.svelte";
	import { createComposer, type Composer } from "$lib/document/composer";
	import { outgoingMessage } from "$lib/document/message";
	import { ATTACH_ACCEPT } from "$lib/components/chat/state/attachments.svelte";
	import { COMPOSER_OVERLAY_ATTR } from "$lib/components/chat/state/composerReserve";
	import { contextMenu, type ContextMenuItem } from "$lib/stores/contextMenu.svelte";
	import { canDictate, startDictation } from "$lib/tauri/bridge";
	import { inComposition } from "$lib/utils/ime";
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
		skill = $bindable(null),
		suggestion = null,
		onSuggestion = undefined,
		mentionQuery = $bindable(null),
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
		/** The applet the next message runs as, shown as a chip before the
		 *  text. It stays after a send, so an applet that asks questions gets
		 *  the answers, until the person takes it off. */
		skill?: SkillSummary | null;
		/** A question to offer in the empty field, in the placeholder's place. */
		suggestion?: SuggestedQuestion | null;
		/** What became of the suggestion: drawn (`shown`), sent (`used`), or
		 *  put away with Escape (`dismissed`). */
		onSuggestion?: (id: string, outcome: "shown" | "used" | "dismissed") => void;
		/** What is typed after an open `@`, else null. The chat sky lights
		 *  the moments it matches. */
		mentionQuery?: string | null;
	} = $props();

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
				{ id: "attach", label: "Add files", icon: "mynaui:paperclip", action: pickFiles },
				{ id: "photos", label: "Add photos", icon: "mynaui:image", action: () => photoInputEl?.click() },
			],
			{
				anchor: { x: r.left, y: r.top, width: r.width, height: r.height },
				placement: "top-start",
			},
		);
		attachOpen = true;
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
	// Set once the editor exists, so an effect that ran before it did (a
	// suggestion that arrived first) runs again.
	let editorReady = $state(false);
	let isFocused = $state(false);
	let inputIsEmpty = $state(true);
	// The `@` query while the editor's `@` trigger is open, else null.
	let mention = $state<string | null>(null);
	$effect(() => {
		mentionQuery = mention;
	});
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

	// The suggested question is drawn where the text would start, while the
	// field is empty and nothing else is in the way. It is a layer of its
	// own, not the placeholder, so a long question wraps and the field grows
	// to hold it. Assistive tech reads it as the placeholder.
	const ghost = $derived(
		suggestion && inputIsEmpty && !skill && !trayOpen && !disabled ? suggestion : null,
	);
	let ghostHeight = $state(0);
	// The suggestion filled in with Tab, until the message is sent.
	let filledFrom: string | null = null;
	const reportedShown = new Set<string>();

	$effect(() => {
		const visible = ghost ? "" : placeholder;
		const spoken = ghost?.question ?? placeholder;
		if (editorReady) editor?.setPlaceholder(visible, spoken);
	});

	// Reported once it is drawn in a tab someone can see, so the ledger's
	// "shown" means seen.
	$effect(() => {
		const id = ghost?.id;
		if (!id || reportedShown.has(id) || document.visibilityState !== "visible") return;
		reportedShown.add(id);
		onSuggestion?.(id, "shown");
	});

	// The keycap after the question says Tab takes it. It retires once the
	// person has sent a few suggestions, and shows only while the field has
	// the keyboard, which is when Tab would work.
	const HINT_KEY = "virtues.suggestionHint.used";
	const HINT_RETIRES_AFTER = 3;
	function hintUses(): number {
		try {
			return Number(localStorage.getItem(HINT_KEY) ?? 0) || 0;
		} catch {
			return 0;
		}
	}
	let showKeyHint = $state(hintUses() < HINT_RETIRES_AFTER);

	function useSuggestion() {
		// Held first: filling the field empties `ghost` before the next line.
		const used = ghost;
		if (!used || !editor) return;
		editor.fill(used.question, used.refs);
		filledFrom = used.id;
	}

	// The field is as tall as its content, or as the question drawn in it.
	let contentHeight = MIN_HEIGHT;
	function setHeight(contentPx: number) {
		contentHeight = contentPx;
		fitHeight(ghost ? ghostHeight : 0);
	}
	function fitHeight(ghostPx: number) {
		const want = Math.max(contentHeight, ghostPx > 0 ? ghostPx + 4 : 0);
		inputHeight.target = Math.min(Math.max(want, MIN_HEIGHT), MAX_HEIGHT);
	}
	$effect(() => {
		fitHeight(ghost ? ghostHeight : 0);
	});

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

	// A ref sent with no point, from the chat sky, takes the place of an open
	// `@` and what was typed after it, as a pick from the menu would.
	function mentionDropped(url: string, label: string, at: { x: number; y: number } | null) {
		if (!at && mention !== null) editor?.insertRef(label, url);
		else editor?.insertRefHere(label, url, at ?? undefined);
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
		mentionDropped(dropped.url, dropped.label, { x: e.clientX, y: e.clientY });
	}

	$effect(() => {
		const el = shellEl;
		if (!el) return;
		const onTab = (e: Event) => {
			const { url, label, at } = (
				e as CustomEvent<{ url: string; label: string; at: { x: number; y: number } | null }>
			).detail;
			mentionDropped(url, label, at);
		};
		el.addEventListener(MENTION_DROP_EVENT, onTab);
		return () => el.removeEventListener(MENTION_DROP_EVENT, onTab);
	});

	// The skill chip sits at the start of the first line, and the first
	// paragraph is indented by its width, so the text runs on beside it and
	// wraps under it as it would after an inline token.
	let skillChipWidth = $state(0);

	// Backspace in an empty field takes the skill chip off, as it would
	// delete a character before the caret.
	function onShellKeydown(e: KeyboardEvent) {
		if (e.key === "Backspace" && skill && inputIsEmpty && !trayOpen) {
			e.preventDefault();
			skill = null;
		}
	}

	// Tab takes the suggested question, Escape puts it away: only from the
	// text itself, so Tab still moves between the composer's buttons. Caught
	// before the editor, whose Tab moves focus and whose Escape blurs.
	function onShellKeydownCapture(e: KeyboardEvent) {
		if (!ghost || e.shiftKey || e.altKey || e.metaKey || e.ctrlKey || inComposition(e)) return;
		if (!inputEl?.contains(e.target as Node)) return;
		if (e.key === "Tab") {
			e.preventDefault();
			e.stopPropagation();
			useSuggestion();
		} else if (e.key === "Escape") {
			e.preventDefault();
			e.stopPropagation();
			onSuggestion?.(ghost.id, "dismissed");
		}
	}

	function handleSubmit() {
		const typed = outgoingMessage(docText);
		// A skill rides as the message's leading `/name`, which ChatView
		// reads to run the turn as it and which the transcript shows.
		const content = skill ? `/${skill.name}${typed ? ` ${typed}` : ""}` : typed;
		if ((!content && !allowEmptySubmit) || disabled) return;

		onSubmit?.(content);
		// Used means sent with the question still in it, not only filled in.
		if (filledFrom && suggestion?.id === filledFrom && typed.includes(suggestion.question)) {
			onSuggestion?.(filledFrom, "used");
			try {
				localStorage.setItem(HINT_KEY, String(hintUses() + 1));
			} catch {
				// Storage refused (private window): the hint keeps showing.
			}
			showKeyHint = hintUses() < HINT_RETIRES_AFTER;
		}
		filledFrom = null;

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
		if (target.closest("button")) {
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
				// A focused composer that is removed, by leaving the chat, blurs
				// while Svelte is still tearing the page down, where writing state
				// throws `state_unsafe_mutation`. The write waits a microtask, for
				// focus and blur alike so the two keep their order.
				queueMicrotask(() => (isFocused = f));
			},
			onHeight: setHeight,
			onAttach,
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
		editorReady = true;
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
		class:sudo={skill?.name === "admin"}
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
		onkeydowncapture={onShellKeydownCapture}
		role="textbox"
		tabindex="-1"
	>
		<div
			class="tray"
			class:open={trayOpen}
			inert={!trayOpen}
			{...{ [COMPOSER_OVERLAY_ATTR]: "" }}
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

			<div class="field-row" style:--skill-indent="{skill ? skillChipWidth + 6 : 0}px">
				{#if skill}
					<span class="skill-chip" title={skill.description} bind:offsetWidth={skillChipWidth}>
						<Icon icon={skill.icon ?? "mynaui:zap"} width="13" style="color: inherit" />
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
							<Icon icon="mynaui:x" width="12" />
						</button>
					</span>
				{/if}
				<!-- The editor mounts here. The box's height is the spring; this box
				     scrolls once the content passes the cap, so the caret scrolls
				     into view through the nearest scrollable ancestor, as
				     ProseMirror expects. -->
				{#if ghost}
					<div class="ghost" bind:offsetHeight={ghostHeight}>
						<span aria-hidden="true">{ghost.question}</span>
						{#if mobileLayout.isMobile}
							<!-- A phone has no Tab: the question is taken with a tap.
							     mousedown is held back so the field keeps focus. -->
							<button
								type="button"
								class="ghost-use"
								onmousedown={(e) => e.preventDefault()}
								onclick={(e) => {
									e.stopPropagation();
									useSuggestion();
								}}
								aria-label="Use the suggested question: {ghost.question}"
							>Use</button>
						{:else if showKeyHint && isFocused}
							<kbd class="ghost-key" aria-hidden="true">tab</kbd>
						{/if}
					</div>
				{/if}
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
						<span class="plus-mark"><Icon icon="mynaui:plus" width="18" /></span>
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
						<Icon icon="mynaui:microphone" width="17" />
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
							<Icon icon="mynaui:arrow-up" width="15" style="color: inherit" />
						</span>
						<span class="swap-icon" class:active={trailingMode === "send" && isBusy && !canQueue}>
							<Icon icon="mynaui:spinner" class="animate-spin" width="15" style="color: inherit" />
						</span>
						<span class="swap-icon" class:active={trailingMode === "stop"}>
							<Icon icon="mynaui:stop-solid" width="15" style="color: inherit" />
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
		box-shadow: 0 6px 18px -6px color-mix(in srgb, var(--color-foreground) 10%, transparent);
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

	/* The admin applet holds the border in the error color, focused or not:
	   a shell with sudo has to be impossible to miss while typing into it. */
	.chat-input-wrapper.sudo,
	.chat-input-wrapper.sudo.focused {
		border-color: var(--color-error) !important;
	}

	.field-row {
		position: relative;
		display: flex;
		align-items: flex-start;
		gap: 0.25rem;
		min-width: 0;
	}

	/* Over the start of the first line (the field's padding plus the
	   centring in a 1.5rem line), not beside the field. Shaped like a
	   mention in the composer; a neutral fill, because a skill is a command
	   and not a reference. */
	.skill-chip {
		position: absolute;
		top: 0.1875rem;
		left: 0.625rem;
		z-index: 1;
		display: inline-flex;
		align-items: center;
		gap: 0.1875rem;
		height: 1.375rem;
		padding: 0 0.125rem 0 0.375rem;
		border-radius: var(--radius-sm);
		background: var(--hover-bg);
		font-size: 0.8125rem;
		color: var(--color-foreground);
		white-space: nowrap;
	}

	.chat-input :global(.composer-field > :first-child) {
		text-indent: var(--skill-indent, 0px);
	}

	.chat-input :global(.composer-field p.is-editor-empty:first-child::before) {
		margin-left: var(--skill-indent, 0px);
	}

	/* A mention while it is being written is a token, so it is drawn as one:
	   a tinted pill (ref-badge.css's editing treatment) instead of the
	   dotted underline it has where it is read. Pages keep their own. */
	.chat-input :global(.doc-mention .ref-link) {
		padding: 0 0.3125rem;
		border-bottom: none;
		border-radius: var(--radius-sm);
		background: var(--ref-pill-bg);
		color: var(--ref-pill-color);
		transition: background-color 0.15s ease;
	}

	.chat-input :global(.doc-mention .ref-link:hover) {
		background: var(--ref-pill-bg-hover);
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

	/* The suggested question, drawn where the first character would go
	   (the field's 6px plus the paragraph's 4px, and its 2px top), in the
	   placeholder's color. Clicks pass through to the field. */
	.ghost {
		position: absolute;
		top: 0.125rem;
		left: 0.625rem;
		right: 0.625rem;
		font-size: 1rem;
		line-height: 1.5rem;
		color: color-mix(in srgb, var(--color-foreground-subtle) 60%, transparent);
		overflow-wrap: anywhere;
		pointer-events: none;
	}

	/* A keycap that sits in the line after the question and wraps with it. */
	.ghost-key {
		display: inline-block;
		margin-left: 0.375rem;
		padding: 0 0.3125rem;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-sm);
		font-family: inherit;
		font-size: 0.6875rem;
		line-height: 1.125rem;
		vertical-align: 0.0625rem;
		color: var(--color-foreground-subtle);
	}

	.ghost-use {
		margin-left: 0.375rem;
		padding: 0 0.4375rem;
		border-radius: var(--radius-sm);
		background: var(--hover-bg);
		font-size: 0.8125rem;
		line-height: 1.375rem;
		color: var(--color-foreground);
		pointer-events: auto;
		cursor: pointer;
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



	/* The field around the message as typed. The text itself is prose.css's
	   (`prose prose--compact`), so it reads as it will in the thread. */
	.chat-input :global(.composer-field) {
		padding: 0.125rem 0.25rem;
		outline: none;
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
	/* Softer than the subtle ink, so the empty field sits back. The
	   suggested question above uses the same color. */
	.chat-input :global(.composer-field p.is-editor-empty:first-child::before) {
		content: attr(data-placeholder);
		float: left;
		height: 0;
		pointer-events: none;
		color: color-mix(in srgb, var(--color-foreground-subtle) 60%, transparent);
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
