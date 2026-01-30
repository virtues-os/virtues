/**
 * Chat Components
 *
 * Extracted components from ChatView for better maintainability:
 * - ChatError: Error display with rate limit handling
 * - ChatContextIndicator: Context usage indicator with status colors
 *
 * Future extractions (from ChatView.svelte):
 * - ChatMessageList: Message rendering loop
 * - ChatAssistantMessage: Individual assistant message with thinking/tools
 * - ChatGettingStarted: First-time user experience
 */

export { default as ChatError } from './ChatError.svelte';
export { default as ChatContextIndicator } from './ChatContextIndicator.svelte';
