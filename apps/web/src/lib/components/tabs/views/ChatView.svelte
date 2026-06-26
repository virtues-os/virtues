<script lang="ts">
	import type { Tab } from "$lib/tabs/types";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import ChatInput from "$lib/components/ChatInput.svelte";
	import MediaLightbox from "$lib/components/MediaLightbox.svelte";
	import {
		getSelectedModel,
		getDefaultModel,
		initializeSelectedModel,
		getInitializationPromise,
	} from "$lib/stores/models.svelte";
	import CitedMarkdown from "$lib/components/CitedMarkdown.svelte";
	import StoppedNotice from "$lib/components/StoppedNotice.svelte";
	import Icon from "$lib/components/Icon.svelte";
	import SelectionPopover from "$lib/components/SelectionPopover.svelte";
	import { fetchModels, type ModelOption } from "$lib/config/models";
	import { normalizeImage } from "$lib/multimodal/normalizeImage";
	import { CitationPanel } from "$lib/components/citations";
	import { buildCitationContextFromParts } from "$lib/citations";
	import type { Citation } from "$lib/types/Citation";
	import UserMessage from "$lib/components/UserMessage.svelte";
	import ThinkingBlock from "$lib/components/ThinkingBlock.svelte";
	import SubagentPanel from "$lib/components/SubagentPanel.svelte";
	import { onMount, onDestroy, tick } from "svelte";
	import { chatSessions } from "$lib/stores/chatSessions.svelte";
	import { chatInstances } from "$lib/stores/chatInstances.svelte";
	import { spaceStore } from "$lib/stores/space.svelte";
	import ChatSpaceBreadcrumb from "$lib/components/chat/ChatSpaceBreadcrumb.svelte";
	import type { Chat } from "@ai-sdk/svelte";
	// Active page editing imports
	import { editAllowListStore, type EditableResourceType } from "$lib/stores/editAllowList.svelte";
	import PageBindingInline from "$lib/components/chat/PageBindingInline.svelte";
	import PageEditResult from "$lib/components/chat/PageEditResult.svelte";
	import EditDiffCard from "$lib/components/chat/EditDiffCard.svelte";
	import CodeInterpreterCard from "$lib/components/chat/CodeInterpreterCard.svelte";
	import CompactionCheckpoint from "$lib/components/chat/CompactionCheckpoint.svelte";
	import ContextViewPanel from "$lib/components/chat/ContextViewPanel.svelte";
	import { ChatError } from "$lib/components/chat";
	import { createYjsDocument } from "$lib/yjs";
	import type { EntityResult } from "$lib/components/EntityPicker.svelte";
	import type { AgentModeId } from "$lib/config/agentModes";

	// Generate a random 16-char hex ID (matches backend format)
	function generateHex16(): string {
		const bytes = new Uint8Array(8);
		crypto.getRandomValues(bytes);
		return Array.from(bytes).map(b => b.toString(16).padStart(2, '0')).join('');
	}

	// Type for tool result parts in messages
	interface ToolResultPart {
		type: string;
		state?: string;
		toolCallId?: string;
		output?: {
			page_id?: string;
			title?: string;
		};
	}

	// Props
	let { tab, active }: { tab: Tab; active: boolean } = $props();

	// Extract conversationId from tab route (format: /chat/chat_abc123 or / for new chat)
	// Returns the full chat ID including 'chat_' prefix, or undefined for new chat
	// Strips query params like ?view=context
	// svelte-ignore state_referenced_locally
	function extractConversationId(route: string): string | undefined {
		// Strip query params first
		const pathOnly = route.split('?')[0];
		if (pathOnly === '/' || pathOnly === '/chat') return undefined;
		// Route format: /chat/chat_abc123 → extract chat_abc123 (full ID)
		const match = pathOnly.match(/^\/chat\/(chat_[^/]+)$/);
		return match?.[1];
	}

	// Check if route has ?view=context query param
	function isContextViewRoute(route: string): boolean {
		return route.includes('?view=context');
	}

	// Derived: are we showing the context panel?
	const isContextView = $derived(isContextViewRoute(tab.route));

	// Check if route represents a new/unsaved chat
	function isNewChat(route: string): boolean {
		const pathOnly = route.split('?')[0];
		return pathOnly === '/' || pathOnly === '/chat';
	}

	// Capture initial conversationId from tab prop (intentionally captures initial value only)
	// svelte-ignore state_referenced_locally
	const initialConversationId = extractConversationId(tab.route);
	
	// UI state
	let conversationId = $state(initialConversationId || `chat_${generateHex16()}`);
	let messagesContainer: HTMLDivElement | null = $state(null);
	let scrollContainer: HTMLDivElement | null = $state(null);
	let enableTransitions = $state(false);
	let isLoading = $state(true);
	let isAwaitingResponse = $state(false);
	// Track C: messages typed while the assistant is still streaming are queued
	// and sent automatically when the turn finishes (Cursor-style chips above the
	// composer). Local to the view — a tab drag-away mid-queue is an accepted edge.
	let queuedMessages = $state<string[]>([]);

	// Track D: highlight-to-reference. Select text in a message → comment bar →
	// stage a reference chip above the composer that scopes the next message.
	// Empty note = quote; typed note = quote + comment. Ephemeral. The in-text
	// mark uses the app's own --color-highlight token (one warm marker, not a
	// per-ref rainbow) — references are distinguished by being listed, not colored.
	type StagedRef = {
		id: string;
		messageId: string;
		text: string;
		comment: string;
		range: Range;
	};
	type SelectionDraft = {
		text: string;
		messageId: string;
		rect: { top: number; left: number; bottom: number; width: number };
		range: Range;
	};
	let stagedRefs = $state<StagedRef[]>([]);
	let selectionDraft = $state<SelectionDraft | null>(null);

	function handleWindowMouseup(e: MouseEvent) {
		const sel = window.getSelection();
		const text = sel && !sel.isCollapsed ? sel.toString().trim() : "";
		if (text && sel && sel.rangeCount > 0) {
			const range = sel.getRangeAt(0);
			const node = range.commonAncestorContainer;
			const el = (node.nodeType === 1 ? node : node.parentElement) as HTMLElement | null;
			const wrapper = el?.closest(".message-wrapper") as HTMLElement | null;
			// Only chat messages; ignore selections inside the popover itself.
			if (!wrapper || el?.closest(".vref-bar")) return;
			const rect = range.getBoundingClientRect();
			selectionDraft = {
				text,
				messageId: wrapper.getAttribute("data-message-id") || "",
				rect: { top: rect.top, left: rect.left, bottom: rect.bottom, width: rect.width },
				range: range.cloneRange(),
			};
			return;
		}
		// Collapsed selection = a click → dismiss the popover if clicking outside it.
		if (selectionDraft && !(e.target as HTMLElement)?.closest(".vref-bar")) {
			selectionDraft = null;
		}
	}

	function addStagedRef(comment: string) {
		if (!selectionDraft) return;
		const d = selectionDraft;
		stagedRefs = [
			...stagedRefs,
			{
				id: crypto?.randomUUID?.() ?? `ref-${stagedRefs.length}-${d.text.length}`,
				messageId: d.messageId,
				text: d.text,
				comment,
				range: d.range,
			},
		];
		selectionDraft = null;
		window.getSelection()?.removeAllRanges();
		repaintHighlights();
	}

	function removeStagedRef(id: string) {
		stagedRefs = stagedRefs.filter((r) => r.id !== id);
		repaintHighlights();
	}

	function clearStagedRefs() {
		stagedRefs = [];
		repaintHighlights();
	}

	function copySelectionDraft() {
		if (selectionDraft) navigator.clipboard?.writeText(selectionDraft.text).catch(() => {});
	}

	// Paint staged refs + the pending selection with the CSS Custom Highlight API
	// under one name — no DOM mutation, no reflow, themed via --color-highlight.
	// Painting the pending range keeps the highlight visible after the comment
	// field steals focus (which collapses the native browser selection).
	function repaintHighlights() {
		const cssAny = CSS as any;
		if (typeof CSS === "undefined" || !cssAny.highlights || typeof (window as any).Highlight === "undefined") return;
		const ranges = stagedRefs.map((r) => r.range);
		if (selectionDraft) ranges.push(selectionDraft.range);
		if (ranges.length === 0) {
			cssAny.highlights.delete("vref");
			return;
		}
		try {
			cssAny.highlights.set("vref", new (window as any).Highlight(...ranges));
		} catch {
			/* range invalidated by a re-render — drop silently */
		}
	}

	// Repaint whenever the staged set or the pending selection changes.
	$effect(() => {
		void stagedRefs;
		void selectionDraft;
		repaintHighlights();
	});

	function serializeRef(r: StagedRef): string {
		const quote = `> ${r.text.replace(/\s*\n\s*/g, " ")}`;
		return r.comment ? `${quote}\n\n${r.comment}` : quote;
	}

	// Track E1: multimodal attachments. Files are read to base64 data URLs (so they
	// round-trip to the provider and render on reload) and sent as AI SDK file parts.
	type Attachment = {
		id: string;
		mediaType: string;
		url: string; // data URL
		filename: string;
		size: number;
		kind: "image" | "pdf" | "audio" | "text";
		width?: number;
		height?: number;
	};

	function formatFileSize(bytes: number): string {
		if (bytes < 1024) return `${bytes} B`;
		if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
		return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
	}
	let attachments = $state<Attachment[]>([]);
	let dragActive = $state(false);

	// Click an in-message image to open it in a shared-element lightbox.
	let lightbox = $state<{ src: string; alt: string; rect: DOMRect } | null>(null);
	function openLightbox(e: MouseEvent, src: string, alt: string) {
		const el = e.currentTarget as HTMLImageElement;
		lightbox = { src, alt, rect: el.getBoundingClientRect() };
	}
	let availableModels = $state<ModelOption[]>([]);

	onMount(() => {
		fetchModels()
			.then((m) => (availableModels = m))
			.catch(() => {});
	});

	// Text/code/doc extensions — MIME is unreliable for these, so check the name too.
	const TEXT_EXT =
		/\.(md|markdown|txt|text|csv|tsv|json|html?|xml|ya?ml|toml|ini|env|log|ts|tsx|js|jsx|mjs|cjs|py|rb|rs|go|java|c|h|cpp|cc|cs|php|swift|kt|sh|bash|zsh|sql|css|scss)$/i;

	function attachmentKind(file: File): Attachment["kind"] | null {
		const mt = (file.type || "").toLowerCase();
		if (mt.startsWith("image/")) return "image";
		if (mt === "application/pdf") return "pdf";
		if (mt.startsWith("audio/")) return "audio";
		if (mt.startsWith("text/") || mt === "application/json" || TEXT_EXT.test(file.name))
			return "text";
		return null;
	}

	function readAsDataURL(file: File): Promise<string> {
		return new Promise((resolve, reject) => {
			const r = new FileReader();
			r.onload = () => resolve(r.result as string);
			r.onerror = () => reject(r.error);
			r.readAsDataURL(file);
		});
	}

	function readAsText(file: File): Promise<string> {
		return new Promise((resolve, reject) => {
			const r = new FileReader();
			r.onload = () => resolve(r.result as string);
			r.onerror = () => reject(r.error);
			r.readAsText(file);
		});
	}

	function base64Utf8(s: string): string {
		const bytes = new TextEncoder().encode(s);
		let bin = "";
		for (let i = 0; i < bytes.length; i++) bin += String.fromCharCode(bytes[i]);
		return btoa(bin);
	}

	async function addFiles(files: File[]) {
		const MAX = 100 * 1024 * 1024; // 100 MB, matches the media backend cap
		const MAX_TEXT = 100 * 1024; // inline-text cap (~25k tokens) before truncating
		for (const file of files) {
			const kind = attachmentKind(file);
			if (!kind || file.size > MAX) continue;
			try {
				let mediaType = file.type || "application/octet-stream";
				let url: string;
				let width: number | undefined;
				let height: number | undefined;

				if (kind === "image") {
					const norm = await normalizeImage(file);
					url = norm.dataUrl;
					mediaType = norm.mediaType;
					width = norm.width || undefined;
					height = norm.height || undefined;
				} else if (kind === "text") {
					let text = await readAsText(file);
					if (text.length > MAX_TEXT) text = text.slice(0, MAX_TEXT) + "\n…[truncated]";
					mediaType = "text/plain";
					url = `data:text/plain;base64,${base64Utf8(text)}`;
				} else {
					url = await readAsDataURL(file);
				}

				attachments = [
					...attachments,
					{
						id: crypto?.randomUUID?.() ?? `att-${attachments.length}-${file.size}`,
						mediaType,
						url,
						filename: file.name,
						size: file.size,
						kind,
						width,
						height,
					},
				];
			} catch {
				/* unreadable / undecodable file — skip */
			}
		}
	}

	function removeAttachment(id: string) {
		attachments = attachments.filter((a) => a.id !== id);
	}

	// Capability gate: does the active model support every attached modality? If not,
	// surface a switch to a model that does (or note none is available).
	const capabilityIssue = $derived.by(() => {
		if (attachments.length === 0) return null;
		const model =
			availableModels.find((m) => m.id === selectedModelValue?.id) ??
			selectedModelValue ??
			null;
		const needs = {
			image: attachments.some((a) => a.kind === "image"),
			pdf: attachments.some((a) => a.kind === "pdf"),
			audio: attachments.some((a) => a.kind === "audio"),
		};
		const lacks: string[] = [];
		if (needs.image && !model?.supportsVision) lacks.push("images");
		if (needs.pdf && !model?.supportsPdf) lacks.push("PDFs");
		if (needs.audio && !model?.supportsAudio) lacks.push("audio");
		if (lacks.length === 0) return null;
		const candidate =
			availableModels.find(
				(m) =>
					(!needs.image || m.supportsVision) &&
					(!needs.pdf || m.supportsPdf) &&
					(!needs.audio || m.supportsAudio),
			) ?? null;
		return { lacks, modelName: model?.displayName ?? "This model", candidate };
	});

	function switchToCapableModel() {
		const candidate = capabilityIssue?.candidate;
		if (candidate) selectedModelValue = candidate;
	}
	let loadedMessages = $state<any[]>([]);

	// Track tab route to reset state when switching conversations
	// svelte-ignore state_referenced_locally
	let previousTabRoute = $state<string>(tab.route);
	let preferredName = $state<string | undefined>(undefined);
	let onboardingStatus = $state<string>('active');
	let onboardingStarted = false; // guard to prevent double-trigger

	// AbortController for cancelling in-flight requests on tab switch
	let tabSwitchAbortController: AbortController | null = null;

	// UI preferences from assistant profile
	let uiPreferences = $state<{
		contextIndicator?: {
			alwaysVisible?: boolean;
			showThreshold?: number;
		};
	}>({});

	// Keep a map of message metadata (agentId, provider, etc.) for rendering
	let messageMetadata = $state<
		Map<string, { agentId?: string; provider?: string; stopped?: boolean }>
	>(new Map());

	// Citation panel state
	let citationPanelOpen = $state(false);
	let selectedCitation = $state<Citation | null>(null);

	// The Space (room) this chat lives in — at most one. Its id is sent with each
	// message (drives the agent's active-space context + server-side binding), and
	// the breadcrumb at the top lets the user enter / file / create a room.
	let chatSpaceId = $state<string | null>(null);
	// Which conversation chatSpaceId was seeded for. Seeding happens ONCE per
	// conversation (when its session row is available, or once the session list
	// has finished loading and confirms there's no row yet) so a later session
	// refresh can never clobber a room the user just picked locally.
	let seededSpaceFor = $state<string | null>(null);

	$effect(() => {
		const id = conversationId;
		if (seededSpaceFor === id) return;
		const session = chatSessions.sessions.find((s) => s.conversation_id === id);
		if (session) {
			chatSpaceId = session.space_id ?? null;
			seededSpaceFor = id;
		} else if (!chatSessions.isLoading) {
			// Sessions are loaded and this chat has no row yet (brand-new, not yet
			// persisted) — start unfiled; the create path binds it from the first
			// message's spaceId.
			chatSpaceId = null;
			seededSpaceFor = id;
		}
	});

	async function setChatSpace(spaceId: string | null) {
		chatSpaceId = spaceId; // locally authoritative
		seededSpaceFor = conversationId; // don't let a later seed override this pick
		// Persist only if the chat already exists server-side; a brand-new chat
		// has no row yet and is bound by the create path from getSpaceId().
		const persisted = chatSessions.sessions.some((s) => s.conversation_id === conversationId);
		if (persisted) {
			await spaceStore.setChatSpace(conversationId, spaceId);
		}
	}

	// Open citation panel with selected citation
	function openCitationPanel(citation: Citation) {
		selectedCitation = citation;
		citationPanelOpen = true;
	}

	// Close citation panel
	function closeCitationPanel() {
		citationPanelOpen = false;
		selectedCitation = null;
	}

	// Helper to get the first bound page from the edit allow list
	function getBoundPage() {
		return editAllowListStore.items.find((i) => i.type === 'page');
	}

	function handlePageClear() {
		const pages = editAllowListStore.items.filter((i) => i.type === 'page');
		for (const page of pages) {
			editAllowListStore.remove('page', page.id);
		}
	}

	function handleRemoveItem(type: string, id: string) {
		editAllowListStore.remove(type as EditableResourceType, id);
	}

	function handlePageSelect(pageId: string, pageTitle: string) {
		// Create Yjs document for the page and bind
		// NOTE: No auto-open - user can open the page manually if they want to see it
		handlePageClear();
		const yjsDoc = createYjsDocument(pageId);
		editAllowListStore.addPage(pageId, pageTitle, yjsDoc);
	}

	/**
	 * Handle permission allow for AI edit.
	 * Adds permission then regenerates the AI's last response (which had permission_needed).
	 * regenerate() removes that assistant message and re-requests — no duplicate user messages.
	 */
	async function handlePermissionAllow(entityId: string, entityType: string, title: string) {
		// Add to allow list (await ensures backend has the permission before retry)
		if (entityType === 'page') {
			const yjsDoc = createYjsDocument(entityId);
			await editAllowListStore.addPage(entityId, title, yjsDoc);
		} else {
			// folder / action / wiki_entry — no Yjs doc, granted generically by (type, id)
			await editAllowListStore.add({
				type: entityType as EditableResourceType,
				id: entityId,
				title
			});
		}

		// Regenerate = remove last assistant message + re-request
		if (chat.status === 'ready') {
			try {
				await chat.regenerate();
			} catch (error) {
				console.error('[ChatView] Failed to regenerate after permission grant:', error);
			}
		}
	}

	/**
	 * Handle permission deny for AI edit
	 */
	function handlePermissionDeny() {
		// User denied permission - no action needed
		// The tool result already shows the permission was needed
	}



	function handleSelectEntities(entities: EntityResult[]) {
		// Add each entity to the edit allow list
		for (const entity of entities) {
			// Map entity_type to our EditableResourceType
			const type = entity.entity_type === 'page' ? 'page' :
			             entity.entity_type === 'folder' ? 'folder' : 'page';

			// For pages, create Yjs document for real-time sync
			if (entity.entity_type === 'page') {
				const yjsDoc = createYjsDocument(entity.id);
				editAllowListStore.addPage(entity.id, entity.name, yjsDoc);
			} else {
				editAllowListStore.add({
					type: type as 'page' | 'folder' | 'wiki_entry',
					id: entity.id,
					title: entity.name
				});
			}
		}
	}

	// Track tool calls that were already complete when we mounted (loaded from history)
	// Only auto-open pages created AFTER mount (during streaming)
	let initialCompletedToolCalls: Set<string> | null = null;
	let initialLoadComplete = false;

	/**
	 * Handle create_page tool result - auto-open the new page
	 * Called from $effect when create_page completes during streaming
	 */
	function handlePageCreated(pageId: string, title: string) {
		// Auto-bind and open the newly created page in split view
		handlePageClear();
		// Don't create Yjs doc here — PageContent will create one when the tab mounts.
		// Creating a second doc causes two WebSocket connections to the same room,
		// which races with the server's Y.Text initialization.
		editAllowListStore.addPage(pageId, title);

		if (!windowShellStore.isSplit) {
			windowShellStore.enableSplit();
		}
		windowShellStore.openTabFromRoute(`/page/${pageId}`, { paneId: 'right' });
	}

	// Effect to handle create_page side effects (auto-open new pages)
	// Only triggers for pages created during this session, not when reopening old chats
	$effect(() => {
		if (!chat?.messages) return;

		// Don't auto-open during initial load - wait until loading is complete
		if (isLoading) return;

		// First run after load: capture already-completed tool calls (loaded from history)
		if (initialCompletedToolCalls === null) {
			initialCompletedToolCalls = new Set();
			for (const message of chat.messages) {
				if (message.role !== 'assistant') continue;
				for (const part of message.parts as ToolResultPart[]) {
					if (part.type === 'tool-create_page' && part.state === 'output-available') {
						initialCompletedToolCalls.add(part.toolCallId);
					}
				}
			}
			initialLoadComplete = true;
			return; // Don't auto-open on first run
		}

		// Only process new pages after initial load is complete
		if (!initialLoadComplete) return;

		// Subsequent runs: only auto-open for NEW completions (not loaded from history)
		for (const message of chat.messages) {
			if (message.role !== 'assistant') continue;

			for (const part of message.parts as ToolResultPart[]) {
				if (part.type === 'tool-create_page' && part.state === 'output-available') {
					const output = part.output;
					if (output?.page_id && !initialCompletedToolCalls.has(part.toolCallId)) {
						handlePageCreated(output.page_id, output.title);
						initialCompletedToolCalls.add(part.toolCallId); // Mark as handled
					}
				}
			}
		}
	});



	// Context usage state
	interface ContextUsageState {
		percentage: number;
		tokens: number;
		window: number;
		status: "healthy" | "warning" | "critical";
	}
	let contextUsage = $state<ContextUsageState | undefined>(undefined);

	// Fetch context usage from API
	async function refreshContextUsage() {
		if (!conversationId || isNewChat(tab.route)) return;

		try {
			const res = await fetch(`/api/chats/${conversationId}/usage`);
			if (!res.ok) return;

			const data = await res.json();
			const status: "healthy" | "warning" | "critical" =
				data.usage_percentage >= 85
					? "critical"
					: data.usage_percentage >= 70
						? "warning"
						: "healthy";

			contextUsage = {
				percentage: data.usage_percentage,
				tokens: data.total_tokens,
				window: data.context_window,
				status,
			};

		} catch {
			// Non-critical, continue without usage data
		}
	}


	// Handle context indicator click - open context tab in split view
	function handleContextClick() {
		const currentPane = windowShellStore.findTabPane(tab.id);
		windowShellStore.openChatContext(conversationId, currentPane);
	}

	// Handle compaction completion from ContextViewPanel - refresh messages
	async function handleCompacted() {
		if (!conversationId) return;
		const messagesRes = await fetch(`/api/chats/${conversationId}`);
		if (messagesRes.ok) {
			const data = await messagesRes.json();
			loadedMessages = data.messages || [];
			chat.messages = deduplicateMessages(loadedMessages).map((msg: any) => ({
				id: msg.id,
				role: msg.role as "user" | "assistant" | "checkpoint",
				parts: convertMessageToParts(msg),
			}));
		}
	}

	// Helper function to convert database messages to Chat parts
	function convertMessageToParts(msg: any) {
		// Carry agent/provider + the user-stopped flag (subject='cancelled') so the
		// "Stopped" notice survives a reload.
		const stopped = msg.subject === "cancelled";
		if (msg.agentId || msg.provider || stopped) {
			messageMetadata.set(msg.id, {
				agentId: msg.agentId,
				provider: msg.provider,
				stopped,
			});
		}

		// If message already has parts array (e.g., checkpoint messages), use it directly
		if (msg.parts && Array.isArray(msg.parts) && msg.parts.length > 0) {
			return msg.parts;
		}

		// Otherwise, construct parts from individual fields (legacy format)
		const parts: any[] = [];

		if (msg.reasoning) {
			parts.push({
				type: "reasoning" as const,
				text: msg.reasoning,
				state: "done" as const,
			});
		}

		if (msg.content) {
			parts.push({
				type: "text" as const,
				text: msg.content,
			});
		}

		if (msg.tool_calls && Array.isArray(msg.tool_calls)) {
			for (const toolCall of msg.tool_calls) {
				parts.push({
					type: `tool-${toolCall.tool_name}` as const,
					toolCallId:
						toolCall.tool_call_id ||
						`${msg.id}_${toolCall.tool_name}_${Date.now()}`,
					toolName: toolCall.tool_name,
					input: toolCall.arguments,
					state: "output-available" as const,
					output: toolCall.result,
				});
			}
		}

		return parts;
	}

	// Helper function to deduplicate messages by ID
	function deduplicateMessages(messages: any[]): any[] {
		if (!messages || messages.length === 0) return [];
		const seen = new Set<string>();
		return messages.filter((msg) => {
			if (seen.has(msg.id)) {
				return false;
			}
			seen.add(msg.id);
			return true;
		});
	}

	// Chat instance - fetched from shared store to survive remounts
	let chat = $state<Chat>(null!);
	let currentChatConversationId = $state<string | null>(null);

	// Getter for current model - used by Chat transport
	function getCurrentModel(): string {
		return selectedModelValue?.id || getDefaultModel()?.id || "";
	}

	// Getter for the chat's Space (room) ID — sent with each message so the agent
	// gets the active-space context block and the server keeps the binding fresh.
	function getSpaceId(): string | null {
		return chatSpaceId;
	}

	// Get or create chat instance for the current conversationId
	function ensureChatInstance() {
		if (currentChatConversationId !== conversationId) {
			// Release old instance if we had one
			if (currentChatConversationId) {
				chatInstances.release(currentChatConversationId);
			}
			// Get or create new instance with model, space, active page, persona, and agent mode getters
			chat = chatInstances.getOrCreate({
				conversationId,
				getModel: getCurrentModel,
				getSpaceId,
				getActivePageContext: () => {
					const page = getBoundPage();
					if (!page) return null;

					// Include the current Yjs content so AI edits match what's in the editor
					const content = page.yjsDoc?.ytext.toString() || '';

					return {
						page_id: page.id,
						page_title: page.title || undefined,
						content: content
					};
				},
				getPersona: () => selectedPersona,
				getAgentMode: () => selectedAgentMode,
			});
			currentChatConversationId = conversationId;
		}
	}

	// Initialize chat on first render
	$effect(() => {
		ensureChatInstance();
	});

	// Watch for tab.route changes to reset state when switching conversations
	$effect(() => {
		const currentTabRoute = tab.route;
		const currentTabConversationId = extractConversationId(currentTabRoute);

		// If the tab's route changed, reset the chat state
		if (currentTabRoute !== previousTabRoute) {
			// IMPORTANT: Skip reset if we're just transitioning from 'new' to a real chat ID
			// This happens after the first message is sent - we're not switching conversations,
			// just updating the tab's route to reflect the persisted chat
			const isSameConversation =
				isNewChat(previousTabRoute) &&
				currentTabConversationId === conversationId;

			if (isSameConversation) {
				// Just update the tracking variable, don't reset state
				previousTabRoute = currentTabRoute;
				return;
			}

			// Cancel any in-flight requests from previous tab
			tabSwitchAbortController?.abort();
			tabSwitchAbortController = new AbortController();
			const signal = tabSwitchAbortController.signal;

			previousTabRoute = currentTabRoute;

			// Generate new conversationId for new chats, or use the extracted conversationId
			const newConversationId =
				currentTabConversationId || `chat_${generateHex16()}`;
			conversationId = newConversationId;

			// Reset chat state
			chat.messages = [];
			loadedMessages = [];
			messageMetadata = new Map();
			contextUsage = undefined;
			titleGenerated = false;
			isAwaitingResponse = false;
			// Reset page create tracking (for auto-open)
			initialCompletedToolCalls = null;
			initialLoadComplete = false;
			// NOTE: We no longer unbind the active page when switching chats.
			// Binding is now additive/persistent to the chat session context.
			// handlePageClear();

			// Load conversation if switching to an existing one
			if (currentTabConversationId && !isNewChat(currentTabRoute)) {
				isLoading = true;
				(async () => {
					try {
						const response = await fetch(
							`/api/chats/${currentTabConversationId}`,
							{ signal },
						);
						if (signal.aborted) return; // Check if we were aborted
						if (response.ok) {
							const data = await response.json();
							if (signal.aborted) return; // Check again after parsing
							loadedMessages = data.messages || [];
							chat.messages = deduplicateMessages(
								loadedMessages,
							).map((msg: any) => ({
								id: msg.id,
								role: msg.role as "user" | "assistant" | "checkpoint",
								parts: convertMessageToParts(msg),
							}));
							if (data.conversation?.model) {
								initializeSelectedModel(
									data.conversation.model,
								);
							}
							await Promise.all([
								refreshContextUsage(),
								editAllowListStore.init(currentTabConversationId),
							]);
						}
					} catch (error) {
						// Ignore abort errors - they're expected when switching tabs
						if (
							error instanceof Error &&
							error.name === "AbortError"
						)
							return;
						console.error(
							"[ChatView] Error loading conversation on tab change:",
							error,
						);
					} finally {
						if (!signal.aborted) {
							isLoading = false;
							// Scroll to bottom after loading existing chat
							setTimeout(() => scrollToBottom("instant"), 10);
						}
					}
				})();
			} else {
				// New chat - set chatId so permissions can sync when granted
				editAllowListStore.setChatId(newConversationId);
				isLoading = false;
			}
		}
	});

	// Load conversation data on mount
	onMount(() => {
		// Load Spaces so the room breadcrumb can resolve name/accent immediately.
		spaceStore.load();
		(async () => {
			// Stage 1: Models must load first (other code depends on model list)
			await getInitializationPromise();

			// Stage 2: Profile fetches + conversation load in parallel (independent)
			const tabConversationId = extractConversationId(tab.route);

			let profileDefaultModelId: string | undefined;
			let profileDefaultPersona: string | undefined;

			const profilePromise = (async () => {
				try {
					const profileResponse = await fetch("/api/assistant-profile");
					if (profileResponse.ok) {
						const profile = await profileResponse.json();
						if (profile.ui_preferences) {
							uiPreferences = profile.ui_preferences;
						}
						profileDefaultModelId = profile.chat_model_id || profile.default_model_id;
						profileDefaultPersona = profile.persona;
					}
				} catch (error) {
					console.error("Failed to load assistant profile:", error);
				}
			})();

			const namePromise = (async () => {
				try {
					const response = await fetch("/api/profile");
					if (response.ok) {
						const profile = await response.json();
						preferredName = profile.preferred_name;
						onboardingStatus = profile.onboarding_status || 'active';
					}
				} catch {
					// Non-critical, continue without preferred name
				}
			})();

			const conversationPromise = tabConversationId ? (async () => {
				try {
					const response = await fetch(`/api/chats/${tabConversationId}`);
					if (response.ok) {
						const data = await response.json();
						loadedMessages = data.messages || [];
						chat.messages = deduplicateMessages(loadedMessages).map(
							(msg: any) => ({
								id: msg.id,
								role: msg.role as "user" | "assistant" | "checkpoint",
								parts: convertMessageToParts(msg),
							}),
						);
						if (data.conversation?.model) {
							initializeSelectedModel(data.conversation.model);
						}
					}
				} catch (error) {
					console.error("[ChatView] Error loading conversation:", error);
				}
			})() : null;

			await Promise.all([profilePromise, namePromise, conversationPromise]);

			// Stage 3: Post-load tasks (depend on conversation being loaded)
			if (tabConversationId) {
				await Promise.all([
					refreshContextUsage(),
					editAllowListStore.init(tabConversationId),
				]);
			} else {
				// New chat - set defaults from profile
				editAllowListStore.setChatId(conversationId);
				initializeSelectedModel(undefined, profileDefaultModelId);
				const defaultModel = getSelectedModel() || getDefaultModel();
				if (defaultModel) {
					selectedModelValue = defaultModel;
				}
				if (profileDefaultPersona) {
					selectedPersona = profileDefaultPersona;
				}
			}

			isLoading = false;
			setTimeout(() => {
				scrollToBottom("instant");
				enableTransitions = true;
			}, 50);

			// Auto-start onboarding for new users with no messages
			// DISABLED for demo — onboarding was repeating the same message
			// if (onboardingStatus === 'new' && loadedMessages.length === 0) {
			// 	setTimeout(() => startOnboarding(), 100);
			// }
		})();

		return () => {
			if (inactivityTimer) clearTimeout(inactivityTimer);
			if (refreshDataTimeout) clearTimeout(refreshDataTimeout);
			tabSwitchAbortController?.abort();
		};
	});

	// Release chat instance on destroy
	onDestroy(() => {
		if (currentChatConversationId) {
			chatInstances.release(currentChatConversationId);
		}
		// Clear any staged highlight ranges from the global CSS highlight registry.
		stagedRefs = [];
		repaintHighlights();
	});

	// Derive thinking state from chat status
	const isThinking = $derived.by(() => {
		const status = chat?.status;
		return status === "submitted" || status === "streaming";
	});

	// Deduplicated messages for rendering
	const uniqueMessages = $derived(chat?.messages ? deduplicateMessages(chat.messages) : []);

	// Get the last assistant message
	const lastAssistantMessage = $derived.by(() => {
		for (let i = uniqueMessages.length - 1; i >= 0; i--) {
			if (uniqueMessages[i].role === "assistant") {
				return uniqueMessages[i];
			}
		}
		return null;
	});

	// Whether the last assistant message has any visible content yet
	// (text, reasoning, or tool calls). Used to keep the optimistic thinking
	// indicator showing until real content takes over.
	const lastAssistantHasVisibleContent = $derived.by(() => {
		if (!lastAssistantMessage) return false;
		return lastAssistantMessage.parts.some((p: any) =>
			(p.type === 'text' && p.text) ||
			(p.type === 'reasoning' && p.text) ||
			p.type?.startsWith('tool-')
		);
	});

	// Track thinking duration
	let thinkingStartTime = $state<number | null>(null);
	let thinkingDuration = $state(0);

	$effect(() => {
		if (isThinking && !thinkingStartTime) {
			thinkingStartTime = Date.now();
		} else if (!isThinking && thinkingStartTime) {
			thinkingDuration = (Date.now() - thinkingStartTime) / 1000;
			thinkingStartTime = null;
		}
	});

	// Local input state
	let input = $state("");
	let inputFocused = $state(false);

	// Auto-focus chat input when new chat tab becomes active
	$effect(() => {
		if (active && isEmpty && !isLoading) {
			// Small delay to ensure DOM is ready
			setTimeout(() => {
				inputFocused = true;
			}, 50);
		}
	});

	// Title generation state
	let titleGenerated = $state(false);
	let inactivityTimer: ReturnType<typeof setTimeout> | null = null;
	let refreshDataTimeout: ReturnType<typeof setTimeout> | null = null;

	// Model selection state - use bindable for ChatInput toolbar
	let selectedModelValue = $state<
		import("$lib/config/models").ModelOption | undefined
	>(undefined);

	// Agent mode and persona selection state - used for tool filtering on backend
	let selectedAgentMode = $state<AgentModeId>('chat');
	let selectedPersona = $state<string>('default');

	// Sync selected model with store (only on initial load)
	$effect(() => {
		const storeModel = getSelectedModel();
		if (storeModel && !selectedModelValue) {
			selectedModelValue = storeModel;
		}
	});

	// Safety timeout
	let thinkingTimeout: ReturnType<typeof setTimeout> | null = null;
	$effect(() => {
		if (isThinking) {
			thinkingTimeout = setTimeout(() => {
				if (chat.status === "error") {
					chat.clearError();
				} else if (
					chat.status === "streaming" ||
					chat.status === "submitted"
				) {
					if (chat.clearError) {
						chat.clearError();
					}
				}
			}, 300000); // 5 minutes to match backend streaming timeout

			return () => {
				if (thinkingTimeout) {
					clearTimeout(thinkingTimeout);
					thinkingTimeout = null;
				}
			};
		} else if (thinkingTimeout) {
			clearTimeout(thinkingTimeout);
			thinkingTimeout = null;
		}
	});

	// Derived state for layout mode
	// Also gate on isLoading to prevent flashing "new chat" while fetching an existing conversation
	let isEmpty = $derived(uniqueMessages.length === 0 && !isLoading);


	// Generate title after first assistant response
	async function generateTitle() {
		if (titleGenerated || chat.messages.length < 2) return;

		try {
			const response = await fetch("/api/chats/title", {
				method: "POST",
				headers: { "Content-Type": "application/json" },
				body: JSON.stringify({
					chatId: conversationId,
					messages: chat.messages.map((m) => ({
						role: m.role,
						content:
							m.parts.find((p) => p.type === "text")?.text || "",
					})),
				}),
			});

			if (response.ok) {
				const data = await response.json();
				titleGenerated = true;
				// Update tab label with the new title
				if (data.title) {
					windowShellStore.updateTab(tab.id, { label: data.title });
				}
			}
		} catch (error) {
			// Title generation is non-critical
		}
	}

	function scrollToBottom(behavior: ScrollBehavior = "smooth") {
		if (scrollContainer) {
			scrollContainer.scrollTo({
				top: scrollContainer.scrollHeight,
				behavior,
			});
		}
	}

	async function handleChatStop() {
		// Stop the client-side stream
		chat.stop();

		// Mark the in-flight assistant message as user-stopped so the "Stopped"
		// notice shows immediately (reload reads the persisted subject='cancelled').
		const stoppedId = lastAssistantMessage?.id;
		if (stoppedId) {
			const existing = messageMetadata.get(stoppedId) ?? {};
			messageMetadata.set(stoppedId, { ...existing, stopped: true });
			// $state(Map) doesn't track .set() — reassign so the chip re-renders live.
			messageMetadata = new Map(messageMetadata);
		}

		// Also notify the backend to cancel the agent loop
		try {
			await fetch('/api/chat/cancel', {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ chatId: conversationId })
			});
		} catch (e) {
			console.error('[ChatView] Failed to cancel chat:', e);
		}
	}

	async function startOnboarding() {
		if (onboardingStarted || !chat) return;
		onboardingStarted = true;

		// Generate a fresh conversationId if the current one is stale (from a deleted DB)
		if (!isNewChat(tab.route)) {
			conversationId = `chat_${generateHex16()}`;
		}

		ensureChatInstance();

		// Use the AI SDK flow: send a greeting to trigger the backend's onboarding detection.
		// The backend sees is_new_user + appends NEW_USER_PROMPT to the system prompt.
		// Tool calls (set_assistant_name, set_user_name) work natively through the SDK.
		try {
			await chat.sendMessage({ text: "👋" });
		} catch (error) {
			console.error('[ChatView] Onboarding error:', error);
		}

		onboardingStatus = 'active';

		// Update tab route to reflect the new chat
		const newRoute = `/chat/${conversationId}`;
		previousTabRoute = newRoute;
		windowShellStore.updateTab(tab.id, { route: newRoute });
		windowShellStore.invalidateViewCache('chat');
	}

	async function handleChatSubmit(value: string) {
		let messageToSend = value.trim();

		// Track D: prepend any staged highlight references as quoted context +
		// comments. Only present on a direct send (cleared before queue-drain).
		if (stagedRefs.length > 0) {
			const refsBlock = stagedRefs.map(serializeRef).join("\n\n");
			messageToSend = refsBlock + (messageToSend ? `\n\n${messageToSend}` : "");
			clearStagedRefs();
		}

		// Track E1: block sending if an attachment isn't supported by the active
		// model — the capability banner prompts a switch instead.
		if (capabilityIssue) return;

		if (!messageToSend && attachments.length === 0) return;

		if (chat.status !== "ready") {
			// Queue text; attachments stay staged and ride along when the drain
			// effect re-sends this once the current turn finishes.
			queuedMessages = [...queuedMessages, messageToSend];
			input = "";
			return;
		}
		input = "";

		// Capture + clear attachments as AI SDK file parts.
		const files = attachments.map((a) => ({
			type: "file" as const,
			mediaType: a.mediaType,
			url: a.url,
			filename: a.filename,
		}));
		attachments = [];

		// New turn → clear any leftover Deep Research panel from the previous turn.
		chatInstances.clearSubagents(conversationId);

		// Optimistic: show thinking indicator immediately (before network round-trip)
		isAwaitingResponse = true;
		await tick(); // Flush DOM so the indicator renders before the network call

		// Auto-scroll to bottom on submit
		scrollToBottom("smooth");

		try {
			// Sync permissions to backend BEFORE sending (so AI tool calls have them during streaming)
			// add_permission endpoint handles chat creation via INSERT OR IGNORE INTO chats
			if (isNewChat(tab.route) && editAllowListStore.hasItems) {
				await editAllowListStore.markChatCreated();
			}

			await chat.sendMessage(
				files.length > 0 ? { text: messageToSend, files } : { text: messageToSend },
			);

			if (chat.messages.length === 2) {
				await generateTitle();
				// Update tab route if it's a new chat
				if (isNewChat(tab.route)) {
					// Update previousTabRoute first to prevent the tab-switch effect
					// from treating this as a tab change and resetting state
					const newRoute = `/chat/${conversationId}`;
					previousTabRoute = newRoute;
					console.log(
						"[ChatView] Updating tab with route:",
						{
							tabId: tab.id,
							conversationId,
							newRoute,
						},
					);
					windowShellStore.updateTab(tab.id, {
						route: newRoute,
					});
					// Ensure chat is marked as created (may already be done above if hasItems)
					await editAllowListStore.markChatCreated();
					// Invalidate the Chats view cache so it refreshes with the new chat
					windowShellStore.invalidateViewCache('chat');
				}
				await chatSessions.refresh();
			}

			if (refreshDataTimeout) {
				clearTimeout(refreshDataTimeout as any);
			}
			refreshDataTimeout = setTimeout(() => {
				refreshContextUsage();
				refreshDataTimeout = null;
			}, 2000);
		} catch (error) {
			console.error("[handleChatSubmit] Error:", error);
			input = "";
		} finally {
			isAwaitingResponse = false;
		}
	}

	// Track C: drain the queue when the assistant goes idle.
	$effect(() => {
		if (
			chat.status === "ready" &&
			queuedMessages.length > 0 &&
			!isAwaitingResponse
		) {
			const [next, ...rest] = queuedMessages;
			queuedMessages = rest;
			handleChatSubmit(next);
		}
	});

	function removeQueued(index: number) {
		queuedMessages = queuedMessages.filter((_, i) => i !== index);
	}
