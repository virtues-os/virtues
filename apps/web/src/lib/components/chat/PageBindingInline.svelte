<script lang="ts">
	/**
	 * PageBindingInline
	 *
	 * Inline component shown when the AI needs user to bind a page OR
	 * when AI needs permission to edit a page.
	 *
	 * Modes:
	 * - Binding mode (default): User needs to select/open a page to edit
	 * - Permission mode: AI wants to edit a specific page, user must allow/deny
	 */

	interface Props {
		/** Entity ID (page_id, person_id, etc.) */
		entityId?: string;
		/** Entity type (page, person, etc.) */
		entityType?: string;
		/** Display title */
		entityTitle?: string;
		/** Custom message to display */
		message?: string;
		/** Proposed action description (for permission mode) */
		proposedAction?: string;
		/** Enable permission mode (shows Allow/Deny buttons) */
		permissionMode?: boolean;
		/** Called when user clicks Open & Edit (binding mode) */
		onBind?: (entityId: string, title: string) => void;
		/** Called when user clicks Allow (permission mode - allow once) */
		onAllow?: (entityId: string, entityType: string, title: string) => void;
		/** Called when user clicks Allow for this chat (permission mode - add to list) */
		onAllowForChat?: (entityId: string, entityType: string, title: string) => void;
		/** Called when user clicks Deny (permission mode) */
		onDeny?: () => void;
	}

	let {
		entityId,
		entityType = 'page',
		entityTitle,
		message,
		proposedAction,
		permissionMode = false,
		onBind,
		onAllow,
		onAllowForChat,
		onDeny
	}: Props = $props();

	/** Tracks which choice the user made (null = no choice yet) */
	let selectedChoice = $state<'allow' | 'allow-chat' | 'deny' | null>(null);

	const displayTitle = $derived(entityTitle || 'Untitled');

	// Default messages based on mode
	const displayMessage = $derived(() => {
		if (message) return message;
		if (permissionMode) return `AI wants to edit ${displayTitle}`;
		return 'Select a page to edit';
	});

	function handleBind() {
		if (entityId && onBind) {
			onBind(entityId, displayTitle);
		}
	}

	function handleAllow() {
		if (entityId && onAllow && !selectedChoice) {
			selectedChoice = 'allow';
			onAllow(entityId, entityType, displayTitle);
		}
	}

	function handleAllowForChat() {
		if (entityId && onAllowForChat && !selectedChoice) {
			selectedChoice = 'allow-chat';
			onAllowForChat(entityId, entityType, displayTitle);
		}
	}

	function handleDeny() {
		if (onDeny && !selectedChoice) {
			selectedChoice = 'deny';
			onDeny();
		}
	}

	function getIconForType(type: string): string {
		switch (type) {
			case 'page': return 'M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z';
			case 'person': return 'M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2M12 11a4 4 0 1 0 0-8 4 4 0 0 0 0 8z';
			case 'place': return 'M21 10c0 7-9 13-9 13s-9-6-9-13a9 9 0 0 1 18 0z';
			default: return 'M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z';
		}
	}
</script>

