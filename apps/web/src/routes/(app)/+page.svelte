<script lang="ts">
	import Page from "$lib/components/Page.svelte";
	import ChatInput from "$lib/components/ChatInput.svelte";
	import TableOfContents from "$lib/components/TableOfContents.svelte";
	import type { ModelOption } from "$lib/config/models";
	import {
		getSelectedModel,
		initializeSelectedModel,
		getInitializationPromise,
	} from "$lib/stores/models.svelte";
	import CitedMarkdown from "$lib/components/CitedMarkdown.svelte";
	import { CitationPanel } from "$lib/components/citations";
	import { buildCitationContextFromParts } from "$lib/citations";
	import type { Citation, CitationContext } from "$lib/types/Citation";
	import UserMessage from "$lib/components/UserMessage.svelte";
	import ThinkingBlock from "$lib/components/ThinkingBlock.svelte";
	import { onMount } from "svelte";
	import { goto } from "$app/navigation";
	import type { PageData } from "./$types";
	import { chatSessions } from "$lib/stores/chatSessions.svelte";
	import { Chat } from "@ai-sdk/svelte";
	import { DefaultChatTransport } from "ai";

	// Get data from page loader
	let { data }: { data: PageData } = $props();

	// UI state
	let conversationId = $state(data.conversationId);
	let messagesContainer: HTMLDivElement | null = $state(null);
	let scrollContainer: HTMLDivElement | null = $state(null);
	let enableTransitions = $state(false);

	// UI preferences from assistant profile
	let uiPreferences = $state<{
		contextIndicator?: {
			alwaysVisible?: boolean;
			showThreshold?: number;
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

	// Helper function to convert database messages to Chat parts
	// Formats stored messages to match AI SDK v6 UIMessagePart structure
	function convertMessageToParts(msg: any) {
		// Store metadata for rendering
		if (msg.agentId || msg.provider) {
			messageMetadata.set(msg.id, {
				agentId: msg.agentId,
				provider: msg.provider,
			});
		}

		const parts: any[] = [];

		// Add reasoning content if present (for historical messages)
		if (msg.reasoning) {
			console.log(
				"[convertMessageToParts] Found reasoning in DB:",
				msg.reasoning.slice(0, 50),
			);
			parts.push({
				type: "reasoning" as const,
				text: msg.reasoning,
				state: "done" as const,
			});
		}

		// Add text content
		if (msg.content) {
			parts.push({
				type: "text" as const,
				text: msg.content,
			});
		}

		// Add tool calls if they exist
		// Tool calls from DB are already completed, so state is always "output-available"
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
	// This prevents Svelte each_key_duplicate errors from race conditions or DB issues
	function deduplicateMessages(messages: any[]): any[] {
		if (!messages || messages.length === 0) return [];
		const seen = new Set<string>();
		return messages.filter((msg) => {
			if (seen.has(msg.id)) {
				console.warn(
					"[deduplicateMessages] Duplicate message ID detected:",
					msg.id,
				);
				return false;
			}
			seen.add(msg.id);
			return true;
		});
	}

	// Initialize Chat instance (Svelte uses classes instead of hooks)
	const chat = new Chat({
		id: conversationId,
		transport: new DefaultChatTransport({
			api: "/api/chat",
			prepareSendMessagesRequest: ({ id, messages }) => {
				return {
					body: {
						sessionId: conversationId,
						model: selectedModel?.id || "openai/gpt-oss-120b",
						agentId: selectedAgent,
						messages,
					},
				};
			},
		}),
		messages: deduplicateMessages(data.messages || []).map((msg: any) => ({
			id: msg.id,
			role: msg.role as "user" | "assistant",
			parts: convertMessageToParts(msg),
		})),
		// Error handling callback
		onError: (error) => {
			console.error("[Chat] Error occurred:", error);
			// Error is automatically stored in chat.error state
		},
	});

	// Derive thinking state from chat status (single source of truth)
	const isThinking = $derived.by(() => {
		const status = chat.status;
		const thinking = status === "submitted" || status === "streaming";

		// Defensive logging for unexpected states
		if (!["ready", "submitted", "streaming", "error"].includes(status)) {
			console.warn("[isThinking] Unexpected chat status:", status);
		}

		return thinking;
	});

	// Deduplicated messages for rendering - prevents each_key_duplicate errors
	// This handles duplicates that may appear during streaming
	const uniqueMessages = $derived.by(() => {
		const seen = new Set<string>();
		return chat.messages.filter((msg) => {
			if (seen.has(msg.id)) {
				return false;
			}
			seen.add(msg.id);
			return true;
		});
	});

	// Get the last assistant message (for checking streaming state)
	const lastAssistantMessage = $derived.by(() => {
		for (let i = uniqueMessages.length - 1; i >= 0; i--) {
			if (uniqueMessages[i].role === "assistant") {
				return uniqueMessages[i];
			}
		}
		return null;
	});

	// Check if actual text content is streaming (not just tool calls)
	const isStreamingContent = $derived.by(() => {
		if (chat.status !== "streaming") return false;
		if (!lastAssistantMessage) return false;
		// Check if there's actual text content beyond just tool calls
		return lastAssistantMessage.parts.some(
			(p: any) => p.type === "text" && p.text && p.text.trim().length > 0,
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

	// Local input state for ChatInput component
	let input = $state("");
	let inputFocused = $state(false);

	// Track the last loaded conversation ID to avoid overwriting messages during active chat
	// Initialize to null to ensure first load always triggers the effect
	let lastLoadedConversationId = $state<string | null>(null);

	// Update when conversation changes (navigation to different conversation)
	$effect(() => {
		// Only reload messages if we're navigating to a DIFFERENT conversation
		// Don't overwrite messages during active streaming in the same conversation
		if (data.conversationId !== lastLoadedConversationId) {
			// Abort any pending chat requests to prevent race condition
			if (chat.status === "streaming" || chat.status === "submitted") {
				if (chat.clearError) {
					chat.clearError();
				}
			}

			// Disable transitions temporarily during navigation
			enableTransitions = false;

			// Update conversation ID
			conversationId = data.conversationId;
			lastLoadedConversationId = data.conversationId;

			// Note: Chat.id is read-only and was already set during initialization

			// Clear thinking timeout when switching conversations
			if (thinkingTimeout) {
				clearTimeout(thinkingTimeout);
				thinkingTimeout = null;
			}

			// Update messages in Chat instance only when switching conversations
			chat.messages = deduplicateMessages(data.messages || []).map(
				(msg: any) => ({
					id: msg.id,
					role: msg.role as "user" | "assistant",
					parts: convertMessageToParts(msg),
				}),
			);

			// Re-enable transitions after a brief moment
			setTimeout(() => {
				enableTransitions = true;
			}, 50);
		}
	});

	// Title generation state
	let titleGenerated = $state(false);
	let inactivityTimer: ReturnType<typeof setTimeout> | null = null;
	let refreshDataTimeout: ReturnType<typeof setTimeout> | null = null;
	const INACTIVITY_TIMEOUT = 15 * 60 * 1000; // 15 minutes

	// Model selection state
	const selectedModel = $derived(getSelectedModel());

	// Initialize selectedModel once loaded
	$effect(() => {
		if (!selectedModel) {
			initializeSelectedModel(data.conversation?.model);
		}
	});

	// Agent selection state (default to 'auto' for intelligent routing)
	let selectedAgent = $state("auto");

	// Context tracking - estimate tokens from message content
	// Rough estimate: ~4 characters per token
	let cumulativeTokens = $state(0);

	// Calculate cumulative tokens from messages
	// Only run after each turn completes (not during streaming) to avoid expensive recalculation
	$effect(() => {
		if (chat.status !== "streaming") {
			if (chat.messages.length > 0) {
				const totalChars = chat.messages.reduce((sum, msg) => {
					const content = msg.parts
						.filter((p: any) => p.type === "text")
						.map((p: any) => p.text)
						.join(" ");
					return sum + content.length;
				}, 0);
				// Rough estimate: 4 chars per token
				cumulativeTokens = Math.ceil(totalChars / 4);
			} else {
				cumulativeTokens = 0;
			}
		}
	});

	// Reactive messages with subjects (can be updated independently of page data)
	let messagesWithSubjects = $state<any[]>(data.messages || []);

	// Update when page data changes
	$effect(() => {
		messagesWithSubjects = data.messages || [];
	});

	// Extract exchanges for table of contents
	// An exchange is: one user message + all following assistant/tool messages until next user message
	const exchanges = $derived.by(() => {
		const result: Array<{
			index: number;
			subject: string | undefined;
			userContent: string;
		}> = [];

		let currentExchangeIndex = 0;

		for (const message of chat.messages) {
			if (message.role === "user") {
				// Extract text content from user message
				const userContent = message.parts
					.filter((p: any) => p.type === "text")
					.map((p: any) => p.text)
					.join("");

				// Get subject from message metadata (loaded from database)
				// The subject is stored in messagesWithSubjects (reactive)
				const originalMessage = messagesWithSubjects?.find(
					(m: any) => m.id === message.id,
				);
				const subject = originalMessage?.subject;

				result.push({
					index: currentExchangeIndex,
					subject,
					userContent,
				});

				currentExchangeIndex++;
			}
		}

		return result;
	});

	// Refresh session data to get updated subjects
	async function refreshSessionData() {
		// Only run in browser, not during SSR
		if (typeof window === "undefined") return;

		try {
			const response = await fetch(`/api/sessions/${conversationId}`);
			if (response.ok) {
				const sessionData = await response.json();
				messagesWithSubjects = sessionData.messages || [];
			}
		} catch (error) {
			console.error("[refreshSessionData] Error:", error);
		}
	}

	// Load assistant profile defaults on mount for new conversations
	onMount(async () => {
		// Wait for models to load first
		await getInitializationPromise();

		try {
			const response = await fetch("/api/assistant-profile");
			if (response.ok) {
				const profile = await response.json();

				// Load UI preferences (for all conversations)
				if (profile.ui_preferences) {
					uiPreferences = profile.ui_preferences;
				}

				// Apply defaults only for new conversations
				if (data.isNew) {
					// Apply default agent if set
					if (profile.default_agent_id) {
						selectedAgent = profile.default_agent_id;
					}

					// Apply default model if set
					if (profile.default_model_id && !selectedModel) {
						initializeSelectedModel(
							data.conversation?.model,
							profile.default_model_id,
						);
					}
				}
			}
		} catch (error) {
			console.error("Failed to load assistant profile defaults:", error);
			// Continue with hardcoded defaults if profile fetch fails
		}
	});

	// Safety timeout - keep as failsafe but should rarely trigger
	let thinkingTimeout: ReturnType<typeof setTimeout> | null = null;
	$effect(() => {
		if (isThinking) {
			thinkingTimeout = setTimeout(() => {
				// Try to abort the stream if possible
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
			}, 30000);

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
		const hour = new Date().getHours();

		if (hour >= 3 && hour < 12) {
			return "Good Morning";
		} else if (hour >= 12 && hour < 17) {
			return "Good Afternoon";
		} else {
			return "Good Evening";
		}
	}

	let greeting = $state(getTimeBasedGreeting());

	// Generate title after first assistant response
	async function generateTitle() {
		if (titleGenerated || chat.messages.length < 2) return;

		try {
			const response = await fetch("/api/sessions/title", {
				method: "POST",
				headers: {
					"Content-Type": "application/json",
				},
				body: JSON.stringify({
					sessionId: conversationId,
					messages: chat.messages.map((m) => ({
						role: m.role,
						content:
							m.parts.find((p) => p.type === "text")?.text || "",
					})),
				}),
			});

			if (response.ok) {
				await response.json();
				titleGenerated = true;
			}
		} catch (error) {
			// Title generation is non-critical, fail silently
		}
	}

	// Reset inactivity timer and potentially refine title
	function resetInactivityTimer() {
		if (inactivityTimer) {
			clearTimeout(inactivityTimer);
		}

		// Only set timer if we already have a title and messages
		if (titleGenerated && chat.messages.length > 0) {
			inactivityTimer = setTimeout(async () => {
				// Regenerate title with full conversation context
				try {
					const response = await fetch("/api/sessions/title", {
						method: "POST",
						headers: {
							"Content-Type": "application/json",
						},
						body: JSON.stringify({
							sessionId: conversationId,
							messages: chat.messages.map((m) => ({
								role: m.role,
								content:
									m.parts.find((p) => p.type === "text")
										?.text || "",
							})),
						}),
					});

					if (response.ok) {
						await response.json();
					}
				} catch (error) {
					// Title refinement is non-critical, fail silently
				}
			}, INACTIVITY_TIMEOUT);
		}
	}

	async function handleChatSubmit(value: string) {
		// Store trimmed message value immediately
		const messageToSend = value.trim();
		if (!messageToSend) return;

		// Block submission if chat is not in ready state
		// (prevents multiple submissions during "submitted", "streaming", or "error" states)
		if (chat.status !== "ready") {
			return;
		}

		// Clear input IMMEDIATELY to prevent double-submission
		input = "";

		// Reset inactivity timer on user activity
		resetInactivityTimer();

		try {
			// Send message using Chat class (handles streaming automatically)
			await chat.sendMessage({ text: messageToSend });

			// Generate title after first exchange
			if (chat.messages.length === 2) {
				await generateTitle();
				// Update URL with conversationId and refresh sidebar
				goto(`/?conversationId=${conversationId}`, {
					replaceState: true,
					noScroll: true,
				});
				// Refresh sidebar to show new conversation
				await chatSessions.refresh();
			}

			// Refresh session data after a short delay to get generated subjects
			// Subject generation happens in the backend onFinish callback
			// Clear any pending refresh first to avoid multiple calls
			if (refreshDataTimeout) {
				clearTimeout(refreshDataTimeout);
			}
			refreshDataTimeout = setTimeout(() => {
				refreshSessionData();
				refreshDataTimeout = null;
			}, 2000); // Wait 2 seconds for subject generation to complete
		} catch (error) {
			console.error("[handleChatSubmit] Error:", error);
			// Clear input even on error so user can try again
			input = "";
		}
		// No finally block needed - thinking state is derived from chat.status!
	}

	// Cleanup timers on unmount
	// Note: thinkingTimeout is cleaned up by its own effect
	onMount(() => {
		return () => {
			// Only clean up timers not managed by effects
			if (inactivityTimer) {
				clearTimeout(inactivityTimer);
			}
			if (refreshDataTimeout) {
				clearTimeout(refreshDataTimeout);
			}
		};
	});
</script>

<Page scrollable={false} className="h-full p-0!">
	<!-- Table of Contents -->
	<TableOfContents {exchanges} />

	<div class="chat-container">
		<!-- Main chat area -->
		<div class="chat-area">
			<div class="page-container" class:is-empty={isEmpty}>
				<!-- Messages area - scrollable, fades in when not empty -->
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
										.filter((m) => m.role === "user").length
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
											(p) => p.type === "text" && p.text,
										)}
								>
									{#if message.role === "assistant"}
										<!-- Build citation context from tool calls -->
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
											chat.status === "streaming" &&
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
												.map((p: any) => p.text || "")
												.filter(Boolean)
												.join("\n")}
										{@const hasThinkingContent =
											messageReasoning ||
											messageToolParts.length > 0}

										<!-- ThinkingBlock at top of assistant message (persists with message) -->
										{#if hasThinkingContent}
											<ThinkingBlock
												isThinking={isStreaming &&
													isLastMessage &&
													chat.status === "streaming"}
												toolCalls={messageToolParts}
												reasoningContent={messageReasoning}
												{isStreaming}
												duration={isLastMessage
													? thinkingDuration
													: 0}
											/>
										{/if}

										<!-- Render text parts with inline citations -->
										{#each message.parts as part, partIndex (part.type === "text" ? `text-${partIndex}` : (part as any).toolCallId || `part-${partIndex}`)}
											{#if part.type === "text"}
												<div
													class="text-base text-neutral-900"
												>
													<CitedMarkdown
														content={part.text}
														{isStreaming}
														citations={citationContext}
														onCitationClick={openCitationPanel}
													/>
												</div>
											{:else if part.type.startsWith("tool-") && (part as any).state === "output-error"}
												<!-- Only show tool errors, not successful tool calls -->
												<div
													class="tool-error mb-3 text-sm text-red-600 p-3 bg-red-50 rounded-lg"
												>
													<span class="font-medium"
														>Error:</span
													>
													{(part as any).toolName} failed
													{#if (part as any).errorText}
														- {(part as any)
															.errorText}
													{/if}
												</div>
											{/if}
										{/each}
									{:else}
										<!-- User messages: collapsible for long messages -->
										<UserMessage
											text={message.parts
												.filter(
													(p) => p.type === "text",
												)
												.map((p) => p.text)
												.join("")}
										/>
									{/if}
								</div>
							</div>
						{/each}

						<!-- Show thinking indicator for new streaming message before any parts arrive -->
						{#if isThinking && (!lastAssistantMessage || lastAssistantMessage.parts.length === 0)}
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

						<!-- Error message with retry button -->
						{#if chat.error}
							{@const isRateLimitError =
								chat.error.message?.includes(
									"Rate limit exceeded",
								) ||
								chat.error.message?.includes("rate limit") ||
								chat.error.message?.includes("429")}
							<div class="flex justify-start">
								<div
									class="error-container"
									class:rate-limit-error={isRateLimitError}
								>
									<div class="error-icon">
										<iconify-icon
											icon={isRateLimitError
												? "ri:time-line"
												: "ri:error-warning-line"}
											width="20"
										></iconify-icon>
									</div>
									<div class="error-content">
										<div class="error-title">
											{isRateLimitError
												? "Rate Limit Reached"
												: "An error occurred"}
										</div>
										<div class="error-message">
											{#if isRateLimitError}
												You've reached your API usage
												limit. Please wait for the limit
												to reset or check your usage
												dashboard for details.
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
													<iconify-icon
														icon="ri:bar-chart-line"
														width="16"
													></iconify-icon>
													View Usage Dashboard
												</a>
											{:else}
												<button
													type="button"
													class="retry-button"
													onclick={() => {
														// Clear error and retry last message
														chat.regenerate();
													}}
												>
													<iconify-icon
														icon="ri:refresh-line"
														width="16"
													></iconify-icon>
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

				<!-- ChatInput - animates from center to bottom -->
				<div
					class="chat-input-wrapper"
					class:is-empty={isEmpty}
					class:has-messages={!isEmpty}
					class:transitions-enabled={enableTransitions}
					class:focused={inputFocused}
				>
					<!-- Hero section - fades out when chat starts -->
					<div
						class="hero-section"
						class:visible={isEmpty}
						class:transitions-enabled={enableTransitions}
					>
						<h1
							class="hero-title shiny-title font-serif text-4xl text-navy mb-6"
						>
							{greeting}, Adam
						</h1>
					</div>

					<ChatInput
						bind:value={input}
						bind:focused={inputFocused}
						disabled={false}
						sendDisabled={chat.status !== "ready"}
						placeholder="Message..."
						maxWidth="max-w-3xl"
						on:submit={(e) => {
							if (chat.status === "ready") {
								handleChatSubmit(e.detail);
							}
						}}
					/>
				</div>
			</div>
		</div>
	</div>
</Page>

<!-- Citation detail panel (slide-out) -->
<CitationPanel
	citation={selectedCitation}
	open={citationPanelOpen}
	onClose={closeCitationPanel}
/>

<style>
	/* Chat container layout */
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
	}

	.chat-layout.visible {
		opacity: 1;
		pointer-events: auto;
	}

	/* Custom scrollbar for chat messages - always visible */
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
		max-width: 48rem; /* max-w-3xl */
		margin: 0 auto;
		width: 100%;
		padding: 3rem 3rem 12rem 3rem;
		display: flex;
		flex-direction: column;
		gap: 2rem;
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
		max-width: 48rem; /* max-w-3xl */
		padding: 0 3rem 3rem 3rem;
		background: var(--color-background);
		box-sizing: border-box;
		z-index: 10;
	}

	/* Only apply transitions when explicitly enabled */
	.chat-input-wrapper.transitions-enabled {
		transition:
			bottom 0.6s cubic-bezier(0.4, 0, 0.2, 1),
			transform 0.6s cubic-bezier(0.4, 0, 0.2, 1);
	}

	/* When empty, center the input vertically */
	.chat-input-wrapper.is-empty {
		bottom: auto;
		top: 50%;
		transform: translateY(-50%);
	}

	/* Reduce bottom padding when there are active messages */
	.chat-input-wrapper.has-messages {
		padding-bottom: 1.5rem;
	}

	/* When not empty, make it sticky so it doesn't scroll */
	.page-container:not(.is-empty) .chat-input-wrapper {
		position: sticky;
	}

	.hero-section {
		text-align: center;
		opacity: 0;
		max-height: 0;
		overflow: hidden;
	}

	/* Only apply transitions when explicitly enabled */
	.hero-section.transitions-enabled {
		transition:
			opacity 0.3s ease-in-out,
			max-height 0.3s ease-in-out;
	}

	.hero-section.visible {
		opacity: 1;
		max-height: 300px;
	}

	.hero-title {
		text-align: center;
	}

	.message-wrapper {
		position: relative;
		width: 100%;
		padding: 0.75rem 0; /* py-3 */
	}

	/* Remove top offsets on headings so content starts at wrapper top */
	.message-wrapper :global(h1),
	.message-wrapper :global(h2),
	.message-wrapper :global(h3),
	.message-wrapper :global(h4) {
		margin-top: 0;
	}

	/* Colored dot indicator for messages */
	.message-wrapper::before {
		content: "";
		position: absolute;
		width: 0.375rem;
		height: 0.375rem;
		border-radius: 9999px;
		left: -1.2rem;
		top: 1.35rem;
	}

	/* Hide dot when message is still loading */
	.message-wrapper[data-loading="true"]::before {
		display: none;
	}

	/* Blue dot for user messages only */
	.message-wrapper[data-role="user"]::before {
		background-color: var(--color-primary);
	}

	/* Hide dot for assistant messages */
	.message-wrapper[data-role="assistant"]::before {
		display: none;
	}

	/* Error container styles */
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

	/* Rate limit specific error styles */
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

	.error-container.rate-limit-error .error-message {
		color: var(--color-foreground-muted);
	}

	.error-actions {
		margin-top: 0.5rem;
	}

	.retry-button {
		display: inline-flex;
		align-items: center;
		gap: 0.375rem;
		padding: 0.375rem 0.75rem;
		background-color: var(--color-surface);
		border: 1px solid var(--color-error);
		border-radius: 0.375rem;
		color: var(--color-error);
		font-size: 0.875rem;
		font-weight: 500;
		cursor: pointer;
		transition: all 0.15s ease;
		align-self: flex-start;
	}

	.retry-button:hover {
		background-color: var(--color-error-subtle);
		border-color: var(--color-error);
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
		align-self: flex-start;
	}

	.usage-link:hover {
		background-color: var(--color-warning-subtle);
		border-color: var(--color-warning);
	}

	.usage-link:active {
		transform: scale(0.98);
	}

	.shiny-title {
		overflow: visible;
		padding-bottom: 0.25rem;
	}

	/* Shiny gradient effect for hero title when textarea is focused */
	.chat-input-wrapper.focused .shiny-title {
		background-image: -webkit-linear-gradient(
			left,
			var(--color-primary) 0%,
			var(--color-primary) 30%,
			transparent 55%,
			var(--color-foreground) 80%,
			var(--color-foreground) 100%
		);
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
		text-fill-color: transparent;
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
