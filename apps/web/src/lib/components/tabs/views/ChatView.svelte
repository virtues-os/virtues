<script lang="ts">
	import type { Tab } from "$lib/tabs/types";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import ChatInput from "$lib/components/ChatInput.svelte";
	import MediaLightbox from "$lib/components/MediaLightbox.svelte";
	import { getInitializationPromise } from "$lib/stores/models.svelte";
	import Markdown from "$lib/components/Markdown.svelte";
	import StoppedNotice from "$lib/components/StoppedNotice.svelte";
	import Icon from "$lib/components/Icon.svelte";
	import SelectionPopover from "$lib/components/SelectionPopover.svelte";
	import ContextIndicator from "$lib/components/ContextIndicator.svelte";

	// ── the controller ─────────────────────────────────────────────────────
	// This view is markup plus thin bindings; the state it renders lives in
	// $lib/components/chat/state, one module per responsibility. Each says at
	// its head what it owns.
	import {
		generateHex16,
		extractConversationId,
		isContextViewRoute,
		isNewChat,
		isTemporaryRoute,
	} from "$lib/components/chat/state/chatRoute";
	import {
		type MessageMeta,
		toUiMessage as toUiMessageWith,
		deduplicateMessages,
		isSettledLine,
		introductionsRecorded,
		eyebrowsFor,
	} from "$lib/components/chat/state/transcript";
	import {
		AttachmentsController,
		formatFileSize,
	} from "$lib/components/chat/state/attachments.svelte";
	import { StagedRefsController } from "$lib/components/chat/state/stagedRefs.svelte";
	import { ModelChoiceController } from "$lib/components/chat/state/modelChoice.svelte";
	import { OpeningRevealController } from "$lib/components/chat/state/openingReveal.svelte";
	import { ToolSideEffects } from "$lib/components/chat/state/toolSideEffects";
	import { observeComposerReserve } from "$lib/components/chat/state/composerReserve";
	import { readDraft, writeDraft, NEW_CHAT_DRAFT_ID } from "$lib/components/chat/state/drafts";

	// ── the narrative interview ────────────────────────────────────────────
	// The one chat that is not a chat. Its substance — the id, the authored
	// opening, the close detection, the resident bot — lives in
	// $lib/components/chat/interview; this view keeps only the branches.
	import {
		INTERVIEW_CHAT_ID,
		INTERVIEW_OPENING_ID,
		applyInterviewOpening,
		findWriteItUpOutput,
	} from "$lib/components/chat/interview/interview";
	import Composing from "$lib/components/chat/Composing.svelte";
	import Trivet from "$lib/components/chat/Trivet.svelte";
	// Getting started — the room after the founder's letter. Same shape as
	// the interview: the id decides everything, the top of the room is
	// synthetic and rebuilt from derived state, the cards do the work.
	import {
		GS_INTERVIEW_OPENING_ID,
		SKIP_COMMAND,
		isGettingStartedChat,
		applyInterviewOpening as applyRoomInterviewOpening,
	} from "$lib/components/chat/getting-started/getting-started";
	import RoomControls from "$lib/components/chat/getting-started/RoomControls.svelte";
	import GettingStartedDoor from "$lib/components/chat/getting-started/GettingStartedDoor.svelte";
	import RoomCover from "$lib/components/chat/getting-started/RoomCover.svelte";
	import StepEyebrow from "$lib/components/chat/getting-started/StepEyebrow.svelte";
	import IntroductionsRecorded from "$lib/components/chat/getting-started/IntroductionsRecorded.svelte";
	import GraduatedDoors from "$lib/components/chat/getting-started/GraduatedDoors.svelte";
	import { gettingStarted } from "$lib/stores/gettingStarted.svelte";
	import ChapterLifelineLive from "$lib/components/chat/interview/ChapterLifelineLive.svelte";
	import { CitationPanel } from "$lib/components/citations";
	import { buildCitationContextFromParts } from "$lib/citations";
	import type { Citation } from "$lib/types/Citation";
	import UserMessage from "$lib/components/UserMessage.svelte";
	import ThinkingBlock from "$lib/components/ThinkingBlock.svelte";
	import SubagentPanel from "$lib/components/SubagentPanel.svelte";
	import { onMount, onDestroy, tick, untrack } from "svelte";
	import { goto } from "$app/navigation";
	import { fade, fly } from "svelte/transition";
	import { cubicInOut } from "svelte/easing";
	import { chatSessions } from "$lib/stores/chatSessions.svelte";
	import { mobileLayout } from "$lib/stores/mobileLayout.svelte";
	import { chatInstances } from "$lib/stores/chatInstances.svelte";
	import { pendingPrompt } from "$lib/stores/pendingPrompt.svelte";
	import {
		deleteChat,
		getChat,
		getChatUsage,
		getAssistantProfile,
		getProfile,
		setChatTitle,
		cancelChat,
	} from "$lib/api/client";
	import { contextMenu, type ContextMenuItem } from "$lib/stores/contextMenu.svelte";
	import type { Chat } from "@ai-sdk/svelte";
	// Active page editing imports
	import { editAllowListStore } from "$lib/stores/editAllowList.svelte";
	import {
		activePageContext,
		bindPage,
		grantEditPermission,
		openCreatedPage,
	} from "$lib/components/chat/state/pageBinding";
	import PageBindingInline from "$lib/components/chat/PageBindingInline.svelte";
	import ChapterLifeline from "$lib/components/chat/interview/ChapterLifeline.svelte";
	import PageEditResult from "$lib/components/chat/PageEditResult.svelte";
	import EditDiffCard from "$lib/components/chat/EditDiffCard.svelte";
	import InterviewClosedCard from "$lib/components/chat/interview/InterviewClosedCard.svelte";
	import { setupStateStore } from "$lib/stores/setupState.svelte";
	import CodeInterpreterCard from "$lib/components/chat/CodeInterpreterCard.svelte";
	import AppletProposalCard from '$lib/components/chat/AppletProposalCard.svelte';
	import CompactionCheckpoint from "$lib/components/chat/CompactionCheckpoint.svelte";
	import ContextViewPanel from "$lib/components/chat/ContextViewPanel.svelte";
	import { ChatError } from "$lib/components/chat";
	import type { AgentModeId } from "$lib/config/agentModes";

	// Props
	let { tab, active }: { tab: Tab; active: boolean } = $props();

	// Derived: are we showing the context panel?
	const isContextView = $derived(isContextViewRoute(tab.route));

	// svelte-ignore state_referenced_locally
	let isGhost = $state(isTemporaryRoute(tab.route));

	// Capture initial conversationId from tab prop (intentionally captures initial value only)
	// svelte-ignore state_referenced_locally
	const initialConversationId = extractConversationId(tab.route);

	// UI state
	let conversationId = $state(initialConversationId || `chat_${generateHex16()}`);
	let scrollContainer: HTMLDivElement | null = $state(null);
	// The composer is absolutely positioned OVER the scroller, so the transcript
	// has to reserve its height itself — see composerReserve, which the effect
	// below hands the two elements to.
	let composerEl: HTMLDivElement | null = $state(null);
	let enableTransitions = $state(false);
	// A NEW chat has nothing to load. Starting this at a blanket `true` meant
	// the composer painted docked at the bottom for one frame and then jumped
	// to its centered empty-state position once the tab effect flipped it —
	// with transitions still disabled, that jump was the launch flicker. The
	// route is known synchronously, so loading starts true only when there is
	// actually a conversation to fetch. Initial value only, on purpose — the
	// tab-change effect below owns every later transition.
	// svelte-ignore state_referenced_locally
	let isLoading = $state(!isNewChat(tab.route));
	let isAwaitingResponse = $state(false);
	// Track C: messages typed while the assistant is still streaming are queued
	// and sent automatically when the turn finishes (Cursor-style chips above the
	// composer). Local to the view — a tab drag-away mid-queue is an accepted edge.
	let queuedMessages = $state<string[]>([]);

	// Track D: highlight-to-reference (see state/stagedRefs).
	const refs = new StagedRefsController();

	// Repaint whenever the staged set changes.
	$effect(() => {
		void refs.staged;
		refs.repaint();
	});

	// Track E1: multimodal attachments (see state/attachments).
	const attachments = new AttachmentsController();

	// What the picker shows, what goes on the wire, and the capability gate
	// that judges the staged attachments against it (see state/modelChoice).
	const models = new ModelChoiceController(() => attachments.items);

	function switchToRecommendedAndRetry() {
		models.switchToRecommended();
		chat.regenerate();
	}

	// Click an in-message image to open it in a shared-element lightbox.
	let lightbox = $state<{ src: string; alt: string; rect: DOMRect } | null>(null);
	function openLightbox(e: MouseEvent, src: string, alt: string) {
		const el = e.currentTarget as HTMLImageElement;
		lightbox = { src, alt, rect: el.getBoundingClientRect() };
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
	let messageMetadata = $state<Map<string, MessageMeta>>(new Map());

	/** The converter, bound to this view's metadata map. */
	function toUiMessage(msg: any) {
		return toUiMessageWith(msg, messageMetadata);
	}

	// Citation panel state
	let citationPanelOpen = $state(false);
	let selectedCitation = $state<Citation | null>(null);

	// The Notebook (room) this chat lives in — at most one. Its id is sent with
	// each message (drives the agent's active-space context + server-side
	// binding). Read-only here now: the picker that used to set it from this
	// view is gone, so the binding is seeded from the session row and changed
	// where the filing happens — in the notebook.
	let chatNotebookId = $state<string | null>(null);
	// Which conversation chatNotebookId was seeded for. Seeding happens ONCE per
	// conversation (when its session row is available, or once the session list
	// has finished loading and confirms there's no row yet) so a later session
	// refresh can never clobber a room the user just picked locally.
	let seededNotebookFor = $state<string | null>(null);

	$effect(() => {
		const id = conversationId;
		if (seededNotebookFor === id) return;
		const session = chatSessions.sessions.find((s) => s.conversation_id === id);
		if (session) {
			chatNotebookId = session.notebook_id ?? null;
			seededNotebookFor = id;
		} else if (!chatSessions.isLoading) {
			// Sessions are loaded and this chat has no row yet (brand-new, not yet
			// persisted) — start unfiled; the create path binds it from the first
			// message's notebookId.
			chatNotebookId = null;
			seededNotebookFor = id;
		}
	});

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

	/**
	 * Handle permission allow for AI edit.
	 * Adds permission then regenerates the AI's last response (which had permission_needed).
	 * regenerate() removes that assistant message and re-requests — no duplicate user messages.
	 */
	async function handlePermissionAllow(entityId: string, entityType: string, title: string) {
		// Await ensures the backend has the permission before the retry.
		await grantEditPermission(entityId, entityType, title);

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

	// The two tool results that reach outside the transcript — create_page
	// opens a page beside the chat, edit_page animates one (see
	// state/toolSideEffects for why both seed before they act).
	const tools = new ToolSideEffects();

	// Effect to handle create_page side effects (auto-open new pages)
	// Only triggers for pages created during this session, not when reopening old chats
	$effect(() => {
		if (!chat?.messages) return;

		// Don't auto-open during initial load - wait until loading is complete
		if (isLoading) return;

		for (const page of tools.collectNewPages(chat.messages)) {
			openCreatedPage(page.pageId, page.title);
		}
	});

	// (The interview's write_it_up auto-open lives in chatInstances.onData —
	// the backend sends a transient data-narrative-document part, because tool
	// parts land in the messages array mutably where no effect observes them.)

	// Effect to drive the AI presence animation when a chat `edit_page` lands.
	$effect(() => {
		if (!chat?.messages || isLoading) return;
		tools.animateNewEdits(chat.messages);
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
			const data = await getChatUsage<{
				usage_percentage: number;
				total_tokens: number;
				context_window: number;
			}>(conversationId);
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

	/** Re-read the stored transcript. Used after a compaction, and after the
	 *  getting-started room speaks (its lines are appended server-side, so
	 *  the thread has to be re-read to show them). */
	async function reloadMessages() {
		if (!conversationId) return;
		try {
			const data = await getChat<{ messages?: any[] }>(conversationId);
			loadedMessages = data.messages || [];
			chat.messages = deduplicateMessages(loadedMessages).map(
				toUiMessage,
			) as unknown as typeof chat.messages;
			// Re-reading drops the interview's opening, which is shown rather
			// than stored; put it back where it belongs.
			applyRoomInterviewOpening(chat, conversationId, gettingStarted.state);
		} catch {
			// Non-critical refresh — leave the current messages in place on failure.
		}
	}
	const handleCompacted = reloadMessages;

	// A chat that opens with your message last and no reply may be a turn the
	// box is still running (VIR-323): a turn outlives its request now, so ask
	// for its live stream. The SDK replays what was said and follows the rest;
	// a 204 means nothing is running and the load stands as it is. Never for a
	// ghost: its transcript lives in this tab and nowhere the box could resume.
	function resumeIfDangling() {
		if (isGhost) return;
		const last = chat.messages[chat.messages.length - 1];
		if (!last || last.role !== "user") return;
		if (chat.status !== "ready") return;
		void chat.resumeStream().catch((e: unknown) => {
			console.warn("[ChatView] could not rejoin the running turn:", e);
		});
	}

	// Chat instance - fetched from shared store to survive remounts
	let chat = $state<Chat>(null!);
	let currentChatConversationId = $state<string | null>(null);
	// The interview's chrome (no thinking block, the resident companion)
	// applies in the old standalone room and in the getting-started room
	// while the interview is underway there.
	// Setup and the interview are both rooms where the machine's workings are
	// not the subject: no thinking block, no tool names.
	const inInterview = $derived(
		currentChatConversationId === INTERVIEW_CHAT_ID || isGettingStartedChat(currentChatConversationId),
	);

	// Getter for the chat's Notebook (room) ID — sent with each message so the agent
	// gets the active-space context block and the server keeps the binding fresh.
	function getNotebookId(): string | null {
		return chatNotebookId;
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
				getModel: () => models.idForWire(),
				getNotebookId,
				getActivePageContext: activePageContext,
				getPersona: () => selectedPersona,
				getAgentMode: () => selectedAgentMode,
				getChatMode: () => chatMode,
				getTemporary: () => isGhost,
			});
			// The draft this conversation left behind, if the composer is empty.
			if (!isGhost && !input) {
				const draft = readDraft(draftId);
				if (draft) input = draft;
			}
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
			tools.reset();
			// NOTE: We no longer unbind the active page when switching chats.
			// Binding is now additive/persistent to the chat session context.
			// clearBoundPages();

			// Load conversation if switching to an existing one
			if (currentTabConversationId && !isNewChat(currentTabRoute)) {
				isLoading = true;
				(async () => {
					try {
						// Raw fetch (not getChat): this load carries an AbortSignal so
						// switching tabs mid-load cancels it. The client wrapper has no
						// signal channel, so this site stays on fetch by design.
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
							).map(toUiMessage) as unknown as typeof chat.messages;
							resumeIfDangling();
							applyInterviewOpening(chat, currentTabConversationId);
							applyRoomInterviewOpening(chat, currentTabConversationId, gettingStarted.state);
							// The picker is deliberately left alone on a tab
							// switch. It used to be re-seeded from the model
							// that last answered THIS conversation, which is
							// neither the person's choice nor what the next
							// turn will use — a chat is not pinned to a model.
							// Leaving it holds their pick across chats, and an
							// unpicked picker keeps showing the slot default.
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
							// Scroll to bottom after loading existing chat — but
							// the getting-started room opens on its cover.
							setTimeout(() => {
								if (!openAtStart()) scrollToBottom("instant");
							}, 10);
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
		// (The notebook list used to be fetched here for the breadcrumb's name
		// and accent. The app layout already loads it, and nothing in this view
		// renders a notebook's name any more.)

		// Claim any prompt handed off from Home / ⌘K / "Ask this notebook"
		// (consume-once, synchronously — so only this freshly-opened chat sends it).
		const initialPrompt = pendingPrompt.take();
		// If the ask came from a notebook, bind this new chat to it before the
		// first message so the create path files it + grounds retrieval there.
		const seededNotebook = pendingPrompt.takeNotebook();
		if (seededNotebook) {
			chatNotebookId = seededNotebook;
			seededNotebookFor = conversationId;
		}
		(async () => {
			// Stage 1: Models must load first (other code depends on model list)
			await getInitializationPromise();

			// Stage 2: Profile fetches + conversation load in parallel (independent)
			const tabConversationId = extractConversationId(tab.route);

			let profileDefaultModelId: string | undefined;
			let profileDefaultPersona: string | undefined;

			const profilePromise = (async () => {
				try {
					const profile = await getAssistantProfile<{
						ui_preferences?: Record<string, unknown>;
						chat_model_id?: string;
						persona?: string;
					}>();
					if (profile.ui_preferences) {
						uiPreferences = profile.ui_preferences;
					}
					profileDefaultModelId = profile.chat_model_id;
					profileDefaultPersona = profile.persona;
				} catch (error) {
					console.error("Failed to load assistant profile:", error);
				}
			})();

			const namePromise = (async () => {
				try {
					const profile = await getProfile();
					preferredName = profile.preferred_name ?? undefined;
					onboardingStatus = profile.onboarding_status || 'active';
				} catch {
					// Non-critical, continue without preferred name
				}
			})();

			const conversationPromise = tabConversationId ? (async () => {
				try {
					const data = await getChat<{
						messages?: any[];
						conversation?: { model?: string };
					}>(tabConversationId);
					loadedMessages = data.messages || [];
					chat.messages = deduplicateMessages(loadedMessages).map(
						toUiMessage,
					) as unknown as typeof chat.messages;
					resumeIfDangling();
				} catch (error) {
					console.error("[ChatView] Error loading conversation:", error);
				}
			})() : null;

			await Promise.all([profilePromise, namePromise, conversationPromise]);

			// After the load, not inside it: a failed fetch must still leave
			// the interview speaking rather than showing a blank room.
			applyInterviewOpening(chat, tabConversationId);
			applyRoomInterviewOpening(chat, tabConversationId, gettingStarted.state);

			// What the picker SHOWS, for every chat old or new: the owner's
			// standing preference, else the Virtues default. Deliberately not
			// the model that last answered this conversation — a chat is not
			// pinned to the model it opened with, so showing the last one
			// would name a model the next turn may not use.
			models.prefillDisplay(profileDefaultModelId);

			// Stage 3: Post-load tasks (depend on conversation being loaded)
			if (tabConversationId) {
				await Promise.all([
					refreshContextUsage(),
					editAllowListStore.init(tabConversationId),
				]);
			} else {
				// New chat - set defaults from profile
				editAllowListStore.setChatId(conversationId);
				if (profileDefaultPersona) {
					selectedPersona = profileDefaultPersona;
				}
			}

			isLoading = false;
			setTimeout(() => {
				if (!openAtStart()) scrollToBottom("instant");
				enableTransitions = true;
			}, 50);

			// Auto-send the handed-off prompt on a brand-new chat. handleChatSubmit
			// queues internally if the instance isn't "ready" yet, so this is safe.
			if (initialPrompt && isNewChat(tab.route)) {
				handleChatSubmit(initialPrompt);
			}

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
		refs.clear();
	});

	// Derive thinking state from chat status
	const isThinking = $derived.by(() => {
		const status = chat?.status;
		return status === "submitted" || status === "streaming";
	});

	// Deduplicated messages for rendering
	const uniqueMessages = $derived(chat?.messages ? deduplicateMessages(chat.messages) : []);
	/** The interview's opening plate is in this thread — mid-thread here,
	 *  not first as in the old standalone room — so the container must not
	 *  clip paint at its edge. Without this the plate lost both ends. */
	const roomHoldsPlate = $derived(uniqueMessages.some((m) => m.id === GS_INTERVIEW_OPENING_ID));

	/** The one line per step that carries its number — keyed by message id. */
	const eyebrowFor = $derived(
		eyebrowsFor(uniqueMessages as { id: string; subject?: string }[]),
	);


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

	// Draft persistence — see state/drafts for why an unsent chat shares one key.
	const draftId = $derived(extractConversationId(tab.route) ?? NEW_CHAT_DRAFT_ID);
	let draftTimer: ReturnType<typeof setTimeout> | null = null;
	$effect(() => {
		const id = draftId;
		const text = input;
		if (isGhost) return;
		if (draftTimer) clearTimeout(draftTimer);
		draftTimer = setTimeout(() => {
			draftTimer = null;
			writeDraft(id, text);
		}, 250);
	});
	onDestroy(() => {
		// A tab closed inside the debounce window still keeps its draft.
		if (draftTimer) {
			clearTimeout(draftTimer);
			draftTimer = null;
			if (!isGhost) writeDraft(draftId, input);
		}
	});

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

	// Agent mode and persona selection state - used for tool filtering on backend
	let selectedAgentMode = $state<AgentModeId>('chat');
	let selectedPersona = $state<string>('default');

	// Retrieval scope. 'scoped' (grounded in a notebook's items only) still
	// exists on the wire and in the retriever — what's gone is the pill above
	// the composer that switched it, which was a permanent piece of chrome for
	// a setting almost nobody moved. Every chat is 'open': the whole graph,
	// with the notebook up-weighted when there is one. If scoped comes back it
	// belongs somewhere it can be explained, not as a two-state word.
	const chatMode = 'open' as const;

	// Sync selected model with store (only on initial load). Still a prefill,
	// not a choice — record it as one.
	$effect(() => {
		models.adoptStoreSelection();
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
			}, 300000); // 5 minutes: the box's stream has no total timeout any more, only a 300s idle one

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

	// The interview's close. Three witnesses, any one suffices: the tool
	// result in this session (the transient data part — see chatInstances),
	// a write_it_up part in the loaded transcript (a reload after the close),
	// or the box saying the document stands (the HTTP path wrote it, or the
	// transcript's tool part didn't survive). Once closed, the composer
	// retires: the drafter runs once, so a message typed here now would reach
	// nothing — the page is where corrections go.
	// The getting-started room re-renders its top whenever the derived
	// state changes (a source lands, the interview closes elsewhere, a skip).
	// `untrack` on the transcript: the rebuild assigns it, and reading it
	// tracked would re-run this effect on its own write.
	$effect(() => {
		const state = gettingStarted.state;
		const convId = currentChatConversationId;
		if (!isGettingStartedChat(convId)) return;
		// Never while a turn is streaming: the transcript is the SDK's to
		// write then. Reading `status` here re-runs this once it settles.
		if (chat.status !== "ready") return;
		untrack(() => applyRoomInterviewOpening(chat, convId, state));
	});
	$effect(() => {
		if (isGettingStartedChat(currentChatConversationId)) gettingStarted.start();
	});

	/** A turn just settled in the room, so ask the box where the walk stands.
	 *
	 *  The server narrates the coda the moment the assistant's turn is on
	 *  disk (see api/chat.rs), but the CLIENT only learns a step moved on the
	 *  store's 30-second poll — so the ending sat unread while the person
	 *  typed their next message over the top of it. Asking on `ready` closes
	 *  that window: the answer moves the walk signature, and the effect below
	 *  re-reads the thread with the new lines in it. */
	let lastTurnStatus = $state<string | null>(null);
	$effect(() => {
		const status = chat.status;
		const wasStreaming = lastTurnStatus === "streaming" || lastTurnStatus === "submitted";
		lastTurnStatus = status;
		if (!isGettingStartedChat(currentChatConversationId)) return;
		if (status !== "ready" || !wasStreaming) return;
		untrack(() => void gettingStarted.refresh());
	});

	// The getting-started interview's opening, revealed as a turn arrives
	// (see state/openingReveal).
	const reveal = new OpeningRevealController(() => chat?.messages ?? []);
	$effect(() => {
		if (!gettingStarted.revealOpening) return;
		if (!reveal.hasOpening()) return;
		gettingStarted.revealOpening = false;
		untrack(() => reveal.start());
	});
	onDestroy(() => reveal.stop());

	/** The room speaks server-side, so when its state moves the thread has
	 *  new lines in it. Re-read on any change of the walk — the step
	 *  statuses and the interview's start are the whole of it. */
	let lastWalk = $state<string | null>(null);
	$effect(() => {
		const st = gettingStarted.state;
		if (!isGettingStartedChat(currentChatConversationId) || !st) return;
		const walk =
			st.steps.map((x) => `${x.id}:${x.status}`).join("|") + `|${st.interview_started_at ?? ""}`;
		if (lastWalk === null) {
			lastWalk = walk;
			return;
		}
		if (lastWalk === walk) return;
		lastWalk = walk;
		if (chat.status !== "ready") return;
		untrack(() => void reloadMessages());
	});

	const interviewClosedPart = $derived(
		currentChatConversationId === INTERVIEW_CHAT_ID ? findWriteItUpOutput(uniqueMessages) : null,
	);
	const interviewClosed = $derived(
		currentChatConversationId === INTERVIEW_CHAT_ID &&
			(chatInstances.narrativeDocumentPageId !== null ||
				interviewClosedPart !== null ||
				setupStateStore.done("narrative_identity_ready")),
	);
	const interviewDocumentPageId = $derived(
		interviewClosedPart?.document_page_id ?? chatInstances.narrativeDocumentPageId,
	);


	// The chat's title, from the persisted session so it stays in step with the
	// sidebar. It is no longer DRAWN here: a title fixed to the top-left of the
	// pane restated what the tab above it already said, and doubled as a rename
	// affordance nobody looked for. Renaming lives where the name is shown —
	// right-click the tab, or the sidebar row.
	const chatTitle = $derived(
		chatSessions.sessions.find((s) => s.conversation_id === conversationId)?.title ?? "",
	);

	// Adopt the stored title into the tab label.
	//
	// The route registry stamps a new chat tab "Chat" because at parse time the
	// title isn't known — it lives in the session list, which may not have
	// loaded yet. Openers that HAVE a title (the Desk, Home, chat history) pass
	// it along, so this only bites on the paths that don't: a deep link, a
	// restored tab, "open beside". The label then stayed "Chat" forever, which
	// was survivable while the pane also printed the title and is not now.
	//
	// Placeholders only — a label the user has renamed by hand must not be
	// overwritten by the server's copy.
	const PLACEHOLDER_LABELS = new Set(["Chat", "New Chat", "Temporary Chat"]);
	$effect(() => {
		if (!chatTitle || !tab) return;
		if (!PLACEHOLDER_LABELS.has(tab.label)) return;
		// Never write a label that is already there. This effect READS
		// `tab.label` and WRITES it, so it is only safe while every write
		// changes the value — and the guard above assumed a saved title is
		// never itself a placeholder. A chat actually titled "New Chat" breaks
		// that assumption: the write lands, `updatePane` hands back a fresh
		// `panes` array, the effect re-runs on the same placeholder and writes
		// again. The result is `effect_update_depth_exceeded`, which takes the
		// whole window's reactivity down, and it reads as a sidebar bug because
		// WindowTabBar's DnD effect rebuilds on every `panes` change and lands
		// on top of the stack trace.
		if (tab.label === chatTitle) return;
		windowShellStore.updateTab(tab.id, { label: chatTitle });
	});

	// A real, saved chat the user can act on (not the empty new-chat state, not a ghost).
	// Getting started has no menu: it cannot be deleted or renamed, and its
	// header holds one control, the door.
	const canManageChat = $derived(!isEmpty && !isGhost && !isGettingStartedChat(currentChatConversationId));

	async function deleteThisChat() {
		try {
			windowShellStore.closeTabsByRoute(`/chat/${conversationId}`);
			await deleteChat(conversationId);
			chatSessions.remove(conversationId);
			windowShellStore.invalidateViewCache("chat");
		} catch (e) {
			console.error("[ChatView] Failed to delete chat:", e);
		}
	}

	function openChatMenu(e: MouseEvent) {
		const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
		const pinned = !!windowShellStore.findTab((t) => t.id === tab.id)?.tab.pinned;
		const items: ContextMenuItem[] = [
			{
				id: "pin",
				label: pinned ? "Unpin tab" : "Pin tab",
				icon: pinned ? "ri:unpin-line" : "ri:pushpin-line",
				action: () => windowShellStore.togglePin(tab.id),
			},
			{
				id: "delete",
				label: "Delete chat",
				icon: "ri:delete-bin-line",
				variant: "destructive",
				dividerBefore: true,
				action: deleteThisChat,
			},
		];
		contextMenu.show(
			{ x: rect.right, y: rect.bottom },
			items,
			{
				anchor: { x: rect.left, y: rect.top, width: rect.width, height: rect.height },
				placement: "bottom-end",
			},
		);
	}

	// Generate title after first assistant response
	async function generateTitle() {
		if (titleGenerated || chat.messages.length < 2) return;
		// The interview keeps the name it was seeded with. Its transcript is
		// the most private text on the box, and a generated title puts a
		// summary of it in the sidebar — this chat had renamed itself after
		// the person's own childhood. The server refuses this too (the id
		// decides, never the client); this only saves the round trip.
		if (conversationId === INTERVIEW_CHAT_ID || isGettingStartedChat(conversationId)) {
			titleGenerated = true;
			return;
		}

		try {
			const data = await setChatTitle<{ title?: string }>({
				chatId: conversationId,
				messages: chat.messages.map((m) => ({
					role: m.role,
					content: m.parts.find((p) => p.type === "text")?.text || "",
				})),
			});

			// Only mark done once we actually have a title, so an ok-but-empty
			// response retries on the next turn instead of giving up silently.
			if (data.title) {
				titleGenerated = true;
				windowShellStore.updateTab(tab.id, { label: data.title });
				// Optimistically seed the shared session store so the header
				// breadcrumb (and any store-bound surface) updates immediately,
				// without waiting on the server-persist → refetch round-trip.
				chatSessions.applyTitle(conversationId, data.title);
			}
		} catch (error) {
			// Title generation is non-critical
		}
	}

	$effect(() => {
		const composer = composerEl;
		const scroller = scrollContainer;
		if (!composer || !scroller) return;
		return observeComposerReserve(composer, scroller);
	});

	/**
	 * Where a room opens. A chat opens at its newest message; the
	 * getting-started room opens at the TOP, on the cover — the painting is
	 * the first thing in it and the first thing anyone should see, and on a
	 * short window the walk's own length was starting it half scrolled off.
	 * Only on load: once the person is in the conversation, new turns pull
	 * the view down as they do anywhere else.
	 */
	function openAtStart(behavior: ScrollBehavior = "instant") {
		if (isGettingStartedChat(currentChatConversationId)) {
			scrollContainer?.scrollTo({ top: 0, behavior });
			return true;
		}
		return false;
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
			await cancelChat(conversationId);
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

		// The one slash command, and it does exactly what the door does: leave.
		// It used to also skip connect_ai, which was the key out of a locked
		// app; nothing is locked now, so the room keeps its place and you come
		// back to it from the sidebar. Deterministic and client-side; the model
		// never sees it. Any other slash text sends.
		if (messageToSend === SKIP_COMMAND) {
			input = "";
			void goto("/home");
			return;
		}

		// Track D: prepend any staged highlight references as quoted context +
		// comments. Only present on a direct send (cleared before queue-drain).
		if (refs.staged.length > 0) {
			const refsBlock = refs.serialize();
			messageToSend = refsBlock + (messageToSend ? `\n\n${messageToSend}` : "");
			refs.clear();
		}

		// Track E1: block sending if an attachment isn't supported by the active
		// model — the capability banner prompts a switch instead.
		if (models.capabilityIssue) return;

		if (!messageToSend && attachments.count === 0) return;

		if (chat.status !== "ready") {
			// Queue text; attachments stay staged and ride along when the drain
			// effect re-sends this once the current turn finishes.
			queuedMessages = [...queuedMessages, messageToSend];
			input = "";
			return;
		}
		input = "";

		// Capture + clear attachments as AI SDK file parts.
		const files = attachments.takeAsFileParts();

		// New turn → clear any leftover Deep Research panel from the previous turn.
		chatInstances.clearSubagents(conversationId);

		// Optimistic: show thinking indicator immediately (before network round-trip)
		isAwaitingResponse = true;
		await tick(); // Flush DOM so the indicator renders before the network call

		// Auto-scroll to bottom on submit
		scrollToBottom("smooth");

		try {
			// Sync permissions to backend BEFORE sending (so AI tool calls have them during streaming)
			// add_permission endpoint handles chat creation via INSERT OR IGNORE INTO chats.
			// Ghost chats are never created server-side, so skip this.
			if (!isGhost && isNewChat(tab.route) && editAllowListStore.hasItems) {
				await editAllowListStore.markChatCreated();
			}

			// No `text` key when nothing was typed: the SDK appends a text part
			// for any non-null text, and an EMPTY text block is a 400 from
			// Anthropic that then rode in this chat's history forever (every
			// later Claude turn failed; Grok did not care). An attachment on
			// its own is a complete message — no filler words on the person's
			// behalf.
			await chat.sendMessage(
				files.length > 0
					? messageToSend
						? { text: messageToSend, files }
						: { files }
					: { text: messageToSend },
			);

			if (chat.messages.length >= 2 && !isGhost && !titleGenerated) {
				await generateTitle();
				// Update tab route if it's a new chat
				if (isNewChat(tab.route)) {
					// Update previousTabRoute first to prevent the tab-switch effect
					// from treating this as a tab change and resetting state
					const newRoute = `/chat/${conversationId}`;
					previousTabRoute = newRoute;
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

	// Flip the current (empty) chat into a temporary/ghost chat, or back. Only
	// allowed before the first message — we can't retroactively un-persist a turn.
	function toggleGhost() {
		if (!isEmpty) return;
		isGhost = !isGhost;
	}

	// Publish this chat's state to the phone shell, whose top-right button is
	// modal: ghost toggle while the chat is empty (compose would be a no-op),
	// compose once a conversation exists. Only the active view speaks; the
	// cleanup keeps a stale claim from surviving a view swap.
	$effect(() => {
		if (!mobileLayout.isMobile || !active) return;
		mobileLayout.setChatChrome({ empty: isEmpty, ghost: isGhost, toggleGhost });
		return () => mobileLayout.setChatChrome(null);
	});
</script>

<svelte:window onmouseup={(e) => refs.handleWindowMouseup(e)} />

{#if refs.draft && !isGettingStartedChat(currentChatConversationId)}
	<SelectionPopover
		rect={refs.draft.rect}
		onAdd={() => refs.add()}
		onClose={() => (refs.draft = null)}
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
				attachments.dragActive = true;
			}
		}}
		ondragleave={(e) => {
			// Only clear when leaving the root, not when crossing child boundaries.
			if (!e.relatedTarget || !(e.currentTarget as HTMLElement).contains(e.relatedTarget as Node)) {
				attachments.dragActive = false;
			}
		}}
		ondrop={(e) => {
			attachments.dragActive = false;
			// The composer is a CodeMirror editor INSIDE this root, and it
			// handles drops on itself (calling preventDefault + onAttach).
			// Returning true from a CodeMirror dom event handler does not stop
			// DOM propagation, so that drop still bubbles here — and until
			// 2026-09-17 this added the same files a second time. Cleared
			// dragActive first, because the sticky part of the overlay is not
			// optional: no dragleave fires on the element that took the drop.
			if (e.defaultPrevented) return;
			e.preventDefault();
			if (e.dataTransfer?.files?.length) attachments.add(Array.from(e.dataTransfer.files));
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
			<div class="chat-area" class:ghost={isGhost}>
				<!-- Top-right chrome: temporary-chat toggle + live context ring -->
				<div class="chat-topbar-right">
					{#if !isGhost && contextUsage && extractConversationId(tab.route) && !isGettingStartedChat(currentChatConversationId)}
						<ContextIndicator
							conversationId={extractConversationId(tab.route)!}
							usagePercentage={contextUsage.percentage}
							totalTokens={contextUsage.tokens}
							contextWindow={contextUsage.window}
							status={contextUsage.status}
							onclick={handleContextClick}
						/>
					{/if}
					{#if isGettingStartedChat(currentChatConversationId)}
						<!-- One door. A glyph through the walk, and a word
						     ("Stop for now") once the interview is underway,
						     which is the one beat with no controls of its own.
						     Nothing else lives up here — progress is numbered in
						     the thread, on the axis the eye is already reading
						     along. -->
						<GettingStartedDoor />
					{/if}
					<!-- On the phone the ghost toggle lives in the shell's top bar
					     (the modal top-right slot), not here. -->
					{#if (isEmpty || isGhost) && !mobileLayout.isMobile}
						<button
							type="button"
							class="ghost-toggle"
							class:active={isGhost}
							disabled={!isEmpty}
							onclick={toggleGhost}
							aria-pressed={isGhost}
							title={isGhost ? "Temporary chat — won't be saved" : "Start a temporary chat"}
						>
							<Icon icon="ri:ghost-line" width="16" />
						</button>
					{/if}
					{#if canManageChat}
						<button
							type="button"
							class="chat-menu-btn"
							onclick={openChatMenu}
							aria-haspopup="menu"
							aria-label="Chat options"
							title="Chat options"
						>
							<Icon icon="ri:more-2-fill" width="16" />
						</button>
					{/if}
				</div>
				<div class="page-container" class:is-empty={isEmpty}>
					<!-- Messages area -->
					<div
						bind:this={scrollContainer}
						class="flex-1 overflow-y-auto chat-layout"
						class:visible={!isEmpty}
					>
						<div
							class="messages-container"
							class:bleeds={uniqueMessages[0]?.id === INTERVIEW_OPENING_ID ||
								roomHoldsPlate ||
								isGettingStartedChat(currentChatConversationId)}
							class:room={isGettingStartedChat(currentChatConversationId)}
						>
							{#if isGettingStartedChat(currentChatConversationId) && uniqueMessages.length > 0}
								<!-- The frontispiece, at the measure of the words.
								     It ran the full width of the pane as an oil
								     painting and made the room read as two products
								     stacked. -->
								<RoomCover />
							{/if}
							{#each uniqueMessages as message, messageIndex (message.id)}
								{@const isUserMessage = message.role === "user"}
								{@const exchangeIndex = isUserMessage
									? uniqueMessages
											.slice(0, messageIndex)
											.filter((m) => m.role === "user")
											.length
									: -1}
								<div
									class="flex {isUserMessage
										? 'justify-end'
										: 'justify-start'}"
									id={isUserMessage
										? `exchange-${exchangeIndex}`
										: undefined}
								>
									<div
										class="message-wrapper"
										class:bleeds={message.id === INTERVIEW_OPENING_ID || message.id === GS_INTERVIEW_OPENING_ID}
										class:settled={isSettledLine(message)}
										class:user-has-attachment={isUserMessage &&
											message.parts.some((p: any) => p.type === "file")}
										data-message-id={message.id}
										data-subject={message.subject}
										data-role={message.role}
										data-agent-id={messageMetadata.get(
											message.id,
										)?.agentId || "general"}
										data-loading={message.role ===
											"assistant" &&
											!message.parts.some(
												(p: any) =>
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
											<!-- A turn is several runs of text with tool calls
											     between them. A run with a tool call AFTER it was
											     the model saying what it was about to do; the run
											     with nothing after it is the reply. Only the reply
											     belongs in the transcript — the rest is working-out
											     and goes to the thinking block, which is where the
											     status label comes from.
											     "Has a tool after it" rather than "is not the last
											     one" because it has to hold mid-turn too: the line
											     the model just wrote is its answer until a tool
											     starts, and at that moment it becomes narration and
											     moves. A message stored before `parts` carried this
											     order has one text run and no tool before it, so it
											     is all reply and nothing moves. -->
											{@const lastToolPartIndex =
												message.parts.reduce(
													(last: number, p: any, i: number) =>
														p.type.startsWith("tool-") ? i : last,
													-1,
												)}
											{@const lastTextPartIndex =
												message.parts.reduce(
													(last: number, p: any, i: number) =>
														p.type === "text" && p.text?.trim() ? i : last,
													-1,
												)}
											<!-- Where the reply starts. Normally just past the last
											     tool call. But a turn that ENDED on a tool call — an
											     error, a stop, the model quitting — never wrote one,
											     and treating all of its text as narration would
											     leave a blank message on screen with the words
											     hidden in a collapsed block. So once the turn is
											     over, the last thing it said stands as the reply.
											     While it is still streaming we do NOT do that: there
											     genuinely is no reply yet, and the line the model
											     wrote is already showing as the status label. -->
											{@const bodyFromIndex =
												isStreaming ||
												message.parts.some(
													(p: any, i: number) =>
														p.type === "text" &&
														p.text?.trim() &&
														i > lastToolPartIndex,
												)
													? lastToolPartIndex + 1
													: lastTextPartIndex}
											{@const messageNarration =
												message.parts
													.filter(
														(p: any, i: number) =>
															p.type === "text" &&
															p.text?.trim() &&
															i < bodyFromIndex,
													)
													.map((p: any) => p.text.trim())}
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
												messageToolParts.length > 0 ||
												messageNarration.length > 0}

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

											{#if !inInterview && (hasThinkingContent || (isStreaming && isLastMessage))}
												<!-- Interview room excluded: the companion below is
												     its one indicator, and the model's reasoning
												     about the person must never surface as chrome
												     in the room built on their own account. -->
												<ThinkingBlock
													isThinking={isStreaming &&
														isLastMessage &&
														chat.status ===
															"streaming"}
													toolCalls={messageToolParts}
													reasoningContent={messageReasoning}
													narration={messageNarration}
													duration={isLastMessage
														? thinkingDuration
														: 0}
													agentMode={selectedAgentMode}
												/>
											{/if}

											{#if eyebrowFor.has(message.id)}
												<StepEyebrow stepId={eyebrowFor.get(message.id)!} />
											{/if}
											{#each message.parts as part, partIndex (part.type === "text" ? `text-${partIndex}` : (part as any).toolCallId || `part-${partIndex}`)}
												{#if part.type === "text" && part.text.trim() && partIndex >= bodyFromIndex}
													{@const shown = reveal.revealed(message.id, part.text)}
													<div
														class="text-base text-foreground assistant-response"
													>
														<Markdown
															content={shown.content}
															isStreaming={isStreaming || shown.arriving}
															citations={citationContext}
															onCitationClick={openCitationPanel}
														/>
														{#if (message.id === INTERVIEW_OPENING_ID && partIndex === 0) || (message.id === GS_INTERVIEW_OPENING_ID && reveal.plateReady)}
															<!-- Right under the heading, wider than the column:
															     one fictional life on one wire, α toward Ω. The
															     table that follows lists the same chapters. -->
															<ChapterLifeline />
														{/if}
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
											{:else if part.type === "tool-record_introductions"}
												<!-- Nothing here: the receipt goes under the whole
												     turn, not wherever in it the model reached for
												     the tool. See after this loop. -->
											{:else if part.type === "tool-write_it_up" && isGettingStartedChat(currentChatConversationId) && (part as any).state === "output-available" && (part as any).output?.document_page_id}
												<!-- In the getting-started room the interview closes inline
												     and the thread goes on: the two doors, here — and the
												     plate with them. The close ANSWERS THE OPENING, which
												     was a lifeline of a fictional life at the top of the
												     interview; this is the same drawing made of their own
												     chapters, and it only reads as an answer if it sits at
												     the moment of the close. It stood above the composer
												     until 2026-09-16, where it was permanent furniture and
												     every later message pushed in above it. -->
												{@const out = (part as any).output}
												<InterviewClosedCard
													pageId={out.document_page_id}
													chaptersWritten={out.chapters_written ?? 0}
													alreadyExisted={out.document_already_existed ?? false}
													chaptersError={out.chapters_error ?? null}
												/>
												{#if !out.chapters_error}
													<ChapterLifelineLive />
												{/if}
											{:else if part.type === "tool-write_it_up"}
												<!-- Nothing inline: the standing card in place of the composer
												     holds the two doors (it used to render here as well, so the
												     same tiles showed twice), and a REFUSED close (the box's
												     gate saying "not yet") is the interviewer's to relay in
												     prose, never a card. -->
											{:else if part.type === "tool-create_page" && (part as any).state === "output-available"}
												{@const output = (part as any).output}
												{#if output?.page_id}
													<PageEditResult
														type="page_created"
														title={output.title}
														pageId={output.page_id}
														onOpenPage={(id) => {
													// Open the created page WITHOUT creating a split: beside the
													// chat only when already in split view, else a new tab here.
													windowShellStore.openRouteInSplitOrActive(`/page/${id}`);
												}}
														/>
												{/if}
											{:else if part.type === "tool-edit_page" && (part as any).state === "output-available"}
												{@const output = (part as any).output}
												{#if output?.needs_binding}
													<PageBindingInline
														entityId={output.page_id}
														entityTitle={output.page_title}
														message={output.message}
														onBind={bindPage}
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
															// View the edited page WITHOUT creating a split: beside the
															// chat only when already in split view, else a new tab here.
															windowShellStore.openRouteInSplitOrActive(`/page/${editPageId}`);
														} : undefined}
													/>
												{/if}
											{:else if part.type === "tool-setup_applet" && (part as any).state === "output-available"}
											{@const out = (part as any).output}
											{#if out?.applet_id && out?.status !== "check_failed"}
												<!-- The gate, in the conversation. An applet that crosses a
												     boundary is created disabled and the model cannot enable
												     it; that invariant stands. What changes is that approving
												     no longer means walking to another page to find a toggle. -->
												<AppletProposalCard
													appletId={out.applet_id}
													name={out.name}
													description={out.description}
													schedule={out.schedule}
													capabilities={out.capabilities ?? []}
													estimatedCostPerDay={out.estimated_cost_per_day}
													gated={out.gated}
													lifecycle={out.lifecycle}
													updated={out.status === "updated"}
												/>
											{/if}
											{:else if part.type === "tool-code_interpreter"}
												{@const toolPart = part as any}
												{@const isRunning = toolPart.state === "input-streaming" || toolPart.state === "input-available"}
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
											{#if introductionsRecorded(message)}
												<!-- Under the words, always: the model may call the
												     tool before it writes its sentence, and a receipt
												     printed above the sentence it belongs to is what
												     made this read backwards. -->
												<IntroductionsRecorded fields={introductionsRecorded(message)} />
											{/if}
											{#if message.subject === "gs:graduated"}
												<!-- The room's last line names what now exists; these
												     are the way in to each thing it named. Under the
												     sentence, never instead of it. -->
												<GraduatedDoors />
											{/if}
											{#if messageMetadata.get(message.id)?.stopped}
												<StoppedNotice />
											{:else if messageMetadata.get(message.id)?.cutShort}
												<StoppedNotice reason="length" />
											{:else if messageMetadata.get(message.id)?.interrupted}
												<StoppedNotice reason="interrupted" />
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
												.filter((p: any) => p.type === "text")
												.map((p: any) => p.text)
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
							{#if isGettingStartedChat(currentChatConversationId)}
								<!-- The step's controls, right under what the room
								     just said: pinned above the composer they sat a
								     screen away from it on a short thread. -->
								<RoomControls />
							{/if}
							{#if currentChatConversationId === INTERVIEW_CHAT_ID}
								{#if interviewClosed}
									<!-- The close answers the opening: the same plate, drawn from
									     the chapters the person just named. -->
									<ChapterLifelineLive />
								{/if}
								<!-- One element, two poses: the mark's points while a turn
								     works, the figure they imply while it does not. Always
								     something in the margin, so nothing jumps when the turn
								     starts — and the room is never empty of its occupant. -->
								{#if chat.status === "submitted" || chat.status === "streaming"}
									<Composing label="Composing a reply" />
								{:else}
									<Trivet />
								{/if}
							{:else if inInterview}
								{#if reveal.chars || chat.status === "submitted" || chat.status === "streaming"}
									<Composing label="Composing a reply" />
								{:else}
									<Trivet />
								{/if}
							{:else if isAwaitingResponse && !lastAssistantMessage}
								<div class="flex justify-start">
									<div class="message-wrapper" data-role="assistant">
										<ThinkingBlock
											isThinking={true}
											toolCalls={[]}
											reasoningContent=""
											duration={0}
											agentMode={selectedAgentMode}
										/>
									</div>
								</div>
							{/if}

							<ChatError
											error={chat.error ?? null}
											onRetry={() => chat.regenerate()}
											recommendedName={models.recommendedFallback?.displayName}
											onSwitchAndRetry={models.recommendedFallback
												? switchToRecommendedAndRetry
												: undefined}
										/>
						</div>
					</div>

					{#if isEmpty && !isGhost && attachments.count === 0}
						<!-- The opening image: the mark assembling itself in the space
						     a conversation will fill. Both layouts left this expanse
						     blank — the phone docks the composer to the bottom, the
						     desktop floats it at center, and either way a new chat
						     opened onto nothing at all (VIR-313). Positioned off the
						     midpoint, so it sits above the composer in both. It yields to
						     staged attachments: the row of chips grows upward from the
						     composer and would otherwise collide with the word, and once
						     someone has dropped a file the blank canvas has done its job.
						     Decorative, so hidden from the tree and transparent to
						     touches. -->
						<div class="init-hero" aria-hidden="true" out:fade={{ duration: 200 }}>
							<svg class="init-mark" viewBox="0 0 12 10.5" width="30" height="26.25" fill="currentColor">
								<circle class="init-dot init-dot-1" cx="6" cy="2.4" r="1.5" />
								<circle class="init-dot init-dot-2" cx="2.6" cy="8.1" r="1.5" />
								<circle class="init-dot init-dot-3" cx="9.4" cy="8.1" r="1.5" />
							</svg>
							<span class="init-word">Virtues</span>
						</div>
					{/if}

					{#if isEmpty && isGhost}
						<div
							class="ghost-hero"
							in:fade={{ duration: 300 }}
							out:fly={{ y: -14, duration: 300, easing: cubicInOut }}
						>
							<!-- The title alone: the tiled ghost field and the inverted
							     composer already say what this mode is — an icon and an
							     explainer on top of them was the same fact three times. -->
							<h1 class="ghost-hero-title">Temporary Chat</h1>
						</div>
					{/if}

					<!-- ChatInput -->
					<div
						bind:this={composerEl}
						class="chat-input-wrapper"
						class:is-empty={isEmpty}
						class:has-messages={!isEmpty}
						class:transitions-enabled={enableTransitions}
						class:focused={inputFocused}
						class:drag-active={attachments.dragActive}
					>
						{#if attachments.dragActive}
							<div class="drop-hint">
								<Icon icon="ri:download-2-line" width="15" />
								<span>Drop to attach &middot; images, PDFs, or audio</span>
							</div>
						{/if}
						{#if attachments.count > 0}
							<div class="attachments">
								{#each attachments.items as a (a.id)}
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
											<!-- An image carries only its size (VIR-237): a screenshot's
											     generated filename says nothing the thumbnail has not
											     already shown. Every other kind keeps its name, because a
											     type icon and a byte count cannot tell two PDFs apart. The
											     name still reaches assistive tech through the img alt. -->
											{#if a.kind !== "image"}
												<span class="attachment-name">{a.filename}</span>
											{/if}
											<span class="attachment-size">{formatFileSize(a.size)}</span>
										</div>
										<button
											type="button"
											class="attachment-remove"
											aria-label="Remove attachment"
											onclick={() => attachments.remove(a.id)}
										>
											<Icon icon="ri:close-line" width="13" />
										</button>
									</div>
								{/each}
							</div>
						{/if}
						{#if models.capabilityIssue}
							<div class="capability-banner">
								<span>
									{models.capabilityIssue.modelName} can't read {models.capabilityIssue.lacks.join(" or ")}.
								</span>
								{#if models.capabilityIssue.candidate}
									<button type="button" class="capability-switch" onclick={() => models.switchToCapable()}>
										Switch to {models.capabilityIssue.candidate.displayName}
									</button>
								{:else}
									<span class="capability-none">No available model can read {models.capabilityIssue.lacks.join(" or ")} yet.</span>
								{/if}
							</div>
						{/if}
						{#if refs.staged.length > 0}
							<div class="staged-refs">
								{#each refs.staged as r (r.id)}
									<div class="staged-ref">
										<Icon
											icon="ri:double-quotes-l"
											width="13"
											class="staged-ref-mark"
										/>
										<div class="staged-ref-body">
											<span class="staged-ref-quote">{r.text}</span>
										</div>
										<button
											type="button"
											class="queued-remove"
											aria-label="Remove reference"
											onclick={() => refs.remove(r.id)}
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
						{#if interviewClosed}
							<!-- The interview is over: no composer, the two doors instead. -->
							<InterviewClosedCard
								standing
								pageId={interviewDocumentPageId}
								chaptersWritten={interviewClosedPart?.chapters_written ?? 0}
								alreadyExisted={interviewClosedPart?.document_already_existed ?? false}
								chaptersError={interviewClosedPart?.chapters_error ?? null}
							/>
						{:else if isGettingStartedChat(currentChatConversationId) && !gettingStarted.aiConnected}
							<!-- No model yet. The composer STAYS and says why it
							     cannot be used: the app is no longer closed off, so
							     the thing that genuinely does not work has to
							     explain itself where it is, rather than the whole
							     surface disappearing around it. -->
							<ChatInput
								bind:value={input}
								disabled={true}
								sendDisabled={true}
								maxWidth="max-w-3xl"
								placeholder="Connect AI above to write here"
								onSubmit={() => {}}
							/>
						{:else}
						<ChatInput
							allowEmptySubmit={refs.staged.length > 0 || attachments.count > 0}
							onAttach={(f) => attachments.add(f)}
							bind:value={input}
							bind:focused={inputFocused}
							disabled={false}
							sendDisabled={chat.status !== "ready"}
							isStreaming={chat.status === "streaming"}
							maxWidth="max-w-3xl"
							placeholder={isGhost ? "Ask Virtues (temporary)" : "Ask Virtues"}
							onSubmit={(text) => handleChatSubmit(text)}
							onStop={() => handleChatStop()}
						/>
						{/if}

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

	/* `.chat-topbar` and its title/input rules lived here. The pane no longer
	   prints the chat's name at all — the tab above it does, and one name per
	   thing is the rule. `.chat-topbar-right` (context ring, ghost toggle, ⋮)
	   stays; it holds controls, not a label. */

	.chat-topbar-right {
		position: absolute;
		top: 8px;
		right: 12px;
		z-index: 6;
		display: flex;
		align-items: center;
		gap: 6px;
	}



	.ghost-toggle {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 28px;
		height: 28px;
		border-radius: 9px;
		color: var(--color-foreground-subtle);
		background: color-mix(in srgb, var(--color-surface) 72%, transparent);
		backdrop-filter: blur(8px);
		-webkit-backdrop-filter: blur(8px);
		transition:
			color 0.15s ease,
			background-color 0.15s ease;
		cursor: pointer;
	}

	/* 28px of visible chip, 44pt of reachable square — the chip is deliberately
	   small and floats over the transcript, so the target grows around it
	   rather than under it. */
	@media (max-width: 768px), (pointer: coarse) {
		/* (The composer's phone padding lives in the docked-composer block at
		   the end of these styles — it must follow the base rules to win the
		   cascade.) */

		.ghost-toggle {
			position: relative;
		}

		.ghost-toggle::after {
			content: "";
			position: absolute;
			top: 50%;
			left: 50%;
			width: 44px;
			height: 44px;
			transform: translate(-50%, -50%);
		}
	}

	.ghost-toggle:hover:not(:disabled) {
		color: var(--color-foreground);
		background: var(--hover-bg);
	}

	.ghost-toggle.active {
		color: var(--color-primary);
		background: color-mix(in srgb, var(--color-primary) 14%, transparent);
	}

	.ghost-toggle:disabled {
		cursor: default;
	}

	.chat-menu-btn {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 28px;
		height: 28px;
		border-radius: 9px;
		color: var(--color-foreground-subtle);
		background: color-mix(in srgb, var(--color-surface) 72%, transparent);
		backdrop-filter: blur(8px);
		-webkit-backdrop-filter: blur(8px);
		transition:
			color 0.15s ease,
			background-color 0.15s ease;
		cursor: pointer;
	}

	.chat-menu-btn:hover {
		color: var(--color-foreground);
		background: var(--hover-bg);
	}

	.chat-topbar-right > :global(*) {
		animation: topbar-pop-in 260ms cubic-bezier(0.34, 1.4, 0.64, 1) backwards;
	}

	@keyframes topbar-pop-in {
		from {
			opacity: 0;
			transform: scale(0.7);
		}
		to {
			opacity: 1;
			transform: scale(1);
		}
	}

	/* Ghost/temporary chat — faint tiled ghost field, theme-aware via mask. The
	   field reveals as a circle expanding from the composer (screen center) so the
	   ghosts ripple outward from the middle. */
	.chat-area.ghost::before {
		content: "";
		position: absolute;
		inset: 0;
		z-index: 0;
		pointer-events: none;
		background: var(--color-foreground);
		opacity: 0.035;
		-webkit-mask-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24'%3E%3Cpath d='M12 2a8 8 0 0 0-8 8v10l2.5-2 2.5 2 2.5-2 2.5 2 2.5-2 2.5 2V10a8 8 0 0 0-8-8z' fill='%23000'/%3E%3C/svg%3E");
		mask-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24'%3E%3Cpath d='M12 2a8 8 0 0 0-8 8v10l2.5-2 2.5 2 2.5-2 2.5 2 2.5-2 2.5 2V10a8 8 0 0 0-8-8z' fill='%23000'/%3E%3C/svg%3E");
		-webkit-mask-size: 46px 46px;
		mask-size: 46px 46px;
		-webkit-mask-repeat: repeat;
		mask-repeat: repeat;
		animation: ghost-wave-in 900ms cubic-bezier(0.22, 1, 0.36, 1) both;
	}

	@keyframes ghost-wave-in {
		from {
			opacity: 0;
			clip-path: circle(0% at 50% 50%);
		}
		to {
			opacity: 0.035;
			clip-path: circle(120% at 50% 50%);
		}
	}

	/* Ghost mode inverts the composer: the pill you type into flips to the
	   theme's ink, so the mode is a material change under your fingers, not a
	   label you have to remember reading. Done by remapping the pill's tokens
	   — everything inside (placeholder, buttons, the model pill) follows on
	   its own. The originals are captured one scope up because a custom
	   property cannot swap with itself in place. */
	.chat-area.ghost {
		--ghost-pill-bg: var(--color-foreground);
		--ghost-pill-ink: var(--color-surface);
	}
	.chat-area.ghost :global(.chat-input-wrapper.bg-surface) {
		/* Both token families: the Tailwind theme tokens (--foreground, used
		   by text-foreground et al.) and the component tokens (--color-*). */
		--surface: var(--ghost-pill-bg);
		--foreground: var(--ghost-pill-ink);
		--color-surface: var(--ghost-pill-bg);
		--color-foreground: var(--ghost-pill-ink);
		--color-foreground-muted: color-mix(in srgb, var(--ghost-pill-ink) 65%, transparent);
		--color-foreground-subtle: color-mix(in srgb, var(--ghost-pill-ink) 45%, transparent);
		--color-border-strong: transparent;
		--color-border: color-mix(in srgb, var(--ghost-pill-ink) 18%, transparent);
		--hover-bg: color-mix(in srgb, var(--ghost-pill-ink) 12%, transparent);
		background: var(--ghost-pill-bg);
		color: var(--ghost-pill-ink);
	}

	/* The send control flips with the pill: ink circle, pill-colored glyph —
	   otherwise `.btn-primary` (secondary bg, surface ink) lands dark-on-dark
	   inside the inverted pill. */
	.chat-area.ghost :global(.chat-input-wrapper .btn-primary) {
		background-color: var(--ghost-pill-ink);
		color: var(--ghost-pill-bg);
	}

	@media (prefers-reduced-motion: reduce) {
		.chat-area.ghost::before,
		.chat-topbar-right > :global(*) {
			animation: none;
		}
		.chat-input-wrapper.transitions-enabled,
		.chat-layout {
			transition: opacity 0.2s ease;
		}
	}

	.ghost-hero {
		position: absolute;
		left: 0;
		right: 0;
		bottom: calc(50% + 52px);
		z-index: 2;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.375rem;
		text-align: center;
		padding: 0 1.5rem;
		pointer-events: none;
	}

	.ghost-hero-title {
		font-family: var(--font-serif);
		font-size: 1.75rem;
		font-weight: 400;
		color: var(--color-foreground);
	}

	/* ── The phone's opening image ──
	   The ∴ mark and wordmark, seated in the upper half of the empty room —
	   above center so the (bottom-docked) composer and rising keyboard never
	   crowd it. The entrance is the mark ASSEMBLING: three dots settle into
	   the trivet one by one, then the word surfaces under them. All
	   keyframes are from-only with `backwards` fill — an explicit `to` with
	   a fill-mode is what once pinned a disabled button solid ink (see the
	   airlock's rise animation for the same rule). */
	.init-hero {
		position: absolute;
		left: 0;
		right: 0;
		bottom: calc(50% + 72px);
		z-index: 2;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.875rem;
		text-align: center;
		pointer-events: none;
		color: var(--color-foreground);
	}

	.init-mark {
		overflow: visible;
	}

	.init-word {
		/* The masthead's register, verbatim: serif, never bold, hairline
		   stroke for logo presence at text weight. */
		font-family: var(--font-serif);
		font-size: 1.375rem;
		font-weight: 400;
		letter-spacing: 0.03em;
		-webkit-text-stroke: 0.2px currentColor;
	}

	@media (prefers-reduced-motion: no-preference) {
		.init-dot {
			animation: init-dot-settle 0.55s cubic-bezier(0.22, 1, 0.36, 1) backwards;
			transform-origin: center;
			transform-box: fill-box;
		}
		.init-dot-1 {
			animation-delay: 0.15s;
		}
		.init-dot-2 {
			animation-delay: 0.32s;
		}
		.init-dot-3 {
			animation-delay: 0.49s;
		}
		.init-word {
			animation: init-word-rise 0.7s cubic-bezier(0.22, 1, 0.36, 1) 0.75s backwards;
		}
	}

	@keyframes init-dot-settle {
		from {
			opacity: 0;
			transform: scale(0.3) translateY(3px);
		}
	}

	@keyframes init-word-rise {
		from {
			opacity: 0;
			transform: translateY(8px);
		}
	}


	.page-container {
		height: 100%;
		position: relative;
	}

	.chat-layout {
		height: 100%;
		/* The plate in the interview's opening measures its bleed against
		   this scroller (cqw), never the viewport. */
		container-type: inline-size;
		opacity: 0;
		pointer-events: none;
		/* Fade + rise in as the composer glides down (matched to the ~400ms glide). */
		transform: translateY(10px);
		transition:
			opacity 0.32s ease,
			transform 0.4s cubic-bezier(0.76, 0, 0.24, 1);
		position: relative;
		z-index: 1;
		/* Keep scroll position stable as streamed content grows above the fold */
		overflow-anchor: auto;
		/* Scrollbar: inherited from the :root rule in app.css. This was the one
		   place in the app that got it right — standard properties, so overlay
		   behaviour survived — and the comment explaining why is now that
		   rule's comment. */
	}

	.chat-layout.visible {
		opacity: 1;
		transform: translateY(0);
		pointer-events: auto;
	}

	.messages-container {
		max-width: 48rem;
		margin: 0 auto;
		width: 100%;
		padding: 1.5rem 2rem 10rem 2rem;
		/* The docked composer's measured height (set by the observer in the
		   script) plus a breath of room, never less than the resting reserve.
		   The composer overlays the scroller rather than pushing it, so this is
		   the only thing keeping a tall draft off the last reply. */
		padding-bottom: max(10rem, calc(var(--composer-height, 0px) + 1.5rem));
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

	/* The interview's opening plate bleeds past the column (see
	   ChapterLifeline.svelte: it sizes itself in cqw of the scroller). Paint
	   containment would clip it at the column's edge, so the room that shows
	   it keeps layout containment only. */
	.messages-container.bleeds {
		contain: layout;
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
		/* The composer's resting inset off the window edge. It sat at 78px,
		   a leftover of "1rem plus the floating tab bar's reserve" kept after
		   the bar went; on a desktop window that read as the composer
		   floating a hand's width above the bottom (Adam, 2026-09-14). The
		   phone override below sets its own. */
		padding-bottom: 1.5rem;
		background-color: var(--color-surface);
		background-image: var(--background-image);
		background-blend-mode: multiply;
		box-sizing: border-box;
		z-index: 10;
		/* Docked resting state. The empty state centers itself relative to this same
		   bottom-anchored box (bottom:50% + translateY) so the whole center→dock
		   travel is one interpolatable transition — no snap, no position swap. */
		transform: translateY(0);
		will-change: bottom, transform;
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

	/* Four times the area of the old 2.25rem chip (VIR-238), which was too
	   small to tell one screenshot from another. Linear 4x (9rem) was the
	   other reading of the ticket and is far too tall — it would own the
	   composer. The icon below stays at 2.25rem: a file chip is identified by
	   its name, which it keeps, so it has nothing to gain from the height. */
	.attachment-thumb {
		width: 4.5rem;
		height: 4.5rem;
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
		border-radius: var(--radius-full);
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
		/* Deliberate ~600ms ease-in-out glide. The surface mask fades in only near
		   the end (delayed) so it doesn't read as a panel sliding over the messages. */
		transition:
			bottom 0.4s cubic-bezier(0.76, 0, 0.24, 1),
			transform 0.4s cubic-bezier(0.76, 0, 0.24, 1),
			background-color 0.22s ease 0.2s;
	}

	.chat-input-wrapper.is-empty {
		/* Centered relative to the same bottom anchor: bottom edge to mid-container,
		   then nudged down half its own height → exact vertical center, any height. */
		bottom: 50%;
		transform: translateY(50%);
		/* Nothing to mask when centered — let the background (incl. ghost field)
		   show through instead of a solid surface block around the composer. */
		background-color: transparent;
		background-image: none;
	}

	/* Phones dock the composer PERMANENTLY — no centered empty state, no
	   center→dock travel to animate, nothing to snap. The input rests on the
	   bottom and the keyboard pushes it up (main's content box shrinks under
	   it via --keyboard-inset). This block sits after the base rules on
	   purpose: it ties them on specificity, and cascade order is what lets it
	   win — an earlier version lived above them and silently lost. */
	@media (max-width: 768px), (pointer: coarse) {
		.chat-input-wrapper,
		.chat-input-wrapper.is-empty {
			bottom: 0;
			transform: translateY(0);
			/* Snug above the keys: the home-indicator gap collapses as the
			   keyboard inset grows, so the pill hugs the keyboard when it's up
			   and clears the indicator when it's not. */
			padding-bottom: calc(
				1rem + max(env(safe-area-inset-bottom) - var(--keyboard-inset, 0px), 0px)
			);
		}
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

	.message-wrapper.bleeds {
		contain: layout;
	}

	/* ── The room's rhythm ──
	   The getting-started room is one voice writing down the page, not two
	   people taking turns: its lines are separate assistant messages only
	   because the server appends them one at a time. At the default turn
	   spacing they drift 3rem apart (a paragraph's trailing margin, the
	   wrapper's padding twice, and the list's gap, none of which collapse
	   through `contain`) while the paragraphs INSIDE one message sit 1rem
	   apart — so the page reads as pulled apart. Here the gap is the only
	   spacing left, set to the paragraph's own rhythm. A user turn keeps its
	   bubble, which separates itself. */
	.messages-container.room {
		gap: 1rem;
	}
	.messages-container.room .message-wrapper:not([data-role="user"]) {
		padding: 0;
	}
	/* `.markdown > .streamdown-content > <blocks>` — Streamdown nests its
	   output one level, so the original `.markdown > :last-child` matched
	   that wrapper and never the last paragraph. The trailing margin stayed,
	   and the room's lines sat 2rem apart while paragraphs inside a line sat
	   at 1rem — the very thing this rule was written to fix. */
	.messages-container.room .message-wrapper:not([data-role="user"]) :global(.markdown > * > :last-child) {
		margin-bottom: 0;
	}

	/* ── The standfirst ──
	   The welcome's opening line is the most important sentence in the
	   product and was set as body copy, indistinguishable from the admin
	   paragraph beneath it. It carries the page's one voice change: the
	   book's serif, a size up, with air under it. Everything else in the
	   room stays body. */
	/* Descendant, not child: Streamdown nests its output one level inside
	   `.markdown`, so `>` never matched. */
	.messages-container.room .message-wrapper[data-subject="gs:welcome"] :global(.markdown p:first-of-type) {
		font-family: var(--md-heading-major-family, var(--font-serif));
		font-size: 1.3125rem;
		line-height: 1.5;
		margin-bottom: 1.25rem;
	}

	/* `.settled` is still set on every line the room speaks about a step that
	   is already done — the hook is kept, the check that hung in the margin is
	   not. It read as a UI artifact stuck onto prose, in a green nothing else
	   in the room uses, and it broke the column's left edge. */

	.message-wrapper :global(h1),
	.message-wrapper :global(h2),
	.message-wrapper :global(h3),
	.message-wrapper :global(h4) {
		margin-top: 0;
	}

	/* User message card styling — hugs its content (left-aligned). Radius mirrors
	   the composer's language: big enough that a one-line bubble caps into a pill
	   (radius ≥ half its height) to match the input, but still leaves flat sides
	   once the text wraps, so 2+ lines read as a clean rounded rect, not a lozenge. */
	.message-wrapper[data-role="user"] {
		background: var(--color-surface-elevated);
		border: 1px solid var(--color-border);
		border-radius: 1.5rem;
		padding: 10px 16px;
		width: fit-content;
		max-width: 80%;
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
