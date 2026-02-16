<script lang="ts">
	import type { Tab } from "$lib/tabs/types";
	import { spaceStore } from "$lib/stores/space.svelte";
	import Page from "$lib/components/Page.svelte";
	import ChatInput from "$lib/components/ChatInput.svelte";
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
	import { onMount, onDestroy, tick } from "svelte";
	import { chatSessions } from "$lib/stores/chatSessions.svelte";
	import { chatInstances } from "$lib/stores/chatInstances.svelte";
	import type { Chat } from "@ai-sdk/svelte";
	// Active page editing imports
	import { editAllowListStore } from "$lib/stores/editAllowList.svelte";
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
		editAllowListStore.remove(type as 'page' | 'folder' | 'wiki_entry', id);
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
			await editAllowListStore.add({
				type: (entityType === 'folder' ? 'folder' : 'page') as 'page' | 'folder',
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

		if (!spaceStore.isSplit) {
			spaceStore.enableSplit();
		}
		spaceStore.openTabFromRoute(`/page/${pageId}`, { paneId: 'right' });
		spaceStore.refreshViews();
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
		const currentPane = spaceStore.findTabPane(tab.id);
		spaceStore.openChatContext(conversationId, currentPane);
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
	let selectedAgentMode = $state<AgentModeId>('agent');
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
					// Ensure chat is marked as created (may already be done above if hasItems)
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
</script>

{#if !chat}
	<!-- wait for chat to initialize -->
{:else if isContextView}
	<ContextViewPanel {conversationId} {active} onCompacted={handleCompacted} />
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
														onAllow={(id, type, title) => handlePermissionAllow(id, type, title)}
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
													{@const editPageId = output.edit.page_id}
													<EditDiffCard
														status={output.applied ? 'applied' : 'failed'}
														pageId={editPageId}
														find={output.edit.find || ''}
														replace={output.edit.replace || ''}
														isFullReplace={!output.edit.find}
														onViewPage={editPageId ? () => {
															if (!spaceStore.isSplit) {
																spaceStore.enableSplit();
															}
															spaceStore.openTabFromRoute(`/page/${editPageId}`, { paneId: 'right', forceNew: true });
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
							pageBinding={getBoundPage() ? { pageId: getBoundPage()!.id, pageTitle: getBoundPage()!.title || 'Untitled' } : undefined}
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
