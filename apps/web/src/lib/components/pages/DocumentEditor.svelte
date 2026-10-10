<!--
	DocumentEditor: the block editor for a page whose text is a Yjs tree
	(lib/document). Mount it once the binding has synced; it writes nothing on
	mount.

	It drives the page's existing pieces: RefPicker on `@`, SlashMenu on `/`,
	SelectionToolbar on a selection (lib/document/toolbar.ts says when and
	where; in a code block it offers Ask AI alone), AiPromptPopover on ⌘J,
	`/ai` and Ask AI, the link panel on ⌘K (claimed from the app's own ⌘K
	while there is a link or text to link), FindBar on ⌘F, TableMenu in a
	table (Alt-F10 takes the keyboard there), SuggestionBar in a proposal.
	It reports what the page around it shows: the status bar's counts
	(`onDocChange`, 300 ms after the last change, saying whether any of it
	was typed here), the outline (`onOutline`, `onOutlineNav`) and the
	editor itself (`onEditorReady`, and the registry by page).
-->
<script lang="ts">
	import { onDestroy, onMount } from "svelte";
	import type { Editor } from "@tiptap/core";
	import { Extension } from "@tiptap/core";
	import { isChangeOrigin } from "@tiptap/extension-collaboration";
	import { TextSelection } from "@tiptap/pm/state";
	import type { EditorView } from "@tiptap/pm/view";
	import { toast } from "svelte-sonner";
	import "$lib/document/document.css";
	import { codeAt, createPageEditor, setPageEditorState, type EditorBinding, type TreeAiDriver } from "$lib/document/editor";
	import { appletCommands, insertEntity, treeCommands, type TreeCommand } from "$lib/document/commands";
	import { closeTrigger, pickedRange, putBackQuery, triggerState, triggers } from "$lib/document/triggers";
	import { find } from "$lib/document/find";
	import { focusMode, setFocusMode } from "$lib/document/focus";
	import { codeHighlight } from "$lib/document/code";
	import { pasteHandling } from "$lib/document/paste";
	import { mediaUploads, uploadFiles } from "$lib/document/media";
	import { anchorAt, anchorRange, pinAt, placeOf, resolveRange, type Anchor, type AnchoredRange, type Pinned } from "$lib/document/anchor";
	import { canEditLinkAtSelection, editLinkAtSelection, linkContext, links, openLinkBeside, previewType } from "$lib/document/links";
	import { cellAt, selectionForCellMenu, tableActions, tableBarAnchor, tableMenuItems } from "$lib/document/tables";
	import { aiPresence } from "$lib/document/presence";
	import {
		acceptAllProposals,
		acceptProposal,
		proposalAt,
		proposalIds,
		proposalKeys,
		proposalSaid,
		rejectAllProposals,
		rejectProposal,
	} from "$lib/document/suggestions";
	import { selectionToolbarAt, toolbarTiming, visibleBottom, type ToolbarTiming } from "$lib/document/toolbar";
	import { shortcuts } from "$lib/shortcuts/registry.svelte";
	import { contextGesture, type ContextHandler } from "$lib/document/gesture";
	import { registerTreeEditor, unregisterTreeEditor } from "$lib/document/registry";
	import { treeHeadings, treeOutlineNav, scrollerOf } from "$lib/document/outline";
	import { treeStats } from "$lib/document/stats";
	import { filterCommands } from "$lib/components/menuCommand";
	import type { DocStats } from "$lib/components/pages/stats";
	import type { OutlineNav, PageHeading } from "$lib/components/pages/outline";
	import type { AiIntent } from "$lib/ai/inlineComplete";
	import { listApplets } from "$lib/api/client";
	import { contextMenu } from "$lib/stores/contextMenu.svelte";
	import { pageDisplay } from "$lib/stores/pageDisplay.svelte";
	import RefPicker, { type EntityResult } from "$lib/components/RefPicker.svelte";
	import RefPreview from "$lib/components/RefPreview.svelte";
	import SlashMenu from "$lib/components/SlashMenu.svelte";
	import SelectionToolbar from "$lib/components/SelectionToolbar.svelte";
	import AiPromptPopover from "$lib/components/pages/AiPromptPopover.svelte";
	import FindBar from "$lib/components/pages/FindBar.svelte";
	import TableMenu from "$lib/components/pages/TableMenu.svelte";
	import SuggestionBar from "$lib/components/pages/SuggestionBar.svelte";

	interface Props {
		/** The page's binding: a `TreeDocument`, synced. */
		doc: EditorBinding;
		pageId: string;
		/** The page's title, for the inline writer's context. */
		pageTitle?: string;
		editable: boolean;
		placeholder?: string;
		/** The inline writer; without one, Ask AI is not offered. */
		ai?: TreeAiDriver;
		/** 300 ms after the last change; `local` when any of it was typed here. */
		onDocChange?: (stats: DocStats, local: boolean) => void;
		onOutline?: (headings: PageHeading[]) => void;
		onOutlineNav?: (nav: OutlineNav | null) => void;
		onEditorReady?: (editor: Editor | null) => void;
	}

	let {
		doc,
		pageId,
		pageTitle,
		editable,
		placeholder = "Start writing, or press / for commands…",
		ai,
		onDocChange,
		onOutline,
		onOutlineNav,
		onEditorReady,
	}: Props = $props();

	const DOC_CHANGE_MS = 300;

	let host: HTMLDivElement;
	let frame: HTMLDivElement;
	let editor = $state<Editor | null>(null);
	/** Bumped on every transaction, so overlays that read the editor follow it. */
	let revision = $state(0);

	// ── Menus ────────────────────────────────────────────────────────────
	let slashOpen = $state(false);
	let slashPos = $state({ x: 0, y: 0 });
	let slashQuery = $state("");
	let slashFrom = 0;
	/** The applets to insert, when the Applet command asked for them; otherwise the commands. */
	let appletList = $state<TreeCommand[] | null>(null);
	/** Where the menus over the page opened, held to the page as it scrolls (`placeMenus`). */
	let slashPin: Pinned | null = null;
	let pickerPin: Pinned | null = null;
	let aiPin: Pinned | null = null;

	let pickerOpen = $state(false);
	let pickerPos = $state({ x: 0, y: 0 });
	/** What was typed after the `@` before the picker opened: its first search. */
	let pickerQuery = $state("");
	/**
	 * What the mention replaces when the `@` is no longer open: the `@`, or
	 * nothing at a position. Anchored, so edits made while the picker is
	 * open move it with the text.
	 */
	let pickerRange: AnchoredRange | null = null;

	let toolbarOpen = $state(false);
	let toolbarPos = $state({ x: 0, y: 0 });
	/** The selection is in a code block, which holds no marks: the toolbar offers Ask AI alone. */
	let toolbarInCode = $state(false);
	/** When the toolbar may show: never mid-drag, a keyboard selection once it holds still. */
	let toolbarTimer: ToolbarTiming | null = null;
	let toolbarMarks = $state({ strong: false, em: false, underline: false, code: false, strikethrough: false, link: false });
	/** The selection the toolbar was dismissed on; it stays away until the selection changes. */
	let toolbarDismissed: { from: number; to: number } | null = null;
	/** When the selection last moved: a click that moves it also closes the toolbar, and is no dismissal. */
	let selectionMovedAt = 0;

	let aiOpen = $state(false);
	let aiPos = $state({ x: 0, y: 0 });
	let aiIntent = $state<AiIntent>("continue");

	let findOpen = $state(false);
	let findInitial = $state("");

	let tableAnchor = $state<{ x: number; y: number; width: number } | null>(null);
	/** The element the page scrolls in, once mounted. */
	let pageScroller: HTMLElement | null = null;
	let tableMenu = $state<{ focus(): void } | null>(null);
	/** The keyboard is in the table's bar, which stays while it is. */
	let tableMenuFocused = false;

	let suggestion = $state<{ id: string; x: number; y: number; count: number } | null>(null);
	/** What a screen reader is told as the caret comes into a proposal; the bar beside it is never focused. */
	let suggestionSaid = $state("");

	let hover = $state<{ anchor: HTMLElement; href: string; label: string } | null>(null);
	let hoverHide: ReturnType<typeof setTimeout> | null = null;

	let fileInput: HTMLInputElement;
	let fileAccept = $state("*/*");
	/** Where the files the file dialog returns go: anchored while it is open. */
	let fileAt: Anchor | null = null;

	// ── Reporting ────────────────────────────────────────────────────────
	let changeTimer: ReturnType<typeof setTimeout> | null = null;
	let changedHere = false;

	function report() {
		changeTimer = null;
		if (!editor || editor.isDestroyed) return;
		const local = changedHere;
		changedHere = false;
		onDocChange?.(treeStats(editor.state.doc), local);
		onOutline?.(treeHeadings(editor.state.doc));
	}

	function scheduleReport(local: boolean) {
		changedHere ||= local;
		if (changeTimer) clearTimeout(changeTimer);
		changeTimer = setTimeout(report, DOC_CHANGE_MS);
	}

	// ── Helpers ──────────────────────────────────────────────────────────
	function coordsAt(view: EditorView, pos: number): { x: number; y: number } {
		const c = view.coordsAtPos(Math.min(pos, view.state.doc.content.size));
		return { x: c.left, y: c.bottom };
	}

	const coarse = () => typeof matchMedia !== "undefined" && matchMedia("(pointer: coarse)").matches;

	// ── The insert menu and its pickers ──────────────────────────────────
	const commandHost = {
		pickFiles(kind: "image" | "file", pos: number) {
			if (!editor) return;
			fileAccept = kind === "image" ? "image/*" : "*/*";
			fileAt = anchorAt(editor.state, pos);
			// The accept change must reach the input before it opens.
			queueMicrotask(() => fileInput?.click());
		},
		async pickApplet(pos: number) {
			if (!editor) return;
			const at = coordsAt(editor.view, pos);
			const pin = pinAt(editor.view, pos, at);
			try {
				const list = appletCommands(await listApplets(), pin.at);
				if (!list.length) {
					toast("No applet on your server has a face to show in a page yet.");
					return;
				}
				appletList = list;
				slashPin = pin;
				slashPos = placeOf(editor.view, pin) ?? at;
				slashOpen = true;
			} catch (err) {
				toast.error("Couldn't load your applets", {
					description: err instanceof Error ? err.message : String(err),
				});
			}
		},
		mention(pos: number) {
			if (!editor) return;
			pickerRange = anchorRange(editor.state, pos, pos);
			pickerQuery = "";
			pickerPos = coordsAt(editor.view, pos);
			pickerPin = pinAt(editor.view, pos, pickerPos);
			pickerOpen = true;
		},
		askAi(pos: number) {
			if (!editor || !ai) return;
			openAi(coordsAt(editor.view, pos), "continue", pos);
		},
	};

	const allCommands = treeCommands(commandHost);
	const commands = $derived.by(() => {
		if (appletList) return appletList;
		const list = ai ? allCommands : allCommands.filter((c) => c.label !== "Ask AI");
		return filterCommands(list, slashQuery);
	});

	function handleSlashSelect(cmd: TreeCommand) {
		if (!editor) return;
		const listing = appletList !== null;
		slashOpen = false;
		appletList = null;
		if (listing) {
			cmd.run(editor, { from: 0, to: 0 });
			return;
		}
		// Where the `/` is now: someone else's edit may have moved it.
		const at = triggerState(editor.state, "/");
		const range = { from: at.active ? at.from : slashFrom, to: editor.state.selection.from };
		closeTrigger(editor.view, "/");
		cmd.run(editor, range);
	}

	function handleSlashClose() {
		slashOpen = false;
		appletList = null;
		if (editor) {
			closeTrigger(editor.view, "/");
			editor.commands.focus();
		}
	}

	function handleEntity(entity: EntityResult) {
		if (!editor) return;
		pickerOpen = false;
		const caret = editor.state.selection.from;
		const at = (pickerRange && resolveRange(pickerRange)) ?? { from: caret, to: caret };
		const range = pickedRange(editor.state, "@", at);
		closeTrigger(editor.view, "@");
		insertEntity(editor, range, entity);
	}

	/** The picker closed; with nothing picked, what was typed in its box goes back after the `@`. */
	function handlePickerClose(typed?: string) {
		pickerOpen = false;
		if (!editor) return;
		if (typed) {
			const caret = editor.state.selection.from;
			const at = (pickerRange && resolveRange(pickerRange)) ?? { from: caret, to: caret };
			putBackQuery(editor.view, "@", typed, pickerQuery, at);
		} else {
			closeTrigger(editor.view, "@");
		}
		editor.commands.focus();
	}

	function handleFiles(e: Event) {
		const input = e.target as HTMLInputElement;
		const files = Array.from(input.files ?? []);
		input.value = "";
		if (editor && files.length) void uploadFiles(editor.view, files, fileAt ?? editor.state.selection.from);
	}

	// ── Selection toolbar ────────────────────────────────────────────────
	type MarkName = "strong" | "em" | "underline" | "code" | "strikethrough" | "link";

	function handleFormat(mark: MarkName) {
		if (!editor) return;
		const c = editor.chain().focus();
		if (mark === "strong") c.toggleBold().run();
		else if (mark === "em") c.toggleItalic().run();
		else if (mark === "underline") c.toggleUnderline().run();
		else if (mark === "code") c.toggleCode().run();
		else if (mark === "strikethrough") c.toggleStrike().run();
		else {
			toolbarOpen = false;
			editLinkAtSelection(editor, toolbarPos);
		}
	}

	function handleToolbarClose() {
		if (!editor) return;
		// A click in the page that made a new selection: the bar follows it.
		if (performance.now() - selectionMovedAt < 400) {
			updateToolbar(editor);
			return;
		}
		toolbarOpen = false;
		toolbarDismissed = { from: editor.state.selection.from, to: editor.state.selection.to };
	}

	function updateToolbar(e: Editor) {
		const sel = e.state.selection;
		const place = codeAt(e.state);
		const show =
			e.isEditable &&
			sel instanceof TextSelection &&
			!sel.empty &&
			place !== "across" &&
			(place === "text" || !!ai) &&
			e.view.hasFocus() &&
			!e.view.composing &&
			(toolbarTimer?.ready() ?? true) &&
			!(toolbarDismissed && toolbarDismissed.from === sel.from && toolbarDismissed.to === sel.to);
		if (!show) {
			toolbarOpen = false;
			if (toolbarDismissed && (sel.from !== toolbarDismissed.from || sel.to !== toolbarDismissed.to)) toolbarDismissed = null;
			return;
		}
		const start = e.view.coordsAtPos(sel.from);
		const end = e.view.coordsAtPos(sel.to);
		// On touch the platform's own menu sits above a selection; the bar goes
		// under it, unless the keyboard leaves no room there.
		toolbarPos = selectionToolbarAt(start, end, { coarse: coarse(), bottom: visibleBottom() });
		toolbarInCode = place === "code";
		toolbarMarks = {
			strong: e.isActive("bold"),
			em: e.isActive("italic"),
			underline: e.isActive("underline"),
			code: e.isActive("code"),
			strikethrough: e.isActive("strike"),
			link: e.isActive("link"),
		};
		toolbarOpen = true;
	}

	// ── The inline writer ────────────────────────────────────────────────
	/** The prompt at `at` on screen, opened at `pos` in the page. */
	function openAi(at: { x: number; y: number }, intent: AiIntent, pos: number) {
		// A code block takes a rewrite outright; across its edge nothing could be proposed.
		if (!editor || !ai || codeAt(editor.state) === "across") return;
		aiPos = at;
		aiPin = pinAt(editor.view, pos, at);
		aiIntent = intent;
		aiOpen = true;
	}

	function openAiAtCaret() {
		if (!editor) return;
		const sel = editor.state.selection;
		openAi(coordsAt(editor.view, sel.head), sel.empty ? "continue" : "rewrite", sel.head);
	}

	function handleAiSubmit(instruction: string) {
		aiOpen = false;
		if (!editor || !ai) return;
		editor.commands.focus();
		ai.start({ editor, intent: aiIntent, instruction, pageTitle });
	}

	function handleAiClose() {
		aiOpen = false;
		editor?.commands.focus();
	}

	// ── Tables and proposals, while the caret is in one ─────────────────
	function updateTable(e: Editor) {
		const at = e.isEditable && (e.view.hasFocus() || tableMenuFocused) ? cellAt(e.state) : null;
		if (!at) {
			tableAnchor = null;
			return;
		}
		const dom = e.view.nodeDOM(at.tablePos) as HTMLElement | null;
		const box = dom?.getBoundingClientRect();
		// The top of the page's visible part: its scroller's, or the window's.
		const top = !pageScroller || pageScroller === document.scrollingElement ? 0 : Math.max(0, pageScroller.getBoundingClientRect().top);
		tableAnchor = box ? tableBarAnchor(box, top) : null;
	}

	/**
	 * The menus opened at a place in the page (the `/` menu, the applet
	 * list, the `@` picker, the inline writer's prompt) go where that place
	 * is now: each is placed once, by screen coordinates, and the page
	 * scrolls under it.
	 */
	function placeMenus(e: Editor) {
		if (slashOpen) slashPos = placeOf(e.view, slashPin) ?? slashPos;
		if (pickerOpen) pickerPos = placeOf(e.view, pickerPin) ?? pickerPos;
		if (aiOpen) aiPos = placeOf(e.view, aiPin) ?? aiPos;
	}

	function updateSuggestion(e: Editor) {
		const sel = e.state.selection;
		const p = e.isEditable && sel.empty ? proposalAt(e.state, sel.head) : null;
		if (!p) {
			suggestion = null;
			suggestionSaid = "";
			return;
		}
		if (suggestion?.id !== p.id) {
			suggestionSaid = proposalSaid(p, {
				accept: shortcuts.format("mod+alt+enter"),
				reject: shortcuts.format("mod+alt+backspace"),
			});
		}
		const at = coordsAt(e.view, sel.head);
		suggestion = { id: p.id, ...at, count: proposalIds(e.state).length };
	}

	const proposalContext: ContextHandler = (view, at) => {
		const e = editor;
		if (!e || !e.isEditable) return false;
		const p = proposalAt(view.state, at.pos);
		if (!p) return false;
		contextMenu.show({ x: at.x, y: at.y }, [
			{ id: "accept", label: "Accept", icon: "ri:check-line", action: () => void acceptProposal(e, p.id) },
			{ id: "reject", label: "Reject", icon: "ri:close-line", action: () => void rejectProposal(e, p.id) },
		]);
		return true;
	};

	const tableContext: ContextHandler = (view, at) => {
		const e = editor;
		if (!e || !e.isEditable || !at.target.closest("td, th")) return false;
		const selection = selectionForCellMenu(view.state, at.pos);
		if (!selection.eq(view.state.selection)) view.dispatch(view.state.tr.setSelection(selection));
		const actions = tableActions(e);
		if (!actions.length) return false;
		contextMenu.show({ x: at.x, y: at.y }, tableMenuItems(actions));
		return true;
	};

	// ── Link previews ────────────────────────────────────────────────────
	function showPreview(anchor: HTMLElement, href: string, label: string) {
		if (hoverHide) clearTimeout(hoverHide);
		hover = { anchor, href, label };
	}

	function hidePreview() {
		if (hoverHide) clearTimeout(hoverHide);
		hoverHide = setTimeout(() => (hover = null), 160);
	}

	// ── The page's keys ──────────────────────────────────────────────────
	const pageKeys = Extension.create({
		name: "pageKeys",
		addKeyboardShortcuts() {
			return {
				"Mod-k": ({ editor: e }) => {
					const c = coordsAt(e.view, e.state.selection.from);
					return editLinkAtSelection(e, c);
				},
				// The table's bar, from the keyboard (as Alt-F10 reaches a toolbar elsewhere).
				"Alt-F10": ({ editor: e }) => {
					if (!cellAt(e.state) || !tableMenu) return false;
					tableMenu.focus();
					return true;
				},
				"Mod-j": () => {
					if (!ai) return false;
					openAiAtCaret();
					return true;
				},
				"Mod-Shift-f": () => {
					pageDisplay.toggleFocus();
					return true;
				},
				Escape: () => {
					if (!ai?.isActive()) return false;
					if (editor) ai.abortIn(editor);
					return true;
				},
			};
		},
	});

	// ── Lifecycle ────────────────────────────────────────────────────────
	let cleanup: (() => void) | null = null;

	onMount(() => {
		const e = createPageEditor({
			element: host,
			doc,
			editable,
			placeholder,
			label: pageTitle,
			spellcheck: pageDisplay.spellcheck,
			plugins: [
				triggers([
					{
						char: "/",
						onOpen: (coords, from) => {
							slashFrom = from;
							slashPos = coords;
							slashPin = editor ? pinAt(editor.view, from, coords) : null;
							appletList = null;
							slashOpen = true;
						},
						onQuery: (q) => (slashQuery = q),
						onClose: () => {
							slashQuery = "";
							if (!appletList) slashOpen = false;
						},
					},
					{
						char: "@",
						onOpen: (coords, from) => {
							pickerRange = editor ? anchorRange(editor.state, from, from + 1) : null;
							pickerQuery = editor ? triggerState(editor.state, "@").query : "";
							pickerPos = coords;
							pickerPin = editor ? pinAt(editor.view, from, coords) : null;
							pickerOpen = true;
						},
						onQuery: () => {},
						onClose: () => {},
					},
				]),
				find({
					onOpen: (selected) => {
						findInitial = selected;
						// Reopening with a new selection remounts the bar.
						findOpen = false;
						queueMicrotask(() => (findOpen = true));
					},
					onClose: () => (findOpen = false),
				}),
				focusMode(pageDisplay.focusMode),
				codeHighlight(),
				pasteHandling(),
				mediaUploads(),
				links({ onHover: showPreview, onLeave: hidePreview }),
				aiPresence(),
				contextGesture([linkContext(() => editor), proposalContext, tableContext]),
				proposalKeys(),
				pageKeys,
			],
		});
		editor = e;

		// The editor can dispatch while Svelte is rendering (a menu taking
		// focus blurs it), and state must not change mid-render: the menus
		// follow the editor a microtask later, once per burst of transactions.
		let followQueued = false;
		const follow = () => {
			if (followQueued) return;
			followQueued = true;
			queueMicrotask(() => {
				followQueued = false;
				if (e.isDestroyed) return;
				revision++;
				updateToolbar(e);
				updateTable(e);
				updateSuggestion(e);
			});
		};
		toolbarTimer = toolbarTiming(e.view, follow);
		e.on("transaction", ({ transaction }) => {
			if (transaction.selectionSet) selectionMovedAt = performance.now();
			toolbarTimer?.note(transaction);
			if (transaction.docChanged) scheduleReport(!isChangeOrigin(transaction));
			follow();
		});
		e.on("focus", follow);
		// A click on a menu over the page keeps focus (they cancel mousedown);
		// anything else that takes it hides the menus that need it.
		e.on("blur", follow);

		// The person typing over the inline writer stops it.
		const onInput = () => {
			if (ai?.isActive()) ai.abortIn(e);
		};
		e.view.dom.addEventListener("beforeinput", onInput);

		// Everything over the page follows it when it scrolls: the table's
		// bar, the toolbar, Accept and Reject, and the menus opened at a place.
		// The document's own scroller reports its scrolling on the window.
		pageScroller = scrollerOf(frame);
		const scroller: HTMLElement | Window = pageScroller === document.scrollingElement ? window : pageScroller;
		const onScroll = () => {
			updateTable(e);
			updateSuggestion(e);
			if (toolbarOpen) updateToolbar(e);
			placeMenus(e);
		};
		scroller.addEventListener("scroll", onScroll, { passive: true });

		// ⌘K is the app's (Ask or search) everywhere but here, where it links
		// the selection or edits the link at the caret, when there is one.
		const releaseLinkKey = shortcuts.claim(e.view.dom, "mod+k", () => canEditLinkAtSelection(e.state));

		cleanup = () => {
			e.view.dom.removeEventListener("beforeinput", onInput);
			scroller.removeEventListener("scroll", onScroll);
			releaseLinkKey();
			toolbarTimer?.destroy();
			toolbarTimer = null;
		};

		registerTreeEditor(pageId, e);
		onEditorReady?.(e);
		onOutlineNav?.(treeOutlineNav(e));
		onDocChange?.(treeStats(e.state.doc), false);
		onOutline?.(treeHeadings(e.state.doc));
	});

	$effect(() => {
		const on = pageDisplay.focusMode;
		if (editor && !editor.isDestroyed) setFocusMode(editor.view, on);
	});

	// Editable or read only, spelling checked or not, and named for its page:
	// the editor's attributes say each to assistive technology too.
	$effect(() => {
		const state = { editable, spellcheck: pageDisplay.spellcheck, label: pageTitle };
		if (editor && !editor.isDestroyed) setPageEditorState(editor, state);
	});

	onDestroy(() => {
		const e = editor;
		if (changeTimer) clearTimeout(changeTimer);
		if (hoverHide) clearTimeout(hoverHide);
		cleanup?.();
		if (e) {
			ai?.abortIn(e);
			unregisterTreeEditor(pageId, e);
			onEditorReady?.(null);
			onOutlineNav?.(null);
			e.destroy();
		}
		editor = null;
	});
