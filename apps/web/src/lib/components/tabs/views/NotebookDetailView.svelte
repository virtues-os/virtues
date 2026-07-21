<script lang="ts">
	import type { Tab } from '$lib/tabs/types';
	import type { NotebookDetail } from '$lib/api/client';
	import Icon from '$lib/components/Icon.svelte';
	import { notebookStore } from '$lib/stores/notebook.svelte';
	import { chatSessions } from '$lib/stores/chatSessions.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { contextMenu } from '$lib/stores/contextMenu.svelte';
	import RefPicker from '$lib/components/RefPicker.svelte';
	import ColorPickerModal from '$lib/components/sidebar/ColorPickerModal.svelte';
	import { getRefSummary } from '$lib/utils/refSummary';
	import { getPage, getDriveFile, uploadDriveFile, addNotebookItem, reextractDriveFile, listNotebookAnnotations, exportNotebookAnnotations, downloadMarkdown, type NotebookAnnotation } from '$lib/api/client';
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

	// (Re)load whenever the tab points at a different room.
	$effect(() => {
		if (notebookId) load();
	});

	// ---- Highlights across the notebook's documents (D2.5) -------------------
	// Every annotation on the notebook's library files, grouped by file so the
	// notebook reads as one marked-up corpus.
	let highlights = $state<NotebookAnnotation[]>([]);
	async function loadHighlights() {
		const id = notebookId;
		if (!id) {
			highlights = [];
			return;
		}
		try {
			highlights = await listNotebookAnnotations(id);
		} catch (e) {
			console.error('[NotebookDetailView] Failed to load highlights:', e);
			highlights = [];
		}
	}
	$effect(() => {
		if (notebookId) loadHighlights();
	});
	// [{ file_id, filename, items: [...] }] in the query's file/reading order.
	const highlightGroups = $derived.by(() => {
		const groups: { file_id: string; filename: string; items: NotebookAnnotation[] }[] = [];
		for (const h of highlights) {
			let g = groups.at(-1);
			if (!g || g.file_id !== h.file_id) {
				g = { file_id: h.file_id, filename: h.filename, items: [] };
				groups.push(g);
			}
			g.items.push(h);
		}
		return groups;
	});
	const HL_TINT: Record<string, string> = {
		yellow: '#ffd54a',
		green: '#7ee081',
		blue: '#6fb5ff',
		pink: '#ff8fc7',
	};
	/** Download every highlight in this notebook as markdown (D4.3). */
	async function exportHighlights() {
		const id = notebookId;
		if (!id) return;
		try {
			const md = await exportNotebookAnnotations(id);
			downloadMarkdown(`${(detail?.name ?? 'notebook')}-highlights`, md);
		} catch (e) {
			console.error('[NotebookDetailView] export failed:', e);
		}
	}

	function openHighlight(h: NotebookAnnotation) {
		let route = `/drive/${h.file_id}`;
		const params = new URLSearchParams();
		if (h.page_num) params.set('page', String(h.page_num));
		params.set('hl', h.id);
		windowShellStore.openTabFromRoute(`${route}?${params.toString()}`);
	}

	// Chats filed into this room — sourced from the authoritative session list,
	// not from membership rows, so removing a pinned member can't desync a chat.
	const roomChats = $derived(
		chatSessions.sessions.filter((s) => s.notebook_id === notebookId),
	);

	// Members = everything except chats (chats render in their own list).
	const pinnedItems = $derived((detail?.items ?? []).filter((i) => !i.url.startsWith('/chat/')));

	// ---- Resolve real member names (not the type slug) -----------------------
	let memberNames = $state<Record<string, string>>({});
	// Per-item extraction state for drive files (honest indexing chips).
	let memberStatus = $state<Record<string, string>>({});
	const requestedNames = new Set<string>();
	async function resolveMemberName(url: string): Promise<string> {
		if (url.startsWith('http://') || url.startsWith('https://')) {
			try { return new URL(url).hostname.replace(/^www\./, ''); } catch { return url; }
		}
		const parts = url.split('/'); // ['', type, id, ...]
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
		} catch { /* fall through to type label */ }
		return type ? type[0].toUpperCase() + type.slice(1) : url;
	}
	$effect(() => {
		for (const it of pinnedItems) {
			if (!requestedNames.has(it.url)) {
				requestedNames.add(it.url);
				resolveMemberName(it.url).then((n) => {
					memberNames = { ...memberNames, [it.url]: n };
				});
			}
		}
	});

	// ---- Ask this notebook (opens a new chat bound + grounded here) ----------
	let askDraft = $state('');
	function submitAsk(e: Event) {
		e.preventDefault();
		const text = askDraft.trim();
		const id = notebookId;
		if (!text || !id) return;
		askVirtues(text, id);
		askDraft = '';
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

	function memberType(url: string): string {
		if (url.startsWith('http://') || url.startsWith('https://')) return 'Link';
		const t = url.split('/')[1] ?? '';
		const map: Record<string, string> = {
			person: 'Person', page: 'Page', org: 'Org', place: 'Place',
			thing: 'Thing', day: 'Day', year: 'Year', source: 'Source', drive: 'File',
		};
		return map[t] ?? t;
	}

	// Status chip for drive-file members: where the file is in the corpus
	// pipeline. Empty for non-files and non-text files (no chip, no noise).
	function statusChip(url: string): { label: string; retry: boolean } | null {
		const s = memberStatus[url];
		if (!s || s === 'skipped') return null;
		switch (s) {
			case 'done': return { label: 'indexed', retry: false };
			case 'pending': return { label: 'queued', retry: false };
			case 'extracting': return { label: 'extracting…', retry: false };
			case 'no_text': return { label: 'no text layer', retry: false };
			case 'failed': return { label: 'failed — retry', retry: true };
			default: return null;
		}
	}

	async function retryExtraction(url: string, e: Event) {
		e.stopPropagation();
		const fileId = url.split('/')[2];
		if (!fileId) return;
		try {
			await reextractDriveFile(fileId);
			memberStatus = { ...memberStatus, [url]: 'pending' };
		} catch { /* chip stays; next open refreshes */ }
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
			}
		}
		await load(true);
		await loadHighlights();
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
		const id = notebookId;
		if (!id || !detail) return;
		const name = nameDraft.trim();
		if (!name || name === detail.name) return;
		await notebookStore.update(id, { name });
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
		const id = notebookId;
		if (!id) return;
		await notebookStore.update(id, { current_status: memoDraft.trim() || null });
		await load(true);
	}

	// ---- Accent color --------------------------------------------------------
	let colorOpen = $state(false);
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
	}
	async function removeMember(url: string) {
		const id = notebookId;
		if (!id) return;
		await notebookStore.removeItem(id, url);
	}
	function memberMenu(e: MouseEvent, url: string) {
		e.preventDefault();
		contextMenu.show({ x: e.clientX, y: e.clientY }, [
			{ id: 'remove', label: 'Remove from Notebook', icon: 'ri:close-line', action: () => removeMember(url) },
		]);
	}

	// ---- Delete --------------------------------------------------------------
	async function deleteNotebook() {
		const id = notebookId;
		if (!id || !detail) return;
		contextMenu.hide?.();
		if (!confirm(`Delete the Notebook "${detail.name}"? Chats and pages stay; they're just unfiled.`)) return;
		await notebookStore.remove(id);
		windowShellStore.openTabFromRoute('/notebooks');
	}

	const accent = $derived(detail?.accent_color || null);
