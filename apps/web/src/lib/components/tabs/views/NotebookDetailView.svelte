<script lang="ts">
	import type { Tab } from '$lib/tabs/types';
	import type { NotebookDetail, NotebookGraph, NotebookItemRole } from '$lib/api/client';
	import Icon from '$lib/components/Icon.svelte';
	import { notebookStore } from '$lib/stores/notebook.svelte';
	import { chatSessions } from '$lib/stores/chatSessions.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { contextMenu } from '$lib/stores/contextMenu.svelte';
	import RefPicker from '$lib/components/RefPicker.svelte';
	import ColorPickerModal from '$lib/components/sidebar/ColorPickerModal.svelte';
	import IconPicker from '$lib/components/IconPicker.svelte';
	import NotebookGraphBand from '$lib/components/notebook/NotebookGraphBand.svelte';
	import UniversalDataGrid, { type Column } from '$lib/components/datagrid/UniversalDataGrid.svelte';
	import { Popover } from '$lib/floating';
	import { confirmAction } from '$lib/stores/dialog.svelte';
	import { toast } from 'svelte-sonner';
	import { getRefSummary } from '$lib/utils/refSummary';
	import {
		getPage,
		getDriveFile,
		uploadDriveFile,
		addNotebookItem,
		reextractDriveFile,
		getNotebookGraph
	} from '$lib/api/client';
	import { askVirtues } from '$lib/stores/pendingPrompt.svelte';

	let { tab }: { tab: Tab; active?: boolean } = $props();

	const notebookId = $derived.by(() => {
		const m = tab.route.match(/^\/notebook\/([^/]+)$/);
		return m?.[1] ?? null;
	});

	let detail = $state<NotebookDetail | null>(null);
	let loading = $state(false);
	let error = $state<string | null>(null);

	async function load(force = false) {
		const id = notebookId;
		if (!id) return;
		loading = true;
		error = null;
		try {
			detail = await notebookStore.get(id, { force });
		} catch (e) {
			console.error('[NotebookDetailView] Failed to load notebook:', e);
			error = e instanceof Error ? e.message : 'Failed to load notebook';
			detail = null;
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		if (notebookId) load();
	});

	// ---- The entity graph over the members -----------------------------------
	let graph = $state<NotebookGraph>({ nodes: [], edges: [] });
	let selectedEntity = $state<string | null>(null);

	async function loadGraph() {
		const id = notebookId;
		if (!id) {
			graph = { nodes: [], edges: [] };
			return;
		}
		try {
			graph = await getNotebookGraph(id);
		} catch (e) {
			// A missing graph shouldn't take the page down — it's an aid, not the content.
			console.error('[NotebookDetailView] Failed to load graph:', e);
			graph = { nodes: [], edges: [] };
		}
	}
	$effect(() => {
		if (notebookId) loadGraph();
	});
	// A filter pinned to an entity that no longer has a node would silently hide
	// every row; drop it when the graph changes underneath us.
	$effect(() => {
		if (selectedEntity && !graph.nodes.some((n) => n.url === selectedEntity)) {
			selectedEntity = null;
		}
	});

	const selectedNode = $derived(graph.nodes.find((n) => n.url === selectedEntity) ?? null);

	// Chats filed into this room — sourced from the authoritative session list,
	// not from membership rows, so removing a member can't desync a chat.
	const roomChats = $derived(chatSessions.sessions.filter((s) => s.notebook_id === notebookId));

	// Members = everything except chats (chats render in their own list).
	const memberItems = $derived((detail?.items ?? []).filter((i) => !i.url.startsWith('/chat/')));

	// ---- Resolve real member names (not the type slug) -----------------------
	let memberNames = $state<Record<string, string>>({});
	let memberStatus = $state<Record<string, string>>({});
	const requestedNames = new Set<string>();

	async function resolveMemberName(url: string): Promise<string> {
		if (url.startsWith('http://') || url.startsWith('https://')) {
			try {
				return new URL(url).hostname.replace(/^www\./, '');
			} catch {
				return url;
			}
		}
		const parts = url.split('/');
		const type = parts[1] ?? '';
		const id = parts.slice(2).join('/');
		try {
			if (type === 'person' || type === 'place' || type === 'org' || type === 'thing') {
				const s = await getRefSummary(type, id);
				if (s?.name) return s.name;
			} else if (type === 'page') {
				const p = await getPage(id);
				if (p) return p.title?.trim() || 'Untitled page';
			} else if (type === 'drive') {
				const f = await getDriveFile(parts[2] ?? id);
				if (f) {
					memberStatus = { ...memberStatus, [url]: f.extraction_status };
					if (f.filename) return f.filename;
				}
			} else if (type === 'day' || type === 'year') {
				return id;
			}
		} catch {
			/* fall through to the type label */
		}
		return type ? type[0].toUpperCase() + type.slice(1) : url;
	}

	$effect(() => {
		for (const it of memberItems) {
			if (!requestedNames.has(it.url)) {
				requestedNames.add(it.url);
				resolveMemberName(it.url).then((n) => {
					memberNames = { ...memberNames, [it.url]: n };
				});
			}
		}
	});

	// ---- Member rows ---------------------------------------------------------
	const ENTITY_TYPES = ['person', 'place', 'org', 'thing'];

	function memberType(url: string): string {
		if (url.startsWith('http://') || url.startsWith('https://')) return 'Link';
		const t = url.split('/')[1] ?? '';
		const map: Record<string, string> = {
			person: 'Person',
			page: 'Page',
			org: 'Org',
			place: 'Place',
			thing: 'Thing',
			day: 'Day',
			year: 'Year',
			source: 'Source',
			drive: 'File'
		};
		return map[t] ?? t;
	}

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
			drive: 'ri:file-line'
		};
		return map[prefix] ?? 'ri:links-line';
	}

	/**
	 * What this member is to the notebook, in the user's terms rather than the
	 * schema's. `manuscript` and `pin` are stored roles; "Bible" is derived —
	 * a filed person or place is reference material by nature, not by a flag.
	 */
	function roleLabel(url: string, role: NotebookItemRole): string {
		if (role === 'manuscript') return 'Manuscript';
		if (role === 'pin') return 'Pin';
		const t = url.split('/')[1] ?? '';
		if (ENTITY_TYPES.includes(t)) return 'Bible';
		return 'Source';
	}

	function statusLabel(url: string): string {
		const s = memberStatus[url];
		if (!s || s === 'skipped') return '—';
		switch (s) {
			case 'done':
				return 'Indexed';
			case 'pending':
				return 'Queued';
			case 'extracting':
				return 'Extracting…';
			case 'no_text':
				return 'No text layer';
			case 'failed':
				return 'Failed';
			default:
				return '—';
		}
	}

	function formatAdded(iso: string): string {
		const d = new Date(iso);
		if (Number.isNaN(d.getTime())) return '—';
		const now = new Date();
		return d.toLocaleDateString('en-US', {
			month: 'short',
			day: 'numeric',
			year: now.getFullYear() !== d.getFullYear() ? 'numeric' : undefined
		});
	}

	interface MemberRow {
		id: string;
		url: string;
		name: string;
		kind: string;
		role: NotebookItemRole;
		roleText: string;
		status: string;
		added: string;
		icon: string;
	}

	/**
	 * Everything the notebook holds, in one table. Chats used to live in their
	 * own list below; they are members like any other and splitting them made
	 * the page two near-identical lists.
	 *
	 * Chats are still sourced from the authoritative session list rather than
	 * from membership rows, so a stale `/chat/` member row can't resurrect a
	 * deleted conversation.
	 */
	const allRows = $derived.by<MemberRow[]>(() => {
		const members: MemberRow[] = memberItems.map((it) => ({
			id: it.url,
			url: it.url,
			name: memberNames[it.url] || memberType(it.url),
			kind: memberType(it.url),
			role: it.role,
			roleText: roleLabel(it.url, it.role),
			status: statusLabel(it.url),
			added: formatAdded(it.added_at),
			icon: iconForUrl(it.url)
		}));

		const chats: MemberRow[] = roomChats.map((c) => ({
			id: `/chat/${c.conversation_id}`,
			url: `/chat/${c.conversation_id}`,
			name: c.title ?? 'Untitled chat',
			kind: 'Chat',
			// A chat grounds retrieval like any other member, so its role is
			// honestly 'Source'; the Kind column carries that it's a thread.
			role: 'library',
			roleText: 'Source',
			status: `${c.message_count} ${c.message_count === 1 ? 'message' : 'messages'}`,
			added: formatAdded(c.last_message_at ?? c.first_message_at),
			icon: c.icon || 'ri:chat-3-line'
		}));

		return [...members, ...chats];
	});

	// The graph is a filter over this list — that's what earns it the top slot.
	const rows = $derived.by(() => {
		if (!selectedNode) return allRows;
		const keep = new Set(selectedNode.item_urls);
		return allRows.filter((r) => keep.has(r.url));
	});

	const columns: Column<MemberRow>[] = [
		{ key: 'name', label: 'Name', icon: 'ri:file-list-line', width: '40%', minWidth: '200px' },
		{ key: 'kind', label: 'Kind', width: '12%', minWidth: '80px' },
		{ key: 'roleText', label: 'Role', width: '14%', minWidth: '90px' },
		{ key: 'status', label: 'Status', width: '18%', minWidth: '110px', hideOnMobile: true },
		{ key: 'added', label: 'Added', width: '16%', minWidth: '90px', hideOnMobile: true }
	];

	// ---- Actions -------------------------------------------------------------
	function openUrl(url: string) {
		if (url.startsWith('http://') || url.startsWith('https://')) {
			window.open(url, '_blank', 'noopener,noreferrer');
			return;
		}
		windowShellStore.openTabFromRoute(url);
	}

	async function setRole(url: string, role: NotebookItemRole) {
		const id = notebookId;
		if (!id) return;
		try {
			await notebookStore.setItemRole(id, url, role);
			await load(true);
			await loadGraph();
		} catch (e) {
			console.error('[NotebookDetailView] set role failed:', e);
			toast.error('Could not change the role');
		}
	}

	async function removeMember(url: string) {
		const id = notebookId;
		if (!id) return;
		await notebookStore.removeItem(id, url);
		await loadGraph();
	}

	function rowMenu(row: MemberRow, e: MouseEvent) {
		e.preventDefault();
		const items = [
			{ id: 'open', label: 'Open', icon: 'ri:external-link-line', action: () => openUrl(row.url) }
		];
		// Only things that can actually ground or be written have a role worth
		// changing; a filed person is reference material either way.
		if (!ENTITY_TYPES.includes(row.url.split('/')[1] ?? '')) {
			if (row.role === 'manuscript') {
				items.push({
					id: 'source',
					label: 'Treat as source',
					icon: 'ri:book-open-line',
					action: () => setRole(row.url, 'library')
				});
			} else {
				items.push({
					id: 'manuscript',
					label: 'Treat as manuscript',
					icon: 'ri:quill-pen-line',
					action: () => setRole(row.url, 'manuscript')
				});
			}
		}
		items.push({
			id: 'remove',
			label: 'Remove from notebook',
			icon: 'ri:close-line',
			dividerBefore: true,
			variant: 'destructive',
			action: () => removeMember(row.url)
		} as (typeof items)[number]);
		contextMenu.show({ x: e.clientX, y: e.clientY }, items);
	}

	async function retryExtraction(url: string) {
		const fileId = url.split('/')[2];
		if (!fileId) return;
		try {
			await reextractDriveFile(fileId);
			memberStatus = { ...memberStatus, [url]: 'pending' };
		} catch {
			/* chip stays; next open refreshes */
		}
	}

	// ---- Ask this notebook ---------------------------------------------------
	let askDraft = $state('');
	function submitAsk(e: Event) {
		e.preventDefault();
		const text = askDraft.trim();
		const id = notebookId;
		if (!text || !id) return;
		askVirtues(text, id);
		askDraft = '';
	}

	// ---- Drag-drop onto the notebook: upload + add in one motion -------------
	let dropActive = $state(false);
	async function handleDrop(e: DragEvent) {
		e.preventDefault();
		dropActive = false;
		const id = notebookId;
		const dropped = e.dataTransfer?.files;
		if (!id || !dropped || dropped.length === 0) return;
		for (const f of dropped) {
			try {
				const uploaded = await uploadDriveFile('uploads', f);
				await addNotebookItem(id, `/drive/${uploaded.id}`);
			} catch (err) {
				console.error('[NotebookDetailView] drop-add failed:', err);
				toast.error(`Could not add ${f.name}`);
			}
		}
		await load(true);
		await loadGraph();
	}

	// ---- Title + description: always live, no edit mode ----------------------
	let nameDraft = $state('');
	let memoDraft = $state('');
	let nameFocused = $state(false);
	let memoFocused = $state(false);
	$effect(() => {
		if (!detail) return;
		if (!nameFocused) nameDraft = detail.name;
		if (!memoFocused) memoDraft = detail.current_status ?? '';
	});

	async function commitName() {
		nameFocused = false;
		const id = notebookId;
		if (!id || !detail) return;
		const name = nameDraft.trim();
		if (!name) {
			nameDraft = detail.name;
			return;
		}
		if (name === detail.name) return;
		await notebookStore.update(id, { name });
	}

	async function commitMemo() {
		memoFocused = false;
		const id = notebookId;
		if (!id || !detail) return;
		const memo = memoDraft.trim() || null;
		if (memo === (detail.current_status ?? null)) return;
		await notebookStore.update(id, { current_status: memo });
	}

	// ---- Icon + accent colour ------------------------------------------------
	let colorOpen = $state(false);
	let iconOpen = $state(false);
	let overflowOpen = $state(false);
	async function setIcon(icon: string | null) {
		const id = notebookId;
		if (!id) return;
		await notebookStore.update(id, { icon });
		await load(true);
	}
	async function setAccent(color: string | null) {
		colorOpen = false;
		const id = notebookId;
		if (!id) return;
		await notebookStore.update(id, { accent_color: color });
		await load(true);
	}

	// ---- Membership ----------------------------------------------------------
	let pickerPos = $state<{ x: number; y: number } | null>(null);
	function openPicker(e: MouseEvent) {
		pickerPos = { x: e.clientX, y: e.clientY };
	}
	async function addMember(entity: { url: string }) {
		pickerPos = null;
		const id = notebookId;
		if (!id || !entity.url) return;
		await notebookStore.addItem(id, entity.url);
		await loadGraph();
	}

	// ---- Delete --------------------------------------------------------------
	async function doDelete() {
		const id = notebookId;
		if (!id || !detail) return;
		const ok = await confirmAction({
			title: 'Delete notebook?',
			body: `"${detail.name}" will be deleted. Its chats, pages and files stay where they are — they just stop being filed here.`,
			confirmLabel: 'Delete',
			danger: true
		});
		if (!ok) return;
		try {
			await notebookStore.remove(id);
			windowShellStore.closeTabsByRoute(`/notebook/${id}`);
			windowShellStore.openTabFromRoute('/notebooks', { focusExisting: true });
		} catch (e) {
			console.error('[NotebookDetailView] delete failed:', e);
			toast.error('Failed to delete notebook');
		}
	}

	const accent = $derived(detail?.accent_color || null);
	const manuscriptCount = $derived(memberItems.filter((i) => i.role === 'manuscript').length);
