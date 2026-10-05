<script lang="ts">
	import type { Tab } from '$lib/tabs/types';
	import type { ProjectChat, ProjectDetail, ProjectGraph, ProjectItemRole } from '$lib/api/client';
	import Icon from '$lib/components/Icon.svelte';
	import ProjectHeader from '$lib/components/projects/ProjectHeader.svelte';
	import ProjectChats from '$lib/components/projects/ProjectChats.svelte';
	import ProjectSection from '$lib/components/projects/ProjectSection.svelte';
	import { PROJECT_ICON } from '$lib/utils/iconHelpers';
	import { Button, IconButton, TextAction } from '$lib';
	import { projectStore } from '$lib/stores/project.svelte';
	import { chatSessions } from '$lib/stores/chatSessions.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { contextMenu } from '$lib/stores/contextMenu.svelte';
	import RefPicker from '$lib/components/RefPicker.svelte';
	import UniversalDataGrid, { type Column } from '$lib/components/datagrid/UniversalDataGrid.svelte';
	import type { FilterDef } from '$lib/components/datagrid/types';
	import { confirmAction } from '$lib/stores/dialog.svelte';
	import { toast } from 'svelte-sonner';
	import {
		uploadDriveFile,
		addProjectItem,
		reextractDriveFile,
		getProjectGraph
	} from '$lib/api/client';
	import { askVirtues } from '$lib/stores/pendingPrompt.svelte';
	import {
		droppedRefUrl,
		fileIntoProject,
		memberIcon,
		memberName,
		projectMemberUrl,
		removeFromProject,
	} from '$lib/utils/projectActions';
	import { getProjectMenuItems } from '$lib/utils/contextMenuItems';

	let { tab }: { tab: Tab; active?: boolean } = $props();

	const projectId = $derived.by(() => {
		const m = tab.route.match(/^\/project\/([^/]+)$/);
		return m?.[1] ?? null;
	});

	/**
	 * A live view of the store's copy, not a snapshot of it. A rename, recolor
	 * or archive from the sidebar, ⌘K or another pane lands in the store, and
	 * this page and its tab follow without being told.
	 */
	const detail = $derived<ProjectDetail | null>(
		projectId ? (projectStore.getCached(projectId) ?? null) : null
	);
	let loading = $state(false);
	let error = $state<string | null>(null);

	async function load(force = false) {
		const id = projectId;
		if (!id) return;
		loading = true;
		error = null;
		try {
			await projectStore.get(id, { force });
		} catch (e) {
			console.error('[ProjectDetailView] Failed to load project:', e);
			error = "Your server couldn't open this project. If you deleted it, you can restore it from Recently deleted.";
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		if (projectId) load();
	});

	// The tab wears the project's name and mark, whichever door opened it —
	// a pin, a citation, ⌘K and a reload all arrive with only the id.
	$effect(() => {
		if (!detail) return;
		const icon = detail.icon || PROJECT_ICON;
		if (tab.label !== detail.name || tab.icon !== icon) {
			windowShellStore.updateTab(tab.id, { label: detail.name, icon });
		}
	});

	const archived = $derived(!!detail?.archived_at);

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

	// Chats filed into this room, from the project's own detail: every chat
	// whose `project_id` is this one. The session list holds only the box's
	// most recent chats, so reading it here dropped a project's older ones. It
	// stays as the fallback for a box older than `detail.chats`.
	const roomChats = $derived.by<ProjectChat[]>(() => {
		if (detail?.chats) return detail.chats;
		return chatSessions.sessions
			.filter((s) => s.project_id === projectId)
			.map((s) => ({
				id: s.conversation_id,
				title: s.title ?? '',
				icon: s.icon,
				message_count: s.message_count,
				last_message_at: s.last_message_at || s.first_message_at
			}));
	});

	// Members = everything except chats (chats render in their own list).
	const memberItems = $derived((detail?.items ?? []).filter((i) => !i.url.startsWith('/chat/')));

	// Names and file statuses come with the project, resolved by the server in
	// one batch. A retry sets a row's status here until the next read.
	let statusOverride = $state<Record<string, string>>({});

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


	/**
	 * What this member is to the project, in the user's terms rather than the
	 * schema's. `manuscript` and `pin` are stored roles; Reference is derived —
	 * a filed person or place is reference material by nature, not by a flag.
	 *
	 * Deliberately NOT "Source": that word already means a credential connection
	 * elsewhere in the app. And not "Bible", which is author jargon that reads as
	 * nonsense in a project about a kitchen remodel.
	 */
	function statusLabel(url: string, role?: string, stored?: string): string {
		// Your own draft says so before anything else: it is the one fact about
		// the row that changes what the assistant does with it.
		if (role === 'manuscript') return 'Your draft';
		const s = statusOverride[url] ?? stored;
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
		role: string;
	}

	/**
	 * The page is two things, in two shapes. Chats are conversations you go
	 * back into: a short list at the top, newest first, with "New chat" at its
	 * head. Files, pages, people and links are the material: the grid, with its
	 * filters, statuses and manual order. They were one table for a while, and
	 * before that two near-identical tables; neither said which was which.
	 *
	 * Chats come from the project's own detail (`detail.chats`), not from
	 * membership rows, so a stale `/chat/` row can't resurrect a deleted chat.
	 */
	const memberRows = $derived.by<MemberRow[]>(() =>
		memberItems.map((it) => ({
			id: it.url,
			url: it.url,
			name: memberName(it),
			kind: memberType(it.url),
			status: statusLabel(it.url, it.role, it.status),
			added: formatAdded(it.added_at),
			icon: memberIcon(it.url),
			role: it.role
		}))
	);


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
					icon: memberIcon(n.url)
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
	const anyStatus = $derived(memberRows.some((r) => r.status !== '—'));

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
			body: 'They stay where they are. They just stop being filed in this project.',
			confirmLabel: 'Remove',
			danger: true
		});
		if (!ok) return;
		try {
			for (const r of rows) await projectStore.removeItem(id, r.url);
			await loadGraph();
		} catch (e) {
			console.error('[ProjectDetailView] bulk remove failed:', e);
			toast.error("Your server couldn't remove every item", {
				description: 'The ones still listed are still in the project. Try again',
			});
		} finally {
			clear();
		}
	}

	async function removeMember(url: string) {
		if (!detail) return;
		await removeFromProject(detail, url);
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
		} catch (e) {
			console.error('[ProjectDetailView] reorder failed:', e);
			toast.error("Your server couldn't move that item", { description: 'Try again' });
		}
	}

	function rowMenu(row: MemberRow, e: MouseEvent) {
		e.preventDefault();
		const items = [
			{ id: 'open', label: 'Open', icon: 'ri:external-link-line', action: () => openUrl(row.url) }
		];
		// Your own writing is kept out of retrieval, so the assistant never
		// cites your draft back to you as if it were a source.
		if (row.url.startsWith('/page/') && !archived) {
			const draft = row.role === 'manuscript';
			items.push({
				id: 'draft',
				label: draft ? 'Use as a source' : 'Mark as your draft',
				description: draft
					? 'The assistant can draw on it again'
					: "The assistant won't cite it back to you",
				icon: draft ? 'ri:book-open-line' : 'ri:quill-pen-line',
				dividerBefore: true,
				action: () => setRole(row.url, draft ? 'library' : 'manuscript')
			} as (typeof items)[number]);
		}
		if (memberItems.length > 1) {
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
		// Somewhere else too: a chat moves (it lives in one project), anything
		// else is filed there as well. The shared menu's own "Remove from" is
		// left out; this page says it below, as its own verb.
		const elsewhere = getProjectMenuItems(row.url).map((i) => ({
			...i,
			submenu: i.submenu?.filter((s) => s.id !== 'remove-from-project')
		}));
		items.push(...(elsewhere as (typeof items)[number][]));
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

	async function setRole(url: string, role: ProjectItemRole) {
		const id = projectId;
		if (!id) return;
		try {
			await projectStore.setItemRole(id, url, role);
		} catch (e) {
			console.error('[ProjectDetailView] role change failed:', e);
			toast.error("Your server couldn't change that", { description: 'Nothing changed. Try again' });
		}
	}

	async function retryExtraction(url: string) {
		const fileId = url.split('/')[2];
		if (!fileId) return;
		try {
			await reextractDriveFile(fileId);
			statusOverride = { ...statusOverride, [url]: 'pending' };
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
		// A chat, page or pin dragged from the sidebar files itself.
		const ref = droppedRefUrl(e);
		if (ref) {
			if (detail) await fileIntoProject(detail, ref);
			await loadGraph();
			return;
		}
		const dropped = e.dataTransfer?.files;
		if (!id || archived || !dropped || dropped.length === 0) return;
		const files = [...dropped];
		// An upload can take a while and the row only appears at the end, so
		// the drop says it was taken before it says it is done.
		const pending = toast.loading(
			files.length === 1 ? `Adding ${files[0].name}…` : `Adding ${files.length} files…`
		);
		let failed = 0;
		for (const f of files) {
			try {
				const uploaded = await uploadDriveFile('uploads', f);
				await addProjectItem(id, `/drive/${uploaded.id}`);
			} catch (err) {
				failed += 1;
				console.error('[ProjectDetailView] drop-add failed:', err);
			}
		}
		toast.dismiss(pending);
		if (failed > 0) {
			toast.error(
				failed === 1 && files.length === 1
					? `Your server couldn't add ${files[0].name}`
					: `Your server couldn't add ${failed} of ${files.length} files`,
				{ description: 'Try dropping them again' }
			);
		}
		await Promise.all([load(true), projectStore.load()]);
		await loadGraph();
	}

	// ---- Membership ----------------------------------------------------------
	let pickerPos = $state<{ x: number; y: number } | null>(null);
	function openPicker(e: MouseEvent) {
		pickerPos = { x: e.clientX, y: e.clientY };
	}
	async function addMember(entity: { url: string }) {
		pickerPos = null;
		// Only what can be filed: not a project, not Setup or the interview.
		// The picker hides them, and the server refuses any that get through.
		const url = projectMemberUrl(entity.url);
		if (!url || !detail) return;
		// The shared verb: it says what happened, and why when it fails.
		await fileIntoProject(detail, url);
		await loadGraph();
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
				dropActive = !archived;
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
			<ProjectHeader project={detail} />

			<!-- The way into a new chat about this project, shaped like the
			     composer it leads to so it reads as a place to type, not a
			     caption. Enter starts the chat with the question already asked. -->
			<form class="ask" onsubmit={submitAsk}>
				<input
					class="ask-input"
					bind:value={askDraft}
					placeholder={`Ask about ${detail.name}…`}
					aria-label={`Ask about ${detail.name}`}
				/>
				<!-- The send stays INVISIBLE until there is something to send.
				     The slot holds the box and does the hiding, so the input
				     never reflows on the first keystroke. -->
				<span class="ask-send" class:idle={!askDraft.trim()}>
					<IconButton
						icon="ri:arrow-up-line"
						label="Ask, grounded in this project"
						size="sm"
						type="submit"
						disabled={!askDraft.trim()}
					/>
				</span>
			</form>

			<ProjectChats project={detail} chats={roomChats} onremove={removeMember} />

			<!-- Files and pages: the material. The grid, with its filters,
			     statuses and your order. -->
			<ProjectSection
				title="Files and pages"
				count={memberRows.length}
				empty={memberRows.length === 0 && archived ? 'Nothing was filed here.' : null}
			>
				{#if memberRows.length === 0}
					{#if !archived}
						<button class="add-row" onclick={openPicker}>
							<Icon icon="ri:add-line" width="15" /> Add pages, people, places, or links, or drop files here
						</button>
					{/if}
				{:else}
					<UniversalDataGrid
						items={memberRows}
						{columns}
						entityType="project-item"
						emptyIcon="ri:filter-line"
						emptyMessage="Nothing here matches that filter"
						searchPlaceholder="Search files and pages…"
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
							{#if !archived}
								<IconButton
									icon="ri:add-line"
									label="Add a page, person, place, file, or link"
									variant="secondary"
									onclick={openPicker}
								/>
							{/if}
						{/snippet}

						{#snippet tableRow(row: MemberRow)}
							<!-- No icon here: it is in the grid's leading column, where it
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
											Failed, retry
										</TextAction>
									{:else if row.role === 'manuscript'}
										<span class="draft-tag">{row.status}</span>
									{:else}
										{row.status}
									{/if}
								</td>
							{/if}
							<td class="c-dim hide-mobile">{row.added}</td>
						{/snippet}

						{#snippet card(row: MemberRow)}
							<div class="project-card">
								<span class="project-card-top">
									<Icon icon={row.icon} width="15" />
								</span>
								<span class="project-card-name">{row.name}</span>
								{#if row.status !== '—'}
									<span class="project-card-meta">{row.status}</span>
								{/if}
							</div>
						{/snippet}
					</UniversalDataGrid>
				{/if}
			</ProjectSection>
		</div>
	{:else}
		<div class="state">Project not found.</div>
	{/if}
</div>

{#if pickerPos}
	<RefPicker
		mode="single"
		position={pickerPos}
		placeholder="Add a chat, page, person, or link…"
		excludeIds={memberItems.map((i) => i.url)}
		filter={(e) => !!projectMemberUrl(e.url)}
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

	/* The ask: shaped like the composer it opens, so it reads as a place to
	   type. Quiet at rest (a hairline and the page's own surface), fully
	   there when focused. */
	.ask {
		display: flex;
		align-items: center;
		gap: 8px;
		min-height: 44px;
		padding: 4px 6px 4px 16px;
		border: 1px solid var(--color-border);
		border-radius: 12px;
		background: var(--color-surface);
		transition: border-color 120ms ease;
	}
	.ask:focus-within {
		border-color: var(--color-foreground-subtle);
	}
	.ask-input {
		flex: 1;
		min-width: 0;
		border: none;
		background: transparent;
		outline: none;
		font: inherit;
		font-size: 0.9375rem;
		color: var(--color-foreground);
	}
	.ask-input::placeholder {
		color: var(--color-foreground-subtle);
	}

	/* Holds its box so the input never reflows; `visibility` rather than
	   `display` for the same reason, and it also takes the control out of the
	   tab order while there is nothing to send. */
	.ask-send {
		display: inline-flex;
		transition: opacity 120ms ease;
	}
	.ask-send.idle {
		opacity: 0;
		visibility: hidden;
	}

	@media (prefers-reduced-motion: reduce) {
		.ask-send {
			transition: none;
		}
	}

	.draft-tag {
		color: var(--color-foreground);
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

	/* The phone's margins: the header wraps itself (ProjectHeader). */
	@media (max-width: 768px) {
		.inner {
			padding: 1.25rem 1rem 4rem;
			gap: 1.25rem;
		}
	}

	/* Card view */
	.project-card {
		display: flex; flex-direction: column; gap: 0.4rem;
		width: 100%; height: 100%; padding: 0.85rem 0.9rem;
		border: 1px solid var(--color-border); border-radius: 10px;
		background: var(--color-surface);
		transition: background-color 0.12s ease, border-color 0.12s ease;
	}
	:global(.card:hover) .project-card {
		background: var(--color-background-hover);
		border-color: color-mix(in srgb, var(--color-primary) 32%, var(--color-border));
	}
	.project-card-top { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
	.project-card-top :global(svg) { color: var(--color-foreground-subtle); flex-shrink: 0; }
	.project-card-name {
		font-size: 0.875rem; font-weight: 550; line-height: 1.35; color: var(--color-foreground);
		display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2;
		-webkit-box-orient: vertical; overflow: hidden;
	}
	.project-card-meta { font-size: 10px; letter-spacing: 0.03em; color: var(--color-foreground-subtle); }

	:global(.spin) { animation: spin 0.8s linear infinite; }
	@keyframes spin { to { transform: rotate(360deg); } }
</style>
