<script lang="ts">
	import type { Tab } from '$lib/tabs/types';
	import type { SpaceDetail } from '$lib/api/client';
	import Icon from '$lib/components/Icon.svelte';
	import { spaceStore } from '$lib/stores/space.svelte';
	import { chatSessions } from '$lib/stores/chatSessions.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { contextMenu } from '$lib/stores/contextMenu.svelte';
	import EntityPicker from '$lib/components/EntityPicker.svelte';
	import ColorPickerModal from '$lib/components/sidebar/ColorPickerModal.svelte';

	let { tab }: { tab: Tab; active?: boolean } = $props();

	const spaceId = $derived.by(() => {
		const m = tab.route.match(/^\/space\/(space_[^/]+)$/);
		return m?.[1] ?? null;
	});

	let detail = $state<SpaceDetail | null>(null);
	let loading = $state(false);
	let error = $state<string | null>(null);

	async function load(force = false) {
		const id = spaceId;
		if (!id) return;
		loading = true;
		error = null;
		try {
			detail = await spaceStore.get(id, { force });
		} catch (e) {
			console.error('[SpaceDetailView] Failed to load space:', e);
			error = e instanceof Error ? e.message : 'Failed to load space';
			detail = null;
		} finally {
			loading = false;
		}
	}

	// (Re)load whenever the tab points at a different room.
	$effect(() => {
		if (spaceId) load();
	});

	// Chats filed into this room — sourced from the authoritative session list,
	// not from membership rows, so removing a pinned member can't desync a chat.
	const roomChats = $derived(
		chatSessions.sessions.filter((s) => s.space_id === spaceId),
	);

	// Pinned members = everything except chats (chats render in their own list).
	const pinnedItems = $derived((detail?.items ?? []).filter((i) => !i.url.startsWith('/chat/')));

	function iconForUrl(url: string): string {
		if (url.startsWith('http://') || url.startsWith('https://')) return 'ri:external-link-line';
		const prefix = url.split('/')[1] ?? '';
		const map: Record<string, string> = {
			person: 'ri:user-line',
			page: 'ri:file-text-line',
			org: 'ri:building-line',
			place: 'ri:map-pin-line',
			thing: 'ri:shapes-line',
			day: 'ri:calendar-line',
			year: 'ri:calendar-2-line',
			source: 'ri:database-2-line',
			drive: 'ri:file-line',
		};
		return map[prefix] ?? 'ri:links-line';
	}

	function labelForUrl(url: string): string {
		if (url.startsWith('http://') || url.startsWith('https://')) {
			try { return new URL(url).hostname; } catch { return url; }
		}
		// e.g. "/person/p_abc" → "person"; the destination view shows the full name.
		return url.split('/')[1] ?? url;
	}

	function openUrl(url: string) {
		if (url.startsWith('http://') || url.startsWith('https://')) {
			window.open(url, '_blank', 'noopener,noreferrer');
			return;
		}
		windowShellStore.openTabFromRoute(url);
	}

	function openChat(conversationId: string) {
		windowShellStore.openTabFromRoute(`/chat/${conversationId}`);
	}

	// ---- Inline name edit ----------------------------------------------------
	let editingName = $state(false);
	let nameDraft = $state('');
	function startRename() {
		if (!detail) return;
		nameDraft = detail.name;
		editingName = true;
	}
	async function commitRename() {
		editingName = false;
		const id = spaceId;
		if (!id || !detail) return;
		const name = nameDraft.trim();
		if (!name || name === detail.name) return;
		await spaceStore.update(id, { name });
		await load(true);
	}

	// ---- Catch-up memo -------------------------------------------------------
	let editingMemo = $state(false);
	let memoDraft = $state('');
	function startMemo() {
		if (!detail) return;
		memoDraft = detail.current_status ?? '';
		editingMemo = true;
	}
	async function commitMemo() {
		editingMemo = false;
		const id = spaceId;
		if (!id) return;
		await spaceStore.update(id, { current_status: memoDraft.trim() || null });
		await load(true);
	}

	// ---- Accent color --------------------------------------------------------
	let colorOpen = $state(false);
	async function setAccent(color: string | null) {
		colorOpen = false;
		const id = spaceId;
		if (!id) return;
		await spaceStore.update(id, { accent_color: color });
		await load(true);
	}

	// ---- Membership ----------------------------------------------------------
	let pickerPos = $state<{ x: number; y: number } | null>(null);
	function openPicker(e: MouseEvent) {
		pickerPos = { x: e.clientX, y: e.clientY };
	}
	async function addMember(entity: { url: string }) {
		pickerPos = null;
		const id = spaceId;
		if (!id || !entity.url) return;
		await spaceStore.addItem(id, entity.url);
	}
	async function removeMember(url: string) {
		const id = spaceId;
		if (!id) return;
		await spaceStore.removeItem(id, url);
	}
	function memberMenu(e: MouseEvent, url: string) {
		e.preventDefault();
		contextMenu.show({ x: e.clientX, y: e.clientY }, [
			{ id: 'remove', label: 'Remove from Space', icon: 'ri:close-line', action: () => removeMember(url) },
		]);
	}

	// ---- Delete --------------------------------------------------------------
	async function deleteSpace() {
		const id = spaceId;
		if (!id || !detail) return;
		contextMenu.hide?.();
		if (!confirm(`Delete the Space "${detail.name}"? Chats and pages stay; they're just unfiled.`)) return;
		await spaceStore.remove(id);
		windowShellStore.openTabFromRoute('/spaces');
	}

	const accent = $derived(detail?.accent_color || null);
