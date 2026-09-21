<script lang="ts">
	import type { Tab } from '$lib/tabs/types';
	import type { ProjectDetail, ProjectGraph } from '$lib/api/client';
	import Icon from '$lib/components/Icon.svelte';
	import { Button, IconButton, TextAction } from '$lib';
	import { projectStore } from '$lib/stores/project.svelte';
	import { chatSessions } from '$lib/stores/chatSessions.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { contextMenu } from '$lib/stores/contextMenu.svelte';
	import RefPicker from '$lib/components/RefPicker.svelte';
	import IconPicker from '$lib/components/IconPicker.svelte';
	import MenuItem from '$lib/components/MenuItem.svelte';
	import UniversalDataGrid, { type Column } from '$lib/components/datagrid/UniversalDataGrid.svelte';
	import type { FilterDef } from '$lib/components/datagrid/types';
	import { Popover } from '$lib/floating';
	import { confirmAction } from '$lib/stores/dialog.svelte';
	import { toast } from 'svelte-sonner';
	import { getRefSummary } from '$lib/utils/refSummary';
	import {
		getPage,
		getDriveFile,
		uploadDriveFile,
		addProjectItem,
		reextractDriveFile,
		getProjectGraph
	} from '$lib/api/client';
	import { askVirtues } from '$lib/stores/pendingPrompt.svelte';
	import { isProjectUrl } from '$lib/utils/contextMenuItems';

	let { tab }: { tab: Tab; active?: boolean } = $props();

	const projectId = $derived.by(() => {
		const m = tab.route.match(/^\/project\/([^/]+)$/);
		return m?.[1] ?? null;
	});

	let detail = $state<ProjectDetail | null>(null);
	let loading = $state(false);
	let error = $state<string | null>(null);

	async function load(force = false) {
		const id = projectId;
		if (!id) return;
		loading = true;
		error = null;
		try {
			detail = await projectStore.get(id, { force });
		} catch (e) {
			console.error('[ProjectDetailView] Failed to load project:', e);
			error = e instanceof Error ? e.message : 'Failed to load project';
			detail = null;
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		if (projectId) load();
	});

	// ---- Entity facets over the members --------------------------------------
	// Same endpoint as before; it is a filter source now, not a picture. A graph
	// of three unconnected nodes cost the top of the page to say less than a row
	// of chips does.
	let graph = $state<ProjectGraph>({ nodes: [], edges: [] });

	async function loadGraph() {
		const id = projectId;
		if (!id) {
			graph = { nodes: [], edges: [] };
			return;
		}
		try {
			graph = await getProjectGraph(id);
		} catch (e) {
			// A missing graph shouldn't take the page down — it's an aid, not the content.
			console.error('[ProjectDetailView] Failed to load graph:', e);
			graph = { nodes: [], edges: [] };
		}
	}
	$effect(() => {
		if (projectId) loadGraph();
	});

	// Chats filed into this room — sourced from the authoritative session list,
	// not from membership rows, so removing a member can't desync a chat.
	const roomChats = $derived(chatSessions.sessions.filter((s) => s.project_id === projectId));

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
			if (type === 'person' || type === 'place' || type === 'org') {
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

	function memberType(url: string): string {
		if (url.startsWith('http://') || url.startsWith('https://')) return 'Link';
		const t = url.split('/')[1] ?? '';
		const map: Record<string, string> = {
			person: 'Person',
			page: 'Page',
			org: 'Org',
			place: 'Place',
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
	 * What this member is to the project, in the user's terms rather than the
	 * schema's. `manuscript` and `pin` are stored roles; Reference is derived —
	 * a filed person or place is reference material by nature, not by a flag.
	 *
	 * Deliberately NOT "Source": that word already means a credential connection
	 * elsewhere in the app. And not "Bible", which is author jargon that reads as
	 * nonsense in a project about a kitchen remodel.
	 */
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
		status: string;
		added: string;
		icon: string;
	}

	/**
	 * Everything the project holds, in one table. Chats used to live in their
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
			status: statusLabel(it.url),
			added: formatAdded(it.added_at),
			icon: iconForUrl(it.url)
		}));

		const chats: MemberRow[] = roomChats.map((c) => ({
			id: `/chat/${c.conversation_id}`,
			url: `/chat/${c.conversation_id}`,
			name: c.title ?? 'Untitled chat',
			kind: 'Chat',
			status: `${c.message_count} ${c.message_count === 1 ? 'message' : 'messages'}`,
			added: formatAdded(c.last_message_at ?? c.first_message_at),
			icon: c.icon || 'ri:chat-3-line'
		}));

		return [...members, ...chats];
	});

	/**
	 * The entity facets, expressed as one of the grid's own filters instead of a
	 * bespoke chip rail above it. They were a second filtering surface in a
	 * second visual language sitting 40px from the first — same job, nothing
	 * shared. As a filter they get the grid's chip, its clear affordance and its
	 * active-count badge for free.
	 */
	const nodeMembers = $derived(new Map(graph.nodes.map((n) => [n.url, new Set(n.item_urls)])));

	const entityFilters = $derived.by<FilterDef<MemberRow>[]>(() => {
		if (graph.nodes.length === 0) return [];
		return [
			{
				id: 'entity',
				kind: 'multi',
				label: 'Mentions',
				options: graph.nodes.map((n) => ({
					value: n.url,
					label: n.name,
					icon: iconForUrl(n.url)
				})),
				predicate: (row, value) => {
					const urls = Array.isArray(value) ? value : value ? [value] : [];
					if (urls.length === 0) return true;
					return urls.some((u) => nodeMembers.get(u)?.has(row.url));
				}
			}
		];
	});

	/**
	 * Kind has no column: the row icon already says what a thing is, and a word
	 * repeating the glyph beside it was one of five competing text styles.
	 * Status only earns its column when something in the set actually has one —
	 * otherwise it's a column of em-dashes.
	 */
	const anyStatus = $derived(allRows.some((r) => r.status !== '—'));

	const columns = $derived.by<Column<MemberRow>[]>(() => {
		const cols: Column<MemberRow>[] = [
			{ key: 'name', label: 'Name', width: '62%', minWidth: '220px' },
			// Groupable but not rendered: the row icon already says what a thing is.
			{ key: 'kind', label: 'Kind', groupable: true, hidden: true }
		];
		if (anyStatus) {
			cols.push({ key: 'status', label: 'Status', width: '18%', minWidth: '110px', hideOnMobile: true });
		}
		cols.push({ key: 'added', label: 'Added', width: '14%', minWidth: '90px', hideOnMobile: true });
		return cols;
	});

	// ---- Actions -------------------------------------------------------------
	/**
	 * Open a member *beside* the project rather than over it. This is the whole
	 * of "work mode": the project narrows to a rail and stays reachable while
	 * you read or write the thing you picked, so there is no mode to switch.
	 */
	function openUrl(url: string) {
		if (url.startsWith('http://') || url.startsWith('https://')) {
			window.open(url, '_blank', 'noopener,noreferrer');
			return;
		}
		windowShellStore.openRouteBeside(url);
	}

	async function removeMembers(rows: MemberRow[], clear: () => void) {
		const id = projectId;
		if (!id) return;
		const ok = await confirmAction({
			title: rows.length === 1 ? 'Remove item?' : `Remove ${rows.length} items?`,
			body: 'They stay where they are — they just stop being filed in this project.',
			confirmLabel: 'Remove',
			danger: true
		});
		if (!ok) return;
		try {
			for (const r of rows) await projectStore.removeItem(id, r.url);
			await loadGraph();
		} catch (e) {
			console.error('[ProjectDetailView] bulk remove failed:', e);
			toast.error('Could not remove every item');
		} finally {
			clear();
		}
	}

	async function removeMember(url: string) {
		const id = projectId;
		if (!id) return;
		await projectStore.removeItem(id, url);
		await loadGraph();
	}

	/**
	 * Manual order. `app_project_items.sort_order` has always existed, has
	 * always been the list's ORDER BY, and has never been settable from the UI —
	 * so the project could only ever be in the order things happened to arrive.
	 *
	 * Order is also the only structure a project has that the user authors
	 * rather than derives: groups come from properties, this comes from you.
	 */
	async function moveTo(url: string, edge: 'top' | 'bottom') {
		const id = projectId;
		if (!id || !detail) return;
		// The whole membership must go in the payload — reorder rewrites the list,
		// so anything omitted would be dropped from the cached detail.
		const rest = detail.items.map((i) => i.url).filter((u) => u !== url);
		const next = edge === 'top' ? [url, ...rest] : [...rest, url];
		try {
			await projectStore.reorderItems(id, next);
			await load(true);
		} catch (e) {
			console.error('[ProjectDetailView] reorder failed:', e);
			toast.error('Could not move that item');
		}
	}

	function rowMenu(row: MemberRow, e: MouseEvent) {
		e.preventDefault();
		const items = [
			{ id: 'open', label: 'Open', icon: 'ri:external-link-line', action: () => openUrl(row.url) }
		];
		// Chats are sourced from the session list, not from membership rows, so
		// there is no sort_order of theirs to set.
		const isMember = memberItems.some((i) => i.url === row.url);
		if (isMember && memberItems.length > 1) {
			items.push(
				{
					id: 'top',
					label: 'Move to top',
					icon: 'ri:skip-up-line',
					dividerBefore: true,
					action: () => moveTo(row.url, 'top')
				} as (typeof items)[number],
				{
					id: 'bottom',
					label: 'Move to bottom',
					icon: 'ri:skip-down-line',
					action: () => moveTo(row.url, 'bottom')
				} as (typeof items)[number]
			);
		}
		items.push({
			id: 'remove',
			label: 'Remove from project',
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

	// ---- Ask this project ---------------------------------------------------
	let askDraft = $state('');
	function submitAsk(e: Event) {
		e.preventDefault();
		const text = askDraft.trim();
		const id = projectId;
		if (!text || !id) return;
		askVirtues(text, id);
		askDraft = '';
	}

	// ---- Drag-drop onto the project: upload + add in one motion -------------
	let dropActive = $state(false);
	async function handleDrop(e: DragEvent) {
		e.preventDefault();
		dropActive = false;
		const id = projectId;
		const dropped = e.dataTransfer?.files;
		if (!id || !dropped || dropped.length === 0) return;
		for (const f of dropped) {
			try {
				const uploaded = await uploadDriveFile('uploads', f);
				await addProjectItem(id, `/drive/${uploaded.id}`);
			} catch (err) {
				console.error('[ProjectDetailView] drop-add failed:', err);
				toast.error(`Could not add ${f.name}`);
			}
		}
		await load(true);
		await loadGraph();
	}

	// ---- Title + brief: always live, no edit mode ----------------------------
	/**
	 * The subtitle is the project's *brief* (`instructions`), not its memo.
	 *
	 * There are two text fields on a project and the wrong one was on screen.
	 * `instructions` is a standing direction the assistant is told to follow in
	 * every chat in this project — a real input — and it had no UI at all.
	 * `current_status` is a transient catch-up note, and it was occupying the
	 * header labelled "description".
	 *
	 * No auto-generated summary: the member list is directly below and fully
	 * legible, so a generated description would only restate what's visible.
	 * What can't be derived is what the project is *for*.
	 */
	let nameDraft = $state('');
	let briefDraft = $state('');
	let memoDraft = $state('');
	let nameFocused = $state(false);
	let briefFocused = $state(false);
	let memoFocused = $state(false);
	$effect(() => {
		if (!detail) return;
		if (!nameFocused) nameDraft = detail.name;
		if (!briefFocused) briefDraft = detail.instructions ?? '';
		if (!memoFocused) memoDraft = detail.current_status ?? '';
	});

	async function commitName() {
		nameFocused = false;
		const id = projectId;
		if (!id || !detail) return;
		const name = nameDraft.trim();
		if (!name) {
			nameDraft = detail.name;
			return;
		}
		if (name === detail.name) return;
		await projectStore.update(id, { name });
	}

	async function commitBrief() {
		briefFocused = false;
		const id = projectId;
		if (!id || !detail) return;
		const brief = briefDraft.trim() || null;
		if (brief === (detail.instructions ?? null)) return;
		await projectStore.update(id, { instructions: brief });
	}

	async function commitMemo() {
		memoFocused = false;
		const id = projectId;
		if (!id || !detail) return;
		const memo = memoDraft.trim() || null;
		if (memo === (detail.current_status ?? null)) return;
		await projectStore.update(id, { current_status: memo });
	}

	/** The memo is only on screen when it has something to say — otherwise it's
	 *  a second empty field competing with the brief. Adding one is a menu item. */
	let memoOpen = $state(false);
	const showMemo = $derived(memoOpen || !!detail?.current_status);

	// ---- Icon ----------------------------------------------------------------
	let iconOpen = $state(false);
	let overflowOpen = $state(false);
	async function setIcon(icon: string | null) {
		const id = projectId;
		if (!id) return;
		await projectStore.update(id, { icon });
		await load(true);
	}

	/**
	 * The project's color, in the same swatch row as its icon.
	 *
	 * Stored in `accent_color`, which predates the token rule and still holds
	 * raw hex for projects colored before it — `accentCss` resolves either, so
	 * old values keep working and new ones are theme-correct. Writing a token
	 * key here converts a project the first time it's recolored, which is the
	 * only migration that doesn't guess on the user's behalf.
	 */
	async function setAccent(color: string | null) {
		const id = projectId;
		if (!id) return;
		await projectStore.update(id, { accent_color: color });
		await load(true);
	}

	// ---- Membership ----------------------------------------------------------
	let pickerPos = $state<{ x: number; y: number } | null>(null);
	function openPicker(e: MouseEvent) {
		pickerPos = { x: e.clientX, y: e.clientY };
	}
	async function addMember(entity: { url: string }) {
		pickerPos = null;
		const id = projectId;
		if (!id || !entity.url) return;
		// you can't put a project in a project — the picker already hides them,
		// and the server answers 400 if one gets through.
		if (isProjectUrl(entity.url)) return;
		await projectStore.addItem(id, entity.url);
		// A chat is listed here from the session list by its `project_id`,
		// which the box has just set; the sessions have to be re-read or the
		// add looks like it did nothing (VIR-359).
		if (entity.url.startsWith('/chat/')) await chatSessions.refresh();
		await loadGraph();
	}

	// ---- Archive -------------------------------------------------------------
	// Reversible, so no confirm. The project stays open in this tab, marked.
	async function toggleArchive() {
		const id = projectId;
		if (!id || !detail) return;
		try {
			if (detail.archived_at) await projectStore.unarchive(id);
			else await projectStore.archive(id);
			await load(true);
		} catch (e) {
			console.error('[ProjectDetailView] archive failed:', e);
			toast.error(detail.archived_at ? 'Failed to unarchive project' : 'Failed to archive project');
		}
	}

	// ---- Delete --------------------------------------------------------------
	async function doDelete() {
		const id = projectId;
		if (!id || !detail) return;
		const ok = await confirmAction({
			title: 'Delete project?',
			body: `"${detail.name}" goes to Recently deleted, where you can restore it for 30 days. Its chats, pages and files stay where they are.`,
			confirmLabel: 'Delete'
		});
		if (!ok) return;
		try {
			await projectStore.remove(id);
			windowShellStore.closeTabsByRoute(`/project/${id}`);
			windowShellStore.openTabFromRoute('/projects', { focusExisting: true });
		} catch (e) {
			console.error('[ProjectDetailView] delete failed:', e);
			toast.error('Failed to delete project');
		}
	}

</script>

<div class="project-detail">
	{#if loading && !detail}
		<div class="state"><Icon icon="ri:loader-4-line" width="18" class="spin" /> Loading…</div>
	{:else if error}
		<div class="state error">{error}</div>
	{:else if detail}
		<div
			class="inner"
			class:drop-active={dropActive}
			role="region"
			aria-label="Project contents"
			ondragover={(e) => {
				e.preventDefault();
				dropActive = true;
			}}
			ondragleave={(e) => {
				// Crossing into a child fires dragleave too; only leaving the
				// project itself should drop the highlight.
				const zone = e.currentTarget as HTMLElement;
				if (!e.relatedTarget || !zone.contains(e.relatedTarget as Node)) {
					dropActive = false;
				}
			}}
			ondrop={handleDrop}
		>
			<header class="head">
				<div class="head-main">
					<Popover bind:open={iconOpen} placement="bottom-start">
						{#snippet trigger({ toggle }: { toggle: () => void })}
							<button class="nb-icon" title="Change icon" onclick={toggle}>
								<Icon icon={detail?.icon || 'ri:folder-3-line'} width="22" />
							</button>
						{/snippet}
						{#snippet children({ close }: { close: () => void })}
							<IconPicker
							value={detail?.icon ?? null}
							onSelect={setIcon}
							{close}
							color={detail?.accent_color ?? null}
							onColorSelect={setAccent}
						/>
						{/snippet}
					</Popover>

					<div class="head-text">
						<textarea
							class="title-input font-serif"
							bind:value={nameDraft}
							rows="1"
							placeholder="Untitled project"
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
							bind:value={briefDraft}
							rows="1"
							placeholder="What this project is for. The assistant follows this in every chat here."
							onfocus={() => (briefFocused = true)}
							onblur={commitBrief}
							onkeydown={(e) => {
								if (e.key === 'Escape') {
									briefDraft = detail?.instructions ?? '';
									e.currentTarget.blur();
								}
							}}
						></textarea>
						{#if showMemo}
							<label class="memo">
								<span class="memo-label font-mono">Where I left off</span>
								<textarea
									class="memo-input"
									bind:value={memoDraft}
									rows="1"
									placeholder="A note to yourself about the current state."
									onfocus={() => (memoFocused = true)}
									onblur={commitMemo}
									onkeydown={(e) => {
										if (e.key === 'Escape') {
											memoDraft = detail?.current_status ?? '';
											e.currentTarget.blur();
										}
									}}
								></textarea>
							</label>
						{/if}
					</div>

					<div class="head-actions">
						<Popover bind:open={overflowOpen} placement="bottom-end">
							{#snippet trigger({ toggle }: { toggle: () => void })}
								<IconButton
									icon="ri:more-line"
									label="More project actions"
									expanded={overflowOpen}
									haspopup="menu"
									onclick={toggle}
								/>
							{/snippet}
							{#snippet children({ close }: { close: () => void })}
								<div class="menu">
									{#if !showMemo}
										<MenuItem
											icon="ri:sticky-note-line"
											label="Add a status note"
											onclick={() => {
												close();
												memoOpen = true;
											}}
										/>
									{/if}
									<MenuItem
										icon={detail?.archived_at ? 'ri:inbox-unarchive-line' : 'ri:archive-line'}
										label={detail?.archived_at ? 'Unarchive project' : 'Archive project'}
										onclick={() => {
											close();
											toggleArchive();
										}}
									/>
									<MenuItem
										icon="ri:delete-bin-line"
										label="Delete project"
										destructive
										onclick={() => {
											close();
											doDelete();
										}}
									/>
								</div>
							{/snippet}
						</Popover>
					</div>
				</div>

				<!-- Counts the same set the grid counts, now that chats are rows in it. -->
				<div class="props font-mono">
					<span>{allRows.length} {allRows.length === 1 ? 'item' : 'items'}</span>
					{#if detail?.archived_at}
						<span class="dot-sep">·</span>
						<span>Archived {new Date(detail.archived_at).toLocaleDateString(undefined, { month: 'short', day: 'numeric' })}</span>
					{/if}
				</div>
			</header>

			<form class="ask" onsubmit={submitAsk}>
				<input class="ask-input" bind:value={askDraft} placeholder="Ask this project…" />
				<!-- The send stays INVISIBLE until there is something to send, which
				     is what the rule this replaced said with `opacity: 0`. A
				     primitive cannot carry that, and neither alternative was
				     acceptable: `{#if}` shrinks the flex:1 input by ~30px on the
				     first keystroke, and reaching into `.v-iconbtn` with :global
				     sets a precedent. So the slot holds the box and does the
				     hiding, and the button inside it is an ordinary disabled one. -->
				<span class="ask-send" class:idle={!askDraft.trim()}>
					<IconButton
						icon="ri:arrow-right-line"
						label="Ask — grounded in this project"
						size="sm"
						type="submit"
						disabled={!askDraft.trim()}
					/>
				</span>
			</form>

			<section class="grid-section">
				{#if allRows.length === 0}
					<button class="add-row" onclick={openPicker}>
						<Icon icon="ri:add-line" width="15" /> Add pages, people, places, or links — or drop files here
					</button>
				{:else}
					<UniversalDataGrid
						items={allRows}
						{columns}
						entityType="project-item"
						emptyIcon="ri:filter-line"
						emptyMessage="No members match that filter"
						searchPlaceholder="Search this project…"
						selectable
						filters={entityFilters}
						rowIcon={(row) => row.icon}
						onItemClick={(row) => openUrl(row.url)}
						onItemContextMenu={rowMenu}
					>
						{#snippet bulkActions(rows: MemberRow[], clear: () => void)}
							<Button
								variant="danger"
								size="sm"
								onclick={() => removeMembers(rows, clear)}>Remove</Button
							>
						{/snippet}

						{#snippet rowActions(row: MemberRow)}
							<IconButton
								icon="ri:more-line"
								label={`Actions for ${row.name}`}
								size="sm"
								haspopup="menu"
								onclick={(e) => rowMenu(row, e)}
							/>
						{/snippet}

						{#snippet toolbarActions()}
							<IconButton
								icon="ri:add-line"
								label="Add a page, person, place, file, or link"
								variant="secondary"
								onclick={openPicker}
							/>
						{/snippet}

						{#snippet tableRow(row: MemberRow)}
							<!-- No icon here: it moved into the grid's leading column, where it
							     shares a slot with the select box. -->
							<td class="c-name">
								<span class="name-text">{row.name}</span>
							</td>
							{#if anyStatus}
								<td class="c-dim hide-mobile">
									{#if row.status === 'Failed'}
										<TextAction
											inline
											onclick={(e) => {
												e.stopPropagation();
												retryExtraction(row.url);
											}}
										>
											Failed — retry
										</TextAction>
									{:else}
										{row.status}
									{/if}
								</td>
							{/if}
							<td class="c-dim hide-mobile">{row.added}</td>
						{/snippet}

						{#snippet card(row: MemberRow)}
							<div class="nb-card">
								<span class="nb-card-top">
									<Icon icon={row.icon} width="15" />
								</span>
								<span class="nb-card-name">{row.name}</span>
								<!-- Not the kind: the glyph above says it, and when the cards are
								     grouped by Kind the column header says it a third time. -->
								{#if row.status !== '—'}
									<span class="nb-card-meta font-mono">{row.status}</span>
								{/if}
							</div>
						{/snippet}
					</UniversalDataGrid>
				{/if}
			</section>
		</div>
	{:else}
		<div class="state">Project not found.</div>
	{/if}
</div>

{#if pickerPos}
	<RefPicker
		mode="single"
		position={pickerPos}
		placeholder="Add a person, page, or link…"
		excludeIds={memberItems.map((i) => i.url)}
		filter={(e) => !isProjectUrl(e.url)}
		onSelect={addMember}
		onClose={() => (pickerPos = null)}
	/>
{/if}


<style>
	.project-detail { width: 100%; height: 100%; overflow-y: auto; }
	.inner {
		max-width: 1080px;
		margin: 0 auto;
		padding: 3rem 2rem 6rem;
		display: flex;
		flex-direction: column;
		gap: 1.6rem;
	}
	.inner.drop-active { outline: 1.5px dashed var(--color-primary); outline-offset: 10px; border-radius: 10px; }
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
	.head-actions { display: flex; gap: 2px; flex-shrink: 0; }
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
		border-bottom-color: color-mix(in srgb, var(--color-foreground) 45%, var(--color-border));
	}
	.desc-input {
		margin-top: 0.4rem; min-height: 1.5rem; padding: 0;
		font: inherit; font-size: 0.95rem; line-height: 1.55;
		color: var(--color-foreground-muted);
	}
	.desc-input:focus { color: var(--color-foreground); }
	.title-input::placeholder, .desc-input::placeholder { color: var(--color-foreground-subtle); }

	/* The memo is a note to self, not identity — labelled so it can't be
	   mistaken for the brief above it, and quieter than both. */
	.memo { display: flex; align-items: baseline; gap: 8px; margin-top: 0.45rem; }
	.memo-label {
		flex-shrink: 0; font-size: 10px; letter-spacing: 0.06em;
 color: var(--color-foreground-subtle);
	}
	.memo-input {
		flex: 1; min-width: 0; display: block; resize: none; overflow: hidden;
		border: none; background: transparent; outline: none; padding: 0;
		field-sizing: content;
		font: inherit; font-size: 0.85rem; line-height: 1.5;
		color: var(--color-foreground-muted);
	}
	.memo-input::placeholder { color: var(--color-foreground-subtle); }

	.props {
		font-size: 11px;
		color: var(--color-foreground-subtle); padding-left: 60px;
		display: flex; gap: 6px; align-items: center;
	}
	.props .dot-sep { opacity: 0.5; }

	/* Overflow menu */
	.menu { display: flex; flex-direction: column; min-width: 190px; padding: 4px; }

	/* Ask bar — a line, not a slab.
	   It was the largest, highest-contrast object on the page and the least
	   important one: a filled 46px card with its own radius and shadow, sitting
	   above the content it asks about. It's an entry point, so it gets one rule
	   and the weight of a caption until you're actually in it. */
	.ask {
		display: flex; align-items: center; gap: 8px; height: 34px;
		padding: 0;
		border-bottom: 1px solid var(--color-border);
		transition: border-color 120ms;
	}
	.ask:focus-within { border-bottom-color: var(--color-foreground-subtle); }
	.ask > :global(svg) { color: var(--color-foreground-subtle); flex-shrink: 0; }
	.ask-input {
		flex: 1; min-width: 0; border: none; background: transparent; outline: none;
		font: inherit; font-size: 0.875rem; color: var(--color-foreground);
	}
	.ask-input::placeholder { color: var(--color-foreground-subtle); }

	/* Holds its box so the input never reflows; `visibility` rather than
	   `display` for the same reason, and it also takes the control out of the
	   tab order while there is nothing to send. */
	.ask-send { display: inline-flex; transition: opacity 120ms ease; }
	.ask-send.idle { opacity: 0; visibility: hidden; }

	@media (prefers-reduced-motion: reduce) {
		.ask-send { transition: none; }
	}

	.add-row {
		display: flex; align-items: center; gap: 8px; width: 100%; text-align: left;
		padding: 1rem 0.6rem; border: 1px dashed var(--color-border); border-radius: 8px;
		background: transparent; cursor: pointer;
		font: inherit; font-size: 0.9rem; color: var(--color-foreground-subtle);
	}
	.add-row:hover { color: var(--color-foreground); border-color: var(--color-primary); }

	/* Grid cells */
	/* Matches the grid's own `th` padding so the column label sits over its
	   values. The name cell used to start flush left to make room for the row
	   icon; the icon has its own column now, so it aligns like any other. */
	.c-name { padding: 0.5rem 0.75rem; }
	.c-dim { padding: 0.5rem 0.75rem; color: var(--color-foreground-muted); }
	.name-text {
		display: block; overflow: hidden; text-overflow: ellipsis;
		white-space: nowrap; font-weight: 450;
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
		border-color: color-mix(in srgb, var(--color-primary) 32%, var(--color-border));
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
