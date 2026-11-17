<script lang="ts">
	/**
	 * A thread's page. The main column is the thread itself: its name and
	 * brief, where it stands, its tasks, the way into a chat about it, and the
	 * thread as a line of what happened in it, down to today and on to what is
	 * planned. The side column holds its properties and its materials, the
	 * chats, pages and files added to it, which take a drop.
	 */
	import type { Tab } from '$lib/tabs/types';
	import type { ProjectChat, ProjectDetail, ProjectGraphNode, ProjectItemRole, Task } from '$lib/api/client';
	import Icon from '$lib/components/Icon.svelte';
	import ProjectHeader from '$lib/components/projects/ProjectHeader.svelte';
	import TaskList from '$lib/components/tasks/TaskList.svelte';
	import RefPicker from '$lib/components/RefPicker.svelte';
	import { THREAD_ICON } from '$lib/utils/iconHelpers';
	import { Button, IconButton, TextAction } from '$lib';
	import { projectStore } from '$lib/stores/project.svelte';
	import { chatSessions } from '$lib/stores/chatSessions.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { contextMenu } from '$lib/stores/contextMenu.svelte';
	import type { ContextMenuItem } from '$lib/stores/contextMenu.svelte';
	import { toast } from 'svelte-sonner';
	import { uploadDriveFile, addProjectItem, reextractDriveFile, getProjectGraph } from '$lib/api/client';
	import { askVirtues } from '$lib/stores/pendingPrompt.svelte';
	import { dropZone, hasFiles } from '$lib/actions/dropZone';
	import {
		droppedRefUrl,
		endProject,
		fileIntoProject,
		memberIcon,
		memberName,
		outcomeLabel,
		projectMemberUrl,
		REF_DRAG_TYPE,
		removeFromProject,
		reopenProject,
	} from '$lib/utils/projectActions';
	import { getProjectMenuItems } from '$lib/utils/contextMenuItems';
	import { openWebLink } from '$lib/utils/webLinks';

	let { tab, active = false }: { tab: Tab; active?: boolean } = $props();

	const threadId = $derived.by(() => {
		const m = tab.route.match(/^\/thread\/([^/?#]+)/);
		return m?.[1] ?? null;
	});

	/**
	 * A live view of the store's copy, not a snapshot of it. A rename, recolor
	 * or ending from the sidebar, ⌘K or another pane lands in the store, and
	 * this page and its tab follow without being told.
	 */
	const detail = $derived<ProjectDetail | null>(
		threadId ? (projectStore.getCached(threadId) ?? null) : null
	);
	let loading = $state(false);
	let error = $state<string | null>(null);

	async function load(force = false) {
		const id = threadId;
		if (!id) return;
		loading = true;
		error = null;
		try {
			await projectStore.get(id, { force });
		} catch (e) {
			console.error('[ThreadView] Failed to load thread:', e);
			error = "Your server couldn't open this thread. If you deleted it, you can restore it from Recently deleted.";
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		if (threadId) load();
	});

	// The tab wears the thread's name and mark, whichever door opened it:
	// a pin, a citation, ⌘K and a reload all arrive with only the id.
	$effect(() => {
		if (!detail) return;
		const icon = detail.icon || THREAD_ICON;
		if (tab.label !== detail.name || tab.icon !== icon) {
			windowShellStore.updateTab(tab.id, { label: detail.name, icon });
		}
	});

	const archived = $derived(!!detail?.archived_at);

	// Who it is with: the people its members name or link, from the thread's
	// graph. Nothing is inferred from free text.
	let people = $state<ProjectGraphNode[]>([]);
	async function loadPeople() {
		const id = threadId;
		if (!id) return;
		try {
			const graph = await getProjectGraph(id);
			people = graph.nodes.filter((n) => n.entity_type === 'person');
		} catch (e) {
			// An aid, not the content: the page stands without it.
			console.error('[ThreadView] Failed to load who it is with:', e);
			people = [];
		}
	}
	$effect(() => {
		void detail?.items.length;
		if (threadId) loadPeople();
	});

	// ── where it stands ────────────────────────────────────────────────────
	let standsDraft = $state('');
	let standsEditing = $state(false);
	$effect(() => {
		if (!standsEditing) standsDraft = detail?.current_status ?? '';
	});

	async function saveStands() {
		standsEditing = false;
		const id = threadId;
		const next = standsDraft.trim();
		if (!id || next === (detail?.current_status ?? '')) return;
		try {
			await projectStore.update(id, { current_status: next || null });
		} catch (e) {
			console.error('[ThreadView] saving where it stands failed:', e);
			toast.error("Your server couldn't save that", { description: 'Nothing changed. Try again' });
		}
	}

	async function setProps(patch: { is_backlog?: boolean; is_adding_automatic?: boolean }) {
		const id = threadId;
		if (!id) return;
		try {
			await projectStore.update(id, patch);
		} catch (e) {
			console.error('[ThreadView] update failed:', e);
			toast.error("Your server couldn't change that", { description: 'Nothing changed. Try again' });
		}
	}

	// ── materials ──────────────────────────────────────────────────────────
	// Chats come from the thread's own detail: every chat whose `thread_id` is
	// this one. The session list holds only the box's most recent chats, so it
	// is only the fallback for a box older than `detail.chats`.
	const chats = $derived.by<ProjectChat[]>(() => {
		if (detail?.chats) return detail.chats;
		return chatSessions.sessions
			.filter((s) => s.thread_id === threadId)
			.map((s) => ({
				id: s.conversation_id,
				title: s.title ?? '',
				icon: s.icon,
				message_count: s.message_count,
				last_message_at: s.last_message_at || s.first_message_at,
			}));
	});

	const memberItems = $derived((detail?.items ?? []).filter((i) => !i.url.startsWith('/chat/')));
	let statusOverride = $state<Record<string, string>>({});

	/** A file's text extraction, in the owner's words, when it needs saying. */
	function fileNote(url: string, role?: string, stored?: string): string | null {
		if (role === 'manuscript') return 'Your draft';
		const s = statusOverride[url] ?? stored;
		if (s === 'pending' || s === 'extracting') return 'Reading…';
		if (s === 'no_text') return 'No text layer';
		if (s === 'failed') return 'failed';
		return null;
	}

	/** Open a member beside the thread, so the thread stays in reach. */
	function openUrl(url: string) {
		if (url.startsWith('http://') || url.startsWith('https://')) {
			void openWebLink(url);
			return;
		}
		windowShellStore.openRouteBeside(url);
	}

	async function removeMember(url: string) {
		if (!detail) return;
		await removeFromProject(detail, url);
	}

	async function moveTo(url: string, edge: 'top' | 'bottom') {
		const id = threadId;
		if (!id || !detail) return;
		const rest = detail.items.map((i) => i.url).filter((u) => u !== url);
		const next = edge === 'top' ? [url, ...rest] : [...rest, url];
		try {
			await projectStore.reorderItems(id, next);
		} catch (e) {
			console.error('[ThreadView] reorder failed:', e);
			toast.error("Your server couldn't move that item", { description: 'Try again' });
		}
	}

	async function setRole(url: string, role: ProjectItemRole) {
		const id = threadId;
		if (!id) return;
		try {
			await projectStore.setItemRole(id, url, role);
		} catch (e) {
			console.error('[ThreadView] role change failed:', e);
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
			/* the note stays; the next open refreshes it */
		}
	}

	function memberMenu(url: string, role: string | undefined, e: MouseEvent) {
		e.preventDefault();
		const items: ContextMenuItem[] = [
			{ id: 'open', label: 'Open', icon: 'mynaui:external-link', action: () => openUrl(url) },
		];
		if (url.startsWith('/page/') && !archived) {
			const draft = role === 'manuscript';
			items.push({
				id: 'draft',
				label: draft ? 'Use as a source' : 'Mark as your draft',
				description: draft ? 'The assistant can draw on it again' : "The assistant won't cite it back to you",
				icon: draft ? 'mynaui:book-open' : 'mynaui:pen',
				dividerBefore: true,
				action: () => setRole(url, draft ? 'library' : 'manuscript'),
			});
		}
		if (!url.startsWith('/chat/') && memberItems.length > 1) {
			items.push(
				{ id: 'top', label: 'Move to top', icon: 'mynaui:chevrons-up', dividerBefore: true, action: () => moveTo(url, 'top') },
				{ id: 'bottom', label: 'Move to bottom', icon: 'mynaui:chevrons-down', action: () => moveTo(url, 'bottom') },
			);
		}
		const elsewhere = getProjectMenuItems(url).map((i) => ({
			...i,
			submenu: i.submenu?.filter((s) => s.id !== 'remove-from-project'),
		}));
		items.push(...elsewhere);
		items.push({
			id: 'remove',
			label: 'Remove from thread',
			icon: 'mynaui:x',
			dividerBefore: true,
			variant: 'destructive',
			action: () => removeMember(url),
		});
		contextMenu.show({ x: e.clientX, y: e.clientY }, items);
	}

	// ── ask about it ───────────────────────────────────────────────────────
	let askDraft = $state('');
	function submitAsk(e: Event) {
		e.preventDefault();
		const text = askDraft.trim();
		const id = threadId;
		if (!text || !id) return;
		askVirtues(text, id);
		askDraft = '';
	}

	// ── dropping into Materials: files upload, rows file themselves ───────
	let dropActive = $state(false);
	function acceptsDrop(data: DataTransfer): boolean {
		return !archived && (hasFiles(data) || data.types.includes(REF_DRAG_TYPE));
	}
	async function handleDrop(e: DragEvent) {
		const id = threadId;
		const ref = droppedRefUrl(e);
		if (ref) {
			if (detail) await fileIntoProject(detail, ref);
			return;
		}
		const dropped = e.dataTransfer?.files;
		if (!id || !dropped || dropped.length === 0) return;
		const files = [...dropped];
		// An upload can take a while and the row only appears at the end, so
		// the drop says it was taken before it says it is done.
		const pending = toast.loading(files.length === 1 ? `Adding ${files[0].name}…` : `Adding ${files.length} files…`);
		let failed = 0;
		for (const f of files) {
			try {
				const uploaded = await uploadDriveFile('uploads', f);
				await addProjectItem(id, `/drive/${uploaded.id}`);
			} catch (err) {
				failed += 1;
				console.error('[ThreadView] drop-add failed:', err);
			}
		}
		toast.dismiss(pending);
		if (failed > 0) {
			toast.error(
				failed === 1 && files.length === 1
					? `Your server couldn't add ${files[0].name}`
					: `Your server couldn't add ${failed} of ${files.length} files`,
				{ description: 'Try dropping them again' },
			);
		}
		await Promise.all([load(true), projectStore.load()]);
	}

	let pickerPos = $state<{ x: number; y: number } | null>(null);
	function openPicker(e: MouseEvent) {
		pickerPos = { x: e.clientX, y: e.clientY };
	}
	async function addMember(entity: { url: string }) {
		pickerPos = null;
		// Only what can be filed: not a thread, not Setup or the interview.
		const url = projectMemberUrl(entity.url);
		if (!url || !detail) return;
		await fileIntoProject(detail, url);
	}

	// ── the thread as a line ───────────────────────────────────────────────
	interface LineEntry {
		key: string;
		date: string | null;
		label: string;
		icon: string;
		kind: 'record' | 'task';
		open: () => void;
	}

	function localDay(d: Date): string {
		const m = String(d.getMonth() + 1).padStart(2, '0');
		const day = String(d.getDate()).padStart(2, '0');
		return `${d.getFullYear()}-${m}-${day}`;
	}
	const today = localDay(new Date());

	function openTask(t: Task) {
		windowShellStore.openRouteBeside(`/task/${t.id}`, t.title);
	}

	const line = $derived.by(() => {
		const records: LineEntry[] = [
			...memberItems.map((i) => ({
				key: i.url,
				date: localDay(new Date(i.occurred_at ?? i.added_at)),
				label: memberName(i),
				icon: memberIcon(i.url),
				kind: 'record' as const,
				open: () => openUrl(i.url),
			})),
			...chats.map((c) => ({
				key: `/chat/${c.id}`,
				date: localDay(new Date(c.last_message_at)),
				label: c.title || 'Untitled chat',
				icon: c.icon || 'mynaui:message',
				kind: 'record' as const,
				open: () => openUrl(`/chat/${c.id}`),
			})),
		];
		const tasks = [...(detail?.tasks ?? []), ...(detail?.tasks_also_about ?? [])];
		const dated: LineEntry[] = tasks
			.filter((t) => t.due_on)
			.map((t) => ({
				key: t.id,
				date: t.due_on,
				label: t.title,
				icon: 'mynaui:circle',
				kind: 'task' as const,
				open: () => openTask(t),
			}));
		const all = [...records, ...dated].sort((a, b) => (a.date ?? '').localeCompare(b.date ?? ''));
		return {
			past: all.filter((e) => (e.date ?? '') <= today),
			ahead: all.filter((e) => (e.date ?? '') > today),
			undated: tasks
				.filter((t) => !t.due_on)
				.map((t) => ({
					key: t.id,
					date: null,
					label: t.title,
					icon: 'mynaui:circle',
					kind: 'task' as const,
					open: () => openTask(t),
				})),
		};
	});

	function shortDay(iso: string | null): string {
		if (!iso) return '';
		const d = new Date(`${iso}T12:00:00`);
		const sameYear = d.getFullYear() === new Date().getFullYear();
		return d.toLocaleDateString('en-US', { month: 'short', day: 'numeric', year: sameYear ? undefined : 'numeric' });
	}

	function longDay(iso: string): string {
		return shortDay(localDay(new Date(iso)));
	}
</script>

<div class="thread-page">
	{#if loading && !detail}
		<div class="state"><Icon icon="mynaui:spinner" width="18" class="spin" /> Loading…</div>
	{:else if error}
		<div class="state">{error}</div>
	{:else if detail}
		<div class="frame">
			<main class="main">
				<ProjectHeader project={detail} />

				<section class="block" aria-labelledby="stands-h">
					<div class="block-head">
						<h2 id="stands-h" class="block-title">Where it stands</h2>
						{#if !standsEditing && !archived}
							<TextAction onclick={() => (standsEditing = true)}>Edit</TextAction>
						{/if}
					</div>
					{#if standsEditing}
						<!-- svelte-ignore a11y_autofocus -->
						<textarea
							class="stands-input"
							bind:value={standsDraft}
							rows="2"
							autofocus
							placeholder="One sentence on where this is now."
							aria-label="Where it stands"
							onblur={saveStands}
							onkeydown={(e) => {
								if (e.key === 'Enter' && !e.shiftKey) {
									e.preventDefault();
									(e.currentTarget as HTMLTextAreaElement).blur();
								} else if (e.key === 'Escape') {
									standsDraft = detail?.current_status ?? '';
									standsEditing = false;
								}
							}}
						></textarea>
					{:else if detail.current_status}
						<p class="stands">{detail.current_status}</p>
					{:else}
						<p class="quiet">Nothing written yet. Edit to say where this is now, in one sentence.</p>
					{/if}
				</section>

				<section class="block" aria-labelledby="tasks-h">
					<h2 id="tasks-h" class="block-title">Tasks</h2>
					<TaskList {active} route={tab.route} projectId={detail.id} readonly={archived} emptyText={null} />
					{#if (detail.tasks_also_about ?? []).length > 0}
						<h3 class="sub">Also about this thread</h3>
						<ul class="also">
							{#each detail.tasks_also_about ?? [] as t (t.id)}
								<li>
									<button class="also-row" onclick={() => openTask(t)}>
										<span class="ring" aria-hidden="true"></span>
										<span class="also-title">{t.title}</span>
										{#if t.due_on}<span class="due">{shortDay(t.due_on)}</span>{/if}
									</button>
								</li>
							{/each}
						</ul>
					{/if}
				</section>

				<!-- The way into a new chat about this thread, shaped like the
				     composer it leads to so it reads as a place to type. -->
				<form class="ask" onsubmit={submitAsk}>
					<input
						class="ask-input"
						bind:value={askDraft}
						placeholder={`Ask about ${detail.name}…`}
						aria-label={`Ask about ${detail.name}`}
					/>
					<span class="ask-send" class:idle={!askDraft.trim()}>
						<IconButton
							icon="mynaui:arrow-up"
							label="Ask, grounded in this thread"
							size="sm"
							type="submit"
							disabled={!askDraft.trim()}
						/>
					</span>
				</form>

				<section class="block" aria-labelledby="line-h">
					<h2 id="line-h" class="block-title">The thread</h2>
					{#if line.past.length + line.ahead.length + line.undated.length === 0}
						<p class="quiet">What you add to this thread, and its dated tasks, line up here by day.</p>
					{:else}
						<ol class="line">
							{#each line.past as e (e.key)}
								<li class="entry">
									<span class="date">{shortDay(e.date)}</span>
									<span class="node" class:task={e.kind === 'task'}></span>
									<button class="entry-label" onclick={e.open}>
										<Icon icon={e.icon} width="14" />
										<span>{e.label}</span>
									</button>
								</li>
							{/each}
							<li class="entry today" aria-label="Today">
								<span class="date">Today</span>
								<span class="node today-node"></span>
								<span></span>
							</li>
							{#each line.ahead as e (e.key)}
								<li class="entry ahead">
									<span class="date">{shortDay(e.date)}</span>
									<span class="node" class:task={e.kind === 'task'} class:planned={e.kind === 'record'}></span>
									<button class="entry-label" onclick={e.open}>
										{#if e.kind === 'record'}<Icon icon={e.icon} width="14" />{/if}
										<span>{e.label}</span>
									</button>
								</li>
							{/each}
							{#each line.undated as e (e.key)}
								<li class="entry ahead">
									<span class="date">No date</span>
									<span class="node task"></span>
									<button class="entry-label" onclick={e.open}>
										<span>{e.label}</span>
									</button>
								</li>
							{/each}
						</ol>
					{/if}
				</section>
			</main>

			<aside class="side" aria-label="About this thread">
				<dl class="props">
					<dt>Status</dt>
					<dd>
						{#if archived}
							{outcomeLabel(detail)} {longDay(detail.archived_at ?? detail.updated_at)}
						{:else}
							<span class="seg" role="group" aria-label="Status">
								<button class="vt" aria-pressed={!detail.is_backlog} onclick={() => setProps({ is_backlog: false })}>Active</button>
								<button class="vt" aria-pressed={!!detail.is_backlog} onclick={() => setProps({ is_backlog: true })}>Backlog</button>
							</span>
						{/if}
					</dd>
					<dt>Started</dt>
					<dd>{longDay(detail.created_at)}</dd>
					{#if detail.thing_title}
						<dt>Under</dt>
						<dd>{detail.thing_title}</dd>
					{/if}
					{#if people.length > 0}
						<dt>With</dt>
						<dd class="with">
							{#each people.slice(0, 6) as person (person.url)}
								<button class="person" onclick={() => openUrl(person.url)}>{person.name}</button>
							{/each}
						</dd>
					{/if}
					<dt>Adding</dt>
					<dd>
						<span class="seg" role="group" aria-label="Adding">
							<button
								class="vt"
								aria-pressed={!detail.is_adding_automatic}
								disabled={archived}
								onclick={() => setProps({ is_adding_automatic: false })}>Manual</button
							>
							<button
								class="vt"
								aria-pressed={!!detail.is_adding_automatic}
								disabled={archived}
								onclick={() => setProps({ is_adding_automatic: true })}>Automatic</button
							>
						</span>
						<span class="hint">
							{detail.is_adding_automatic
								? 'Your server also files what it is sure belongs here.'
								: 'Only what you add is filed here.'}
						</span>
					</dd>
				</dl>

				<section
					class="materials"
					class:drop-active={dropActive}
					aria-labelledby="materials-h"
					use:dropZone={{
						accepts: acceptsDrop,
						onActive: (on) => (dropActive = on),
						onDrop: handleDrop,
						// A sidebar row allows only a link, and a drop whose effect
						// it does not allow never fires.
						effect: (data) => (data.types.includes(REF_DRAG_TYPE) ? 'link' : 'copy'),
					}}
				>
					<div class="block-head">
						<h2 id="materials-h" class="side-title">Materials</h2>
						{#if !archived}
							<IconButton icon="mynaui:plus" label="Add a chat, page, person, file, or link" size="sm" onclick={openPicker} />
						{/if}
					</div>
					{#if chats.length === 0 && memberItems.length === 0}
						<p class="hint">
							{archived ? 'Nothing was added here.' : 'Drop chats, pages and files here, or add them with +.'}
						</p>
					{:else}
						<ul class="members">
							{#each chats as c (c.id)}
								<li>
									<button
										class="member"
										onclick={() => openUrl(`/chat/${c.id}`)}
										oncontextmenu={(e) => memberMenu(`/chat/${c.id}`, undefined, e)}
									>
										<Icon icon={c.icon || 'mynaui:message'} width="14" />
										<span class="member-name">{c.title || 'Untitled chat'}</span>
									</button>
								</li>
							{/each}
							{#each memberItems as it (it.url)}
								{@const note = fileNote(it.url, it.role, it.status)}
								<li>
									<button
										class="member"
										onclick={() => openUrl(it.url)}
										oncontextmenu={(e) => memberMenu(it.url, it.role, e)}
									>
										<Icon icon={memberIcon(it.url)} width="14" />
										<span class="member-name">{memberName(it)}</span>
									</button>
									{#if note === 'failed'}
										<TextAction inline onclick={() => retryExtraction(it.url)}>Reading failed, retry</TextAction>
									{:else if note}
										<span class="member-note">{note}</span>
									{/if}
								</li>
							{/each}
						</ul>
					{/if}
				</section>

				<div class="ends">
					{#if archived}
						<Button variant="secondary" size="sm" onclick={() => reopenProject(detail)}>Reopen</Button>
					{:else}
						<Button variant="secondary" size="sm" onclick={() => endProject(detail, 'finished')}>Finish the thread</Button>
						<TextAction onclick={() => endProject(detail, 'dropped')}>Drop it</TextAction>
					{/if}
				</div>
			</aside>
		</div>
	{:else}
		<div class="state">Thread not found.</div>
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
	.thread-page {
		width: 100%;
		height: 100%;
		overflow-y: auto;
	}
	.frame {
		display: grid;
		grid-template-columns: minmax(0, 1fr) 280px;
		gap: 48px;
		max-width: var(--page-detail);
		margin: 0 auto;
		padding: 48px 32px 96px;
	}
	.main {
		display: flex;
		flex-direction: column;
		gap: 32px;
		min-width: 0;
	}
	.state {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 48px 32px;
		color: var(--color-foreground-muted);
	}

	.block-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
		min-height: 28px;
	}
	.block-title {
		margin: 0 0 8px;
		font-family: var(--font-serif);
		font-weight: 400;
		font-size: var(--md-h2-size);
		line-height: 28px;
		color: var(--color-foreground);
	}
	.block-head .block-title {
		margin: 0;
	}
	.sub {
		margin: 16px 0 4px;
		font-size: 13px;
		font-weight: 400;
		color: var(--color-foreground-muted);
	}
	.stands {
		margin: 8px 0 0;
		font-family: var(--font-serif);
		font-size: 18px;
		line-height: 28px;
		color: var(--color-foreground);
	}
	.stands-input {
		width: 100%;
		margin-top: 8px;
		padding: 8px 12px;
		box-sizing: border-box;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		background: var(--color-surface);
		font-family: var(--font-serif);
		font-size: 18px;
		line-height: 28px;
		color: var(--color-foreground);
		resize: vertical;
	}
	.quiet {
		margin: 8px 0 0;
		font-size: 15px;
		line-height: 24px;
		color: var(--color-foreground-muted);
	}

	.also {
		list-style: none;
		margin: 0;
		padding: 0;
	}
	.also-row {
		display: flex;
		align-items: center;
		gap: 12px;
		width: 100%;
		min-height: 40px;
		padding: 0 12px;
		margin: 0 -12px;
		border: 0;
		border-radius: var(--radius-md);
		background: transparent;
		font: inherit;
		font-size: 14px;
		color: var(--color-foreground);
		text-align: left;
		cursor: pointer;
		transition: background-color var(--motion-hover-out) var(--ease-premium);
	}
	.also-row:hover {
		background: var(--hover-bg);
		transition-duration: var(--motion-hover-in);
	}
	.ring {
		flex: none;
		width: 14px;
		height: 14px;
		border-radius: 50%;
		border: 1px solid var(--color-foreground-muted);
		box-sizing: border-box;
	}
	.also-title {
		flex: 1 1 auto;
		min-width: 0;
	}
	.due {
		flex: none;
		font-size: 13px;
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
	}

	/* The ask: shaped like the composer it opens. */
	.ask {
		display: flex;
		align-items: center;
		gap: 8px;
		min-height: 44px;
		padding: 4px 8px 4px 16px;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-xl);
		background: var(--color-surface);
		transition: border-color var(--motion-state) var(--ease-premium);
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
		font-size: 15px;
		color: var(--color-foreground);
	}
	.ask-input::placeholder {
		color: var(--color-foreground-subtle);
	}
	.ask-send {
		display: inline-flex;
		transition: opacity var(--motion-state) var(--ease-premium);
	}
	.ask-send.idle {
		opacity: 0;
		visibility: hidden;
	}

	/* The thread as a line: dates on the left, a node per entry, solid down
	   to today and dashed after it. */
	.line {
		list-style: none;
		margin: 8px 0 0;
		padding: 0;
		position: relative;
	}
	.entry {
		display: grid;
		grid-template-columns: 72px 16px minmax(0, 1fr);
		align-items: center;
		gap: 12px;
		min-height: 36px;
		position: relative;
	}
	.entry::before {
		content: '';
		position: absolute;
		left: calc(72px + 12px + 8px);
		top: 0;
		bottom: 0;
		width: 0;
		border-left: 1px solid var(--color-border);
	}
	.entry.ahead::before {
		border-left-style: dashed;
	}
	.entry:first-child::before {
		top: 50%;
	}
	.entry:last-child::before {
		bottom: 50%;
	}
	.date {
		font-size: 13px;
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
		text-align: right;
	}
	.node {
		justify-self: center;
		width: 8px;
		height: 8px;
		border-radius: 50%;
		background: var(--color-foreground);
		position: relative;
		z-index: 1;
	}
	.node.task {
		background: var(--color-page);
		border: 1px solid var(--color-foreground-muted);
		box-sizing: border-box;
		width: 10px;
		height: 10px;
	}
	/* A record still ahead, such as a planned calendar event: solid edge,
	   empty middle, so it reads as booked but not yet spent. */
	.node.planned {
		background: var(--color-page);
		border: 2px solid var(--color-foreground);
		box-sizing: border-box;
	}
	.today-node {
		width: 12px;
		height: 12px;
		background: var(--color-page);
		border: 1px dotted var(--color-primary);
		box-sizing: border-box;
	}
	.entry.today .date {
		color: var(--color-primary);
	}
	.entry-label {
		display: flex;
		align-items: center;
		gap: 8px;
		min-width: 0;
		padding: 4px 8px;
		margin-left: -8px;
		border: 0;
		border-radius: var(--radius-md);
		background: transparent;
		font: inherit;
		font-size: 14px;
		color: var(--color-foreground);
		text-align: left;
		cursor: pointer;
		transition: background-color var(--motion-hover-out) var(--ease-premium);
	}
	.entry-label span {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.entry-label :global(svg) {
		flex: none;
		color: var(--color-foreground-muted);
	}
	button.entry-label:hover {
		background: var(--hover-bg);
		transition-duration: var(--motion-hover-in);
	}

	/* The side column. */
	.side {
		display: flex;
		flex-direction: column;
		gap: 32px;
		padding-top: 8px;
	}
	.props {
		display: grid;
		grid-template-columns: 64px minmax(0, 1fr);
		gap: 12px;
		margin: 0;
		align-items: baseline;
	}
	.props dt {
		font-size: 13px;
		line-height: 28px;
		color: var(--color-foreground-muted);
	}
	.props dd {
		margin: 0;
		font-size: 14px;
		line-height: 28px;
		color: var(--color-foreground);
		min-width: 0;
	}
	.seg {
		display: inline-flex;
		gap: 4px;
	}
	.with {
		display: flex;
		flex-wrap: wrap;
		gap: 4px 12px;
	}
	.person {
		padding: 0;
		border: 0;
		background: none;
		font: inherit;
		color: var(--color-primary);
		cursor: pointer;
	}
	.vt {
		height: 28px;
		padding: 0 12px;
		border: 0;
		border-radius: var(--radius-md);
		background: transparent;
		font: inherit;
		font-size: 13px;
		color: var(--color-foreground-muted);
		cursor: pointer;
		transition: background-color var(--motion-hover-out) var(--ease-premium);
	}
	.vt:hover:not(:disabled) {
		background: var(--hover-bg);
		transition-duration: var(--motion-hover-in);
	}
	.vt[aria-pressed='true'] {
		background: var(--active-bg);
		color: var(--color-foreground);
	}
	.vt:disabled {
		cursor: default;
	}
	.hint {
		display: block;
		margin-top: 4px;
		font-size: 13px;
		line-height: 20px;
		color: var(--color-foreground-muted);
	}
	.side-title {
		margin: 0;
		font-family: var(--font-serif-ui);
		font-weight: 400;
		font-size: 18px;
		line-height: 28px;
		color: var(--color-foreground);
	}
	.materials {
		padding: 12px;
		margin: -12px;
		border-radius: var(--radius-xl);
		transition: background-color var(--motion-hover-out) var(--ease-premium);
	}
	.materials.drop-active {
		background: var(--drop-tint);
		transition-duration: var(--motion-hover-in);
	}
	.members {
		list-style: none;
		margin: 8px 0 0;
		padding: 0;
	}
	.members li {
		display: flex;
		flex-direction: column;
	}
	.member {
		display: flex;
		align-items: center;
		gap: 8px;
		min-height: 32px;
		padding: 0 8px;
		margin: 0 -8px;
		border: 0;
		border-radius: var(--radius-md);
		background: transparent;
		font: inherit;
		font-size: 14px;
		color: var(--color-foreground);
		text-align: left;
		cursor: pointer;
		transition: background-color var(--motion-hover-out) var(--ease-premium);
	}
	.member:hover {
		background: var(--hover-bg);
		transition-duration: var(--motion-hover-in);
	}
	.member :global(svg) {
		flex: none;
		color: var(--color-foreground-muted);
	}
	.member-name {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.member-note {
		margin: 0 0 4px 24px;
		font-size: 12px;
		color: var(--color-foreground-muted);
	}
	.ends {
		display: flex;
		align-items: center;
		gap: 16px;
	}

	@media (max-width: 900px) {
		.frame {
			grid-template-columns: minmax(0, 1fr);
			padding: 24px 16px 64px;
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.ask,
		.ask-send,
		.materials,
		.member,
		.entry-label,
		.also-row,
		.vt {
			transition: none;
		}
	}
</style>