</script>

<input bind:this={fileInput} type="file" accept={fileAccept} multiple hidden onchange={handleFiles} />

<div
	bind:this={frame}
	class="document-editor"
	class:doc-page-view={pageDisplay.widthMode === "page"}
	style:--editor-font-family={pageDisplay.fontFamily}
	style:--editor-font-size={pageDisplay.fontSize}
	style:--editor-line-height={pageDisplay.lineHeight}
>
	{#if findOpen && editor}
		<FindBar editor={editor} initial={findInitial} {revision} onClose={() => (findOpen = false)} />
	{/if}

	<div class="document-editor-host" bind:this={host}></div>
	<div class="sr-only" aria-live="polite">{suggestionSaid}</div>

	{#if editor}
		{#if pickerOpen}
			<RefPicker position={pickerPos} initialQuery={pickerQuery} onSelect={handleEntity} onClose={handlePickerClose} />
		{/if}

		{#if slashOpen}
			<SlashMenu {commands} position={slashPos} onSelect={handleSlashSelect} onClose={handleSlashClose} />
		{/if}

		{#if toolbarOpen && !slashOpen && !pickerOpen}
			<SelectionToolbar
				position={toolbarPos}
				activeMarks={toolbarMarks}
				marks={!toolbarInCode}
				onFormat={handleFormat}
				onAskAi={ai
					? () => {
							toolbarOpen = false;
							if (editor) openAi(toolbarPos, "rewrite", editor.state.selection.from);
						}
					: undefined}
				onClose={handleToolbarClose}
			/>
		{/if}

		{#if aiOpen}
			<AiPromptPopover position={aiPos} intent={aiIntent} onSubmit={handleAiSubmit} onClose={handleAiClose} />
		{/if}

		{#if tableAnchor && !toolbarOpen}
			<TableMenu
				bind:this={tableMenu}
				editor={editor}
				anchor={tableAnchor}
				{revision}
				onFocusChange={(inside) => {
					tableMenuFocused = inside;
					if (editor) updateTable(editor);
				}}
			/>
		{/if}

		{#if suggestion && !toolbarOpen}
			{@const e = editor}
			{@const id = suggestion.id}
			<SuggestionBar
				position={suggestion}
				count={suggestion.count}
				onAccept={() => acceptProposal(e, id)}
				onReject={() => rejectProposal(e, id)}
				onAcceptAll={() => acceptAllProposals(e)}
				onRejectAll={() => rejectAllProposals(e)}
			/>
		{/if}

		{#if hover}
			<RefPreview
				anchor={hover.anchor}
				type={previewType(hover.href)}
				label={hover.label}
				url={hover.href}
				onOpen={() => {
					const h = hover;
					hover = null;
					if (h) openLinkBeside(h.href, h.label);
				}}
				oncardenter={() => hoverHide && clearTimeout(hoverHide)}
				oncardleave={hidePreview}
			/>
		{/if}
	{/if}
</div>

<style>
	.document-editor {
		position: relative;
	}

	.document-editor-host {
		position: relative;
	}
</style>