</script>

<div class="notebook-detail" style={accent ? `--room-accent: ${accent}` : ''}>
	{#if loading && !detail}
		<div class="state"><Icon icon="ri:loader-4-line" width="18" class="spin" /> Loading…</div>
	{:else if error}
		<div class="state error">{error}</div>
	{:else if detail}
		<div
			class="inner"
			class:drop-active={dropActive}
			role="region"
			aria-label="Notebook contents"
			ondragover={(e) => {
				e.preventDefault();
				dropActive = true;
			}}
			ondragleave={() => (dropActive = false)}
			ondrop={handleDrop}
		>
			<header class="head">
				<div class="head-main">
					<Popover bind:open={iconOpen} placement="bottom-start">
						{#snippet trigger({ toggle }: { toggle: () => void })}
							<button class="nb-icon" class:tinted={!!accent} title="Change icon" onclick={toggle}>
								<Icon icon={detail?.icon || 'ri:booklet-line'} width="22" />
							</button>
						{/snippet}
						{#snippet children({ close }: { close: () => void })}
							<IconPicker value={detail?.icon ?? null} onSelect={setIcon} {close} />
						{/snippet}
					</Popover>

					<div class="head-text">
						<textarea
							class="title-input font-serif"
							bind:value={nameDraft}
							rows="1"
							placeholder="Untitled notebook"
							onfocus={() => (nameFocused = true)}
							onblur={commitName}
							onkeydown={(e) => {
								if (e.key === 'Enter') {
									e.preventDefault();
									e.currentTarget.blur();
								}
								if (e.key === 'Escape') {
									nameDraft = detail?.name ?? '';
									e.currentTarget.blur();
								}
							}}
						></textarea>
						<textarea
							class="desc-input"
							bind:value={memoDraft}
							rows="1"
							placeholder="Add a description — what this notebook is for. It also gives the assistant context."
							onfocus={() => (memoFocused = true)}
							onblur={commitMemo}
							onkeydown={(e) => {
								if (e.key === 'Escape') {
									memoDraft = detail?.current_status ?? '';
									e.currentTarget.blur();
								}
							}}
						></textarea>
					</div>

					<div class="head-actions">
						<Popover bind:open={overflowOpen} placement="bottom-end">
							{#snippet trigger({ toggle }: { toggle: () => void })}
								<button class="icon-btn" title="More" onclick={toggle}>
									<Icon icon="ri:more-line" width="16" />
								</button>
							{/snippet}
							{#snippet children({ close }: { close: () => void })}
								<div class="menu">
									<button
										class="menu-item"
										onclick={() => {
											close();
											colorOpen = true;
										}}
									>
										<Icon icon="ri:palette-line" width="15" /> Change color
									</button>
									<button
										class="menu-item danger"
										onclick={() => {
											close();
											doDelete();
										}}
									>
										<Icon icon="ri:delete-bin-line" width="15" /> Delete notebook
									</button>
								</div>
							{/snippet}
						</Popover>
					</div>
				</div>

				<div class="props font-mono">
					<span>{memberItems.length} {memberItems.length === 1 ? 'item' : 'items'}</span>
					{#if manuscriptCount > 0}
						<span class="dot">·</span>
						<span>{manuscriptCount} manuscript</span>
					{/if}
					<span class="dot">·</span>
					<span>{roomChats.length} {roomChats.length === 1 ? 'chat' : 'chats'}</span>
				</div>
			</header>

			<form class="ask" onsubmit={submitAsk}>
				<input class="ask-input" bind:value={askDraft} placeholder="Ask this notebook…" />
				<button
					class="ask-send"
					type="submit"
					disabled={!askDraft.trim()}
					title="Ask — grounded in this notebook"
				>
					<Icon icon="ri:arrow-right-line" width="15" />
				</button>
			</form>

			{#if graph.nodes.length > 0}
				<NotebookGraphBand
					nodes={graph.nodes}
					edges={graph.edges}
					selected={selectedEntity}
					onSelect={(url) => (selectedEntity = url)}
				/>
			{/if}

			<section class="grid-section">
				<div class="section-head">
					<h2 class="eyebrow font-mono">Contents</h2>
					<button class="add-btn" onclick={openPicker} title="Add a page, person, place, file, or link">
						<Icon icon="ri:add-line" width="14" />
					</button>
				</div>

				{#if allRows.length === 0}
					<button class="add-row" onclick={openPicker}>
						<Icon icon="ri:add-line" width="15" /> Add pages, people, places, or links — or drop files here
					</button>
				{:else}
					<UniversalDataGrid
						items={rows}
						{columns}
						entityType="notebook-item"
						emptyIcon="ri:filter-line"
						emptyMessage="No members reference that entity"
						searchPlaceholder="Search this notebook…"
						onItemClick={(row) => openUrl(row.url)}
						onItemContextMenu={rowMenu}
					>
						{#snippet tableRow(row: MemberRow)}
							<td class="c-name">
								<span class="name-cell">
									<Icon icon={row.icon} width="15" />
									<span class="name-text">{row.name}</span>
								</span>
							</td>
							<td class="c-dim">{row.kind}</td>
							<td>
								<span class="role-chip" class:manuscript={row.role === 'manuscript'}>
									{row.roleText}
								</span>
							</td>
							<td class="c-dim hide-mobile">
								{#if row.status === 'Failed'}
									<button
										class="retry"
										onclick={(e) => {
											e.stopPropagation();
											retryExtraction(row.url);
										}}
									>
										Failed — retry
									</button>
								{:else}
									{row.status}
								{/if}
							</td>
							<td class="c-dim hide-mobile">{row.added}</td>
						{/snippet}

						{#snippet card(row: MemberRow)}
							<div class="nb-card">
								<span class="nb-card-top">
									<Icon icon={row.icon} width="15" />
									<span class="role-chip" class:manuscript={row.role === 'manuscript'}>
										{row.roleText}
									</span>
								</span>
								<span class="nb-card-name">{row.name}</span>
								<span class="nb-card-meta font-mono">
									{row.kind}{row.status !== '—' ? ` · ${row.status}` : ''}
								</span>
							</div>
						{/snippet}
					</UniversalDataGrid>
				{/if}
			</section>
		</div>
	{:else}
		<div class="state">Notebook not found.</div>
	{/if}
</div>

{#if pickerPos}
	<RefPicker
		mode="single"
		position={pickerPos}
		placeholder="Add a person, page, or link…"
		excludeIds={memberItems.map((i) => i.url)}
		onSelect={addMember}
		onClose={() => (pickerPos = null)}
	/>
{/if}

<ColorPickerModal
	open={colorOpen}
	value={accent}
	onSelect={setAccent}
	onClose={() => (colorOpen = false)}
/>

<style>
	.notebook-detail { width: 100%; height: 100%; overflow-y: auto; }
	.inner {
		max-width: 1080px;
		margin: 0 auto;
		padding: 3rem 2rem 6rem;
		display: flex;
		flex-direction: column;
		gap: 1.6rem;
	}
	.inner.drop-active { outline: 1.5px dashed var(--room-accent, var(--color-primary)); outline-offset: 10px; border-radius: 10px; }
	.state { display: flex; align-items: center; gap: 8px; padding: 3rem 2rem; color: var(--color-foreground-muted); }
	.state.error { color: var(--color-error, #dc2626); }

	/* Header */
	.head { display: flex; flex-direction: column; gap: 0.7rem; }
	.head-main { display: flex; align-items: flex-start; gap: 14px; }
	.head-text { flex: 1; min-width: 0; display: flex; flex-direction: column; }
	.nb-icon {
		display: grid; place-items: center; width: 46px; height: 46px; flex-shrink: 0;
		border-radius: 12px; border: 1px solid var(--color-border);
		background: var(--color-surface-elevated); color: var(--color-foreground); cursor: pointer;
	}
	.nb-icon.tinted {
		background: color-mix(in srgb, var(--room-accent) 14%, var(--color-surface-elevated));
		border-color: color-mix(in srgb, var(--room-accent) 32%, var(--color-border));
		color: color-mix(in srgb, var(--room-accent) 80%, var(--color-foreground));
	}
	.head-actions { display: flex; gap: 2px; flex-shrink: 0; }
	.icon-btn {
		display: grid; place-items: center; width: 30px; height: 30px;
		border: none; border-radius: 8px; background: transparent;
		color: var(--color-foreground-subtle); cursor: pointer;
	}
	.icon-btn:hover { background: var(--color-surface-elevated); color: var(--color-foreground); }

	.title-input, .desc-input {
		display: block; width: 100%; resize: none; overflow: hidden;
		border: none; background: transparent; outline: none;
		field-sizing: content;
	}
	.title-input {
		font-size: 2rem; font-weight: 500; line-height: 1.12;
		color: var(--color-foreground); padding: 0 0 2px;
		border-bottom: 1.5px solid transparent;
	}
	.title-input:focus {
		border-bottom-color: color-mix(in srgb, var(--room-accent, var(--color-foreground)) 45%, var(--color-border));
	}
	.desc-input {
		margin-top: 0.4rem; min-height: 1.5rem; padding: 0;
		font: inherit; font-size: 0.95rem; line-height: 1.55;
		color: var(--color-foreground-muted);
	}
	.desc-input:focus { color: var(--color-foreground); }
	.title-input::placeholder, .desc-input::placeholder { color: var(--color-foreground-subtle); }

	.props {
		font-size: 11px; letter-spacing: 0.04em; text-transform: uppercase;
		color: var(--color-foreground-subtle); padding-left: 60px;
	}
	.props .dot { margin: 0 0.6ch; opacity: 0.5; }

	/* Overflow menu */
	.menu { display: flex; flex-direction: column; min-width: 190px; padding: 4px; }
	.menu-item {
		display: flex; align-items: center; gap: 9px; width: 100%; text-align: left;
		padding: 7px 9px; border: none; border-radius: 7px; background: transparent;
		font: inherit; font-size: 0.85rem; color: var(--color-foreground); cursor: pointer;
	}
	.menu-item:hover { background: var(--color-surface-elevated); }
	.menu-item.danger { color: var(--color-error, #dc2626); }

	/* Ask bar */
	.ask {
		display: flex; align-items: center; gap: 10px; height: 46px;
		padding: 0 6px 0 14px;
		border: 1px solid var(--color-border); border-radius: 12px;
		background: var(--color-surface-elevated);
		transition: border-color 120ms, box-shadow 120ms;
	}
	.ask:focus-within {
		border-color: color-mix(in srgb, var(--room-accent, var(--color-foreground-subtle)) 55%, var(--color-border));
		box-shadow: 0 0 0 3px color-mix(in srgb, var(--room-accent, var(--color-foreground-subtle)) 13%, transparent);
	}
	.ask > :global(svg) { color: var(--color-foreground-subtle); flex-shrink: 0; }
	.ask-input {
		flex: 1; min-width: 0; border: none; background: transparent; outline: none;
		font: inherit; font-size: 0.95rem; color: var(--color-foreground);
	}
	.ask-input::placeholder { color: var(--color-foreground-subtle); }
	.ask-send {
		display: grid; place-items: center; width: 32px; height: 32px; flex-shrink: 0;
		border: none; border-radius: 9px; cursor: pointer;
		background: var(--room-accent, var(--color-foreground)); color: var(--color-background, #fff);
	}
	.ask-send:disabled { opacity: 0.35; cursor: default; }

	/* Sections */
	.section-head {
		display: flex; align-items: center; justify-content: space-between;
		padding-bottom: 0.4rem;
	}
	.eyebrow {
		margin: 0; font-size: 11px; font-weight: 400; letter-spacing: 0.09em;
		text-transform: uppercase; color: var(--color-foreground-subtle);
	}
	.add-btn {
		display: grid; place-items: center; width: 22px; height: 22px;
		border: none; border-radius: 6px; background: transparent;
		color: var(--color-foreground-subtle); cursor: pointer;
	}
	.add-btn:hover { background: var(--color-surface-elevated); color: var(--color-foreground); }
	.add-row {
		display: flex; align-items: center; gap: 8px; width: 100%; text-align: left;
		padding: 1rem 0.6rem; border: 1px dashed var(--color-border); border-radius: 8px;
		background: transparent; cursor: pointer;
		font: inherit; font-size: 0.9rem; color: var(--color-foreground-subtle);
	}
	.add-row:hover { color: var(--color-foreground); border-color: var(--room-accent, var(--color-primary)); }

	/* Grid cells */
	.c-name { padding: 0.5rem 0.75rem; padding-left: 0; }
	.c-dim { padding: 0.5rem 0.75rem; color: var(--color-foreground-muted); }
	.name-cell { display: inline-flex; align-items: center; gap: 0.55rem; min-width: 0; }
	.name-cell :global(svg) { flex-shrink: 0; color: var(--color-foreground-subtle); }
	.name-text { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 450; }
	.role-chip {
		display: inline-block; font-size: 10px; letter-spacing: 0.04em;
		padding: 1.5px 8px; border-radius: 999px;
		border: 1px solid var(--color-border); color: var(--color-foreground-subtle);
		white-space: nowrap;
	}
	.role-chip.manuscript {
		color: var(--room-accent, var(--color-primary));
		border-color: color-mix(in srgb, var(--room-accent, var(--color-primary)) 40%, var(--color-border));
	}
	.retry {
		border: none; background: none; padding: 0; font: inherit; font-size: inherit;
		color: var(--color-error, #dc2626); cursor: pointer; text-decoration: underline;
	}
	@media (max-width: 768px) {
		.hide-mobile { display: none; }
	}

	/* Card view */
	.nb-card {
		display: flex; flex-direction: column; gap: 0.4rem;
		width: 100%; height: 100%; padding: 0.85rem 0.9rem;
		border: 1px solid var(--color-border); border-radius: 10px;
		background: var(--color-surface);
		transition: background-color 0.12s ease, border-color 0.12s ease;
	}
	:global(.card:hover) .nb-card {
		background: var(--color-background-hover);
		border-color: color-mix(in srgb, var(--room-accent, var(--color-primary)) 32%, var(--color-border));
	}
	.nb-card-top { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
	.nb-card-top :global(svg) { color: var(--color-foreground-subtle); flex-shrink: 0; }
	.nb-card-name {
		font-size: 0.875rem; font-weight: 550; line-height: 1.35; color: var(--color-foreground);
		display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2;
		-webkit-box-orient: vertical; overflow: hidden;
	}
	.nb-card-meta { font-size: 10px; letter-spacing: 0.03em; color: var(--color-foreground-subtle); }

	:global(.spin) { animation: spin 0.8s linear infinite; }
	@keyframes spin { to { transform: rotate(360deg); } }
</style>