<div class="page-binding-inline" class:permission-mode={permissionMode}>
	<div class="binding-content">
		<div class="binding-icon" class:permission-icon={permissionMode}>
			<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
				<path d={getIconForType(entityType)} />
				{#if entityType === 'page'}
					<polyline points="14 2 14 8 20 8" />
					<line x1="16" y1="13" x2="8" y2="13" />
					<line x1="16" y1="17" x2="8" y2="17" />
				{/if}
			</svg>
		</div>
		<div class="binding-text">
			<span class="binding-message">{displayMessage()}</span>
			{#if entityId && !permissionMode}
				<span class="binding-page-title">{displayTitle}</span>
			{/if}
		</div>
	</div>

	<div class="actions">
		{#if permissionMode}
			<!-- Permission mode: Allow/Allow for chat/Deny -->
			{#if !selectedChoice || selectedChoice === 'deny'}
				<button
					class="action-btn deny-btn"
					class:selected={selectedChoice === 'deny'}
					onclick={handleDeny}
					disabled={selectedChoice !== null}
					type="button"
				>
					{selectedChoice === 'deny' ? 'Denied' : 'Deny'}
				</button>
			{/if}
			{#if !selectedChoice || selectedChoice === 'allow-chat'}
				<button
					class="action-btn allow-chat-btn"
					class:selected={selectedChoice === 'allow-chat'}
					onclick={handleAllowForChat}
					disabled={selectedChoice !== null}
					type="button"
				>
					{selectedChoice === 'allow-chat' ? 'Allowed for chat' : 'Allow for chat'}
				</button>
			{/if}
			{#if !selectedChoice || selectedChoice === 'allow'}
				<button
					class="action-btn allow-btn"
					class:selected={selectedChoice === 'allow'}
					onclick={handleAllow}
					disabled={selectedChoice !== null}
					type="button"
				>
					{selectedChoice === 'allow' ? 'Allowed' : 'Allow'}
				</button>
			{/if}
		{:else if entityId}
			<!-- Binding mode: Open & Edit -->
			<button class="action-btn bind-btn" onclick={handleBind} type="button">
				Open & Edit
			</button>
		{/if}
	</div>
</div>

<style>
	.page-binding-inline {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 0.75rem;
		padding: 0.75rem 1rem;
		background: var(--color-surface-elevated);
		border: 1px solid var(--color-border);
		border-radius: 0.5rem;
		margin: 0.75rem 0;
	}

	.page-binding-inline.permission-mode {
		/* Same as regular card - no special highlighting */
	}

	.binding-content {
		display: flex;
		align-items: center;
		gap: 0.625rem;
		flex: 1;
		min-width: 0;
	}

	.binding-icon {
		display: flex;
		align-items: center;
		justify-content: center;
		color: var(--color-primary);
		flex-shrink: 0;
	}

	.binding-icon.permission-icon {
		color: var(--color-foreground-muted);
	}

	.binding-text {
		display: flex;
		flex-direction: column;
		gap: 0.125rem;
		min-width: 0;
	}

	.binding-message {
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
	}

	.permission-mode .binding-message {
		/* Same as regular mode */
	}

	.binding-page-title {
		font-size: 0.875rem;
		font-weight: 500;
		color: var(--color-foreground);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.proposed-action {
		font-size: 0.75rem;
		color: var(--color-foreground-muted);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.actions {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		flex-shrink: 0;
	}

	.action-btn {
		padding: 0.375rem 0.75rem;
		border: none;
		border-radius: 0.375rem;
		font-size: 0.8125rem;
		font-weight: 500;
		cursor: pointer;
		transition: opacity 0.15s ease;
		white-space: nowrap;
	}

	.action-btn:hover:not(:disabled) {
		opacity: 0.9;
	}

	.action-btn:disabled {
		cursor: default;
		opacity: 1;
	}

	.action-btn.selected {
		cursor: default;
	}

	.bind-btn {
		background: var(--color-primary);
		color: white;
	}

	.allow-btn {
		background: var(--color-surface);
		color: var(--color-foreground);
		border: 1px solid var(--color-border);
	}

	.allow-btn:hover:not(:disabled) {
		background: var(--color-surface-elevated);
		border-color: var(--color-border-strong);
	}

	.allow-btn.selected {
		background: var(--color-success-subtle);
		color: var(--color-success);
		border-color: var(--color-success);
	}

	.allow-chat-btn {
		background: var(--color-surface);
		color: var(--color-foreground);
		border: 1px solid var(--color-border);
	}

	.allow-chat-btn:hover:not(:disabled) {
		background: var(--color-surface-elevated);
		border-color: var(--color-border-strong);
	}

	.allow-chat-btn.selected {
		background: var(--color-success-subtle);
		color: var(--color-success);
		border-color: var(--color-success);
	}

	.deny-btn {
		background: transparent;
		color: var(--color-foreground-muted);
	}

	.deny-btn:hover:not(:disabled) {
		color: var(--color-error);
		background: var(--color-error-subtle);
	}

	.deny-btn.selected {
		background: var(--color-error-subtle);
		color: var(--color-error);
	}
</style>
