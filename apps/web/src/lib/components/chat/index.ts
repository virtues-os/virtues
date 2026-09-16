/**
 * Chat Components
 *
 * Extracted components from ChatView for better maintainability:
 * - ChatError: Error display with rate limit handling
 * - ChatContextIndicator: Context usage indicator with status colors
 * - ContextViewPanel: Session analytics and token breakdown view
 */

export { default as ChatContextIndicator } from './ChatContextIndicator.svelte';
export { default as ChatError } from './ChatError.svelte';
export { default as ContextViewPanel } from './ContextViewPanel.svelte';

/**
 * The controller, in `./state` — ChatView's script split by responsibility, so
 * the view is markup plus thin bindings. Each module's head says what it owns.
 * Imported by path rather than re-exported here: these are per-view instances,
 * not shared singletons, and a barrel would hide that.
 */
