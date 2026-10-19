<script lang="ts">
	/**
	 * A project's chats: conversations you go back into, newest first, a few
	 * up front and the rest one click away. Each opens in the window you are
	 * in; right-click or ⋯ to open beside, move it, or take it out.
	 */
	import { untitled } from '$lib/refs/identity.svelte';
	import type { ProjectChat, ProjectDetail } from '$lib/api/client';
	import { IconButton } from '$lib';
	import AtlasIcon from '$lib/components/sidebar/AtlasIcon.svelte';
	import ProjectSection from '$lib/components/projects/ProjectSection.svelte';
	import { contextMenu } from '$lib/stores/contextMenu.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { projectColor } from '$lib/sidebar/pin-colors';
	import { formatRelativeTimestamp } from '$lib/utils/dateUtils';
	import { getProjectMenuItems } from '$lib/utils/contextMenuItems';

	interface Props {
		project: ProjectDetail;
		chats: ProjectChat[];
		/** Take a chat out of the project (the page owns what follows). */
		onremove: (url: string) => void;
	}

	let { project, chats, onremove }: Props = $props();

	const SHOWN = 5;
	let expanded = $state(false);
	const shown = $derived(expanded ? chats : chats.slice(0, SHOWN));

	/** A chat opens in the window you are in: it is where you go, not a reference beside. */
	function open(chat: ProjectChat) {
		windowShellStore.openTabFromRoute(`/chat/${chat.id}`, {
			label: chat.title || 'Chat',
			focusExisting: true
		});
	}

	/** Open it, move it to another project, take it out. */
	function menu(chat: ProjectChat, e: MouseEvent) {
		e.preventDefault();
		const url = `/chat/${chat.id}`;
		const elsewhere = getProjectMenuItems(url).map((i) => ({
			...i,
			submenu: i.submenu?.filter((s) => s.id !== 'remove-from-project')
		}));
		contextMenu.show({ x: e.clientX, y: e.clientY }, [
			{ id: 'open', label: 'Open', icon: 'ri:chat-3-line', action: () => open(chat) },
			{
				id: 'open-beside',
				label: 'Open beside',
				icon: 'ri:layout-column-line',
				action: () => void windowShellStore.openRouteBeside(url, chat.title)
			},
			...elsewhere,
			{
				id: 'remove',
				label: 'Remove from project',
				icon: 'ri:close-line',
				dividerBefore: true,
				variant: 'destructive',
				action: () => onremove(url)
			}
		]);
	}
</script>

<ProjectSection
	title="Chats"
	count={chats.length}
	empty={chats.length === 0 ? "No chats yet. Ask something above, or start a new chat, and it's filed here." : null}
>
	<ul class="chat-list">
		{#each shown as chat (chat.id)}
			<li class="chat-item" oncontextmenu={(e) => menu(chat, e)}>
				<button type="button" class="chat-open" onclick={() => open(chat)}>
					<span class="chat-glyph" style={`color: ${projectColor(project)}`}>
						<AtlasIcon name="chats" size={15} bare />
					</span>
					<span class="chat-title">{chat.title || untitled('chat')}</span>
					<span class="chat-meta">
						{chat.message_count === 1 ? '1 message' : `${chat.message_count} messages`}
						· {formatRelativeTimestamp(chat.last_message_at)}
					</span>
				</button>
				<span class="chat-actions">
					<IconButton
						icon="ri:more-line"
						label={`Actions for ${chat.title || 'this chat'}`}
						size="sm"
						haspopup="menu"
						onclick={(e) => menu(chat, e)}
					/>
				</span>
			</li>
		{/each}
	</ul>
	{#if chats.length > SHOWN}
		<button type="button" class="more" onclick={() => (expanded = !expanded)}>
			{expanded ? 'Show fewer' : `Show all ${chats.length} chats`}
		</button>
	{/if}
</ProjectSection>

<style>
	.chat-list {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
	}
	.chat-item {
		position: relative;
		display: flex;
		align-items: center;
		border-radius: 8px;
	}
	.chat-item:hover {
		background: var(--color-background-hover);
	}
	.chat-open {
		flex: 1;
		min-width: 0;
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 8px 12px;
		border: none;
		background: none;
		cursor: pointer;
		text-align: left;
		font: inherit;
		color: var(--color-foreground);
	}
	.chat-open:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: -2px;
		border-radius: 8px;
	}
	.chat-glyph {
		display: flex;
		flex-shrink: 0;
	}
	.chat-title {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-size: 0.9375rem;
	}
	.chat-meta {
		flex-shrink: 0;
		font-size: 0.8125rem;
		font-variant-numeric: tabular-nums;
		color: var(--color-foreground-subtle);
	}
	.chat-actions {
		display: inline-flex;
		padding-right: 4px;
		opacity: 0;
		transition: opacity 120ms ease;
	}
	.chat-item:hover .chat-actions,
	.chat-actions:focus-within {
		opacity: 1;
	}
	.more {
		align-self: flex-start;
		padding: 4px 8px;
		border: none;
		border-radius: 6px;
		background: none;
		cursor: pointer;
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
	}
	.more:hover {
		color: var(--color-foreground);
		background: var(--color-background-hover);
	}
	@media (max-width: 768px) {
		.chat-actions {
			opacity: 1;
		}
		.chat-meta {
			display: none;
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.chat-actions {
			transition: none;
		}
	}
</style>
