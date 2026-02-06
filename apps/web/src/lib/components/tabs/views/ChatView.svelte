<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import type { Tab } from "$lib/tabs/types";
	import { spaceStore } from "$lib/stores/space.svelte";
	import Page from "$lib/components/Page.svelte";
	import ChatInput from "$lib/components/ChatInput.svelte";
	import { toast } from "svelte-sonner";
	import GettingStarted from "$lib/components/GettingStarted.svelte";
	import {
		getSelectedModel,
		getDefaultModel,
		initializeSelectedModel,
		getInitializationPromise,
	} from "$lib/stores/models.svelte";
	import CitedMarkdown from "$lib/components/CitedMarkdown.svelte";
	import { CitationPanel } from "$lib/components/citations";
	import { buildCitationContextFromParts } from "$lib/citations";
	import type { Citation } from "$lib/types/Citation";
	import UserMessage from "$lib/components/UserMessage.svelte";
	import ThinkingBlock from "$lib/components/ThinkingBlock.svelte";
	import { onMount, onDestroy } from "svelte";
	import { goto } from "$app/navigation";
	import { chatSessions } from "$lib/stores/chatSessions.svelte";
	import { chatInstances } from "$lib/stores/chatInstances.svelte";
	import type { Chat } from "@ai-sdk/svelte";
	// Active page editing imports
	import { activePageStore, editAllowListStore } from "$lib/stores/activePage.svelte";
	import { addPendingEdit, acceptEdit, rejectEdit, getPendingEdit, isEditPending } from "$lib/stores/pendingEdits.svelte";
	import { dispatchAIEditHighlight, dispatchAIEditAccept, dispatchAIEditReject } from "$lib/events/aiEdit";
	import PageBindingInline from "$lib/components/chat/PageBindingInline.svelte";
	import PageEditResult from "$lib/components/chat/PageEditResult.svelte";
	import EditDiffCard from "$lib/components/chat/EditDiffCard.svelte";
	import CodeInterpreterCard from "$lib/components/chat/CodeInterpreterCard.svelte";
	import CompactionCheckpoint from "$lib/components/chat/CompactionCheckpoint.svelte";
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
	let loadedMessages = $state<any[]>([]);

	// Track tab route to reset state when switching conversations
	// svelte-ignore state_referenced_locally
	let previousTabRoute = $state<string>(tab.route);
	let preferredName = $state<string | undefined>(undefined);

	// AbortController for cancelling in-flight requests on tab switch
	let tabSwitchAbortController: AbortController | null = null;

	// UI preferences from assistant profile
	let uiPreferences = $state<{
		contextIndicator?: {
			alwaysVisible?: boolean;
			showThreshold?: number;
		};
		gettingStarted?: {
			hidden?: boolean;
			completedSteps?: string[];
		};
	}>({});

	// Keep a map of message metadata (agentId, provider, etc.) for rendering
	let messageMetadata = $state<
		Map<string, { agentId?: string; provider?: string }>
	>(new Map());

	// Citation panel state
	let citationPanelOpen = $state(false);
	let selectedCitation = $state<Citation | null>(null);

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

	// Active page editing handlers
	function handlePageChangePage() {
		// Legacy handler - not used with inline picker
	}

	function handlePageClear() {
		activePageStore.unbind();
	}

	function handleRemoveItem(type: string, id: string) {
		editAllowListStore.remove(type as 'page' | 'folder' | 'wiki_entry', id);
	}

	function handlePageSelect(pageId: string, pageTitle: string) {
		// Create Yjs document for the page and bind
		// NOTE: No auto-open - user can open the page manually if they want to see it
		const yjsDoc = createYjsDocument(pageId);
		activePageStore.bind(pageId, pageTitle, yjsDoc);
	}

	/**
	 * Handle permission allow for AI edit
	 * @param entityId - The entity to allow editing
	 * @param entityType - Type of entity (page, person, etc.)
	 * @param title - Display title
	 * @param forChat - If true, add to allow list for entire chat session
	 */
	async function handlePermissionAllow(entityId: string, entityType: string, title: string, forChat: boolean) {
		// Add to allow list (both "Allow" and "Allow for chat" add permission for retry)
		// For MVP, both paths add to the list; "Allow once" could be refined later
		const type = entityType === 'page' ? 'page' :
		             entityType === 'folder' ? 'folder' : 'page';

		if (entityType === 'page') {
			const yjsDoc = createYjsDocument(entityId);
			await editAllowListStore.addPage(entityId, title, yjsDoc);
		} else {
			await editAllowListStore.add({
				type: type as 'page' | 'folder' | 'wiki_entry',
				id: entityId,
				title: title
			});
		}

		// Auto-retry: Find the last user message and resend it
		// This triggers the AI to retry the edit (now with permission granted)
		const lastUserMessage = chat.messages
			.filter(m => m.role === 'user')
			.pop();

		if (lastUserMessage) {
			// Extract text from the message parts
			const textPart = (lastUserMessage.parts as any[])?.find(p => p.type === 'text');
			const messageText = textPart?.text || '';

			if (messageText && chat.status === 'ready') {
				// Small delay to ensure permission is saved to backend
				setTimeout(async () => {
					try {
						await chat.sendMessage({ text: messageText });
					} catch (error) {
						console.error('[ChatView] Failed to retry after permission grant:', error);
					}
				}, 100);
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

	/**
	 * Revert an AI edit by calling the backend with reversed find/replace
	 */
	async function revertEdit(pageId: string, editId: string) {
		const edit = getPendingEdit(pageId, editId);
		if (!edit) return;

		try {
			// Call backend with reversed arguments to undo the edit
			const response = await fetch(`/api/pages/${pageId}/edit`, {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({
					find: edit.replace,    // Find what AI added
					replace: edit.find,    // Replace with original
				})
			});

			if (response.ok) {
				// Remove from pending edits store
				rejectEdit(pageId, editId);
				// Dispatch reject event to remove highlight in PageEditor
				dispatchAIEditReject({ pageId, editId });
			} else {
				console.error('[ChatView] Failed to revert edit:', await response.text());
				toast.error('Failed to revert edit');
			}
		} catch (error) {
			console.error('[ChatView] Error reverting edit:', error);
			toast.error('Failed to revert edit');
		}
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
		const yjsDoc = createYjsDocument(pageId);
		activePageStore.bind(pageId, title, yjsDoc);

		if (!spaceStore.isSplit) {
			spaceStore.enableSplit();
		}
		spaceStore.openTabFromRoute(`/page/${pageId}`, { paneId: 'right' });
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

	// Track completed edit_page tool calls to avoid re-registering
	let registeredEditCalls = $state<Set<string>>(new Set());

	// Effect to register new edit_page results as pending edits
	$effect(() => {
		if (!chat?.messages) return;
		if (isLoading) return;

		for (const message of chat.messages) {
			if (message.role !== 'assistant') continue;

			for (const part of message.parts as ToolResultPart[]) {
				if (part.type === 'tool-edit_page' && part.state === 'output-available') {
					const output = (part as any).output;
					if (output?.edit?.edit_id && output?.edit?.page_id && !registeredEditCalls.has(part.toolCallId || '')) {
						const { edit_id, page_id, find, replace } = output.edit;

						// Register this edit as pending
						addPendingEdit({
							editId: edit_id,
							pageId: page_id,
							find: find || '',
							replace: replace || '',
							timestamp: Date.now()
						});

						// Dispatch event to highlight the edit in PageEditor
						dispatchAIEditHighlight({
							pageId: page_id,
							editId: edit_id,
							text: replace || ''
						});

						registeredEditCalls.add(part.toolCallId || '');
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

	// ==========================================================================
	// Context View - Detailed session data (for ?view=context mode)
	// ==========================================================================
	interface SessionUsage {
		session_id: string;
		model: string;
		context_window: number;
		total_tokens: number;
		usage_percentage: number;
		input_tokens: number;
		output_tokens: number;
		reasoning_tokens: number;
		cache_read_tokens: number;
		cache_write_tokens: number;
		total_cost_usd: number;
		user_message_count: number;
		assistant_message_count: number;
		first_message_at: string | null;
		last_message_at: string | null;
		compaction_status: {
			summary_exists: boolean;
			messages_summarized: number;
			messages_verbatim: number;
			summary_version: number;
			last_compacted_at: string | null;
		};
		context_status: string;
	}

	interface SessionDetail {
		conversation: {
			conversation_id: string;
			title: string;
			first_message_at: string;
			last_message_at: string;
			message_count: number;
			model?: string;
			provider?: string;
		};
		messages: Array<{
			id: string;
			role: string;
			content: string;
			timestamp: string;
			model?: string;
			tool_calls?: Array<{
				tool_name: string;
				tool_call_id?: string;
				arguments: unknown;
				result?: unknown;
				timestamp: string;
			}>;
			reasoning?: string;
		}>;
	}

	interface Breakdown {
		user: { tokens: number; pct: number };
		assistant: { tokens: number; pct: number };
		toolCalls: { tokens: number; pct: number };
		other: { tokens: number; pct: number };
	}

	let sessionUsage = $state<SessionUsage | null>(null);
	let sessionDetail = $state<SessionDetail | null>(null);
	let contextViewLoading = $state(false);
	let contextViewError = $state<string | null>(null);
	let compacting = $state(false);

	// Fetch detailed session data for context view
	async function fetchContextViewData() {
		if (!conversationId || isNewChat(tab.route)) {
			contextViewError = 'No conversation ID';
			return;
		}

		contextViewLoading = true;
		contextViewError = null;

		try {
			const [usageRes, sessionRes] = await Promise.all([
				fetch(`/api/chats/${conversationId}/usage`),
				fetch(`/api/chats/${conversationId}`)
			]);

			if (!usageRes.ok) throw new Error(`Failed to fetch usage: ${usageRes.status}`);
			if (!sessionRes.ok) throw new Error(`Failed to fetch session: ${sessionRes.status}`);

			sessionUsage = await usageRes.json();
			sessionDetail = await sessionRes.json();
		} catch (e) {
			contextViewError = e instanceof Error ? e.message : 'Unknown error';
		} finally {
			contextViewLoading = false;
		}
	}

	// Compact the session
	async function handleCompact() {
		if (!conversationId || compacting) return;

		compacting = true;
		try {
			const res = await fetch(`/api/chats/${conversationId}/compact`, {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ force: true })
			});

			if (!res.ok) throw new Error(`Failed to compact: ${res.status}`);

			// Refresh context view data
			await fetchContextViewData();

			// Re-fetch messages to show the new checkpoint message
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
		} catch (e) {
			contextViewError = e instanceof Error ? e.message : 'Compaction failed';
		} finally {
			compacting = false;
		}
	}

	// Calculate token breakdown from messages
	function calculateBreakdown(messages: SessionDetail['messages']): Breakdown {
		let user = 0, assistant = 0, toolCalls = 0, other = 0;

		for (const msg of messages) {
			const contentTokens = Math.ceil((msg.content?.length || 0) / 4);

			if (msg.role === 'user') {
				user += contentTokens;
			} else if (msg.role === 'assistant') {
				assistant += contentTokens;
				if (msg.tool_calls) {
					for (const tc of msg.tool_calls) {
						toolCalls += Math.ceil(JSON.stringify(tc).length / 4);
					}
				}
				if (msg.reasoning) {
					other += Math.ceil(msg.reasoning.length / 4);
				}
			} else {
				other += contentTokens;
			}
		}

		const total = user + assistant + toolCalls + other || 1;
		return {
			user: { tokens: user, pct: (user / total) * 100 },
			assistant: { tokens: assistant, pct: (assistant / total) * 100 },
			toolCalls: { tokens: toolCalls, pct: (toolCalls / total) * 100 },
			other: { tokens: other, pct: (other / total) * 100 }
		};
	}

	// Format helpers for context view
	function formatTokens(tokens: number): string {
		if (tokens >= 1_000_000) return `${(tokens / 1_000_000).toFixed(2)}M`;
		if (tokens >= 1_000) return `${(tokens / 1_000).toFixed(1)}K`;
		return tokens.toLocaleString();
	}

	function formatCost(cost: number): string {
		if (cost < 0.01) return `$${cost.toFixed(4)}`;
		return `$${cost.toFixed(2)}`;
	}

	function formatDate(date: string | null): string {
		if (!date) return '—';
		return new Date(date).toLocaleString();
	}

	function formatShortDate(date: string): string {
		return new Date(date).toLocaleString('en-US', {
			month: 'short',
			day: 'numeric',
			hour: 'numeric',
			minute: '2-digit'
		});
	}

	const breakdown = $derived(sessionDetail ? calculateBreakdown(sessionDetail.messages) : null);

	// Fetch context data when entering context view mode
	$effect(() => {
		if (isContextView && active && conversationId) {
			fetchContextViewData();
		}
	});

	// Handle context indicator click - open context tab in split view
	function handleContextClick() {
		const currentPane = spaceStore.findTabPane(tab.id);
		spaceStore.openChatContext(conversationId, currentPane);
	}

	// Helper function to convert database messages to Chat parts
	function convertMessageToParts(msg: any) {
		if (msg.agentId || msg.provider) {
			messageMetadata.set(msg.id, {
				agentId: msg.agentId,
				provider: msg.provider,
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

	// Getter for current space ID - used by Chat transport for auto-add
	// Returns null for system space (Virtues) so chats don't get auto-added
	function getSpaceId(): string | null {
		if (spaceStore.isSystemSpace) return null;
		return spaceStore.activeSpaceId;
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
					const pageId = activePageStore.getBoundPageId();
					const pageTitle = activePageStore.getBoundPageTitle();
					const yjsDoc = activePageStore.getYjsDoc();

					if (!pageId) return null;

					// Include the current Yjs content so AI edits match what's in the editor
					const content = yjsDoc?.yxmlFragment.toString() || '';

					return {
						page_id: pageId,
						page_title: pageTitle || undefined,
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
			thinkingIndicatorVisible = false;
			minTimeElapsed = false;
			// Reset page create tracking (for auto-open)
			initialCompletedToolCalls = null;
			initialLoadComplete = false;
			// NOTE: We no longer unbind the active page when switching chats.
			// Binding is now additive/persistent to the chat session context.
			// activePageStore.unbind();

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
							await refreshContextUsage();
							// Initialize edit allow list for this chat
							await editAllowListStore.init(currentTabConversationId);
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
		// Async initialization
		(async () => {
			// Wait for models to load
			await getInitializationPromise();

			// Load assistant profile
			let profileDefaultModelId: string | undefined;
			let profileDefaultPersona: string | undefined;
			try {
				const profileResponse = await fetch("/api/assistant-profile");
				if (profileResponse.ok) {
					const profile = await profileResponse.json();
					if (profile.ui_preferences) {
						uiPreferences = profile.ui_preferences;
					}
					// Extract default model and persona for new chat initialization
					profileDefaultModelId = profile.chat_model_id || profile.default_model_id;
					profileDefaultPersona = profile.persona;
				}
			} catch (error) {
				console.error("Failed to load assistant profile:", error);
			}

			// Load preferred name from profile
			try {
				const response = await fetch("/api/profile");
				if (response.ok) {
					const profile = await response.json();
					preferredName = profile.preferred_name;
				}
			} catch (error) {
				// Non-critical, continue without preferred name
			}

			// Load conversation if it exists (not a new chat)
			const tabConversationId = extractConversationId(tab.route);
			if (tabConversationId) {
				try {
					const response = await fetch(
						`/api/chats/${tabConversationId}`,
					);
					if (response.ok) {
						const data = await response.json();
						loadedMessages = data.messages || [];

						// Update chat messages
						chat.messages = deduplicateMessages(loadedMessages).map(
							(msg: any) => ({
								id: msg.id,
								role: msg.role as "user" | "assistant" | "checkpoint",
								parts: convertMessageToParts(msg),
							}),
						);

						// Initialize model from conversation
						if (data.conversation?.model) {
							initializeSelectedModel(data.conversation.model);
						}

						// Load initial context usage
						await refreshContextUsage();

						// Initialize edit allow list for this chat
						await editAllowListStore.init(tabConversationId);
					}
				} catch (error) {
					console.error(
						"[ChatView] Error loading conversation:",
						error,
					);
				}
			} else {
				// New chat - set the chatId so permissions can sync when granted
				editAllowListStore.setChatId(conversationId);
				// Initialize model and persona from assistant profile defaults
				initializeSelectedModel(undefined, profileDefaultModelId);
				// Also sync to local state for immediate UI update
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
				// Scroll to bottom after initial load
				scrollToBottom("instant");
				enableTransitions = true;
			}, 50);
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

	// Thinking indicator state machine
	// - Shows when thinking starts
	// - Stays visible for minimum 500ms
	// - Hides when substantial text arrives OR thinking ends (whichever is later)
	let thinkingIndicatorVisible = $state(false);
	let minTimeElapsed = $state(false);

	// Check for substantial text content (more than 20 chars)
	const hasSubstantialTextContent = $derived.by(() => {
		if (!lastAssistantMessage) return false;
		const textParts = lastAssistantMessage.parts.filter(
			(p: any) => p.type === "text" && p.text,
		);
		const totalText = textParts
			.map((p: any) => p.text)
			.join("")
			.trim();
		return totalText.length > 20;
	});

	// Effect: Show indicator when thinking starts, with minimum display timer
	$effect(() => {
		if (isThinking && !thinkingIndicatorVisible) {
			thinkingIndicatorVisible = true;
			minTimeElapsed = false;
			const timer = setTimeout(() => {
				minTimeElapsed = true;
			}, 500);
			return () => clearTimeout(timer);
		}
	});

	// Effect: Hide indicator when ready (min time passed AND either has text OR done thinking)
	$effect(() => {
		if (minTimeElapsed && (hasSubstantialTextContent || !isThinking)) {
			thinkingIndicatorVisible = false;
		}
	});

	const showStandaloneThinking = $derived(thinkingIndicatorVisible);

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
	let selectedAgentMode = $state<AgentModeId>('agent');
	let selectedPersona = $state<string>('default');

	// Sync selected model with store (only on initial load)
	$effect(() => {
		const storeModel = getSelectedModel();
		if (storeModel && !selectedModelValue) {
			selectedModelValue = storeModel;
		}
	});

	// Reactive messages with subjects
	// Refresh chat data (placeholder for future use)
	async function refreshChatData() {
		// Function intentionally empty - can be used for future chat data refresh needs
	}

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
	let isEmpty = $derived(uniqueMessages.length === 0);

	// Time-based greeting
	function getTimeBasedGreeting(): string {
		return "";
	}

	let greeting = $state("");

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
					spaceStore.updateTab(tab.id, { label: data.title });
				}
			}
		} catch (error) {
			// Title generation is non-critical
		}
	}

	// Getting Started handlers
	function handleGettingStartedCreateChat(chatId: string) {
		// Open the new chat in a tab and navigate to it
		spaceStore.openTabFromRoute(`/chat/${chatId}`, {
			forceNew: true,
			label: "Welcome to Virtues",
		});
		chatSessions.refresh();
	}

	function scrollToBottom(behavior: ScrollBehavior = "smooth") {
		if (scrollContainer) {
			scrollContainer.scrollTo({
				top: scrollContainer.scrollHeight,
				behavior,
			});
		}
	}

	function handleGettingStartedFocusInput(placeholder?: string) {
		if (placeholder) {
			input = placeholder;
		}
		inputFocused = true;
	}

	async function handleChatStop() {
		// Stop the client-side stream
		chat.stop();

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

	async function handleChatSubmit(value: string) {
		const messageToSend = value.trim();
		if (!messageToSend) return;

		if (chat.status !== "ready") {
			return;
		}
		input = "";

		// Auto-scroll to bottom on submit
		scrollToBottom("smooth");

		try {
			await chat.sendMessage({ text: messageToSend });

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
					spaceStore.updateTab(tab.id, {
						route: newRoute,
					});
					// Mark chat as created so pending permissions sync to backend
					await editAllowListStore.markChatCreated();
					// Invalidate the Chats view cache so it refreshes with the new chat
					spaceStore.invalidateViewCache('chat');
					// Reload space items so new chat appears in sidebar
					// (Backend already added it via chat.rs auto-add logic)
					if (!spaceStore.isSystemSpace) {
						await spaceStore.loadSpaceItems();
					}
				}
				await chatSessions.refresh();
			}

			if (refreshDataTimeout) {
				clearTimeout(refreshDataTimeout as any);
			}
			refreshDataTimeout = setTimeout(() => {
				refreshChatData();
				// Refresh context usage after message completes
				refreshContextUsage();
				refreshDataTimeout = null;
			}, 2000);
		} catch (error) {
			console.error("[handleChatSubmit] Error:", error);
			input = "";
		}
	}
</script>

{#if !chat}
	<!-- wait for chat to initialize -->
{:else if isContextView}
	<!-- Context View - Full session details -->
	<div class="context-view">
		{#if contextViewLoading}
			<div class="cv-loading">Loading...</div>
		{:else if contextViewError}
			<div class="cv-error">
				<span>{contextViewError}</span>
				<button type="button" onclick={fetchContextViewData}>Retry</button>
			</div>
		{:else if sessionUsage && sessionDetail}
			<dl class="info-grid">
				<dt>Session</dt>
				<dd class="title">{sessionDetail.conversation.title || 'Untitled'}</dd>

				<dt>Messages</dt>
				<dd>{sessionDetail.conversation.message_count}</dd>

				<dt>Provider</dt>
				<dd>{sessionDetail.conversation.provider || '—'}</dd>

				<dt>Model</dt>
				<dd class="mono">{sessionUsage.model}</dd>

				<dt>Context Limit</dt>
				<dd class="mono">{formatTokens(sessionUsage.context_window)}</dd>

				<dt>Total Tokens</dt>
				<dd class="mono">{formatTokens(sessionUsage.total_tokens)}</dd>

				<dt>Usage</dt>
				<dd class="mono">{sessionUsage.usage_percentage.toFixed(1)}%</dd>

				<dt>Input Tokens</dt>
				<dd class="mono">{formatTokens(sessionUsage.input_tokens)}</dd>

				<dt>Output Tokens</dt>
				<dd class="mono">{formatTokens(sessionUsage.output_tokens)}</dd>

				<dt>Reasoning Tokens</dt>
				<dd class="mono">{formatTokens(sessionUsage.reasoning_tokens)}</dd>

				<dt>Cache Tokens</dt>
				<dd class="mono">{formatTokens(sessionUsage.cache_read_tokens)} / {formatTokens(sessionUsage.cache_write_tokens)}</dd>

				<dt>User Messages</dt>
				<dd>{sessionUsage.user_message_count}</dd>

				<dt>Assistant Messages</dt>
				<dd>{sessionUsage.assistant_message_count}</dd>

				<dt>Total Cost</dt>
				<dd class="mono">{formatCost(sessionUsage.total_cost_usd)}</dd>

				<dt>Session Created</dt>
				<dd>{formatDate(sessionUsage.first_message_at)}</dd>

				<dt>Last Activity</dt>
				<dd>{formatDate(sessionUsage.last_message_at)}</dd>
			</dl>

			<!-- Context Breakdown Bar -->
			<div class="cv-breakdown">
				<div class="cv-breakdown-label">Context Breakdown</div>
				{#if breakdown && (breakdown.user.pct > 0 || breakdown.assistant.pct > 0 || breakdown.toolCalls.pct > 0 || breakdown.other.pct > 0)}
					<div class="cv-bar">
						{#if breakdown.user.pct > 0}
							<div class="cv-segment cv-user" style="width: {breakdown.user.pct}%"></div>
						{/if}
						{#if breakdown.assistant.pct > 0}
							<div class="cv-segment cv-assistant" style="width: {breakdown.assistant.pct}%"></div>
						{/if}
						{#if breakdown.toolCalls.pct > 0}
							<div class="cv-segment cv-tools" style="width: {breakdown.toolCalls.pct}%"></div>
						{/if}
						{#if breakdown.other.pct > 0}
							<div class="cv-segment cv-other" style="width: {breakdown.other.pct}%"></div>
						{/if}
					</div>
					<div class="cv-legend">
						<span><i class="cv-dot cv-user"></i> User {breakdown.user.pct.toFixed(1)}%</span>
						<span><i class="cv-dot cv-assistant"></i> Assistant {breakdown.assistant.pct.toFixed(1)}%</span>
						<span><i class="cv-dot cv-tools"></i> Tool Calls {breakdown.toolCalls.pct.toFixed(1)}%</span>
						<span><i class="cv-dot cv-other"></i> Other {breakdown.other.pct.toFixed(1)}%</span>
					</div>
				{:else}
					<div class="cv-bar cv-empty"></div>
					<div class="cv-empty-note">No message data available for breakdown</div>
				{/if}
			</div>

			<!-- Raw Messages -->
			<div class="cv-raw-messages">
				<div class="cv-section-label">Raw messages ({sessionDetail.messages?.length || 0})</div>
				{#if sessionDetail.messages && sessionDetail.messages.length > 0}
					<ul>
						{#each sessionDetail.messages as msg, i}
							<li>
								<span class="cv-role">{msg.role}</span>
								<span class="cv-msg-id">{msg.id || `msg_${i}`}</span>
								<span class="cv-timestamp">{formatShortDate(msg.timestamp)}</span>
							</li>
						{/each}
					</ul>
				{:else}
					<div class="cv-empty-note">No messages found in session data</div>
				{/if}
			</div>

			{#if sessionUsage.usage_percentage > 20}
				<button class="cv-compact-btn" onclick={handleCompact} disabled={compacting}>
					{compacting ? 'Compacting...' : 'Compact Session'}
				</button>
			{/if}
		{:else}
			<div class="cv-loading">Loading session data...</div>
		{/if}
	</div>
{:else}
	<Page scrollable={false} className="h-full p-0!">
		<div class="chat-container">
			<!-- Main chat area -->
			<div class="chat-area">
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

											{#if hasThinkingContent}
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
												{#if part.type === "text"}
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
											{:else if part.type === "tool-create_page" && (part as any).state === "output-available"}
												{@const output = (part as any).output}
												{#if output?.page_id}
													<PageEditResult
														type="page_created"
														title={output.title}
														pageId={output.page_id}
														onOpenPage={(id) => {
													if (!spaceStore.isSplit) {
														spaceStore.enableSplit();
													}
													spaceStore.openTabFromRoute(`/page/${id}`, { paneId: 'right' });
												}}
														onBindPage={handlePageSelect}
													/>
												{/if}
											{:else if part.type === "tool-edit_page" && (part as any).state === "output-available"}
												{@const output = (part as any).output}
												{#if output?.permission_needed}
													<!-- AI needs permission to edit this entity -->
													<PageBindingInline
														entityId={output.entity_id}
														entityType={output.entity_type}
														entityTitle={output.entity_title}
														message={output.message}
														proposedAction={output.proposed_action}
														permissionMode={true}
														onAllow={(id, type, title) => handlePermissionAllow(id, type, title, false)}
														onAllowForChat={(id, type, title) => handlePermissionAllow(id, type, title, true)}
														onDeny={() => handlePermissionDeny()}
													/>
												{:else if output?.needs_binding}
													<PageBindingInline
														entityId={output.page_id}
														entityTitle={output.page_title}
														message={output.message}
														onBind={handlePageSelect}
													/>
												{:else if output?.edit}
													<!-- Edit was applied server-side -->
													{@const editPageId = output.edit.page_id}
													{@const editId = output.edit.edit_id}
													{@const isPending = editId && editPageId ? isEditPending(editPageId, editId) : false}
													<EditDiffCard
														status={isPending ? 'pending' : 'accepted'}
														{editId}
														pageId={editPageId}
														find={output.edit.find || ''}
														replace={output.edit.replace || ''}
														isFullReplace={!output.edit.find}
														onViewPage={editPageId ? () => {
															// Enable split mode if not already, then open page in right pane
															if (!spaceStore.isSplit) {
																spaceStore.enableSplit();
															}
															spaceStore.openTabFromRoute(`/page/${editPageId}`, { paneId: 'right', forceNew: true });
														} : undefined}
														onAccept={isPending && editId && editPageId ? () => {
															// Accept: remove from pending (edit stays in document)
															acceptEdit(editPageId, editId);
															// Dispatch event to remove highlight in PageEditor
															dispatchAIEditAccept({ pageId: editPageId, editId });
														} : undefined}
														onReject={isPending && editId && editPageId ? () => {
															// Reject: revert the edit via API and remove highlight
															revertEdit(editPageId, editId);
														} : undefined}
													/>
												{/if}
											{:else if part.type === "tool-get_page_content" && (part as any).state === "output-available"}
												{@const output = (part as any).output}
												{#if output?.needs_binding}
													<PageBindingInline
														pageId={output.page_id}
														pageTitle={output.page_title}
														message={output.message}
														onBind={handlePageSelect}
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
										{:else}
											<UserMessage
												text={message.parts
													.filter(
														(p) =>
															p.type === "text",
													)
													.map((p) => p.text)
													.join("")}
											/>
										{/if}
									</div>
								</div>
							{/each}

							{#if showStandaloneThinking}
								<div class="flex justify-start">
									<div class="w-full">
										<ThinkingBlock
											{isThinking}
											toolCalls={[]}
											reasoningContent=""
											isStreaming={false}
											duration={thinkingDuration}
										/>
									</div>
								</div>
							{/if}

							{#if chat.error}
								{@const isRateLimitError =
									chat.error.message?.includes(
										"Rate limit exceeded",
									) ||
									chat.error.message?.includes(
										"rate limit",
									) ||
									chat.error.message?.includes("429")}
								<div class="flex justify-start">
									<div
										class="error-container"
										class:rate-limit-error={isRateLimitError}
									>
										<div class="error-icon">
											<Icon
												icon={isRateLimitError
													? "ri:time-line"
													: "ri:error-warning-line"}
												width="20"
											/>
										</div>
										<div class="error-content">
											<div class="error-title">
												{isRateLimitError
													? "Rate Limit Reached"
													: "An error occurred"}
											</div>
											<div class="error-message">
												{#if isRateLimitError}
													You've reached your API
													usage limit. Please wait for
													the limit to reset or check
													your usage dashboard for
													details.
												{:else}
													{chat.error.message ||
														"Something went wrong. Please try again."}
												{/if}
											</div>
											<div class="error-actions">
												{#if isRateLimitError}
													<a
														href="/usage"
														class="usage-link"
													>
														<Icon
															icon="ri:bar-chart-line"
															width="16"
														/>
														View Usage Dashboard
													</a>
												{:else}
													<button
														type="button"
														class="retry-button"
														onclick={() => {
															chat.regenerate();
														}}
													>
														<Icon
															icon="ri:refresh-line"
															width="16"
														/>
														Retry
													</button>
												{/if}
											</div>
										</div>
									</div>
								</div>
							{/if}
						</div>
					</div>

					<!-- ChatInput -->
					<div
						class="chat-input-wrapper"
						class:is-empty={isEmpty}
						class:has-messages={!isEmpty}
						class:transitions-enabled={enableTransitions}
						class:focused={inputFocused}
					>
						<ChatInput
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
							editableItems={editAllowListStore.items}
							pageBinding={activePageStore.boundPageId ? { pageId: activePageStore.boundPageId, pageTitle: activePageStore.boundPageTitle || 'Untitled' } : undefined}
							onPageChange={handlePageChangePage}
							onPageClear={handlePageClear}
							onRemoveItem={handleRemoveItem}
							onPageSelect={handlePageSelect}
							onSelectEntities={handleSelectEntities}
							on:submit={(e) => {
								if (chat.status === "ready") {
									handleChatSubmit(e.detail);
								}
							}}
							on:stop={() => handleChatStop()}
						/>

						{#if isEmpty}
							<GettingStarted
								onCreateChat={handleGettingStartedCreateChat}
								onFocusInput={handleGettingStartedFocusInput}
							/>
						{/if}
					</div>
				</div>
			</div>
		</div>
	</Page>

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

	/* Context View Styles */
	.context-view {
		height: 100%;
		overflow-y: auto;
		padding: 1.5rem;
		max-width: 600px;
	}

	.cv-loading,
	.cv-error {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.75rem;
		padding: 3rem;
		color: var(--color-foreground-muted);
	}

	.cv-error {
		color: var(--color-error);
	}

	.cv-error button {
		padding: 0.5rem 1rem;
		border: 1px solid var(--color-border);
		background: var(--color-surface);
		border-radius: 6px;
		cursor: pointer;
	}

	.info-grid {
		display: grid;
		grid-template-columns: 140px 1fr;
		gap: 0.5rem 1rem;
		margin: 0;
	}

	.info-grid dt {
		color: var(--color-foreground-muted);
		font-size: 0.875rem;
	}

	.info-grid dd {
		margin: 0;
		font-size: 0.875rem;
		color: var(--color-foreground);
	}

	.info-grid dd.title {
		font-weight: 500;
	}

	.info-grid dd.mono {
		font-family: var(--font-mono);
	}

	/* Breakdown bar */
	.cv-breakdown {
		margin-top: 2rem;
	}

	.cv-breakdown-label,
	.cv-section-label {
		font-size: 0.75rem;
		color: var(--color-foreground-muted);
		margin-bottom: 0.5rem;
	}

	.cv-bar {
		display: flex;
		height: 8px;
		border-radius: 4px;
		overflow: hidden;
		background: var(--color-surface-elevated);
	}

	.cv-segment {
		min-width: 2px;
	}

	.cv-segment.cv-user { background: #10b981; }
	.cv-segment.cv-assistant { background: #ec4899; }
	.cv-segment.cv-tools { background: #eab308; }
	.cv-segment.cv-other { background: #6b7280; }

	.cv-legend {
		display: flex;
		flex-wrap: wrap;
		gap: 1rem;
		margin-top: 0.5rem;
		font-size: 0.75rem;
		color: var(--color-foreground-muted);
	}

	.cv-dot {
		display: inline-block;
		width: 8px;
		height: 8px;
		border-radius: 50%;
		margin-right: 4px;
		vertical-align: middle;
	}

	.cv-dot.cv-user { background: #10b981; }
	.cv-dot.cv-assistant { background: #ec4899; }
	.cv-dot.cv-tools { background: #eab308; }
	.cv-dot.cv-other { background: #6b7280; }

	.cv-bar.cv-empty {
		background: var(--color-surface-elevated);
	}

	.cv-empty-note {
		font-size: 0.75rem;
		color: var(--color-foreground-muted);
		font-style: italic;
		margin-top: 0.5rem;
	}

	/* Raw messages */
	.cv-raw-messages {
		margin-top: 2rem;
	}

	.cv-raw-messages ul {
		list-style: none;
		padding: 0;
		margin: 0.5rem 0 0 0;
		max-height: 300px;
		overflow-y: auto;
	}

	.cv-raw-messages li {
		display: flex;
		gap: 0.75rem;
		padding: 0.375rem 0;
		font-size: 0.8125rem;
		border-bottom: 1px solid var(--color-border);
	}

	.cv-raw-messages li:last-child {
		border-bottom: none;
	}

	.cv-role {
		min-width: 70px;
		color: var(--color-foreground-muted);
	}

	.cv-msg-id {
		flex: 1;
		font-family: var(--font-mono);
		font-size: 0.75rem;
		color: var(--color-foreground-muted);
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.cv-timestamp {
		color: var(--color-foreground-muted);
		white-space: nowrap;
	}

	/* Compact button */
	.cv-compact-btn {
		margin-top: 2rem;
		padding: 0.5rem 1rem;
		background: transparent;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		font-size: 0.875rem;
		color: var(--color-foreground);
		cursor: pointer;
		transition: background-color 0.15s ease;
	}

	.cv-compact-btn:hover:not(:disabled) {
		background: var(--color-surface-hover);
	}

	.cv-compact-btn:disabled {
		opacity: 0.5;
		cursor: not-allowed;
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
		/* Add padding to keep scrollbar away from resize handle */
		padding-right: 8px;
	}

	.chat-layout.visible {
		opacity: 1;
		pointer-events: auto;
	}

	.chat-layout::-webkit-scrollbar {
		width: 6px;
	}

	.chat-layout::-webkit-scrollbar-track {
		background: transparent;
	}

	.chat-layout::-webkit-scrollbar-thumb {
		background-color: var(--color-border);
		border-radius: 3px;
	}

	.chat-layout::-webkit-scrollbar-thumb:hover {
		background-color: var(--color-border-strong);
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
		background: var(--color-background);
		box-sizing: border-box;
		z-index: 10;
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

	/* Assistant response text - spacing after thinking block */
	.assistant-response {
		padding-top: 4px;
	}

	.error-container {
		display: flex;
		gap: 0.75rem;
		padding: 1rem;
		background-color: var(--color-error-subtle);
		border: 1px solid var(--color-error);
		border-radius: 0.5rem;
		width: 100%;
		max-width: 100%;
	}

	.error-icon {
		flex-shrink: 0;
		color: var(--color-error);
		margin-top: 0.125rem;
	}

	.error-content {
		flex: 1;
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}

	.error-title {
		font-weight: 600;
		color: var(--color-error);
		font-size: 0.875rem;
	}

	.error-message {
		color: var(--color-foreground-muted);
		font-size: 0.875rem;
		line-height: 1.5;
	}

	.error-container.rate-limit-error {
		background-color: var(--color-warning-subtle);
		border-color: var(--color-warning);
	}

	.error-container.rate-limit-error .error-icon {
		color: var(--color-warning);
	}

	.error-container.rate-limit-error .error-title {
		color: var(--color-warning);
	}

	.error-actions {
		margin-top: 0.5rem;
	}

	.retry-button {
		display: inline-flex;
		align-items: center;
		gap: 0.375rem;
		padding: 0.375rem 0.75rem;
		background: color-mix(in srgb, var(--color-error) 15%, transparent);
		border: 1px solid color-mix(in srgb, var(--color-error) 30%, transparent);
		border-radius: 0.375rem;
		color: var(--color-error);
		font-size: 0.875rem;
		font-weight: 500;
		cursor: pointer;
		transition: all 0.15s ease;
	}

	.retry-button:hover {
		background: var(--color-error);
		color: white;
	}

	.retry-button:active {
		transform: scale(0.98);
	}

	.usage-link {
		display: inline-flex;
		align-items: center;
		gap: 0.375rem;
		padding: 0.375rem 0.75rem;
		background-color: var(--color-surface);
		border: 1px solid var(--color-warning);
		border-radius: 0.375rem;
		color: var(--color-warning);
		font-size: 0.875rem;
		font-weight: 500;
		text-decoration: none;
		cursor: pointer;
		transition: all 0.15s ease;
	}

	.usage-link:hover {
		background-color: var(--color-warning-subtle);
	}

	.usage-link:active {
		transform: scale(0.98);
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