</script>

<svelte:window onmouseup={handleWindowMouseup} />

{#if selectionDraft}
	<SelectionPopover
		rect={selectionDraft.rect}
		onAdd={addStagedRef}
		onCopy={copySelectionDraft}
		onClose={() => (selectionDraft = null)}
	/>
{/if}

{#if lightbox}
	<MediaLightbox
		src={lightbox.src}
		alt={lightbox.alt}
		originRect={lightbox.rect}
		onClose={() => (lightbox = null)}
	/>
{/if}

{#if !chat}
	<!-- wait for chat to initialize -->
{:else if isContextView}
	<ContextViewPanel {conversationId} {active} onCompacted={handleCompacted} />
{:else}
	<div
		class="chat-root"
		role="presentation"
		ondragover={(e) => {
			if (e.dataTransfer?.types.includes("Files")) {
				e.preventDefault();
				dragActive = true;
			}
		}}
		ondragleave={(e) => {
			// Only clear when leaving the root, not when crossing child boundaries.
			if (!e.relatedTarget || !(e.currentTarget as HTMLElement).contains(e.relatedTarget as Node)) {
				dragActive = false;
			}
		}}
		ondrop={(e) => {
			e.preventDefault();
			dragActive = false;
			if (e.dataTransfer?.files?.length) addFiles(Array.from(e.dataTransfer.files));
		}}
	>
		{#snippet renderFilePart(part: any, compact = false)}
			{@const mt = part.mediaType || ""}
			{#if mt.startsWith("image/")}
				<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_noninteractive_element_interactions -->
				<img
					src={part.url}
					alt={part.filename || "image"}
					class="msg-image"
					class:compact-img={compact}
					onclick={(e) => openLightbox(e, part.url, part.filename || "image")}
				/>
			{:else if mt.startsWith("audio/")}
				<audio src={part.url} controls class="msg-audio"></audio>
			{:else}
				<a class="msg-file" href={part.url} download={part.filename || "file"}>
					<Icon icon={mt === "application/pdf" ? "ri:file-pdf-fill" : "ri:file-text-line"} width="16" />
					<span>{part.filename || "Document"}</span>
				</a>
			{/if}
		{/snippet}

		<div class="chat-container">
			<!-- Main chat area -->
			<div class="chat-area">
				<!-- Room breadcrumb — the Space this chat lives in (top chrome) -->
				<div class="chat-topbar">
					<ChatSpaceBreadcrumb spaceId={chatSpaceId} onChange={setChatSpace} />
				</div>
				<div class="page-container" class:is-empty={isEmpty}>
					<!-- Messages area -->
					<div
						bind:this={scrollContainer}
						class="flex-1 overflow-y-auto chat-layout"
						class:visible={!isEmpty}
					>
						<div class="messages-container">
							{#each uniqueMessages as message, messageIndex (message.id)}
								{@const isUserMessage = message.role === "user"}
								{@const exchangeIndex = isUserMessage
									? uniqueMessages
											.slice(0, messageIndex)
											.filter((m) => m.role === "user")
											.length
									: -1}
								<div
									class="flex justify-start"
									id={isUserMessage
										? `exchange-${exchangeIndex}`
										: undefined}
								>
									<div
										class="message-wrapper"
										class:user-has-attachment={isUserMessage &&
											message.parts.some((p: any) => p.type === "file")}
										data-message-id={message.id}
										data-role={message.role}
										data-agent-id={messageMetadata.get(
											message.id,
										)?.agentId || "general"}
										data-loading={message.role ===
											"assistant" &&
											!message.parts.some(
												(p) =>
													p.type === "text" && p.text,
											)}
									>
										{#if message.role === "checkpoint"}
											<!-- Compaction checkpoint message -->
											{@const checkpointPart = message.parts.find((p: any) => p.type === "checkpoint")}
											{#if checkpointPart}
												<CompactionCheckpoint
													version={(checkpointPart as any).version}
													messagesSummarized={(checkpointPart as any).messagesSummarized || (checkpointPart as any).messages_summarized}
													summary={(checkpointPart as any).summary}
													timestamp={(checkpointPart as any).timestamp}
												/>
											{/if}
										{:else if message.role === "assistant"}
											{@const citationContext =
												buildCitationContextFromParts(
													message.parts,
												)}
											{@const isLastMessage =
												message.id ===
												uniqueMessages[
													uniqueMessages.length - 1
												]?.id}
											{@const isStreaming =
												(chat.status === "streaming" || chat.status === "submitted") &&
												isLastMessage}
											{@const messageReasoningParts =
												message.parts.filter(
													(p: any) =>
														p.type === "reasoning",
												)}
											{@const messageToolParts =
												message.parts.filter((p: any) =>
													p.type.startsWith("tool-"),
												)}
											{@const messageReasoning =
												messageReasoningParts
													.map(
														(p: any) =>
															p.text || "",
													)
													.filter(Boolean)
													.join("\n")}
											{@const hasThinkingContent =
												messageReasoning ||
												messageToolParts.length > 0}

											{@const subagents =
												isLastMessage
													? chatInstances.getSubagents(
															conversationId,
														)
													: []}
											{#if subagents.length > 0}
												<SubagentPanel
													{subagents}
													variant={selectedAgentMode === 'council' ? 'voice' : 'research'}
												/>
											{/if}

											{#if hasThinkingContent || (isStreaming && isLastMessage)}
												<ThinkingBlock
													isThinking={isStreaming &&
														isLastMessage &&
														chat.status ===
															"streaming"}
													toolCalls={messageToolParts}
													reasoningContent={messageReasoning}
													{isStreaming}
													duration={isLastMessage
														? thinkingDuration
														: 0}
												/>
											{/if}

											{#each message.parts as part, partIndex (part.type === "text" ? `text-${partIndex}` : (part as any).toolCallId || `part-${partIndex}`)}
												{#if part.type === "text" && part.text.trim()}
													<div
														class="text-base text-foreground assistant-response"
													>
														<CitedMarkdown
															content={part.text}
															{isStreaming}
															citations={citationContext}
															onCitationClick={openCitationPanel}
														/>
													</div>
											{:else if part.type === "file"}
												{@render renderFilePart(part as any)}
											{:else if part.type.startsWith("tool-") && (part as any).state === "output-available" && (part as any).output?.permission_needed}
												<!-- Any gated tool (run_action, delete_action, …) awaiting the user's "I allow" -->
												{@const output = (part as any).output}
												<PageBindingInline
													entityId={output.entity_id}
													entityType={output.entity_type}
													entityTitle={output.entity_title}
													message={output.message}
													permissionMode={true}
													onAllow={(id, type, title) => handlePermissionAllow(id, type, title)}
													onDeny={() => handlePermissionDeny()}
												/>
											{:else if part.type === "tool-create_page" && (part as any).state === "output-available"}
												{@const output = (part as any).output}
												{#if output?.page_id}
													<PageEditResult
														type="page_created"
														title={output.title}
														pageId={output.page_id}
														onOpenPage={(id) => {
													if (!windowShellStore.isSplit) {
														windowShellStore.enableSplit();
													}
													windowShellStore.openTabFromRoute(`/page/${id}`, { paneId: 'right' });
												}}
														onBindPage={handlePageSelect}
													/>
												{/if}
											{:else if part.type === "tool-edit_page" && (part as any).state === "output-available"}
												{@const output = (part as any).output}
												{#if output?.needs_binding}
													<PageBindingInline
														entityId={output.page_id}
														entityTitle={output.page_title}
														message={output.message}
														onBind={handlePageSelect}
													/>
												{:else if output?.edit}
													{@const editPageId = output.edit.page_id}
													<EditDiffCard
														status={output.applied ? 'applied' : 'failed'}
														pageId={editPageId}
														find={output.edit.find || ''}
														replace={output.edit.replace || ''}
														isFullReplace={!output.edit.find}
														onViewPage={editPageId ? () => {
															if (!windowShellStore.isSplit) {
																windowShellStore.enableSplit();
															}
															windowShellStore.openTabFromRoute(`/page/${editPageId}`, { paneId: 'right', forceNew: true });
														} : undefined}
													/>
												{/if}
											{:else if part.type === "tool-code_interpreter"}
												{@const toolPart = part as any}
												{@const isRunning = toolPart.state === "pending" || toolPart.state === "input-available"}
												{@const isError = toolPart.state === "output-error"}
												<CodeInterpreterCard
													status={isRunning ? 'running' : isError ? 'error' : 'success'}
													code={toolPart.input?.code || ''}
													output={toolPart.output}
												/>
											{:else if part.type === "tool-generate_image"}
												{@const gen = part as any}
												{#if gen.state === "output-available" && gen.output?.url}
													<figure class="generated-image">
														<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_noninteractive_element_interactions -->
														<img
															src={gen.output.url}
															alt={gen.output.prompt || gen.input?.prompt || "Generated image"}
															class="msg-image"
															onclick={(e) => openLightbox(e, gen.output.url, gen.output.prompt || gen.input?.prompt || "Generated image")}
														/>
													</figure>
												{:else if gen.state === "output-error"}
													<div class="tool-error mb-3 text-sm text-error p-3 bg-error-subtle rounded-lg">
														Image generation failed{gen.errorText ? ` — ${gen.errorText}` : ""}.
													</div>
												{:else}
													<div class="generating-image">
														<Icon icon="ri:image-add-line" width="16" />
														<span>Generating image…</span>
													</div>
												{/if}
												{:else if part.type.startsWith("tool-") && (part as any).state === "output-error"}
													<div
														class="tool-error mb-3 text-sm text-error p-3 bg-error-subtle rounded-lg"
													>
														<span
															class="font-medium"
															>Error:</span
														>
														{(part as any).toolName}
														failed
														{#if (part as any).errorText}
															- {(part as any)
																.errorText}
														{/if}
													</div>
												{/if}
											{/each}
											{#if messageMetadata.get(message.id)?.stopped}
												<StoppedNotice />
											{/if}
										{:else}
											{@const fileParts = message.parts.filter((p: any) => p.type === "file")}
											{#if fileParts.length > 0}
												<div class="msg-attachments">
													{#each fileParts as fp, i (i)}
														{@render renderFilePart(fp as any, true)}
													{/each}
												</div>
											{/if}
											{@const userText = message.parts
												.filter((p) => p.type === "text")
												.map((p) => p.text)
												.join("")}
											{#if userText.trim()}
												<UserMessage text={userText} />
											{/if}
										{/if}
									</div>
								</div>
							{/each}


							<!-- Optimistic thinking indicator: shows immediately on submit,
							     only until the AI SDK creates the assistant message (at text-start).
							     Once the assistant message exists, the in-message ThinkingBlock takes over. -->
							{#if isAwaitingResponse && !lastAssistantMessage}
								<div class="flex justify-start">
									<div class="message-wrapper" data-role="assistant">
										<ThinkingBlock
											isThinking={true}
											toolCalls={[]}
											reasoningContent=""
											isStreaming={true}
											duration={0}
										/>
									</div>
								</div>
							{/if}

							<ChatError error={chat.error} onRetry={() => chat.regenerate()} />
						</div>
					</div>

					<!-- ChatInput -->
					<div
						class="chat-input-wrapper"
						class:is-empty={isEmpty}
						class:has-messages={!isEmpty}
						class:transitions-enabled={enableTransitions}
						class:focused={inputFocused}
						class:drag-active={dragActive}
					>
						{#if dragActive}
							<div class="drop-hint">
								<Icon icon="ri:download-2-line" width="15" />
								<span>Drop to attach &middot; images, PDFs, or audio</span>
							</div>
						{/if}
						{#if attachments.length > 0}
							<div class="attachments">
								{#each attachments as a (a.id)}
									<div class="attachment">
										{#if a.kind === "image"}
											<img src={a.url} alt={a.filename} class="attachment-thumb" />
										{:else}
											<span class="attachment-icon">
												<Icon
													icon={a.kind === "pdf"
														? "ri:file-pdf-fill"
														: a.kind === "audio"
															? "ri:music-2-line"
															: "ri:file-text-line"}
													width="18"
												/>
											</span>
										{/if}
										<div class="attachment-meta">
											<span class="attachment-name">{a.filename}</span>
											<span class="attachment-size">{formatFileSize(a.size)}</span>
										</div>
										<button
											type="button"
											class="attachment-remove"
											aria-label="Remove attachment"
											onclick={() => removeAttachment(a.id)}
										>
											<Icon icon="ri:close-line" width="13" />
										</button>
									</div>
								{/each}
							</div>
						{/if}
						{#if capabilityIssue}
							<div class="capability-banner">
								<span>
									{capabilityIssue.modelName} can't read {capabilityIssue.lacks.join(" or ")}.
								</span>
								{#if capabilityIssue.candidate}
									<button type="button" class="capability-switch" onclick={switchToCapableModel}>
										Switch to {capabilityIssue.candidate.displayName}
									</button>
								{:else}
									<span class="capability-none">No available model can read {capabilityIssue.lacks.join(" or ")} yet.</span>
								{/if}
							</div>
						{/if}
						{#if stagedRefs.length > 0}
							<div class="staged-refs">
								{#each stagedRefs as r (r.id)}
									<div class="staged-ref">
										<Icon
											icon="ri:double-quotes-l"
											width="13"
											class="staged-ref-mark"
										/>
										<div class="staged-ref-body">
											<span class="staged-ref-quote">{r.text}</span>
											{#if r.comment}
												<span class="staged-ref-comment">{r.comment}</span>
											{/if}
										</div>
										<button
											type="button"
											class="queued-remove"
											aria-label="Remove reference"
											onclick={() => removeStagedRef(r.id)}
										>
											<Icon icon="ri:close-line" width="13" />
										</button>
									</div>
								{/each}
							</div>
						{/if}
						{#if queuedMessages.length > 0}
							<div class="queued-messages">
								{#each queuedMessages as q, i (i)}
									<div class="queued-chip">
										<span class="queued-text">{q}</span>
										<button
											type="button"
											class="queued-remove"
											aria-label="Remove queued message"
											onclick={() => removeQueued(i)}
										>
											<Icon icon="ri:close-line" width="13" />
										</button>
									</div>
								{/each}
							</div>
						{/if}
						<ChatInput
							allowEmptySubmit={stagedRefs.length > 0 || attachments.length > 0}
							onAttach={addFiles}
							bind:value={input}
							bind:focused={inputFocused}
							bind:selectedModel={selectedModelValue}
							bind:selectedAgentMode={selectedAgentMode}
							bind:selectedPersona={selectedPersona}
							disabled={false}
							sendDisabled={chat.status !== "ready"}
							isStreaming={chat.status === "streaming"}
							maxWidth="max-w-3xl"
							showToolbar={true}
							conversationId={extractConversationId(tab.route)}
							{contextUsage}
							onContextClick={handleContextClick}
							editableItems={editAllowListStore.items.filter((i) => i.type !== 'action')}
							pageBinding={getBoundPage() ? { pageId: getBoundPage()!.id, pageTitle: getBoundPage()!.title || 'Untitled' } : undefined}
							onPageClear={handlePageClear}
							onRemoveItem={handleRemoveItem}
							onPageSelect={handlePageSelect}
							onSelectEntities={handleSelectEntities}
							on:submit={(e) => handleChatSubmit(e.detail)}
							on:stop={() => handleChatStop()}
						/>

					</div>
				</div>
			</div>
		</div>
	</div>

	<CitationPanel
		citation={selectedCitation}
		open={citationPanelOpen}
		onClose={closeCitationPanel}
	/>
{/if}

<style>
	.loading-container {
		display: flex;
		align-items: center;
		justify-content: center;
		height: 100%;
	}

	.loading-spinner {
		width: 24px;
		height: 24px;
		border: 2px solid var(--color-border);
		border-top-color: var(--color-primary);
		border-radius: 50%;
		animation: spin 0.8s linear infinite;
	}

	@keyframes spin {
		to {
			transform: rotate(360deg);
		}
	}

	.chat-root {
		height: 100%;
		width: 100%;
		display: flex;
		position: relative;
	}

	/* Track D in-text reference mark — the app's own warm highlight token, one
	   marker for every staged reference. Painted via the CSS Custom Highlight API
	   (no DOM mutation, no reflow), so it's theme-aware for free. */
	:global(::highlight(vref)) {
		background-color: var(--color-highlight);
		color: var(--color-highlight-foreground);
	}

	.staged-refs {
		display: flex;
		flex-direction: column;
		gap: 0.375rem;
		margin-bottom: 0.5rem;
	}

	.staged-ref {
		display: flex;
		align-items: baseline;
		gap: 0.4375rem;
		padding: 0.375rem 0.5rem 0.375rem 0.625rem;
		border: 1px solid var(--color-border-subtle);
		border-radius: 0.625rem;
		background: var(--color-surface-elevated);
	}

	.staged-ref :global(.staged-ref-mark) {
		flex-shrink: 0;
		color: var(--color-foreground-subtle);
		transform: translateY(1px);
	}

	.staged-ref-body {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 0.0625rem;
	}

	.staged-ref-quote {
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.staged-ref-comment {
		font-size: 0.8125rem;
		color: var(--color-foreground);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.chat-container {
		display: flex;
		height: 100%;
		width: 100%;
		position: relative;
	}

	.chat-area {
		flex: 1;
		height: 100%;
		position: relative;
		overflow: hidden;
	}

	.chat-topbar {
		position: absolute;
		top: 8px;
		left: 12px;
		z-index: 5;
		display: flex;
		align-items: center;
		padding: 2px;
		border-radius: 9px;
		background: color-mix(in srgb, var(--color-surface) 72%, transparent);
		backdrop-filter: blur(8px);
		-webkit-backdrop-filter: blur(8px);
	}

	.page-container {
		height: 100%;
		position: relative;
	}

	.chat-layout {
		height: 100%;
		opacity: 0;
		pointer-events: none;
		transition: opacity 0.2s ease-in-out;
		position: relative;
		z-index: 1;
		/* Keep scroll position stable as streamed content grows above the fold */
		overflow-anchor: auto;
		/* Use standard scrollbar styling — preserves overlay scrollbar behavior on macOS
		   (unlike ::-webkit-scrollbar which forces classic scrollbars that steal layout space) */
		scrollbar-width: thin;
		scrollbar-color: var(--color-border) transparent;
	}

	.chat-layout.visible {
		opacity: 1;
		pointer-events: auto;
	}

	.messages-container {
		max-width: 48rem;
		margin: 0 auto;
		width: 100%;
		padding: 1.5rem 2rem 10rem 2rem;
		display: flex;
		flex-direction: column;
		gap: 1rem;
		position: relative;
		z-index: 1;
		/* Stop a growing streamed message from reflowing/repainting the whole list.
		   Safe here: the sticky .chat-input-wrapper is a sibling of the scroller,
		   not a descendant, so layout containment doesn't affect it. */
		contain: layout paint;
	}

	.chat-input-wrapper {
		position: absolute;
		bottom: 0;
		left: 0;
		right: 0;
		margin: 0 auto;
		width: 100%;
		max-width: 48rem;
		padding: 0 2rem 2rem 2rem;
		background-color: var(--color-surface);
		background-image: var(--background-image);
		background-blend-mode: multiply;
		box-sizing: border-box;
		z-index: 10;
	}

	.queued-messages {
		display: flex;
		flex-direction: column;
		gap: 0.375rem;
		margin-bottom: 0.5rem;
	}

	.queued-chip {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.375rem 0.625rem;
		border: 1px solid var(--color-border);
		border-radius: 0.625rem;
		background: var(--color-surface-elevated);
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
	}

	.queued-text {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.queued-remove {
		flex-shrink: 0;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		padding: 0.125rem;
		border-radius: 0.375rem;
		color: var(--color-foreground-muted);
		transition: background-color 0.15s ease;
	}

	.queued-remove:hover {
		background: var(--color-border);
	}

	/* Track E1 — composer attachment previews */
	.attachments {
		display: flex;
		flex-wrap: wrap;
		gap: 0.5rem;
		margin-bottom: 0.5rem;
	}

	.attachment {
		display: inline-flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.375rem 0.5rem 0.375rem 0.375rem;
		border: 1px solid var(--color-border-subtle);
		border-radius: 0.625rem;
		background: var(--color-surface-elevated);
		max-width: 15rem;
	}

	.attachment-thumb {
		width: 2.25rem;
		height: 2.25rem;
		border-radius: 0.4rem;
		object-fit: cover;
		flex-shrink: 0;
		display: block;
	}

	.attachment-icon {
		width: 2.25rem;
		height: 2.25rem;
		border-radius: 0.4rem;
		display: flex;
		align-items: center;
		justify-content: center;
		background: var(--color-surface);
		color: var(--color-foreground-muted);
		flex-shrink: 0;
	}

	.attachment-meta {
		display: flex;
		flex-direction: column;
		gap: 0.0625rem;
		min-width: 0;
	}

	.attachment-name {
		font-size: 0.8125rem;
		color: var(--color-foreground);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.attachment-size {
		font-size: 0.6875rem;
		color: var(--color-foreground-subtle);
	}

	.attachment-remove {
		flex-shrink: 0;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		padding: 0.125rem;
		border-radius: 0.375rem;
		color: var(--color-foreground-muted);
		transition: background-color 0.15s ease;
	}

	.attachment-remove:hover {
		background: var(--color-border);
	}

	.capability-banner {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 0.5rem;
		margin-bottom: 0.5rem;
		padding: 0.375rem 0.625rem;
		border: 1px solid var(--color-warning, var(--color-border));
		border-radius: 0.625rem;
		background: var(--color-warning-subtle, var(--color-surface-elevated));
		font-size: 0.8125rem;
		color: var(--color-foreground);
	}

	.capability-switch {
		color: var(--color-primary);
		font-weight: 500;
	}

	.capability-switch:hover {
		text-decoration: underline;
	}

	.capability-none {
		color: var(--color-foreground-muted);
	}

	/* Track E1 — in-message media */
	.msg-attachments {
		display: flex;
		flex-wrap: wrap;
		gap: 0.5rem;
		margin-bottom: 0.5rem;
	}

	.msg-image {
		max-width: min(420px, 100%);
		max-height: 420px;
		border-radius: 0.75rem;
		border: 1px solid var(--color-border-subtle);
		display: block;
		cursor: zoom-in;
	}

	.msg-audio {
		width: min(420px, 100%);
	}

	.msg-file {
		display: inline-flex;
		align-items: center;
		gap: 0.375rem;
		padding: 0.5rem 0.75rem;
		border: 1px solid var(--color-border-subtle);
		border-radius: 0.625rem;
		background: var(--color-surface-elevated);
		font-size: 0.875rem;
		color: var(--color-foreground);
		text-decoration: none;
	}

	.msg-file:hover {
		border-color: var(--color-border-strong);
	}

	/* Track E2 — generated image */
	.generated-image {
		margin: 0.5rem 0;
	}

	.generating-image {
		display: inline-flex;
		align-items: center;
		gap: 0.5rem;
		margin: 0.5rem 0;
		padding: 0.5rem 0.75rem;
		border: 1px solid var(--color-border-subtle);
		border-radius: 0.625rem;
		background: var(--color-surface-elevated);
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
	}

	/* Track E1 — in-place drag affordance: the composer becomes the dropzone
	   (no full-screen scrim — context stays visible, the cue points at the
	   exact landing spot). Drop still works anywhere over the chat root. */
	.drop-hint {
		position: absolute;
		left: 50%;
		bottom: calc(100% - 0.5rem);
		transform: translateX(-50%);
		display: inline-flex;
		align-items: center;
		gap: 0.4rem;
		padding: 0.3rem 0.7rem;
		border-radius: 999px;
		background: var(--color-primary);
		color: var(--color-on-primary, #fff);
		font-size: 0.75rem;
		font-weight: 500;
		white-space: nowrap;
		box-shadow: 0 6px 18px -6px color-mix(in srgb, var(--color-primary) 60%, transparent);
		pointer-events: none;
		z-index: 11;
		animation: drop-hint-in 0.18s cubic-bezier(0.22, 1, 0.36, 1);
	}

	@keyframes drop-hint-in {
		from {
			opacity: 0;
			transform: translateX(-50%) translateY(0.35rem);
		}
		to {
			opacity: 1;
			transform: translateX(-50%) translateY(0);
		}
	}

	/* Accent ring + gentle lift on the actual composer box while dragging. */
	.chat-input-wrapper.drag-active :global(.chat-input-container .chat-input-wrapper) {
		border-color: var(--color-primary);
		box-shadow:
			0 0 0 3px color-mix(in srgb, var(--color-primary) 22%, transparent),
			0 10px 28px -14px color-mix(in srgb, var(--color-primary) 50%, transparent);
		transition:
			border-color 0.15s ease,
			box-shadow 0.15s ease;
	}

	.chat-input-wrapper.transitions-enabled {
		transition:
			bottom 0.6s cubic-bezier(0.4, 0, 0.2, 1),
			transform 0.6s cubic-bezier(0.4, 0, 0.2, 1);
	}

	.chat-input-wrapper.is-empty {
		bottom: auto;
		top: 50%;
		transform: translateY(-50%);
	}

	.chat-input-wrapper.has-messages {
		padding-bottom: 1.5rem;
	}

	.page-container:not(.is-empty) .chat-input-wrapper {
		position: sticky;
	}

	.hero-section {
		text-align: center;
		opacity: 0;
		max-height: 0;
		overflow: hidden;
	}

	.hero-section.transitions-enabled {
		transition:
			opacity 0.3s ease-in-out,
			max-height 0.3s ease-in-out;
	}

	.hero-section.visible {
		opacity: 1;
		max-height: 150px;
	}

	.hero-title {
		text-align: center;
	}

	.message-wrapper {
		position: relative;
		width: 100%;
		padding: 0.5rem 0;
		min-width: 0;
		overflow-wrap: break-word;
		word-break: break-word;
		/* Isolate each message's layout/paint so a re-render of one (e.g. the
		   streaming tail) can't reflow siblings. */
		contain: layout paint;
	}

	.message-wrapper :global(h1),
	.message-wrapper :global(h2),
	.message-wrapper :global(h3),
	.message-wrapper :global(h4) {
		margin-top: 0;
	}

	/* User message card styling */
	.message-wrapper[data-role="user"] {
		background: var(--color-surface-elevated);
		border: 1px solid var(--color-border);
		border-radius: 8px;
		padding: 10px 16px;
	}

	/* A user turn with attachments hugs its content (photo + caption) instead of
	   spanning full width with dead space. Text-only user turns are unchanged.
	   Direct class (set in the loop) — robust vs :has()/snippet scoping. */
	.message-wrapper.user-has-attachment {
		width: fit-content;
		max-width: 100%;
	}

	/* User-attached images render as compact thumbnails (class is on the <img>
	   itself); assistant/generated images keep the larger size. */
	.msg-image.compact-img {
		max-width: min(260px, 100%);
		max-height: 260px;
	}

	/* Assistant response text - spacing after thinking block */
	.assistant-response {
		padding-top: 4px;
	}

	.shiny-title {
		overflow: visible;
		padding-bottom: 0.25rem;
	}

	.chat-input-wrapper.focused .shiny-title {
		background-image: linear-gradient(
			90deg,
			var(--color-primary) 0%,
			var(--color-primary) 30%,
			transparent 55%,
			var(--color-foreground) 80%,
			var(--color-foreground) 100%
		);
		background-position: 100% center;
		background-size: 300% auto;
		-webkit-background-clip: text;
		background-clip: text;
		color: var(--color-foreground);
		-webkit-text-fill-color: transparent;
		animation: shiny-title 1.18s cubic-bezier(0.3, 0.9, 0.4, 1) forwards;
	}

	@keyframes shiny-title {
		0% {
			background-position: 100% center;
		}
		3% {
			background-position: 100% center;
		}
		100% {
			background-position: 0% center;
		}
	}

</style>