</script>

<div class="space-detail" style={accent ? `--room-accent: ${accent}` : ''}>
	{#if loading && !detail}
		<div class="state"><Icon icon="ri:loader-4-line" width="18" class="spin" /> Loading…</div>
	{:else if error}
		<div class="state error">{error}</div>
	{:else if detail}
		<header class="head" class:tinted={!!accent}>
			<button class="icon-btn room-icon" title="Set color" onclick={() => (colorOpen = true)}>
				<Icon icon={detail.icon || 'ri:layout-masonry-line'} width="22" />
			</button>
			<div class="title-wrap">
				{#if editingName}
					<!-- svelte-ignore a11y_autofocus -->
					<input
						class="title-input"
						bind:value={nameDraft}
						autofocus
						onblur={commitRename}
						onkeydown={(e) => { if (e.key === 'Enter') commitRename(); if (e.key === 'Escape') editingName = false; }}
					/>
				{:else}
					<h1 class="title" ondblclick={startRename}>{detail.name}</h1>
				{/if}
				<div class="meta">
					<span>{roomChats.length} {roomChats.length === 1 ? 'chat' : 'chats'}</span>
					<span class="dot-sep">·</span>
					<span>{pinnedItems.length} pinned</span>
				</div>
			</div>
			<div class="head-actions">
				<button class="icon-btn" title="Rename" onclick={startRename}><Icon icon="ri:edit-line" width="16" /></button>
				<button class="icon-btn danger" title="Delete Space" onclick={deleteSpace}><Icon icon="ri:delete-bin-line" width="16" /></button>
			</div>
		</header>

		<!-- Catch-up memo -->
		<section class="memo">
			{#if editingMemo}
				<!-- svelte-ignore a11y_autofocus -->
				<textarea
					class="memo-input"
					bind:value={memoDraft}
					autofocus
					placeholder="What's the state of this Space? (a line or two you'll read when you come back)"
					onblur={commitMemo}
				></textarea>
			{:else if detail.current_status}
				<button class="memo-text" onclick={startMemo}>{detail.current_status}</button>
			{:else}
				<button class="memo-empty" onclick={startMemo}>
					<Icon icon="ri:sticky-note-line" width="14" /> Add a catch-up note…
				</button>
			{/if}
		</section>

		<!-- Chats -->
		<section class="group">
			<div class="group-head"><span>Chats</span></div>
			{#if roomChats.length === 0}
				<div class="empty">No chats in this Space yet.</div>
			{:else}
				<ul class="rows">
					{#each roomChats as c (c.conversation_id)}
						<li>
							<button class="row" onclick={() => openChat(c.conversation_id)} title={c.title ?? 'Untitled chat'}>
								<Icon icon={c.icon || 'ri:chat-3-line'} width="15" />
								<span class="row-label">{c.title ?? 'Untitled chat'}</span>
							</button>
						</li>
					{/each}
				</ul>
			{/if}
		</section>

		<!-- Pinned members -->
		<section class="group">
			<div class="group-head">
				<span>Pinned</span>
				<button class="add-btn" onclick={openPicker} title="Add a person, page, or link"><Icon icon="ri:add-line" width="15" /></button>
			</div>
			{#if pinnedItems.length === 0}
				<div class="empty">Nothing pinned. Add people, pages, or links to weight them here.</div>
			{:else}
				<ul class="rows">
					{#each pinnedItems as it (it.url)}
						<li class="row-li">
							<button class="row" onclick={() => openUrl(it.url)} oncontextmenu={(e) => memberMenu(e, it.url)} title={it.url}>
								<Icon icon={iconForUrl(it.url)} width="15" />
								<span class="row-label">{labelForUrl(it.url)}</span>
							</button>
							<button
								class="row-remove"
								title="Remove from Space"
								onclick={() => removeMember(it.url)}
							><Icon icon="ri:close-line" width="13" /></button>
						</li>
					{/each}
				</ul>
			{/if}
		</section>
	{:else}
		<div class="state">Space not found.</div>
	{/if}
</div>

{#if pickerPos}
	<EntityPicker
		mode="single"
		position={pickerPos}
		placeholder="Add a person, page, or link…"
		excludeIds={pinnedItems.map((i) => i.url)}
		onSelect={addMember}
		onClose={() => (pickerPos = null)}
	/>
{/if}

<ColorPickerModal open={colorOpen} value={accent} onSelect={setAccent} onClose={() => (colorOpen = false)} />

<style>
	.space-detail {
		height: 100%;
		overflow-y: auto;
		padding: 24px 28px 48px;
		max-width: 760px;
		margin: 0 auto;
	}
	.state {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 40px 0;
		color: var(--color-foreground-muted);
	}
	.state.error { color: var(--color-error, #dc2626); }

	.head {
		display: flex;
		align-items: center;
		gap: 14px;
		padding: 12px 14px;
		border-radius: 12px;
		margin-bottom: 14px;
	}
	.head.tinted {
		background: color-mix(in srgb, var(--room-accent) 8%, transparent);
		box-shadow: inset 3px 0 0 var(--room-accent);
	}
	.room-icon {
		display: grid;
		place-items: center;
		width: 44px;
		height: 44px;
		border-radius: 10px;
		background: var(--color-surface-elevated);
		color: var(--color-foreground);
		flex-shrink: 0;
	}
	.head.tinted .room-icon {
		background: color-mix(in srgb, var(--room-accent) 16%, transparent);
		color: color-mix(in srgb, var(--room-accent) 78%, var(--color-foreground));
	}
	.title-wrap { flex: 1; min-width: 0; }
	.title {
		font-size: 22px;
		font-weight: 650;
		margin: 0;
		color: var(--color-foreground);
		cursor: text;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.title-input {
		font-size: 22px;
		font-weight: 650;
		width: 100%;
		border: none;
		border-bottom: 1px solid var(--color-border);
		background: transparent;
		color: var(--color-foreground);
		outline: none;
		padding: 0 0 2px;
	}
	.meta {
		display: flex;
		align-items: center;
		gap: 6px;
		font-size: 12px;
		color: var(--color-foreground-muted);
		margin-top: 3px;
	}
	.dot-sep { opacity: 0.5; }
	.head-actions { display: flex; gap: 4px; }
	.icon-btn {
		display: grid;
		place-items: center;
		width: 30px;
		height: 30px;
		border: none;
		border-radius: 7px;
		background: transparent;
		color: var(--color-foreground-muted);
		cursor: pointer;
	}
	.icon-btn:hover { background: var(--color-surface-elevated); color: var(--color-foreground); }
	.icon-btn.danger:hover { color: var(--color-error, #dc2626); }

	.memo { margin-bottom: 18px; }
	.memo-text, .memo-empty {
		display: flex;
		align-items: flex-start;
		gap: 7px;
		width: 100%;
		text-align: left;
		border: none;
		background: transparent;
		cursor: text;
		padding: 8px 10px;
		border-radius: 8px;
		font-size: 14px;
		line-height: 1.5;
		color: var(--color-foreground);
	}
	.memo-text:hover, .memo-empty:hover { background: var(--color-surface-elevated); }
	.memo-empty { color: var(--color-foreground-muted); font-size: 13px; align-items: center; }
	.memo-input {
		width: 100%;
		min-height: 64px;
		resize: vertical;
		border: 1px solid var(--color-border);
		border-radius: 8px;
		background: var(--color-surface-elevated);
		color: var(--color-foreground);
		padding: 8px 10px;
		font: inherit;
		font-size: 14px;
		line-height: 1.5;
		outline: none;
	}

	.group { margin-bottom: 20px; }
	.group-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 0 10px 4px;
		font-size: 11px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--color-foreground-subtle, #9ca3af);
	}
	.add-btn {
		display: grid;
		place-items: center;
		width: 22px;
		height: 22px;
		border: none;
		border-radius: 6px;
		background: transparent;
		color: var(--color-foreground-muted);
		cursor: pointer;
	}
	.add-btn:hover { background: var(--color-surface-elevated); color: var(--color-foreground); }
	.empty { padding: 8px 10px; font-size: 13px; color: var(--color-foreground-muted); }
	.rows { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 1px; }
	.row-li { position: relative; display: flex; align-items: center; }
	.row {
		display: flex;
		align-items: center;
		gap: 9px;
		width: 100%;
		padding: 7px 10px;
		border: none;
		border-radius: 7px;
		background: transparent;
		color: var(--color-foreground);
		font: inherit;
		font-size: 13.5px;
		text-align: left;
		cursor: pointer;
	}
	.row:hover, .row-li:hover .row { background: var(--color-surface-elevated); }
	.row-label { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.row-remove {
		position: absolute;
		right: 6px;
		display: grid;
		place-items: center;
		width: 20px;
		height: 20px;
		border: none;
		border-radius: 5px;
		background: transparent;
		color: var(--color-foreground-subtle, #9ca3af);
		cursor: pointer;
		opacity: 0;
	}
	.row-li:hover .row-remove { opacity: 1; }
	.row-remove:hover { background: var(--color-border); color: var(--color-foreground); }
	:global(.spin) { animation: spin 0.8s linear infinite; }
	@keyframes spin { to { transform: rotate(360deg); } }
</style>
