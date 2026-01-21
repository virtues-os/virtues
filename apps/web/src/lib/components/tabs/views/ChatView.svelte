<script lang="ts">
	import type { Tab } from '$lib/stores/windowTabs.svelte';
	import { windowTabs } from '$lib/stores/windowTabs.svelte';
	import Page from '$lib/components/Page.svelte';
	import ChatInput from '$lib/components/ChatInput.svelte';
	import ContextWarningToast from '$lib/components/ContextWarningToast.svelte';
	import GettingStarted from '$lib/components/GettingStarted.svelte';
	import {
		getSelectedModel,
		getDefaultModel,
		initializeSelectedModel,
		getInitializationPromise
	} from '$lib/stores/models.svelte';
	import CitedMarkdown from '$lib/components/CitedMarkdown.svelte';
	import { CitationPanel } from '$lib/components/citations';
	import { buildCitationContextFromParts } from '$lib/citations';
	import type { Citation } from '$lib/types/Citation';
	import UserMessage from '$lib/components/UserMessage.svelte';
	import ThinkingBlock from '$lib/components/ThinkingBlock.svelte';
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { chatSessions } from '$lib/stores/chatSessions.svelte';
	import { Chat } from '@ai-sdk/svelte';
	import { DefaultChatTransport } from 'ai';

	// Props
	let { tab, active }: { tab: Tab; active: boolean } = $props();

	// UI state
	let conversationId = $state(tab.conversationId || crypto.randomUUID());
	let messagesContainer: HTMLDivElement | null = $state(null);
	let scrollContainer: HTMLDivElement | null = $state(null);
	let enableTransitions = $state(false);
	let isLoading = $state(true);
	let loadedMessages = $state<any[]>([]);

	// Track tab.conversationId to reset state when switching conversations
	let previousTabConversationId = $state<string | undefined>(tab.conversationId);
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
	let messageMetadata = $state<Map<string, { agentId?: string; provider?: string }>>(new Map());

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

	// Context usage state
	interface ContextUsageState {
		percentage: number;
		tokens: number;
		window: number;
		status: 'healthy' | 'warning' | 'critical';
	}
	let contextUsage = $state<ContextUsageState | undefined>(undefined);
	let showContextWarning = $state(false);
	let hasShownWarning = $state(false);

	// Fetch context usage from API
	async function refreshContextUsage() {
		if (!conversationId || !tab.conversationId) return;

		try {
			const res = await fetch(`/api/sessions/${conversationId}/usage`);
			if (!res.ok) return;

			const data = await res.json();
			const status: 'healthy' | 'warning' | 'critical' =
				data.usage_percentage >= 85 ? 'critical' :
				data.usage_percentage >= 70 ? 'warning' : 'healthy';

			contextUsage = {
				percentage: data.usage_percentage,
				tokens: data.total_tokens,
				window: data.context_window,
				status
			};

			// Show warning toast once when crossing 70%
			if (status === 'warning' && !hasShownWarning) {
				showContextWarning = true;
				hasShownWarning = true;
			}
		} catch {
			// Non-critical, continue without usage data
		}
	}

	// Handle context indicator click - open context tab in split view
	function handleContextClick() {
		const currentPane = windowTabs.findTabPane(tab.id);
		windowTabs.openSessionContext(conversationId, currentPane);
	}

	// Handle compact from warning toast
	async function handleCompactNow() {
		if (!conversationId) return;

		try {
			await fetch(`/api/sessions/${conversationId}/compact`, {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ force: true })
			});
			await refreshContextUsage();
			showContextWarning = false;
		} catch {
			// Handle error silently
		}
	}

	// Helper function to convert database messages to Chat parts
	function convertMessageToParts(msg: any) {
		if (msg.agentId || msg.provider) {
			messageMetadata.set(msg.id, {
				agentId: msg.agentId,
				provider: msg.provider
			});
		}

		const parts: any[] = [];

		if (msg.reasoning) {
			parts.push({
				type: 'reasoning' as const,
				text: msg.reasoning,
				state: 'done' as const
			});
		}

		if (msg.content) {
			parts.push({
				type: 'text' as const,
				text: msg.content
			});
		}

		if (msg.tool_calls && Array.isArray(msg.tool_calls)) {
			for (const toolCall of msg.tool_calls) {
				parts.push({
					type: `tool-${toolCall.tool_name}` as const,
					toolCallId: toolCall.tool_call_id || `${msg.id}_${toolCall.tool_name}_${Date.now()}`,
					toolName: toolCall.tool_name,
					input: toolCall.arguments,
					state: 'output-available' as const,
					output: toolCall.result
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

	// Getter functions to ensure Chat transport always uses current values
	// (closures capture initial values, so we need indirection)
	function getCurrentModel() {
		// Use selected model, or fall back to database default from store
		return selectedModelValue?.id || getDefaultModel()?.id || '';
	}
	function getCurrentConversationId() {
		return conversationId;
	}

	// Initialize Chat instance
	const chat = new Chat({
		id: conversationId,
		transport: new DefaultChatTransport({
			api: '/api/chat',
			prepareSendMessagesRequest: ({ id, messages }) => {
				return {
					body: {
						sessionId: getCurrentConversationId(),
						model: getCurrentModel(),
						agentId: 'auto',
						messages
					}
				};
			}
		}),
		messages: [],
		onError: (error) => {
			console.error('[Chat] Error occurred:', error);
		}
	});

	// Watch for tab.conversationId changes to reset state when switching conversations
	$effect(() => {
		const currentTabConversationId = tab.conversationId;

		// If the tab's conversationId changed, reset the chat state
		if (currentTabConversationId !== previousTabConversationId) {
			// Cancel any in-flight requests from previous tab
			tabSwitchAbortController?.abort();
			tabSwitchAbortController = new AbortController();
			const signal = tabSwitchAbortController.signal;

			previousTabConversationId = currentTabConversationId;

			// Generate new conversationId for new chats, or use the tab's conversationId
			const newConversationId = currentTabConversationId || crypto.randomUUID();
			conversationId = newConversationId;

			// Reset chat state
			chat.messages = [];
			loadedMessages = [];
			messageMetadata = new Map();
			contextUsage = undefined;
			titleGenerated = false;
			hasShownWarning = false;
			thinkingIndicatorVisible = false;
			minTimeElapsed = false;

			// Load conversation if switching to an existing one
			if (currentTabConversationId) {
				isLoading = true;
				(async () => {
					try {
						const response = await fetch(`/api/sessions/${currentTabConversationId}`, { signal });
						if (signal.aborted) return; // Check if we were aborted
						if (response.ok) {
							const data = await response.json();
							if (signal.aborted) return; // Check again after parsing
							loadedMessages = data.messages || [];
							chat.messages = deduplicateMessages(loadedMessages).map((msg: any) => ({
								id: msg.id,
								role: msg.role as 'user' | 'assistant',
								parts: convertMessageToParts(msg)
							}));
							if (data.conversation?.model) {
								initializeSelectedModel(data.conversation.model);
							}
							await refreshContextUsage();
						}
					} catch (error) {
						// Ignore abort errors - they're expected when switching tabs
						if (error instanceof Error && error.name === 'AbortError') return;
						console.error('[ChatView] Error loading conversation on tab change:', error);
					} finally {
						if (!signal.aborted) {
							isLoading = false;
						}
					}
				})();
			} else {
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
		try {
			const profileResponse = await fetch('/api/assistant-profile');
			if (profileResponse.ok) {
				const profile = await profileResponse.json();
				if (profile.ui_preferences) {
					uiPreferences = profile.ui_preferences;
				}
			}
		} catch (error) {
			console.error('Failed to load assistant profile:', error);
		}

		// Load preferred name from profile
		try {
			const response = await fetch('/api/profile');
			if (response.ok) {
				const profile = await response.json();
				preferredName = profile.preferred_name;
			}
		} catch (error) {
			// Non-critical, continue without preferred name
		}

		// Load conversation if it exists
		if (tab.conversationId) {
			try {
				const response = await fetch(`/api/sessions/${tab.conversationId}`);
				if (response.ok) {
					const data = await response.json();
					loadedMessages = data.messages || [];

					// Update chat messages
					chat.messages = deduplicateMessages(loadedMessages).map((msg: any) => ({
						id: msg.id,
						role: msg.role as 'user' | 'assistant',
						parts: convertMessageToParts(msg)
					}));

					// Initialize model from conversation
					if (data.conversation?.model) {
						initializeSelectedModel(data.conversation.model);
					}

					// Load initial context usage
					await refreshContextUsage();
				}
			} catch (error) {
				console.error('[ChatView] Error loading conversation:', error);
			}
		}

		isLoading = false;
		setTimeout(() => {
			enableTransitions = true;
		}, 50);
		})();

		return () => {
			if (inactivityTimer) clearTimeout(inactivityTimer);
			if (refreshDataTimeout) clearTimeout(refreshDataTimeout);
			tabSwitchAbortController?.abort();
		};
	});

	// Derive thinking state from chat status
	const isThinking = $derived.by(() => {
		const status = chat.status;
		return status === 'submitted' || status === 'streaming';
	});

	// Deduplicated messages for rendering
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

	// Get the last assistant message
	const lastAssistantMessage = $derived.by(() => {
		for (let i = uniqueMessages.length - 1; i >= 0; i--) {
			if (uniqueMessages[i].role === 'assistant') {
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
			(p: any) => p.type === 'text' && p.text
		);
		const totalText = textParts.map((p: any) => p.text).join('').trim();
		return totalText.length > 20;
	});

	// Effect: Show indicator when thinking starts, with minimum display timer
	$effect(() => {
		if (isThinking && !thinkingIndicatorVisible) {
			thinkingIndicatorVisible = true;
			minTimeElapsed = false;
			const timer = setTimeout(() => { minTimeElapsed = true; }, 500);
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
	let input = $state('');
	let inputFocused = $state(false);

	// Title generation state
	let titleGenerated = $state(false);
	let inactivityTimer: ReturnType<typeof setTimeout> | null = null;
	let refreshDataTimeout: ReturnType<typeof setTimeout> | null = null;

	// Model selection state - use bindable for ChatInput toolbar
	let selectedModelValue = $state<import('$lib/config/models').ModelOption | undefined>(undefined);

	// Sync selected model with store (only on initial load)
	$effect(() => {
		const storeModel = getSelectedModel();
		if (storeModel && !selectedModelValue) {
			selectedModelValue = storeModel;
		}
	});

	// Reactive messages with subjects
	// Refresh session data (placeholder for future use)
	async function refreshSessionData() {
		// Function intentionally empty - can be used for future session data refresh needs
	}

	// Safety timeout
	let thinkingTimeout: ReturnType<typeof setTimeout> | null = null;
	$effect(() => {
		if (isThinking) {
			thinkingTimeout = setTimeout(() => {
				if (chat.status === 'error') {
					chat.clearError();
				} else if (chat.status === 'streaming' || chat.status === 'submitted') {
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
			return 'Good Morning';
		} else if (hour >= 12 && hour < 17) {
			return 'Good Afternoon';
		} else {
			return 'Good Evening';
		}
	}

	let greeting = $state(getTimeBasedGreeting());

	// Generate title after first assistant response
	async function generateTitle() {
		if (titleGenerated || chat.messages.length < 2) return;

		try {
			const response = await fetch('/api/sessions/title', {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({
					sessionId: conversationId,
					messages: chat.messages.map((m) => ({
						role: m.role,
						content: m.parts.find((p) => p.type === 'text')?.text || ''
					}))
				})
			});

			if (response.ok) {
				const data = await response.json();
				titleGenerated = true;
				// Update tab label with the new title
				if (data.title) {
					windowTabs.updateTab(tab.id, { label: data.title });
				}
			}
		} catch (error) {
			// Title generation is non-critical
		}
	}

	// Getting Started handlers
	function handleGettingStartedCreateSession(sessionId: string) {
		// Open the new session in a tab and navigate to it
		windowTabs.openTabFromRoute(`/?conversationId=${sessionId}`, {
			forceNew: true,
			label: 'Welcome to Virtues'
		});
		chatSessions.refresh();
	}

	function handleGettingStartedFocusInput(placeholder?: string) {
		if (placeholder) {
			input = placeholder;
		}
		inputFocused = true;
	}

	async function handleChatSubmit(value: string) {
		const messageToSend = value.trim();
		if (!messageToSend) return;

		if (chat.status !== 'ready') {
			return;
		}

		input = '';

		try {
			await chat.sendMessage({ text: messageToSend });

			if (chat.messages.length === 2) {
				await generateTitle();
				// Update tab with conversationId if it's a new chat
				if (!tab.conversationId) {
					// Update previousTabConversationId first to prevent the tab-switch effect
					// from treating this as a tab change and resetting state
					previousTabConversationId = conversationId;
					console.log('[ChatView] Updating tab with conversationId:', {
						tabId: tab.id,
						conversationId,
						newRoute: `/?conversationId=${conversationId}`
					});
					windowTabs.updateTab(tab.id, {
						conversationId,
						route: `/?conversationId=${conversationId}`
					});
				}
				await chatSessions.refresh();
			}

			if (refreshDataTimeout) {
				clearTimeout(refreshDataTimeout);
			}
			refreshDataTimeout = setTimeout(() => {
				refreshSessionData();
				// Refresh context usage after message completes
				refreshContextUsage();
				refreshDataTimeout = null;
			}, 2000);
		} catch (error) {
			console.error('[handleChatSubmit] Error:', error);
			input = '';
		}
	}
</script>

{#if isLoading}
	<div class="loading-container">
		<div class="loading-spinner"></div>
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
								{@const isUserMessage = message.role === 'user'}
								{@const exchangeIndex = isUserMessage
									? uniqueMessages.slice(0, messageIndex).filter((m) => m.role === 'user').length
									: -1}
								<div
									class="flex justify-start"
									id={isUserMessage ? `exchange-${exchangeIndex}` : undefined}
								>
									<div
										class="message-wrapper"
										data-role={message.role}
										data-agent-id={messageMetadata.get(message.id)?.agentId || 'general'}
										data-loading={message.role === 'assistant' &&
											!message.parts.some((p) => p.type === 'text' && p.text)}
									>
										{#if message.role === 'assistant'}
											{@const citationContext = buildCitationContextFromParts(message.parts)}
											{@const isLastMessage =
												message.id === uniqueMessages[uniqueMessages.length - 1]?.id}
											{@const isStreaming = chat.status === 'streaming' && isLastMessage}
											{@const messageReasoningParts = message.parts.filter(
												(p: any) => p.type === 'reasoning'
											)}
											{@const messageToolParts = message.parts.filter((p: any) =>
												p.type.startsWith('tool-')
											)}
											{@const messageReasoning = messageReasoningParts
												.map((p: any) => p.text || '')
												.filter(Boolean)
												.join('\n')}
											{@const hasThinkingContent =
												messageReasoning || messageToolParts.length > 0}

											{#if hasThinkingContent}
												<ThinkingBlock
													isThinking={isStreaming && isLastMessage && chat.status === 'streaming'}
													toolCalls={messageToolParts}
													reasoningContent={messageReasoning}
													{isStreaming}
													duration={isLastMessage ? thinkingDuration : 0}
												/>
											{/if}

											{#each message.parts as part, partIndex (part.type === 'text' ? `text-${partIndex}` : (part as any).toolCallId || `part-${partIndex}`)}
												{#if part.type === 'text'}
													<div class="text-base text-foreground assistant-response">
														<CitedMarkdown
															content={part.text}
															{isStreaming}
															citations={citationContext}
															onCitationClick={openCitationPanel}
														/>
													</div>
												{:else if part.type.startsWith('tool-') && (part as any).state === 'output-error'}
													<div class="tool-error mb-3 text-sm text-error p-3 bg-error-subtle rounded-lg">
														<span class="font-medium">Error:</span>
														{(part as any).toolName} failed
														{#if (part as any).errorText}
															- {(part as any).errorText}
														{/if}
													</div>
												{/if}
											{/each}
										{:else}
											<UserMessage
												text={message.parts
													.filter((p) => p.type === 'text')
													.map((p) => p.text)
													.join('')}
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
									chat.error.message?.includes('Rate limit exceeded') ||
									chat.error.message?.includes('rate limit') ||
									chat.error.message?.includes('429')}
								<div class="flex justify-start">
									<div class="error-container" class:rate-limit-error={isRateLimitError}>
										<div class="error-icon">
											<iconify-icon
												icon={isRateLimitError ? 'ri:time-line' : 'ri:error-warning-line'}
												width="20"
											></iconify-icon>
										</div>
										<div class="error-content">
											<div class="error-title">
												{isRateLimitError ? 'Rate Limit Reached' : 'An error occurred'}
											</div>
											<div class="error-message">
												{#if isRateLimitError}
													You've reached your API usage limit. Please wait for the limit to reset
													or check your usage dashboard for details.
												{:else}
													{chat.error.message || 'Something went wrong. Please try again.'}
												{/if}
											</div>
											<div class="error-actions">
												{#if isRateLimitError}
													<a href="/usage" class="usage-link">
														<iconify-icon icon="ri:bar-chart-line" width="16"></iconify-icon>
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
														<iconify-icon icon="ri:refresh-line" width="16"></iconify-icon>
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
						<div class="hero-section" class:visible={isEmpty} class:transitions-enabled={enableTransitions}>
							<h1 class="hero-title shiny-title font-serif text-4xl text-navy mb-6">
								{greeting}{preferredName ? `, ${preferredName}` : ''}
							</h1>
						</div>

						<ChatInput
							bind:value={input}
							bind:focused={inputFocused}
							bind:selectedModel={selectedModelValue}
							disabled={false}
							sendDisabled={chat.status !== 'ready'}
							maxWidth="max-w-3xl"
							showToolbar={true}
							conversationId={tab.conversationId}
							{contextUsage}
							onContextClick={handleContextClick}
							on:submit={(e) => {
								if (chat.status === 'ready') {
									handleChatSubmit(e.detail);
								}
							}}
						/>

						{#if isEmpty}
							<GettingStarted
								onCreateSession={handleGettingStartedCreateSession}
								onFocusInput={handleGettingStartedFocusInput}
							/>
						{/if}
					</div>
				</div>
			</div>
		</div>
	</Page>

	<CitationPanel citation={selectedCitation} open={citationPanelOpen} onClose={closeCitationPanel} />

	<!-- Context Warning Toast -->
	{#if showContextWarning && contextUsage}
		<div class="toast-container">
			<ContextWarningToast
				usagePercentage={contextUsage.percentage}
				oncompact={handleCompactNow}
				ondismiss={() => showContextWarning = false}
			/>
		</div>
	{/if}
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
	}

	.message-wrapper :global(h1),
	.message-wrapper :global(h2),
	.message-wrapper :global(h3),
	.message-wrapper :global(h4) {
		margin-top: 0;
	}

	/* User message card styling */
	.message-wrapper[data-role='user'] {
		background: var(--color-surface-elevated);
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
		background-color: var(--color-surface);
		border: 1px solid var(--color-error);
		border-radius: 0.375rem;
		color: var(--color-error);
		font-size: 0.875rem;
		font-weight: 500;
		cursor: pointer;
		transition: all 0.15s ease;
	}

	.retry-button:hover {
		background-color: var(--color-error-subtle);
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

	.toast-container {
		position: fixed;
		top: 1rem;
		right: 1rem;
		z-index: 100;
	}
</style>
