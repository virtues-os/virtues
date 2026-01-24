<script lang="ts">
	import "iconify-icon";
	import { workspaceStore } from "$lib/stores/workspace.svelte";
	import { chatSessions } from "$lib/stores/chatSessions.svelte";

	interface Props {
		open?: boolean;
		onClose: () => void;
	}

	let { open = false, onClose }: Props = $props();

	let searchQuery = $state("");
	let selectedIndex = $state(0);
	let inputEl: HTMLInputElement | null = $state(null);

	// Quick actions
	const quickActions = [
		{
			id: "new-chat",
			label: "New Chat",
			icon: "ri:add-line",
			shortcut: "Cmd+N",
			action: () => workspaceStore.openTabFromRoute("/"),
		},
		{
			id: "wiki",
			label: "Go to Wiki",
			icon: "ri:book-2-line",
			action: () => workspaceStore.openTabFromRoute("/wiki"),
		},
		{
			id: "sources",
			label: "Go to Sources",
			icon: "ri:device-line",
			action: () => workspaceStore.openTabFromRoute("/data/sources"),
		},
		{
			id: "settings",
			label: "Open Settings",
			icon: "ri:settings-4-line",
			action: () => workspaceStore.openTabFromRoute("/profile/account"),
		},
	];

	// Filter results based on search
	const filteredResults = $derived.by(() => {
		const query = searchQuery.toLowerCase().trim();

		if (!query) {
			// Show quick actions and recent chats when empty
			return {
				actions: quickActions,
				chats: chatSessions.sessions.slice(0, 5),
			};
		}

		// Filter quick actions
		const matchedActions = quickActions.filter((a) =>
			a.label.toLowerCase().includes(query),
		);

		// Filter chats
		const matchedChats = chatSessions.sessions
			.filter((c) =>
				(c.title || "Untitled").toLowerCase().includes(query),
			)
			.slice(0, 5);

		return {
			actions: matchedActions,
			chats: matchedChats,
		};
	});

	// Total results count for keyboard navigation
	const totalResults = $derived(
		filteredResults.actions.length + filteredResults.chats.length,
	);

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === "Escape") {
			e.preventDefault();
			onClose();
		} else if (e.key === "ArrowDown") {
			e.preventDefault();
			selectedIndex = Math.min(selectedIndex + 1, totalResults - 1);
		} else if (e.key === "ArrowUp") {
			e.preventDefault();
			selectedIndex = Math.max(selectedIndex - 1, 0);
		} else if (e.key === "Enter") {
			e.preventDefault();
			selectCurrentItem();
		}
	}

	function selectCurrentItem() {
		const actionsCount = filteredResults.actions.length;

		if (selectedIndex < actionsCount) {
			// It's an action
			const action = filteredResults.actions[selectedIndex];
			action.action();
			onClose();
		} else {
			// It's a chat
			const chatIndex = selectedIndex - actionsCount;
			const chat = filteredResults.chats[chatIndex];
			if (chat) {
				workspaceStore.openTabFromRoute(
					`/?conversationId=${chat.conversation_id}`,
					{
						label: chat.title || "Chat",
					},
				);
				onClose();
			}
		}
	}

	function handleBackdropClick(e: MouseEvent) {
		if (e.target === e.currentTarget) {
			onClose();
		}
	}

	// Focus input when modal opens
	$effect(() => {
		if (open && inputEl) {
			inputEl.focus();
			searchQuery = "";
			selectedIndex = 0;
		}
	});
</script>