</script>

<div class="notebook-detail" style={accent ? `--room-accent: ${accent}` : ''}>
	{#if loading && !detail}
		<div class="state"><Icon icon="ri:loader-4-line" width="18" class="spin" /> Loading…</div>
	{:else if error}
		<div class="state error">{error}</div>
	{:else if detail}
		<div class="inner">
			<header class="head">
				<div class="head-top">
					<button class="nb-icon" class:tinted={!!accent} title="Set color" onclick={() => (colorOpen = true)}>
						<Icon icon={detail.icon || 'ri:booklet-line'} width="22" />
					</button>
					<div class="head-actions">
						<button class="icon-btn" title="Rename" onclick={startRename}><Icon icon="ri:edit-line" width="15" /></button>
						<button class="icon-btn danger" title="Delete Notebook" onclick={deleteNotebook}><Icon icon="ri:delete-bin-line" width="15" /></button>
					</div>
				</div>

				{#if editingName}
					<!-- svelte-ignore a11y_autofocus -->
					<input
						class="title-input font-serif"
						bind:value={nameDraft}
						autofocus
						onblur={commitRename}
						onkeydown={(e) => { if (e.key === 'Enter') commitRename(); if (e.key === 'Escape') editingName = false; }}
					/>
				{:else}
					<h1 class="title font-serif" ondblclick={startRename}>{detail.name}</h1>
				{/if}

				{#if editingMemo}
					<!-- svelte-ignore a11y_autofocus -->
					<textarea
						class="desc-input"
						bind:value={memoDraft}
						autofocus
						placeholder="Add a description — what this notebook is for. It also gives the assistant context."
						onblur={commitMemo}
					></textarea>
				{:else if detail.current_status}
					<button class="desc" onclick={startMemo}>{detail.current_status}</button>
				{:else}
					<button class="desc desc-empty" onclick={startMemo}>Add a description…</button>
				{/if}

				<div class="meta font-mono">
					{pinnedItems.length} {pinnedItems.length === 1 ? 'item' : 'items'}
					<span class="dot">·</span>
					{roomChats.length} {roomChats.length === 1 ? 'chat' : 'chats'}
				</div>
			</header>

			<!-- Ask this notebook — a new chat grounded in these sources -->
			<form class="ask" onsubmit={submitAsk}>
				<Icon icon="ri:sparkling-2-line" width="16" />
				<input class="ask-input" bind:value={askDraft} placeholder="Ask this notebook…" />
				<button class="ask-send" type="submit" disabled={!askDraft.trim()} title="Ask — grounded in this notebook">
					<Icon icon="ri:arrow-right-line" width="15" />
				</button>
			</form>

			<!-- The notebook's items — what grounds its chats. Drop files here
			     to upload + add in one motion. -->
			<section
				class="section"
				class:drop-active={dropActive}
				ondragover={(e) => { e.preventDefault(); dropActive = true; }}
				ondragleave={() => (dropActive = false)}
				ondrop={handleDrop}
			>
				<div class="eyebrow font-mono">
					<span>Materials</span>
					<button class="add-btn" onclick={openPicker} title="Add a page, person, place, file, or link"><Icon icon="ri:add-line" width="14" /></button>
				</div>
				{#if pinnedItems.length === 0}
					<button class="add-row" onclick={openPicker}>
						<Icon icon="ri:add-line" width="15" /> Add pages, people, places, or links — or drop files
					</button>
				{:else}
					<ul class="ledger">
						{#each pinnedItems as it (it.url)}
							<li class="ledger-row">
								<button class="ledger-item" onclick={() => openUrl(it.url)} oncontextmenu={(e) => memberMenu(e, it.url)} title={memberNames[it.url] || it.url}>
									<Icon icon={iconForUrl(it.url)} width="16" class="ledger-ic" />
									<span class="ledger-name">{memberNames[it.url] || labelForUrl(it.url)}</span>
									{#if statusChip(it.url)}
										{@const chip = statusChip(it.url)}
										{#if chip?.retry}
											<span class="status-chip failed font-mono" role="button" tabindex="-1" onclick={(e) => retryExtraction(it.url, e)} onkeydown={(e) => e.key === 'Enter' && retryExtraction(it.url, e)}>{chip.label}</span>
										{:else}
											<span class="status-chip font-mono">{chip?.label}</span>
										{/if}
									{/if}
									<span class="ledger-type font-mono">{memberType(it.url)}</span>
								</button>
								<button class="ledger-remove" title="Remove from Notebook" onclick={() => removeMember(it.url)}><Icon icon="ri:close-line" width="13" /></button>
							</li>
						{/each}
					</ul>
				{/if}
			</section>

			<!-- Highlights across the notebook's documents -->
			{#if highlights.length > 0}
				<section class="section">
					<div class="eyebrow font-mono">
						<span>Highlights</span>
						<span class="eyebrow-right">
							<span class="eyebrow-count">{highlights.length}</span>
							<button class="add-btn" title="Export highlights as markdown" onclick={exportHighlights}>
								<Icon icon="ri:download-line" width="13" />
							</button>
						</span>
					</div>
					{#each highlightGroups as g (g.file_id)}
						<div class="hl-group">
							<button class="hl-file" onclick={() => openUrl(`/drive/${g.file_id}`)} title={g.filename}>
								<Icon icon="ri:file-line" width="13" class="hl-file-ic" />
								<span class="hl-file-name">{g.filename}</span>
								<span class="hl-file-count font-mono">{g.items.length}</span>
							</button>
							<ul class="ledger">
								{#each g.items as h (h.id)}
									<li class="hl-row">
										<button class="hl-item" onclick={() => openHighlight(h)}>
											<span class="hl-bar" style="background:{HL_TINT[h.color] ?? HL_TINT.yellow}"></span>
											<span class="hl-body">
												<span class="hl-quote">{h.quote_text}</span>
												{#if h.note_md}<span class="hl-note">{h.note_md}</span>{/if}
											</span>
											{#if h.page_num}<span class="hl-page font-mono">p{h.page_num}</span>{/if}
										</button>
									</li>
								{/each}
							</ul>
						</div>
					{/each}
				</section>
			{/if}

			<!-- Chats filed here -->
			<section class="section">
				<div class="eyebrow font-mono"><span>Chats</span></div>
				{#if roomChats.length === 0}
					<p class="empty">No chats yet — ask something above to start one here.</p>
				{:else}
					<ul class="ledger">
						{#each roomChats as c (c.conversation_id)}
							<li class="ledger-row">
								<button class="ledger-item" onclick={() => openChat(c.conversation_id)} title={c.title ?? 'Untitled chat'}>
									<Icon icon={c.icon || 'ri:chat-3-line'} width="16" class="ledger-ic" />
									<span class="ledger-name">{c.title ?? 'Untitled chat'}</span>
								</button>
							</li>
						{/each}
					</ul>
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
		excludeIds={pinnedItems.map((i) => i.url)}
		onSelect={addMember}
		onClose={() => (pickerPos = null)}
	/>
{/if}

<ColorPickerModal open={colorOpen} value={accent} onSelect={setAccent} onClose={() => (colorOpen = false)} />

<style>
	/* Centered reading column (robust: full-width scroller, auto-margin inner) */
	.notebook-detail { width: 100%; height: 100%; overflow-y: auto; }
	.inner { max-width: 720px; margin: 0 auto; padding: 3.5rem 2rem 6rem; }
	.state { display: flex; align-items: center; gap: 8px; padding: 3rem 2rem; color: var(--color-foreground-muted); }

	/* Per-file extraction chips + drop affordance */
	.status-chip {
		flex-shrink: 0;
		padding: 1px 7px;
		font-size: 0.625rem;
		border-radius: 999px;
		border: 1px solid var(--color-border);
		color: var(--color-foreground-subtle);
		white-space: nowrap;
	}
	.status-chip.failed {
		border-color: var(--color-danger, #e5484d);
		color: var(--color-danger, #e5484d);
		cursor: pointer;
	}
	.section.drop-active {
		outline: 1.5px dashed var(--color-primary);
		outline-offset: 6px;
		border-radius: 8px;
	}
	.state.error { color: var(--color-error, #dc2626); }

	/* Header — serif title + one description (the app's title/description pattern) */
	.head { margin-bottom: 2rem; }
	.head-top { display: flex; align-items: center; justify-content: space-between; margin-bottom: 1.1rem; }
	.nb-icon {
		display: grid; place-items: center; width: 46px; height: 46px;
		border-radius: 12px; border: 1px solid var(--color-border);
		background: var(--color-surface-elevated); color: var(--color-foreground); cursor: pointer;
	}
	.nb-icon.tinted {
		background: color-mix(in srgb, var(--room-accent) 14%, var(--color-surface-elevated));
		border-color: color-mix(in srgb, var(--room-accent) 32%, var(--color-border));
		color: color-mix(in srgb, var(--room-accent) 80%, var(--color-foreground));
	}
	.head-actions { display: flex; gap: 2px; }
	.icon-btn {
		display: grid; place-items: center; width: 30px; height: 30px;
		border: none; border-radius: 8px; background: transparent;
		color: var(--color-foreground-subtle, #9ca3af); cursor: pointer;
	}
	.icon-btn:hover { background: var(--color-surface-elevated); color: var(--color-foreground); }
	.icon-btn.danger:hover { color: var(--color-error, #dc2626); }

	.title {
		font-size: 2rem; font-weight: 500; line-height: 1.12; margin: 0;
		color: var(--color-foreground); cursor: text;
	}
	.title-input {
		font-size: 2rem; font-weight: 500; line-height: 1.12; width: 100%;
		border: none; background: transparent; color: var(--color-foreground); outline: none; padding: 0 0 2px;
		border-bottom: 1.5px solid color-mix(in srgb, var(--room-accent, var(--color-foreground)) 45%, var(--color-border));
	}
	.desc {
		display: block; width: 100%; text-align: left; margin-top: 0.6rem;
		border: none; background: transparent; cursor: text; padding: 0;
		font: inherit; font-size: 0.95rem; line-height: 1.55; color: var(--color-foreground-muted);
	}
	.desc:hover { color: var(--color-foreground); }
	.desc-empty { color: var(--color-foreground-subtle, #9ca3af); }
	.desc-input {
		width: 100%; margin-top: 0.6rem; min-height: 3rem; resize: vertical;
		border: none; border-left: 2px solid var(--room-accent, var(--color-border));
		background: var(--color-surface-elevated); border-radius: 0 8px 8px 0;
		padding: 0.55rem 0.7rem; font: inherit; font-size: 0.95rem; line-height: 1.55;
		color: var(--color-foreground); outline: none;
	}
	.meta {
		margin-top: 1rem; font-size: 11px; letter-spacing: 0.04em;
		text-transform: uppercase; color: var(--color-foreground-subtle, #9ca3af);
	}
	.meta .dot { margin: 0 0.6ch; opacity: 0.5; }

	/* Ask bar — the primary action */
	.ask {
		display: flex; align-items: center; gap: 10px; height: 48px;
		padding: 0 6px 0 14px;
		border: 1px solid var(--color-border); border-radius: 12px;
		background: var(--color-surface-elevated); margin-bottom: 2.5rem;
		transition: border-color 120ms, box-shadow 120ms;
	}
	.ask:focus-within {
		border-color: color-mix(in srgb, var(--room-accent, var(--color-foreground-subtle, #9ca3af)) 55%, var(--color-border));
		box-shadow: 0 0 0 3px color-mix(in srgb, var(--room-accent, var(--color-foreground-subtle, #9ca3af)) 13%, transparent);
	}
	.ask > :global(svg) { color: var(--color-foreground-subtle, #9ca3af); flex-shrink: 0; }
	.ask-input {
		flex: 1; min-width: 0; border: none; background: transparent; outline: none;
		font: inherit; font-size: 0.95rem; color: var(--color-foreground);
	}
	.ask-input::placeholder { color: var(--color-foreground-subtle, #9ca3af); }
	.ask-send {
		display: grid; place-items: center; width: 34px; height: 34px; flex-shrink: 0;
		border: none; border-radius: 9px; cursor: pointer;
		background: var(--room-accent, var(--color-foreground)); color: var(--color-background, #fff);
	}
	.ask-send:disabled { opacity: 0.35; cursor: default; }

	/* Sections — quiet ledger lists (hairline rows, not boxes) */
	.section { margin-bottom: 2.25rem; }
	.eyebrow {
		display: flex; align-items: center; justify-content: space-between;
		font-size: 11px; letter-spacing: 0.09em; text-transform: uppercase;
		color: var(--color-foreground-subtle, #9ca3af);
		padding-bottom: 0.55rem; border-bottom: 1px solid var(--color-border);
	}
	.add-btn {
		display: grid; place-items: center; width: 22px; height: 22px;
		border: none; border-radius: 6px; background: transparent;
		color: var(--color-foreground-subtle, #9ca3af); cursor: pointer;
	}
	.add-btn:hover { background: var(--color-surface-elevated); color: var(--color-foreground); }
	.empty { margin: 0; padding: 0.85rem 0; font-size: 0.9rem; color: var(--color-foreground-muted); }
	.add-row {
		display: flex; align-items: center; gap: 8px; width: 100%; text-align: left;
		padding: 0.85rem 0; border: none; background: transparent; cursor: pointer;
		font: inherit; font-size: 0.9rem; color: var(--color-foreground-subtle, #9ca3af);
	}
	.add-row:hover { color: var(--color-foreground); }

	.ledger { list-style: none; margin: 0; padding: 0; }
	.ledger-row {
		position: relative; display: flex; align-items: center;
		border-bottom: 1px solid color-mix(in srgb, var(--color-border) 55%, transparent);
	}
	.ledger-item {
		display: flex; align-items: center; gap: 12px; width: 100%;
		padding: 0.7rem 0.25rem; border: none; background: transparent;
		color: var(--color-foreground); font: inherit; font-size: 0.95rem; text-align: left; cursor: pointer;
	}
	.ledger-item :global(.ledger-ic) { color: var(--color-foreground-subtle, #9ca3af); flex-shrink: 0; }
	.ledger-row:hover .ledger-item :global(.ledger-ic) { color: color-mix(in srgb, var(--room-accent, var(--color-foreground)) 70%, var(--color-foreground)); }
	.ledger-name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 450; }
	.ledger-type { flex-shrink: 0; font-size: 10px; letter-spacing: 0.06em; color: var(--color-foreground-subtle, #9ca3af); }
	.ledger-remove {
		position: absolute; right: 0;
		display: grid; place-items: center; width: 22px; height: 22px;
		border: none; border-radius: 6px; background: var(--color-background);
		color: var(--color-foreground-subtle, #9ca3af); cursor: pointer; opacity: 0;
	}
	.ledger-row:hover .ledger-remove { opacity: 1; }
	.ledger-remove:hover { color: var(--color-foreground); }

	/* Highlights — grouped by file, each a quiet quote row that opens the
	   viewer at the mark. */
	.eyebrow-count {
		font-size: 10px; padding: 1px 7px; border-radius: 999px;
		background: var(--color-surface-elevated); color: var(--color-foreground-subtle, #9ca3af);
	}
	.eyebrow-right { display: flex; align-items: center; gap: 4px; }
	.hl-group { margin-top: 0.9rem; }
	.hl-file {
		display: flex; align-items: center; gap: 7px; width: 100%; text-align: left;
		padding: 0.3rem 0.25rem; border: none; background: transparent; cursor: pointer;
		color: var(--color-foreground-muted);
	}
	.hl-file:hover { color: var(--color-foreground); }
	.hl-file :global(.hl-file-ic) { color: var(--color-foreground-subtle, #9ca3af); flex-shrink: 0; }
	.hl-file-name {
		min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
		font-size: 0.78rem; font-weight: 500; letter-spacing: 0.01em;
	}
	.hl-file-count { flex-shrink: 0; font-size: 10px; color: var(--color-foreground-subtle, #9ca3af); }
	.hl-row { display: flex; }
	.hl-item {
		display: flex; align-items: flex-start; gap: 10px; width: 100%; text-align: left;
		padding: 0.45rem 0.25rem 0.45rem 0.4rem; border: none; background: transparent;
		cursor: pointer; font: inherit; border-radius: 6px;
	}
	.hl-item:hover { background: color-mix(in srgb, var(--color-border) 30%, transparent); }
	.hl-bar { flex-shrink: 0; width: 3px; align-self: stretch; border-radius: 2px; min-height: 1.1rem; }
	.hl-body { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
	.hl-quote {
		font-size: 0.875rem; line-height: 1.4; color: var(--color-foreground);
		display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden;
	}
	.hl-note {
		font-size: 0.78rem; line-height: 1.4; color: var(--color-foreground-muted);
		display: -webkit-box; -webkit-line-clamp: 1; -webkit-box-orient: vertical; overflow: hidden;
	}
	.hl-page { flex-shrink: 0; font-size: 10px; color: var(--color-foreground-subtle, #9ca3af); padding-top: 2px; }

	:global(.spin) { animation: spin 0.8s linear infinite; }
	@keyframes spin { to { transform: rotate(360deg); } }
</style>
