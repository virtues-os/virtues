<script lang="ts">
	import type { Tab } from "$lib/tabs/types";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { notifyTrashed, routeIfOpen } from "$lib/utils/toasts";
	import { toast } from "svelte-sonner";
	import ChatInput from "$lib/components/ChatInput.svelte";
	import MediaLightbox from "$lib/components/MediaLightbox.svelte";
	import { getInitializationPromise } from "$lib/stores/models.svelte";
	import Markdown from "$lib/components/Markdown.svelte";
	import StoppedNotice from "$lib/components/StoppedNotice.svelte";
	import Icon from "$lib/components/Icon.svelte";

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
		railTurns,
		splitTurn,
		stopReason,
		turnMovedPast,
	} from "$lib/components/chat/state/transcript";
	import { AttachmentsController } from "$lib/components/chat/state/attachments.svelte";
	import { ModelChoiceController } from "$lib/components/chat/state/modelChoice.svelte";
	import { OpeningRevealController } from "$lib/components/chat/state/openingReveal.svelte";
	import { ToolSideEffects } from "$lib/components/chat/state/toolSideEffects";
	import { toolErrorDetail, toolErrorSummary } from "$lib/components/chat/state/toolError";
	import { observeComposerReserve } from "$lib/components/chat/state/composerReserve";
	import { holdReadingPosition } from "$lib/components/chat/state/holdReadingPosition";
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
	import ConversationRail from "$lib/components/chat/ConversationRail.svelte";
	import RestingMark from "$lib/components/chat/RestingMark.svelte";
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
	import { toolStatus } from "$lib/components/chat/state/toolPresentation";
	import { TurnPhaseController } from "$lib/components/chat/state/turnPhase.svelte";
	import TurnFigures from "$lib/components/chat/TurnFigures.svelte";
	import SubagentPanel from "$lib/components/SubagentPanel.svelte";
	import { onMount, onDestroy, tick, untrack } from "svelte";
	import { SvelteSet } from "svelte/reactivity";
	import { goto } from "$app/navigation";
	import { fade, fly } from "svelte/transition";
	import { cubicInOut } from "svelte/easing";
	import { chatSessions } from "$lib/stores/chatSessions.svelte";
	import { chatActivity } from "$lib/stores/chatActivity.svelte";
	import { mobileLayout } from "$lib/stores/mobileLayout.svelte";
	import { projectStore } from "$lib/stores/project.svelte";
	import ProjectChip from "$lib/components/ProjectChip.svelte";
	import { openProject, projectChipMenuItems, projectMenuItems, targetForTab } from "$lib/utils/projectActions";
	import { chatInstances } from "$lib/stores/chatInstances.svelte";
	import { pendingPrompt } from "$lib/stores/pendingPrompt.svelte";
	import {
		deleteChat,
		getChat,
		getChatUsage,
		getAssistantProfile,
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
	import MessageFile from "$lib/components/chat/MessageFile.svelte";
	import ComposerTray from "$lib/components/chat/ComposerTray.svelte";
	import { ChatError } from "$lib/components/chat";
	import { availableModes, type AgentModeId } from "$lib/config/agentModes";
	import LocalModelCard from "$lib/components/chat/local/LocalModelCard.svelte";
	import LocalStatsLine from "$lib/components/chat/local/LocalStatsLine.svelte";
	import { localModel } from "$lib/stores/localModel.svelte";
	import { chatUsage } from "$lib/stores/chatUsage.svelte";

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

	// Whether a turn is working, and for how long (see state/turnPhase). A
	// stream silent past LET_GO_MS is let go and rejoined: the box's turn
	// outlives the request, so a dead socket is the only thing lost.
	const turnPhase = new TurnPhaseController(
		() => chat,
		() => {
			void chat.stop();
			setTimeout(resumeIfDangling, 500);
		},
	);

	// Track E1: multimodal attachments (see state/attachments).
	const attachments = new AttachmentsController();

	// What the picker shows, what goes on the wire, and the capability gate
	// that judges the staged attachments against it (see state/modelChoice).
	const models = new ModelChoiceController(() => attachments.items);

	// Through the same door as the error card's Try again: the turn that
	// failed may still be running on the box, and a bare regenerate would
	// start a second, billed one on top of it.
	function switchToRecommendedAndRetry() {
		models.switchToRecommended();
		void retryLastTurn();
	}

	// Click an in-message image to open it in a shared-element lightbox.
	let lightbox = $state<{ src: string; alt: string; rect: DOMRect } | null>(null);
	function openLightbox(e: MouseEvent, src: string, alt: string) {
		const el = e.currentTarget as HTMLImageElement;
		lightbox = { src, alt, rect: el.getBoundingClientRect() };
	}

	// The route the tab-change effect last handled — a transition detector,
	// not something the markup renders, so a plain variable.
	// svelte-ignore state_referenced_locally
	let previousTabRoute: string = tab.route;
	// The tab's history position at that route. "New chat" on a new chat
	// navigates to the same route; only the history moves.
	// svelte-ignore state_referenced_locally
	let previousHistoryIndex: number = tab.historyIndex;

	// AbortController for cancelling in-flight requests on tab switch
	let tabSwitchAbortController: AbortController | null = null;

	// Keep a map of message metadata (agentId, provider, etc.) for rendering
	let messageMetadata = $state<Map<string, MessageMeta>>(new Map());

	/** The converter, bound to this view's metadata map. */
	function toUiMessage(msg: any) {
		return toUiMessageWith(msg, messageMetadata);
	}

	// Citation panel state
	let citationPanelOpen = $state(false);
	let selectedCitation = $state<Citation | null>(null);

	// The Project (room) this chat lives in — at most one. Its id is sent with
	// each message: the agent's active-project context, and on a chat's first
	// message the server files it there.
	//
	// Read live through `chatSessions.projectOf`. It used to be copied once
	// per conversation, so filing an open chat from a menu or the project
	// page left this view grounding answers in the project it had just left,
	// or in none.
	//
	// An unsent chat has no row and nothing on the server to file, so it
	// carries a draft instead (`projectStore.draftFor`): set by "Ask this
	// project", "New chat here", dropping its tab on a project, or its own
	// menu. The draft goes out with the first message, the server files the
	// chat, and the draft is dropped once the filing comes back, so unfiling
	// the chat later cannot be undone by a leftover.
	const savedProjectId = $derived(chatSessions.projectOf(conversationId));
	// A draft whose project was deleted before the first message is dropped,
	// not sent: the server would file the chat into a project in the trash.
	const draftProjectId = $derived.by(() => {
		const id = projectStore.draftFor(conversationId);
		return id && (!projectStore.loaded || projectStore.byId(id)) ? id : null;
	});
	const chatProjectId = $derived(savedProjectId ?? draftProjectId);

	$effect(() => {
		if (draftProjectId && savedProjectId === draftProjectId) {
			projectStore.setDraft(conversationId, null);
		}
	});

	// A tab showing an unsent chat is fileable by its draft; the store needs to
	// know which chat that is. "Unsent" is the route, not the session list: a
	// new chat sits at `/` until its first message, while an old chat can be
	// missing from the list and still be saved.
	const unsent = $derived(!extractConversationId(tab.route) && !isGhost && !isGettingStartedChat(conversationId));
	$effect(() => {
		projectStore.noteUnsentChat(tab.id, unsent ? conversationId : null);
	});
	$effect(() => {
		const tabId = tab.id;
		return () => projectStore.noteUnsentChat(tabId, null);
	});

	// A temporary chat is never written to the box, so it cannot be filed;
	// turning one on drops the draft rather than grounding a chat nobody keeps.
	$effect(() => {
		if (isGhost && draftProjectId) projectStore.setDraft(conversationId, null);
	});

	// The project, when there is one, is said at the top of the chat: its
	// brief and items shape every answer here, and without this the only way
	// to know was to go looking. It is a door back to the project, not a
	// picker; filing happens from "Move to project".
	const chatProject = $derived(chatProjectId ? projectStore.byId(chatProjectId) : undefined);

	function openChatProject() {
		if (chatProject) openProject(chatProject);
	}

	/** The project's own menu, from the chip: right-click here, a tap on the phone's bar. */
	function showProjectChipMenu(e: MouseEvent) {
		e.preventDefault();
		const target = targetForTab(tab);
		if (!chatProject || !target) return;
		const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
		contextMenu.show({ x: rect.left, y: rect.bottom }, projectChipMenuItems(chatProject, target), {
			anchor: { x: rect.left, y: rect.top, width: rect.width, height: rect.height },
			placement: "bottom-start",
		});
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

	/**
	 * Handle permission allow for AI edit.
	 * Adds permission then regenerates the AI's last response (which had permission_needed).
	 * regenerate() removes that assistant message and re-requests — no duplicate user messages.
	 */
	async function handlePermissionAllow(entityId: string, entityType: string, title: string) {
		// Await ensures the backend has the permission before the retry.
		await grantEditPermission(entityId, entityType, title);

		// A sudo command: the turn paused on it, and everything before it in
		// the turn stands. Regenerating would throw that away and ask the model
		// to find the command again; instead it is told to run the one allowed.
		// Sent straight to the SDK, not through the composer, whose draft and
		// staged files are the person's and stay where they are.
		if (entityType === "command") {
			if (chat.status === "ready") {
				danglingTurn = false;
				await chat.sendMessage({ text: "Allowed. Run exactly that command." });
				setTimeout(turnWritten, 2000);
			}
			return;
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

	// The two tool results that reach outside the transcript — create_page
	// opens a page beside the chat, edit_page animates one (see
	// state/toolSideEffects for why both seed before they act).
	const tools = new ToolSideEffects();

	// The tool results that act outside the transcript: auto-open a page
	// create_page made, and run the presence animation for an edit_page. Only
	// for calls made during this session, not when reopening old chats — so
	// nothing runs during the initial load (see state/toolSideEffects).
	$effect(() => {
		if (!chat?.messages || isLoading) return;
		for (const page of tools.collectNewPages(chat.messages)) {
			openCreatedPage(page.pageId, page.title);
		}
		tools.animateNewEdits(chat.messages);
	});

	// (The interview's write_it_up auto-open lives in chatInstances.onData —
	// the backend sends a transient data-narrative-document part, because tool
	// parts land in the messages array mutably where no effect observes them.)

	// Context usage state
	interface ContextUsageState {
		percentage: number;
		tokens: number;
		window: number;
		status: "healthy" | "warning" | "critical";
	}
	let contextUsage = $state<ContextUsageState | undefined>(undefined);

	// A turn in this chat has been saved: read its usage here, and tell the
	// context view (another tab) to read it too.
	function turnWritten() {
		refreshContextUsage();
		if (conversationId) chatUsage.turnWritten(conversationId);
	}

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

	/**
	 * Fetch a conversation's stored transcript and put it on screen. Every load
	 * path goes through here — mount, a route change, and the re-reads below —
	 * so they agree about the shape of a turn (see state/transcript). Returns
	 * false when `signal` aborted it before anything was written. Throws on a
	 * failed fetch: what a failure means is each caller's call.
	 */
	async function loadTranscript(id: string, signal?: AbortSignal): Promise<boolean> {
		const data = await getChat<{
			messages?: any[];
			conversation?: { project_id?: string | null };
		}>(id, signal);
		if (signal?.aborted) return false;
		// An older chat is not in the session list; its own detail says where
		// it is filed. A box older than the field leaves it undefined.
		if (data.conversation && data.conversation.project_id !== undefined) {
			chatSessions.noteProject(id, data.conversation.project_id ?? null);
		}
		chat.messages = deduplicateMessages(data.messages || []).map(
			toUiMessage,
		) as unknown as typeof chat.messages;
		return true;
	}

	/** Re-read the stored transcript. Used after a compaction, and after the
	 *  getting-started room speaks (its lines are appended server-side, so
	 *  the thread has to be re-read to show them). */
	async function reloadMessages() {
		if (!conversationId) return;
		try {
			await loadTranscript(conversationId);
			// Re-reading drops the interview's opening, which is shown rather
			// than stored; put it back where it belongs.
			applyRoomInterviewOpening(chat, conversationId, gettingStarted.state);
		} catch {
			// Non-critical refresh — leave the current messages in place on failure.
		}
	}

	// A chat that opens with your message last and no reply may be a turn the
	// box is still running (VIR-323): a turn outlives its request now, so ask
	// for its live stream. The SDK replays what was said and follows the rest;
	// a 204 means nothing is running and the load stands as it is. Never for a
	// ghost: its transcript lives in this tab and nowhere the box could resume.
	// When the last rejoin was attempted, so the several triggers that legitimately
	// fire together on a load do not each open a reader on the same turn.
	//
	// An "in flight" boolean was not enough, and the network log said so: three
	// requests 3ms apart, because a 204 settles fast enough that the `finally`
	// clears the flag before the next caller looks at it. Against a dead turn
	// that is only waste; against a LIVE one it is three readers on one stream,
	// which is the thing the guard exists to prevent. A short floor covers both
	// the overlapping case and the rapid-succession one.
	let lastRejoinAt = 0;
	const REJOIN_FLOOR_MS = 1500;

	function resumeIfDangling() {
		if (isGhost) return;
		// "ready" OR "error": the case this exists for — a Wi-Fi handover, a
		// phone that slept — ends the SDK's fetch with a TypeError, and the
		// SDK sets status to error. Gated on ready alone, the rejoin never
		// fired for the drop it was built to survive, and the error card's
		// Try again started a second turn on top of the one still running.
		if (chat.status !== "ready" && chat.status !== "error") return;
		const last = chat.messages[chat.messages.length - 1];
		if (!last) return;
		// The floor is spent only by an attempt that goes out. A phone wakes
		// while the dead socket still reads "streaming"; stamped before the
		// gate, that wake used the floor up and the rejoin it was for never ran.
		const now = Date.now();
		if (now - lastRejoinAt < REJOIN_FLOOR_MS) return;
		lastRejoinAt = now;

		// No "is the last message a user message?" gate any more. It used to be
		// here, and it meant the rejoin only ever fired for a turn that had not
		// produced a single token: the moment the assistant started streaming,
		// an assistant message existed and the door shut. A phone that slept, a
		// lift, a Wi-Fi handover — all threw away a turn the box was still
		// running and still paying for, and offered a Retry that bills a second
		// one. That is the whole case this exists for.
		//
		// We cannot tell "unfinished" from the client with any confidence, and
		// we do not have to: the box is authoritative and answers 204 when no
		// turn is live, so an unnecessary ask is one cheap GET.
		void chat
			.resumeStream()
			.catch((e: unknown) => {
				console.warn("[ChatView] could not rejoin the running turn:", e);
			})
			.finally(() => {
				// Nothing was running, and the last thing said was ours. The
				// turn died with the box — see `danglingTurn`.
				markDanglingIfUnanswered();
			});
	}

	// A turn the box started and never finished, with nothing left to rejoin.
	//
	// The assistant row is only written once the loop completes, so a box that
	// dies mid-turn (crash, power, upgrade) leaves the person's message sitting
	// there with no reply and NO marker — indistinguishable from a message that
	// was never sent. Reload asks to rejoin, gets 204, and used to leave it
	// looking like nothing had happened.
	let danglingTurn = $state(false);

	function markDanglingIfUnanswered() {
		if (isGhost) {
			danglingTurn = false;
			return;
		}
		const last = chat.messages[chat.messages.length - 1];
		danglingTurn =
			chat.status === "ready" && !!last && last.role === "user" && !isAwaitingResponse;
	}

	/**
	 * The error card's Try again. In order:
	 *
	 * 1. Rejoin — the box may still be writing the reply (a turn outlives the
	 *    request), in which case a new request is a second turn and a second
	 *    charge. `resumeStream` settles at once on a 204 and only after the
	 *    stream ends on a 200, so "still pending after the status moved" is
	 *    the attached case.
	 * 2. Re-read — the turn may have FINISHED while the wire was down; the
	 *    reply is then on disk, and regenerating would delete it. If a reply
	 *    now follows the last question, show it and clear the card.
	 * 3. Only then regenerate. The box, for its part, refuses to start a turn
	 *    while one is live and answers a regenerate for an unsaved question
	 *    by answering it as new — this is the client doing its half.
	 */
	async function retryLastTurn() {
		danglingTurn = false;
		if (!isGhost) {
			// The box refused this send because a turn was still running
			// (turn_in_progress). The message the person just typed was
			// never saved and sits at the end of the transcript; rejoining
			// would stream the OLD reply under it and lose it on reload.
			// Back to the composer it goes, and the rejoin picks up the reply
			// that was already being written.
			if (/turn_in_progress/.test(chat.error?.message ?? "")) {
				const last = chat.messages[chat.messages.length - 1];
				if (last && last.role === "user") {
					const text = messageText(last).trim();
					if (text && !input.trim()) input = text;
					chat.messages = chat.messages.slice(0, -1);
				}
			}
			const settled = { done: false };
			const resume = chat
				.resumeStream()
				.catch((e: unknown) => {
					console.warn("[ChatView] could not rejoin the running turn:", e);
				})
				.finally(() => {
					settled.done = true;
				});
			// Give the GET time to answer. Attached = the SDK moved the status
			// (submitted/streaming) and the promise is still pending.
			const started = Date.now();
			while (!settled.done && Date.now() - started < 8000) {
				if (chat.status === "submitted" || chat.status === "streaming") return;
				await new Promise((r) => setTimeout(r, 50));
			}
			if (chat.status === "submitted" || chat.status === "streaming") return;
			// Still pending after 8s: a slow link, not a 204. Regenerating now
			// could race a rejoin that lands a moment later; leave the card
			// and let the person press again.
			if (!settled.done) return;

			await reloadMessages();
			const last = chat.messages[chat.messages.length - 1];
			if (last && last.role === "assistant") {
				chat.clearError();
				return;
			}
		}
		void chat.regenerate();
	}

	// Chat instance - fetched from shared store to survive remounts
	let chat = $state<Chat>(null!);
	let currentChatConversationId = $state<string | null>(null);
	/** This view is showing the getting-started room. */
	const inRoom = $derived(isGettingStartedChat(currentChatConversationId));
	// The interview's chrome (no thinking block, the resident companion)
	// applies in the old standalone room and in the getting-started room
	// while the interview is underway there.
	// Setup and the interview are both rooms where the machine's workings are
	// not the subject: no thinking block, no tool names.
	const inInterview = $derived(
		currentChatConversationId === INTERVIEW_CHAT_ID || inRoom,
	);

	// Get or create chat instance for the current conversationId
	function ensureChatInstance() {
		if (currentChatConversationId !== conversationId) {
			// Release old instance if we had one
			if (currentChatConversationId) {
				chatInstances.release(currentChatConversationId);
				// A mode belongs to the chat it was chosen in. Sudo above all
				// must not follow the view into the next conversation.
				selectedAgentMode = 'chat';
			}
			// Get or create new instance with model, space, active page, persona, and agent mode getters
			chat = chatInstances.getOrCreate({
				conversationId,
				getModel: () => models.idForWire(),
				// Sent with each message so the agent gets the active-space
				// context block and the server keeps the binding fresh.
				getProjectId: () => chatProjectId,
				getActivePageContext: activePageContext,
				getPersona: () => selectedPersona,
				getAgentMode: () => selectedAgentMode,
				getChatMode: () => chatMode,
				getTemporary: () => isGhost,
				getThink: () => localThink,
			});
			// The draft this conversation left behind, if the composer is empty.
			if (!isGhost && !input) {
				const draft = readDraft(draftId);
				if (draft) input = draft;
			}
			currentChatConversationId = conversationId;
		}
	}

	// Swap the chat instance when the conversation changes, and on first
	// render. `conversationId` is the only dependency on purpose: the rest of
	// what ensureChatInstance reads (the draft, `input`, the instance id it
	// writes) is part of the swap, not a reason to run it again.
	$effect(() => {
		conversationId;
		untrack(ensureChatInstance);
	});

	// A route change is an event, not something derived: the handler below
	// resets and loads, and nothing it reads should re-run it. Only the route
	// and the tab's position in its history.
	$effect(() => {
		const route = tab.route;
		const historyIndex = tab.historyIndex;
		untrack(() => onRouteChange(route, historyIndex));
	});

	/** The tab navigated in place: switch this view to the route's conversation. */
	function onRouteChange(route: string, historyIndex: number) {
		// Navigating to a new chat from a new chat — "New chat" while a
		// temporary chat is open, which never leaves /chat — is a fresh
		// conversation even though the route string did not change.
		const renavigatedToNew = route === previousTabRoute && isNewChat(route) && historyIndex !== previousHistoryIndex;
		previousHistoryIndex = historyIndex;
		if (route === previousTabRoute && !renavigatedToNew) return;
		const routeConversationId = extractConversationId(route);

		// Not a switch: the first message was sent and the tab's route moved
		// from "new" to the persisted id of the same conversation. Keep it all.
		if (isNewChat(previousTabRoute) && routeConversationId === conversationId) {
			previousTabRoute = route;
			return;
		}

		// Cancel any in-flight load from the previous conversation.
		tabSwitchAbortController?.abort();
		tabSwitchAbortController = new AbortController();
		const signal = tabSwitchAbortController.signal;

		previousTabRoute = route;

		// A new chat gets a fresh id; an existing one keeps the route's.
		const newConversationId = routeConversationId || `chat_${generateHex16()}`;
		conversationId = newConversationId;

		// Reset chat state.
		//
		// Order matters: this runs before the instance effect swaps `chat`, so
		// `chat.messages = []` below lands on the instance being left, and the
		// load further down, which awaits first, writes into the new one.
		//
		// EVERYTHING that belongs to ONE conversation resets here. The list
		// was incomplete and each omission was its own bug, because
		// navigation is in-place — `TabContent` keys on `tab.id`, so this
		// component is NOT remounted and anything left behind silently
		// becomes the next conversation's state:
		//
		//  - `isGhost` leaking meant a saved chat inherited "temporary" and
		//    every turn in it went out with `temporary: true`, was never
		//    stored, and was gone on reload — while the toggle that would
		//    undo it is disabled on a non-empty chat. Silent data loss.
		//  - `queuedMessages` leaking sent chat A's follow-up into chat B.
		//  - `input` leaking overwrote B's saved draft with A's text, since
		//    `draftId` is derived from the route and the debounced writer
		//    fires after the switch.
		//  - staged refs pointed into torn-down DOM; staged attachments
		//    rode along on the first send in the new chat.
		//
		// Before adding state to this component, ask whether it belongs to
		// the conversation or to the view. If the conversation: reset here.
		// (Bound pages are not reset: binding belongs to the chat session.)
		chat.messages = [];
		messageMetadata = new Map();
		liveReplies.clear();
		contextUsage = undefined;
		titleDone = false;
		isAwaitingResponse = false;
		turnPhase.reset();
		isGhost = isTemporaryRoute(route);
		danglingTurn = false;
		queuedMessages = [];
		input = "";
		attachments.items = [];
		attachments.dragActive = false;
		// Reset page create tracking (for auto-open)
		tools.reset();

		if (!routeConversationId || isNewChat(route)) {
			// New chat - set chatId so permissions can sync when granted
			editAllowListStore.setChatId(newConversationId, isGhost);
			isLoading = false;
			return;
		}

		isLoading = true;
		void loadRouteConversation(routeConversationId, signal);
	}

	/** Load the conversation a route change switched to, unless another switch overtakes it. */
	async function loadRouteConversation(id: string, signal: AbortSignal) {
		try {
			if (!(await loadTranscript(id, signal))) return;
			resumeIfDangling();
			applyInterviewOpening(chat, id);
			applyRoomInterviewOpening(chat, id, gettingStarted.state);
			// The picker is deliberately left alone on a tab switch: a chat is
			// not pinned to the model that last answered it, so it holds the
			// person's pick across chats, and an unpicked picker keeps showing
			// the slot default.
			await Promise.all([refreshContextUsage(), editAllowListStore.init(id)]);
		} catch (error) {
			// Aborts are expected: the tab switched again mid-load.
			if (error instanceof Error && error.name === "AbortError") return;
			console.error("[ChatView] Error loading conversation on tab change:", error);
		} finally {
			if (!signal.aborted) {
				isLoading = false;
				// Scroll to bottom after loading existing chat — but the
				// getting-started room opens on its cover.
				setTimeout(() => {
					if (!openAtStart()) scrollToBottom("instant");
				}, 10);
			}
		}
	}

	// Rejoin a running turn when the connection or the screen comes back.
	//
	// Mount was the ONLY trigger before, which covers a reload and a tab switch
	// and nothing else — so a view that stayed mounted through a Wi-Fi handover
	// or a locked phone never reconnected, even though the box holds a turn for
	// five minutes precisely so it can be rejoined (live_turn.rs UNATTENDED_CAP).
	// The backend was already doing its half.
	$effect(() => {
		if (!active) return;
		const onBack = () => {
			if (document.visibilityState === "visible") resumeIfDangling();
		};
		window.addEventListener("online", onBack);
		document.addEventListener("visibilitychange", onBack);
		return () => {
			window.removeEventListener("online", onBack);
			document.removeEventListener("visibilitychange", onBack);
		};
	});

	// And when the wire drops while the screen is on. A failed fetch is a
	// TypeError; any other error is the box refusing (turn_in_progress, a
	// wallet), where rejoining would stream an old reply under a message the
	// box never saved — that one waits for the error card's Try again.
	$effect(() => {
		if (!active || chat.status !== "error") return;
		if (chat.error instanceof TypeError) resumeIfDangling();
	});

	// The sidebar's spinner and dot (chatActivity). This view reports its own
	// turn the moment it starts, ahead of the box's list; when the view lets
	// go (another chat, unmount) the box's list takes over, since the turn
	// keeps running there.
	let reportedRunning: string | null = null;
	function reportRunning(id: string | null) {
		if (reportedRunning && reportedRunning !== id) chatActivity.setLocalRunning(reportedRunning, false);
		if (id && reportedRunning !== id) chatActivity.setLocalRunning(id, true);
		reportedRunning = id;
	}
	$effect(() => {
		const running = !isGhost && turnPhase.working;
		const id = conversationId;
		untrack(() => reportRunning(running ? id : null));
	});
	onDestroy(() => reportRunning(null));

	// Seen: this chat is on screen, loaded, and not mid-reply. Re-runs when a
	// reply finishes, so an answer watched as it arrived is never unread.
	let pageVisible = $state(typeof document === "undefined" || !document.hidden);
	$effect(() => {
		const onVis = () => (pageVisible = !document.hidden);
		document.addEventListener("visibilitychange", onVis);
		return () => document.removeEventListener("visibilitychange", onVis);
	});
	$effect(() => {
		if (!active || !pageVisible || isGhost || isLoading || isNewChat(tab.route)) return;
		if (turnPhase.working) return;
		const id = conversationId;
		untrack(() => chatActivity.markSeen(id));
	});

	// Load conversation data on mount
	onMount(() => {
		// Whether this box offers local mode, asked once per session.
		void localModel.load();
		// (The project list used to be fetched here for the breadcrumb's name
		// and accent. The app layout already loads it, and nothing in this view
		// renders a project's name any more.)

		// Claim any prompt handed off from Home / ⌘K / "Ask this project"
		// (consume-once, synchronously — so only this freshly-opened chat sends it).
		const initialPrompt = pendingPrompt.take();
		// If the ask came from a project, bind this new chat to it before the
		// first message so the create path files it + grounds retrieval there.
		const seededProject = pendingPrompt.takeProject();
		if (seededProject) {
			projectStore.setDraft(conversationId, seededProject);
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
						chat_model_id?: string;
						persona?: string;
					}>();
					profileDefaultModelId = profile.chat_model_id;
					profileDefaultPersona = profile.persona;
				} catch (error) {
					console.error("Failed to load assistant profile:", error);
				}
			})();

			const conversationPromise = tabConversationId ? (async () => {
				try {
					await loadTranscript(tabConversationId);
					resumeIfDangling();
				} catch (error) {
					console.error("[ChatView] Error loading conversation:", error);
				}
			})() : null;

			await Promise.all([profilePromise, conversationPromise]);

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
				editAllowListStore.setChatId(conversationId, isGhost);
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
		})();

		return () => {
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

	// Deduplicated messages for rendering
	const uniqueMessages = $derived(chat?.messages ? deduplicateMessages(chat.messages) : []);

	/** How long each stored reply's turn worked, from the span the box
	 *  records with the row (`startedAt`/`endedAt`), for the thinking line of
	 *  a turn this view did not watch. */
	const turnSeconds = $derived.by(() => {
		const out = new Map<string, number>();
		for (const m of uniqueMessages as { id: string; startedAt?: Date; endedAt?: Date }[]) {
			if (!m.startedAt || !m.endedAt) continue;
			const seconds = (m.endedAt.getTime() - m.startedAt.getTime()) / 1000;
			if (seconds >= 0) out.set(m.id, seconds);
		}
		return out;
	});
	/** The interview's opening plate is in this thread — mid-thread here,
	 *  not first as in the old standalone room — so the container must not
	 *  clip paint at its edge. Without this the plate lost both ends. */
	const roomHoldsPlate = $derived(uniqueMessages.some((m) => m.id === GS_INTERVIEW_OPENING_ID));

	// ── the rail ───────────────────────────────────────────────────────────
	// The owner's turns as an index down the left gutter. Not in the two
	// authored rooms: the interview and getting-started are a walk with their
	// own choreography, and a table of contents over a walk is furniture
	// arguing with the floor.
	/** The pane's width, which decides whether there is a gutter to put it in. */
	let pageWidth = $state(0);
	const railTurnList = $derived(
		mobileLayout.isMobile ||
			inRoom ||
			currentChatConversationId === INTERVIEW_CHAT_ID
			? []
			: railTurns(uniqueMessages),
	);

	/** The one line per step that carries its number — keyed by message id. */
	const eyebrowFor = $derived(
		eyebrowsFor(uniqueMessages as { id: string; subject?: string }[]),
	);


	// Which message's Copy just fired, so the tick can replace the icon briefly.
	let copiedMessageId = $state<string | null>(null);

	function messageText(message: any): string {
		return (message.parts ?? [])
			.filter((p: any) => p.type === "text")
			.map((p: any) => p.text)
			.join("");
	}

	async function copyMessage(message: any) {
		const text = messageText(message).trim();
		if (!text) return;
		try {
			await navigator.clipboard.writeText(text);
			copiedMessageId = message.id;
			setTimeout(() => {
				if (copiedMessageId === message.id) copiedMessageId = null;
			}, 1500);
		} catch {
			/* clipboard unavailable (insecure origin, denied) — say nothing */
		}
	}

	// Get the last assistant message
	const lastAssistantMessage = $derived.by(() => {
		for (let i = uniqueMessages.length - 1; i >= 0; i--) {
			if (uniqueMessages[i].role === "assistant") {
				return uniqueMessages[i];
			}
		}
		return null;
	});

	// Local input state
	let input = $state("");
	let inputFocused = $state(false);
	// The hidden figure: "∴" or "therefore" sent into an empty chat opens the
	// three-body orbit instead of sending. Loaded only when asked for.
	let threeBodyOpen = $state(false);

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
		if (!active || !isEmpty || isLoading) return;
		// Small delay to ensure DOM is ready
		const t = setTimeout(() => {
			inputFocused = true;
		}, 50);
		return () => clearTimeout(t);
	});

	// Whether the server has said no automatic title will be written to this
	// chat again. Reset per chat; the server decides from who wrote the
	// title, so asking once after opening an old chat costs a no-op call.
	let titleDone = $state(false);
	let refreshDataTimeout: ReturnType<typeof setTimeout> | null = null;

	// Agent mode and persona selection state - used for tool filtering on backend
	let selectedAgentMode = $state<AgentModeId>('chat');
	// Local mode's "Think first" switch, sent with each local turn.
	let localThink = $state(false);
	const isLocal = $derived(selectedAgentMode === 'local');

	function changeMode(mode: AgentModeId) {
		selectedAgentMode = mode;
	}
	let selectedPersona = $state<string>('default');

	// Retrieval scope. 'scoped' (grounded in a project's items only) still
	// exists on the wire and in the retriever — what's gone is the pill above
	// the composer that switched it, which was a permanent piece of chrome for
	// a setting almost nobody moved. Every chat is 'open': the whole graph,
	// with the project up-weighted when there is one. If scoped comes back it
	// belongs somewhere it can be explained, not as a two-state word.
	const chatMode = 'open' as const;

	// Sync selected model with store (only on initial load). Still a prefill,
	// not a choice — record it as one.
	$effect(() => {
		models.adoptStoreSelection();
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
		if (!inRoom) return;
		// Never while a turn is streaming: the transcript is the SDK's to
		// write then. Reading `status` here re-runs this once it settles.
		if (chat.status !== "ready") return;
		untrack(() => applyRoomInterviewOpening(chat, convId, state));
	});
	$effect(() => {
		if (inRoom) gettingStarted.start();
	});

	/** A turn just settled in the room, so ask the box where the walk stands.
	 *
	 *  The server narrates the coda the moment the assistant's turn is on
	 *  disk (see api/chat.rs), but the CLIENT only learns a step moved on the
	 *  store's 30-second poll — so the ending sat unread while the person
	 *  typed their next message over the top of it. Asking on `ready` closes
	 *  that window: the answer moves the walk signature, and the effect below
	 *  re-reads the thread with the new lines in it. */
	let lastTurnStatus: string | null = null;
	$effect(() => {
		const status = chat.status;
		const wasStreaming = lastTurnStatus === "streaming" || lastTurnStatus === "submitted";
		lastTurnStatus = status;
		if (!inRoom) return;
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
	let lastWalk: string | null = null;
	$effect(() => {
		const st = gettingStarted.state;
		if (!inRoom || !st) return;
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
	// Compared lowercased: openers spell it "New chat" and the registry
	// "New Chat", and an exact match left every tab opened the first way
	// wearing the placeholder for good.
	const PLACEHOLDER_LABELS = new Set(["chat", "new chat", "temporary chat"]);
	$effect(() => {
		if (!chatTitle || !tab) return;
		if (!PLACEHOLDER_LABELS.has(tab.label.toLowerCase())) return;
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
	const canManageChat = $derived(!isEmpty && !isGhost && !inRoom);
	// The menu shows on every chat, a temporary one included: it is the one
	// fixed control in the corner, and on an empty chat it holds the choices
	// that only make sense before the first message — the project, and
	// whether the chat is kept at all.
	const showChatMenu = $derived(!inRoom);

	async function deleteThisChat() {
		// Read before the delete: the title comes off the session row that is
		// about to go, and Undo has to put back the tab this closes.
		const name = chatTitle;
		const reopen = routeIfOpen(`/chat/${conversationId}`);
		try {
			windowShellStore.closeTabsByRoute(`/chat/${conversationId}`);
			await deleteChat(conversationId);
			chatSessions.remove(conversationId);
			windowShellStore.invalidateViewCache("chat");
			notifyTrashed({ kind: "chat", id: conversationId, name, reopen });
		} catch (e) {
			console.error("[ChatView] Failed to delete chat:", e);
			toast.error(`Your server couldn't delete "${name}"`, {
				description: "It's still here. Try again",
			});
		}
	}

	function openChatMenu(e: MouseEvent) {
		const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
		const pinned = !!windowShellStore.findTab((t) => t.id === tab.id)?.tab.pinned;
		const items: ContextMenuItem[] = [];
		if (contextUsage && extractConversationId(tab.route)) {
			const n = new Intl.NumberFormat(undefined, { notation: "compact", maximumFractionDigits: 1 });
			items.push({
				id: "context",
				label: `Context ${Math.round(contextUsage.percentage)}%`,
				description: `${n.format(contextUsage.tokens)} of ${n.format(contextUsage.window)} tokens`,
				icon: "ri:donut-chart-line",
				iconColor:
					contextUsage.status === "critical"
						? "var(--color-error)"
						: contextUsage.status === "warning"
							? "var(--color-warning)"
							: undefined,
				dividerAfter: true,
				action: handleContextClick,
			});
		}
		// Offered only while it can still be true: a turn that has been sent
		// cannot be taken back out of storage.
		if (isEmpty) {
			items.push({
				id: "temporary",
				label: "Temporary chat",
				description: "Not saved, not remembered",
				icon: "ri:ghost-line",
				checked: isGhost,
				dividerAfter: true,
				action: toggleGhost,
			});
		}
		// Filed by url once saved, by draft until then; the same menu either
		// way. A temporary chat has nowhere to be filed.
		if (!isGhost) {
			items.push(...projectMenuItems(targetForTab(tab)).map((i) => ({ ...i, dividerBefore: false })));
		}
		items.push({
			id: "pin",
			label: pinned ? "Unpin tab" : "Pin tab",
			icon: pinned ? "ri:unpin-line" : "ri:pushpin-line",
			action: () => windowShellStore.togglePin(tab.id),
		});
		if (canManageChat) {
			items.push({
				id: "delete",
				label: "Delete chat",
				icon: "ri:delete-bin-line",
				variant: "destructive",
				dividerBefore: true,
				action: deleteThisChat,
			});
		}
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
		if (titleDone || chat.messages.length < 2) return;
		// The interview keeps the name it was seeded with. Its transcript is
		// the most private text on the box, and a generated title puts a
		// summary of it in the sidebar — this chat had renamed itself after
		// the person's own childhood. The server refuses this too (the id
		// decides, never the client); this only saves the round trip.
		if (conversationId === INTERVIEW_CHAT_ID || isGettingStartedChat(conversationId)) {
			titleDone = true;
			return;
		}

		try {
			const data = await setChatTitle<{ title?: string; done?: boolean }>({
				chatId: conversationId,
				messages: chat.messages.map((m) => ({
					role: m.role,
					content: m.parts.find((p) => p.type === "text")?.text || "",
				})),
			});

			// The server answers every ask with the title it has, generated or
			// kept, and `done` once it will never write another. A server that
			// predates `done` generated on every ask, so its answer stops us.
			titleDone = data.done ?? true;
			if (data.title) {
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
	 * Replies this view watched arrive. Their words keep fading in after the
	 * stream ends rather than being rebuilt as plain text (see Markdown's
	 * `animate`); a reply loaded from history never fades.
	 */
	const liveReplies = new SvelteSet<string>();
	$effect(() => {
		if (chat?.status !== "streaming") return;
		const last = chat.messages[chat.messages.length - 1];
		if (last?.role === "assistant") untrack(() => liveReplies.add(last.id));
	});

	// The end of a turn changes heights above a reader scrolled into it, and
	// WebKit does not anchor scroll. `.pre` so the reading line is measured
	// before the DOM takes the change.
	let releaseHold: (() => void) | null = null;
	let wasRunning = false;
	$effect.pre(() => {
		const running = chat?.status === "streaming" || chat?.status === "submitted";
		if (wasRunning && !running && scrollContainer) {
			releaseHold?.();
			releaseHold = holdReadingPosition(scrollContainer);
		}
		wasRunning = running;
	});
	onDestroy(() => releaseHold?.());

	/**
	 * Where a room opens. A chat opens at its newest message; the
	 * getting-started room opens at the TOP, on the cover — the painting is
	 * the first thing in it and the first thing anyone should see, and on a
	 * short window the walk's own length was starting it half scrolled off.
	 * Only on load: once the person is in the conversation, new turns pull
	 * the view down as they do anywhere else.
	 */
	function openAtStart(behavior: ScrollBehavior = "instant") {
		if (inRoom) {
			scrollContainer?.scrollTo({ top: 0, behavior });
			return true;
		}
		return false;
	}

	/**
	 * Whether the reader has scrolled up out of the newest turn. Drives the
	 * jump-to-latest button above the composer; the threshold is generous so
	 * the last line's own padding does not count as "away".
	 */
	let awayFromEnd = $state(false);
	$effect(() => {
		const scroller = scrollContainer;
		if (!scroller) return;
		const measure = () => {
			awayFromEnd = scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight > 240;
		};
		measure();
		// The transcript growing, and the scroller itself resizing (a phone's
		// keyboard, a rotation), both move the end without a scroll event.
		const content = new ResizeObserver(measure);
		content.observe(scroller);
		if (scroller.firstElementChild) content.observe(scroller.firstElementChild);
		scroller.addEventListener("scroll", measure, { passive: true });
		return () => {
			content.disconnect();
			scroller.removeEventListener("scroll", measure);
		};
	});

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

		// And drop anything waiting behind it. Stop is the one gesture that has
		// to mean "nothing more": the queue used to survive it and then drain
		// the instant the drain effect saw `ready` again, so stopping a turn
		// with three follow-ups queued sent all three. The SDK's `stop()`
		// resolves rather than throwing, so `handleChatSubmit`'s `finally`
		// clears `isAwaitingResponse` and the effect fires immediately.
		//
		// Handed back to the composer rather than discarded when there is just
		// one, since a queued line is something the person typed and has not
		// seen sent. More than one cannot go in a single-line composer, so they
		// are dropped — the chips are gone from the screen either way.
		if (queuedMessages.length === 1 && !input.trim()) {
			input = queuedMessages[0];
		}
		queuedMessages = [];

		// Mark the in-flight assistant message as user-stopped so the "Stopped"
		// notice shows immediately (reload reads the persisted subject='cancelled').
		// Only the reply being written: stopped before it existed, the last
		// assistant message is the PREVIOUS turn's, which was not stopped.
		const tail = uniqueMessages[uniqueMessages.length - 1];
		const stoppedId = tail?.role === "assistant" ? tail.id : undefined;
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
		// The box saves the stopped turn once its loop has wound down, after
		// this returns. Read usage once it has, and once more for a slow one.
		setTimeout(turnWritten, 1500);
		setTimeout(turnWritten, 5000);
	}

	/** Move a new chat's tab from "/chat" to its own id. The route is what a
	 *  reload reopens, so it moves the moment the box has the turn: a reload
	 *  mid-reply then rejoins the turn instead of opening a blank chat while
	 *  the reply finishes unseen. A temporary chat keeps its route; the box
	 *  has nothing to reopen. */
	async function promoteNewChatRoute() {
		if (isGhost || !isNewChat(tab.route)) return;
		const newRoute = `/chat/${conversationId}`;
		// Set first, so `onRouteChange` reads the move as this conversation
		// getting its id rather than a switch that resets the view.
		previousTabRoute = newRoute;
		windowShellStore.updateTab(tab.id, { route: newRoute });
		// The draft key moves with the route; a debounced write of the cleared
		// composer to the old key would be dropped and the sent text would come
		// back in the next new chat.
		writeDraft(NEW_CHAT_DRAFT_ID, "");
		await editAllowListStore.markChatCreated();
		windowShellStore.invalidateViewCache("chat");
	}

	// The box has stored the chat and the user's message before its first
	// byte (`store_user_turn` runs ahead of the stream), so "streaming" is the
	// earliest moment the new route is sure to reopen something.
	$effect(() => {
		if (chat.status !== "streaming") return;
		untrack(() => void promoteNewChatRoute());
	});

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

		if (isEmpty && /^(∴|therefore)$/i.test(messageToSend)) {
			input = "";
			threeBodyOpen = true;
			return;
		}

		// Track E1: block sending if an attachment isn't supported by the active
		// model — the capability banner prompts a switch instead.
		if (models.capabilityIssue) return;

		if (!messageToSend && attachments.count === 0) return;

		// After an error the SDK sits in "error" until something clears it,
		// and nothing did: Send greyed out, Enter queued the text into a chip
		// that never drained, the draft gone. The next message IS the
		// recovery — clear the card and send it.
		if (chat.status === "error") chat.clearError();

		if (chat.status !== "ready" || turnPhase.working) {
			// Queue text; attachments stay staged and ride along when the drain
			// effect re-sends this once the current turn finishes.
			queuedMessages = [...queuedMessages, messageToSend];
			input = "";
			return;
		}
		input = "";

		// Capture + clear attachments as AI SDK file parts. The snapshot is what
		// goes back in the tray if this send never reaches the box (see the catch):
		// `items` is replaced wholesale on every change, so the reference is stable.
		const stagedBeforeSend = attachments.items;
		const files = attachments.takeAsFileParts();
		// Everything after sendMessage resolves — the title, the session refresh —
		// is inside the same try. Without this flag a throw from any of THOSE
		// would put an already-sent message back in the composer and its files
		// back in the tray, next to the copies sitting in the transcript.
		let handedOff = false;

		// A new turn answers the dangling one, whatever it was.
		danglingTurn = false;

		// New turn → clear any leftover Deep Research panel from the previous turn.
		chatInstances.clearSubagents(conversationId);

		// The turn is working from here, before the network round-trip.
		isAwaitingResponse = true;
		turnPhase.beginSend();
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

			handedOff = true;

			if (chat.messages.length >= 2 && !isGhost && !titleDone) {
				await generateTitle();
				await chatSessions.refresh();
			}

			if (refreshDataTimeout) {
				clearTimeout(refreshDataTimeout as any);
			}
			refreshDataTimeout = setTimeout(() => {
				turnWritten();
				refreshDataTimeout = null;
			}, 2000);
		} catch (error) {
			console.error("[handleChatSubmit] Error:", error);
			// Nothing was sent, so nothing should have been consumed. Both the
			// text and the files were already cleared on the optimistic path;
			// hand them back rather than leaving the person to retype and
			// re-drag. A staged ref comes back inside the text, since it was
			// serialized into it before the send — flattened, but not lost.
			//
			// Narrow on purpose. A transport failure does NOT land here —
			// measured: the SDK keeps it, the message is already in
			// `chat.messages` with its file parts, and ChatError offers "Try
			// again". What reaches this catch is a throw BEFORE sendMessage
			// resolves, the permissions sync above being the one in the path
			// today, where nothing entered the transcript to retry from.
			if (!handedOff) {
				input = messageToSend;
				attachments.restore(stagedBeforeSend);
			}
		} finally {
			isAwaitingResponse = false;
			turnPhase.endSend();
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
		// The allow list has to know: a ghost's grants stay in the box's memory
		// and never become rows.
		if (conversationId) editAllowListStore.setChatId(conversationId, isGhost);
		// The tab says what the chat is, and its route is what a reload or a
		// restored window reopens, so both follow the switch. The route moves
		// first in `previousTabRoute` so the navigation handler reads this as
		// the same conversation, not a switch — a switch would clear the
		// draft being typed. A label the user renamed by hand is left alone.
		const route = isGhost ? "/chat?temporary=1" : "/chat";
		previousTabRoute = route;
		windowShellStore.updateTab(tab.id, {
			route,
			icon: isGhost ? "ri:ghost-line" : "ri:chat-1-line",
			...(PLACEHOLDER_LABELS.has(tab.label) && {
				label: isGhost ? "Temporary Chat" : "New Chat",
			}),
		});
	}

	// Publish this chat's state to the phone shell, whose top-right button is
	// modal: ghost toggle while the chat is empty (compose would be a no-op),
	// compose once a conversation exists. Only the active view speaks; the
	// cleanup keeps a stale claim from surviving a view swap.
	$effect(() => {
		if (!mobileLayout.isMobile || !active) return;
		mobileLayout.setChatChrome({
			empty: isEmpty,
			ghost: isGhost,
			toggleGhost,
			project: isGhost ? null : (chatProject ?? null),
			showProjectMenu: showProjectChipMenu,
		});
		return () => mobileLayout.setChatChrome(null);
	});
</script>

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
	<ContextViewPanel {conversationId} {active} onCompacted={reloadMessages} />
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
		<div class="chat-container">
			<!-- Main chat area -->
			<div class="chat-area" class:ghost={isGhost}>
				{#if chatProject && !isGhost && !mobileLayout.isMobile && !inRoom}
					<div class="chat-topbar-left">
						<ProjectChip
							project={chatProject}
							title={`Open ${chatProject.name}`}
							onclick={openChatProject}
							oncontextmenu={showProjectChipMenu}
						/>
					</div>
				{/if}
				<!-- Top-right chrome: the chat menu (context usage and the temporary
				     switch live in it) -->
				<div class="chat-topbar-right">
					{#if inRoom}
						<!-- One door. A glyph through the walk, and a word
						     ("Stop for now") once the interview is underway,
						     which is the one beat with no controls of its own.
						     Nothing else lives up here — progress is numbered in
						     the thread, on the axis the eye is already reading
						     along. -->
						<GettingStartedDoor />
					{/if}
					{#if showChatMenu}
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
				<div class="page-container" class:is-empty={isEmpty} bind:clientWidth={pageWidth}>
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
								inRoom}
							class:room={inRoom}
						>
							{#if inRoom && uniqueMessages.length > 0}
								<!-- The frontispiece, at the measure of the words.
								     It ran the full width of the pane as an oil
								     painting and made the room read as two products
								     stacked. -->
								<RoomCover />
							{/if}
							{#if isLocal && uniqueMessages.length > 0}
								<LocalModelCard bind:think={localThink} />
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
											{@const isStreaming = turnPhase.working && isLastMessage}
											<!-- Narration, tool calls and reasoning go to the thinking
											     block; the reply starts at bodyFromIndex (see splitTurn). -->
											{@const turn = splitTurn(message.parts, isStreaming)}

											{@const subagents =
												isLastMessage
													? chatInstances.getSubagents(
															conversationId,
														)
													: []}
											{#if subagents.length > 0}
												<SubagentPanel
													{subagents}
													variant="research"
												/>
											{/if}

											{#if !inInterview && (turn.hasThinkingContent || (isStreaming && isLastMessage))}
												<!-- Interview room excluded: the companion below is
												     its one indicator, and the model's reasoning
												     about the person must never surface as chrome
												     in the room built on their own account. -->
												<ThinkingBlock
													isThinking={isStreaming}
													startedAt={turnPhase.startedAt}
													stalled={isStreaming && turnPhase.stalled}
													toolCalls={turn.toolParts}
													reasoningContent={turn.reasoning}
													narration={turn.narration}
													intent={turn.intent}
													seconds={turnPhase.secondsFor(message.id) ||
														turnSeconds.get(message.id) ||
														0}
													land={!stopReason(messageMetadata.get(message.id)) &&
														!chat.error}
													agentMode={selectedAgentMode}
												/>
											{/if}
											<TurnFigures parts={message.parts} />

											{#if eyebrowFor.has(message.id)}
												<StepEyebrow stepId={eyebrowFor.get(message.id)!} />
											{/if}
											{#each message.parts as part, partIndex (part.type === "text" ? `text-${partIndex}` : (part as any).toolCallId || `part-${partIndex}`)}
												{#if part.type === "text" && part.text.trim() && partIndex >= turn.bodyFromIndex}
													{@const shown = reveal.revealed(message.id, part.text)}
													<!-- No `text-base`. Size and leading are `--md-body-*`,
													     which resolve to the same 1rem/1.5 Tailwind was
													     setting - one declaration, so a change to the
													     register reaches chat rather than passing it by. -->
													<div
														class="text-foreground assistant-response"
													>
														<Markdown
															content={shown.content}
															isStreaming={isStreaming || shown.arriving}
															animate={isStreaming || shown.arriving || liveReplies.has(message.id)}
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
												<MessageFile part={part as any} onOpenImage={openLightbox} />
											{:else if part.type.startsWith("tool-") && (part as any).state === "output-available" && (part as any).output?.permission_needed}
												<!-- Any gated tool (run_action, delete_action, …) awaiting the user's "I allow" -->
												{@const output = (part as any).output}
												<PageBindingInline
													entityId={output.entity_id}
													entityType={output.entity_type}
													entityTitle={output.entity_title}
													message={output.message}
													permissionMode={true}
													onAllow={handlePermissionAllow}
													onDeny={() => {}}
												/>
											{:else if part.type === "tool-record_introductions"}
												<!-- Nothing here: the receipt goes under the whole
												     turn, not wherever in it the model reached for
												     the tool. See after this loop. -->
											{:else if part.type === "tool-write_it_up" && inRoom && (part as any).state === "output-available" && (part as any).output?.document_page_id}
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
												{@const status = toolStatus(toolPart, isStreaming)}
												{@const isError = status === "failed" || status === "unfinished"}
												<CodeInterpreterCard
													status={status === "running" ? 'running' : isError ? 'error' : 'success'}
													code={toolPart.input?.code || ''}
													output={toolPart.output ?? (isError ? { error: status === "unfinished" ? "This didn't finish." : toolPart.errorText } : undefined)}
												/>
											{:else if part.type === "tool-generate_image"}
												{@const gen = part as any}
												{#if gen.state === "output-available" && gen.output?.url}
													<figure class="generated-image">
														<MessageFile
															part={{
																mediaType: "image/*",
																url: gen.output.url,
																filename: gen.output.prompt || gen.input?.prompt || "Generated image",
															}}
															onOpenImage={openLightbox}
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
												{:else if part.type.startsWith("tool-") && (part as any).state === "output-error" && !inInterview && !turnMovedPast(message.parts, partIndex)}
													{@const errorText = (part as any).errorText as string | undefined}
													{@const errorDetail = toolErrorDetail(errorText)}
													<!-- Only a failure the turn ENDED on: it is what the person
													     got instead of an answer. One the model recovered from
													     is in the thinking block (see turnMovedPast).
													     A live static tool part carries no `toolName` (only its
													     `tool-<name>` type); a reloaded row does. Read the name
													     off the type so the line never says "Error:  failed".
													     The text was written for the model: its first line is
													     the message, and the column list it was handed sits
													     behind a disclosure. -->
													<div
														class="tool-error mb-3 text-sm text-error p-3 bg-error-subtle rounded-lg"
													>
														<span class="font-medium">Your assistant couldn't finish</span>
														{(part as any).toolName ?? part.type.slice("tool-".length)}.
														Ask it to try again, or narrow what you asked for.{#if errorText}
															{toolErrorSummary(errorText)}{/if}
														{#if errorDetail}
															<details class="mt-2">
																<summary class="cursor-pointer text-xs opacity-80">Details</summary>
																<pre class="mt-1 whitespace-pre-wrap font-mono text-xs opacity-90">{errorDetail}</pre>
															</details>
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
											{@const stop = stopReason(messageMetadata.get(message.id))}
											{#if stop}
												<StoppedNotice reason={stop} />
											{/if}
											{#if chatInstances.getLocalStats(conversationId, message.id)}
												<LocalStatsLine stats={chatInstances.getLocalStats(conversationId, message.id)!} />
											{/if}

											<!-- What to do with an answer once it exists.
											     `chat.regenerate()` was reachable ONLY through the
											     error card, so an answer that is wrong but did not
											     FAIL had no re-roll at all, and copying one meant
											     dragging a selection across rendered markdown.
											     Only on a finished turn — controls under a
											     sentence still being written invite a click that
											     races the stream. Re-roll is offered on the last
											     answer alone, because regenerating an earlier one
											     would silently discard every turn after it, which
											     is an edit-and-branch feature and not this. -->
											{#if chat.status === "ready" && messageText(message).trim()}
												<div class="message-actions">
													<button
														type="button"
														class="message-action"
														aria-label="Copy this reply"
														title="Copy"
														onclick={() => copyMessage(message)}
													>
														<Icon
															icon={copiedMessageId === message.id
																? "ri:check-line"
																: "ri:file-copy-line"}
															width="14"
														/>
													</button>
													<!-- Not in the rooms: their last line is the room's own
													     narration, and a re-roll there re-asks a step. -->
													{#if message.id === lastAssistantMessage?.id && !isGhost && !inInterview}
														<button
															type="button"
															class="message-action"
															aria-label="Ask for another answer"
															title="Try another answer"
															onclick={() => {
																danglingTurn = false;
																void chat.regenerate();
															}}
														>
															<Icon icon="ri:refresh-line" width="14" />
														</button>
													{/if}
												</div>
											{/if}
										{:else}
											{@const fileParts = message.parts.filter((p: any) => p.type === "file")}
											{#if fileParts.length > 0}
												<div class="msg-attachments">
													{#each fileParts as fp, i (i)}
														<MessageFile part={fp as any} compact onOpenImage={openLightbox} />
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


							{#if inRoom}
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
								<!-- One mark, two poses: the ∴ moving while a turn works,
								     the same ∴ holding still while it does not. Always
								     something in the margin, so nothing jumps when the turn
								     starts — and the room is never empty of its occupant. -->
								{#if turnPhase.working}
									<Composing label="Composing a reply" />
								{:else}
									<RestingMark />
								{/if}
							{:else if inInterview}
								{#if reveal.chars || turnPhase.working}
									<Composing label="Composing a reply" />
								{:else}
									<RestingMark />
								{/if}
							{:else if turnPhase.awaitingReply}
								<!-- The turn before its reply exists: from Send until the
								     box's first event, which can be seconds while it
								     compacts. The block inside the reply takes over from
								     the same clock, so the count does not restart. -->
								<div class="flex justify-start">
									<div class="message-wrapper" data-role="assistant">
										<ThinkingBlock
											isThinking={true}
											startedAt={turnPhase.startedAt}
											stalled={turnPhase.stalled}
											toolCalls={[]}
											agentMode={selectedAgentMode}
										/>
									</div>
								</div>
							{/if}

							<!-- A turn the box began and never finished. Not an error —
							     nothing failed on the wire, the box simply stopped
							     existing mid-turn — so it gets the same quiet chip as
							     the other partial endings rather than the error card,
							     plus the one thing the person actually needs, which is
							     a way to ask again without retyping. -->
							{#if danglingTurn && !chat.error}
								<div class="dangling-turn">
									<StoppedNotice reason="no_reply" />
									<button
										type="button"
										class="dangling-retry"
										onclick={() => {
											danglingTurn = false;
											void chat.regenerate();
										}}
									>
										Try again
									</button>
								</div>
							{/if}

							<ChatError
											error={chat.error ?? null}
											onRetry={() => void retryLastTurn()}
											recommendedName={models.recommendedFallback?.displayName}
											onSwitchAndRetry={models.recommendedFallback
												? switchToRecommendedAndRetry
												: undefined}
										/>
						</div>
					</div>

					<!-- The turns index, in the gutter beside the column. Outside the
					     scroller so it holds still while the transcript moves, and only
					     once the pane is wide enough to have a gutter — below that the
					     column takes the whole pane and the rail would sit on the words. -->
					{#if !isEmpty && pageWidth >= 1000}
						<ConversationRail turns={railTurnList} {scrollContainer} />
					{/if}

					{#if isEmpty && threeBodyOpen}
						{#await import("$lib/components/chat/ThreeBody.svelte") then { default: ThreeBody }}
							<ThreeBody onClose={() => (threeBodyOpen = false)} />
						{/await}
					{:else if isEmpty && !isGhost && attachments.count === 0}
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
							<svg class="init-mark" viewBox="0 0 12 10.5" width="19.87" height="17.39" fill="currentColor">
								<circle class="init-dot init-dot-1" cx="6" cy="2.4" r="1.5" />
								<circle class="init-dot init-dot-2" cx="2.6" cy="8.1" r="1.5" />
								<circle class="init-dot init-dot-3" cx="9.4" cy="8.1" r="1.5" />
							</svg>
							<span class="init-word">Virtues</span>
						</div>
					{/if}

					{#if isEmpty && isLocal}
						<div class="local-hero" in:fade={{ duration: 300 }}>
							<LocalModelCard bind:think={localThink} />
						</div>
					{:else if isEmpty && isGhost}
						<div
							class="ghost-hero"
							in:fade={{ duration: 300 }}
							out:fly={{ y: -14, duration: 300, easing: cubicInOut }}
						>
							<Icon icon="ri:ghost-line" width="22" class="ghost-hero-glyph" />
							<h1 class="ghost-hero-title">Temporary Chat</h1>
							<p class="ghost-hero-note">Not saved, not remembered. Closing the tab ends it.</p>
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
						{#if !isEmpty && awayFromEnd}
							<button
								type="button"
								class="jump-to-end"
								onclick={() => scrollToBottom("smooth")}
								aria-label="Scroll to latest"
								title="Scroll to latest"
								transition:fade={{ duration: 150 }}
							>
								<Icon icon="ri:arrow-down-line" width="16" />
							</button>
						{/if}
						{#if isGhost && !isEmpty}
							<div class="ghost-caption" in:fade={{ duration: 300 }}>
								<Icon icon="ri:ghost-line" width="12" />
								<span>Temporary chat · not saved</span>
							</div>
						{/if}
						<ComposerTray
							dragActive={attachments.dragActive}
							attachments={attachments.items}
							onRemoveAttachment={(id) => attachments.remove(id)}
							capabilityIssue={models.capabilityIssue}
							onSwitchModel={() => models.switchToCapable()}
							queued={queuedMessages}
							onRemoveQueued={removeQueued}
						/>
						{#if interviewClosed}
							<!-- The interview is over: no composer, the two doors instead. -->
							<InterviewClosedCard
								standing
								pageId={interviewDocumentPageId}
								chaptersWritten={interviewClosedPart?.chapters_written ?? 0}
								alreadyExisted={interviewClosedPart?.document_already_existed ?? false}
								chaptersError={interviewClosedPart?.chapters_error ?? null}
							/>
						{:else if inRoom && !gettingStarted.aiConnected}
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
							allowEmptySubmit={attachments.count > 0}
							onAttach={(f) => attachments.add(f)}
							bind:value={input}
							bind:focused={inputFocused}
							disabled={false}
							sendDisabled={turnPhase.working || (isLocal && !localModel.status.ready)}
							isStreaming={turnPhase.working}
							maxWidth="max-w-3xl"
							placeholder={isLocal ? "Ask the local model" : isGhost ? "Ask Virtues (temporary)" : "Ask Virtues"}
							onSubmit={(text) => handleChatSubmit(text)}
							onStop={() => handleChatStop()}
							agentMode={selectedAgentMode}
							onModeChange={changeMode}
							modes={availableModes({ localSupported: localModel.status.supported })}
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
	.chat-root {
		height: 100%;
		width: 100%;
		display: flex;
		position: relative;
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

	/* The project this chat lives in: a label that is also the way back.
	   Quiet until approached, and held to the top-left corner so it never
	   competes with the thread for the measure. */
	.chat-topbar-left {
		position: absolute;
		top: 8px;
		left: 12px;
		z-index: 6;
		max-width: 40%;
	}



	/* 28px of visible chip, 44pt of reachable square — the chip is deliberately
	   small and floats over the transcript, so the target grows around it
	   rather than under it. */
	@media (max-width: 768px), (pointer: coarse) {
		/* (The composer's phone padding lives in the docked-composer block at
		   the end of these styles — it must follow the base rules to win the
		   cascade.) */

		.chat-menu-btn {
			position: relative;
		}

		.chat-menu-btn::after {
			content: "";
			position: absolute;
			top: 50%;
			left: 50%;
			width: 44px;
			height: 44px;
			transform: translate(-50%, -50%);
		}
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

	/* A temporary chat is the ordinary page with one thing changed: the
	   composer flips to the theme's ink, so the mode is a material change
	   under your fingers rather than a room dressed up as another place.
	   Done by remapping the pill's tokens — everything inside (placeholder,
	   buttons, the model pill) follows on its own. The originals are
	   captured one scope up because a custom property cannot swap with
	   itself in place. */
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

	/* Once the title has gone, the caption over the composer is what still
	   says the conversation is not being kept — seated on the input, where
	   the eye already is, rather than in a corner. */
	.ghost-caption {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 5px;
		padding-bottom: 0.5rem;
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
	}

	@media (prefers-reduced-motion: reduce) {
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

	/* The local card sits where the ghost title does, above the centered
	   composer, at the measure of the words. It holds a button and a switch,
	   so it takes clicks, and it scrolls within itself on a short pane. */
	.local-hero {
		position: absolute;
		left: 0;
		right: 0;
		bottom: calc(50% + 52px);
		z-index: 2;
		display: flex;
		justify-content: center;
		padding: 0 16px;
		max-height: calc(50% - 64px);
		overflow-y: auto;
	}
	.local-hero > :global(*) {
		width: 100%;
		max-width: 40em;
	}

	.ghost-hero-title {
		font-family: var(--font-serif);
		font-size: 1.75rem;
		font-weight: 400;
		color: var(--color-foreground);
	}

	.ghost-hero :global(.ghost-hero-glyph) {
		color: var(--color-foreground-subtle);
		margin-bottom: 0.25rem;
	}

	.ghost-hero-note {
		font-size: 0.875rem;
		color: var(--color-foreground-subtle);
	}

	/* ── The phone's opening image ──
	   The ∴ mark and wordmark, seated in the upper half of the empty room —
	   above center so the (bottom-docked) composer and rising keyboard never
	   crowd it. Mark and word sit on one line, the mark a touch under the
	   word's size so the two read as one lockup. The entrance is the mark
	   ASSEMBLING: three dots settle into the trivet one by one, then the
	   word surfaces beside them. All
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
		align-items: center;
		justify-content: center;
		gap: 0.5rem;
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

	/* Rides the composer's top edge, so it climbs with a growing draft. */
	.jump-to-end {
		position: absolute;
		bottom: calc(100% + 0.5rem);
		/* Flush with the pill's right edge (the wrapper's side padding). */
		right: 2rem;
		display: grid;
		place-items: center;
		width: 2.25rem;
		height: 2.25rem;
		border-radius: 999px;
		border: 1px solid var(--color-border);
		background: var(--color-surface-elevated);
		color: var(--color-foreground-muted);
		cursor: pointer;
		transition:
			color 0.15s ease,
			border-color 0.15s ease,
			translate 0.15s ease;
	}

	.jump-to-end:hover {
		color: var(--color-foreground);
		border-color: var(--color-border-strong);
		translate: 0 1px;
	}

	.jump-to-end:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: 2px;
	}

	/* Track E1 — in-message media */
	.msg-attachments {
		display: flex;
		flex-wrap: wrap;
		gap: 0.5rem;
		margin-bottom: 0.5rem;
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

	/* Assistant response text - spacing after thinking block */
	.assistant-response {
		padding-top: 4px;
	}

	.dangling-turn {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		margin-top: 0.25rem;
	}

	.dangling-retry {
		font-size: 0.75rem;
		color: var(--color-primary);
		background: none;
		border: none;
		padding: 0;
		cursor: pointer;
	}

	.dangling-retry:hover {
		text-decoration: underline;
	}

	/* Quiet until the answer is hovered — an answer should read as prose, not
	   as a toolbar with text above it. Always visible on a touch screen, where
	   there is no hover to reveal them. */
	.message-actions {
		display: flex;
		gap: 0.125rem;
		margin-top: 0.375rem;
		opacity: 0;
		transition: opacity 0.12s ease;
	}

	.message-wrapper:hover .message-actions,
	.message-actions:focus-within {
		opacity: 1;
	}

	@media (hover: none) {
		.message-actions {
			opacity: 1;
		}
	}

	.message-action {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		padding: 0.25rem;
		border: none;
		background: none;
		border-radius: 6px;
		color: var(--foreground-subtle);
		cursor: pointer;
	}

	.message-action:hover {
		color: var(--foreground);
		background: var(--surface-elevated);
	}

</style>