{#if open}
	<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
	<div class="modal-backdrop" onclick={handleBackdropClick}>
		<div class="modal" role="dialog" aria-modal="true" aria-label="Search">
			<!-- Search Input -->
			<div class="search-input-container">
				<iconify-icon
					icon="ri:search-line"
					width="18"
					class="search-icon"
				></iconify-icon>
				<input
					bind:this={inputEl}
					bind:value={searchQuery}
					onkeydown={handleKeydown}
					type="text"
					placeholder="Search chats, pages, or actions..."
					class="search-input"
				/>
				<kbd class="escape-hint">Esc</kbd>
			</div>

			<!-- Results -->
			<div class="results">
				{#if filteredResults.actions.length > 0}
					<div class="result-group">
						<span class="group-label">Quick Actions</span>
						{#each filteredResults.actions as action, i}
							<button
								class="result-item"
								class:selected={selectedIndex === i}
								onclick={() => {
									action.action();
									onClose();
								}}
								onmouseenter={() => (selectedIndex = i)}
							>
								<iconify-icon
									icon={action.icon}
									width="16"
									class="result-icon"
								></iconify-icon>
								<span class="result-label">{action.label}</span>
								{#if action.shortcut}
									<kbd class="result-shortcut"
										>{action.shortcut}</kbd
									>
								{/if}
							</button>
						{/each}
					</div>
				{/if}

				{#if filteredResults.chats.length > 0}
					<div class="result-group">
						<span class="group-label">Recent Chats</span>
						{#each filteredResults.chats as chat, i}
							{@const index = filteredResults.actions.length + i}
							<button
								class="result-item"
								class:selected={selectedIndex === index}
								onclick={() => {
									workspaceStore.openTabFromRoute(
										`/?conversationId=${chat.conversation_id}`,
										{
											label: chat.title || "Chat",
										},
									);
									onClose();
								}}
								onmouseenter={() => (selectedIndex = index)}
							>
								<iconify-icon
									icon="ri:message-3-line"
									width="16"
									class="result-icon"
								></iconify-icon>
								<span class="result-label"
									>{chat.title || "Untitled"}</span
								>
							</button>
						{/each}
					</div>
				{/if}

				{#if totalResults === 0 && searchQuery}
					<div class="no-results">
						<span>No results found for "{searchQuery}"</span>
					</div>
				{/if}
			</div>
		</div>
	</div>
{/if}

<style>
	@reference "../../../app.css";

	.modal-backdrop {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.4);
		display: flex;
		align-items: flex-start;
		justify-content: center;
		padding-top: 15vh;
		z-index: 9999;
		animation: backdrop-fade-in 150ms ease-out;
	}

	@keyframes backdrop-fade-in {
		from {
			opacity: 0;
		}
		to {
			opacity: 1;
		}
	}

	.modal {
		width: 100%;
		max-width: 520px;
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: 12px;
		box-shadow: 0 16px 48px rgba(0, 0, 0, 0.2);
		overflow: hidden;
		animation: modal-slide-in 150ms ease-out;
	}

	@keyframes modal-slide-in {
		from {
			opacity: 0;
			transform: translateY(-8px) scale(0.98);
		}
		to {
			opacity: 1;
			transform: translateY(0) scale(1);
		}
	}

	.search-input-container {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 14px 16px;
		border-bottom: 1px solid var(--border);
	}

	.search-icon {
		color: var(--foreground-muted);
		flex-shrink: 0;
	}

	.search-input {
		flex: 1;
		border: none;
		background: transparent;
		font-size: 15px;
		color: var(--foreground);
		outline: none;
	}

	.search-input::placeholder {
		color: var(--foreground-subtle);
	}

	.escape-hint {
		font-family: var(--font-mono);
		font-size: 10px;
		padding: 3px 6px;
		background: var(--surface-elevated);
		border-radius: 4px;
		color: var(--foreground-subtle);
	}

	.results {
		max-height: 400px;
		overflow-y: auto;
		padding: 8px;
	}

	.result-group {
		margin-bottom: 8px;
	}

	.group-label {
		display: block;
		font-size: 11px;
		font-weight: 500;
		text-transform: uppercase;
		letter-spacing: 0.02em;
		color: var(--foreground-subtle);
		padding: 6px 8px;
	}

	.result-item {
		display: flex;
		align-items: center;
		gap: 10px;
		width: 100%;
		padding: 10px 12px;
		border-radius: 8px;
		cursor: pointer;
		background: transparent;
		border: none;
		text-align: left;
		color: var(--foreground);
		transition: background-color 80ms ease-out;
	}

	.result-item:hover,
	.result-item.selected {
		background: var(--surface-overlay);
	}

	.result-item.selected {
		background: var(--primary-subtle);
	}

	.result-icon {
		color: var(--foreground-muted);
		flex-shrink: 0;
	}

	.result-label {
		flex: 1;
		font-size: 14px;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.result-shortcut {
		font-family: var(--font-mono);
		font-size: 10px;
		padding: 2px 6px;
		background: var(--surface-elevated);
		border-radius: 4px;
		color: var(--foreground-subtle);
	}

	.no-results {
		padding: 24px 16px;
		text-align: center;
		color: var(--foreground-subtle);
		font-size: 14px;
	}
</style>
