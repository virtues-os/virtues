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
	import IconPicker from '$lib/components/IconPicker.svelte';
	import { Popover } from '$lib/floating';
	import { confirmAction } from '$lib/stores/dialog.svelte';
	import { toast } from 'svelte-sonner';
	import { getRefSummary } from '$lib/utils/refSummary';
	import { getPage, getDriveFile, uploadDriveFile, addNotebookItem, reextractDriveFile } from '$lib/api/client';
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

	// ---- Title + description: always live, no edit mode ----------------------
	// Both fields are directly typeable (matching the page editor). Drafts hold
	// what's on screen; a blur or Escape settles them against the server.
	let nameDraft = $state('');
	let memoDraft = $state('');
	// Re-seed the drafts whenever a different notebook loads — but never while
	// the user is mid-edit in that field, or their keystrokes would be reverted.
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
		// An emptied title is a slip, not an intent — restore the stored name.
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

	// ---- Icon + accent color -------------------------------------------------
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
	// confirmAction, not window.confirm(): the native dialog is unreliable in the
	// Tauri/WKWebView shell (`prompt()` is already a known no-op there), so a
	// falsy return silently swallowed the delete.
	async function doDelete() {
		const id = notebookId;
		if (!id || !detail) return;
		const ok = await confirmAction({
			title: 'Delete notebook?',
			body: `"${detail.name}" will be deleted. Its chats, pages and files stay where they are — they just stop being filed here.`,
			confirmLabel: 'Delete',
			danger: true,
		});
		if (!ok) return;
		try {
			await notebookStore.remove(id);
			// Close every tab pointed at the now-deleted notebook before
			// navigating, or the stale detail tab stays open and it reads as
			// "delete didn't work".
			windowShellStore.closeTabsByRoute(`/notebook/${id}`);
			// focusExisting, not the default in-place navigate: closing our own
			// tab hands focus to whatever tab was next, and navigating that in
			// place would hijack an unrelated notebook the user still had open.
			windowShellStore.openTabFromRoute('/notebooks', { focusExisting: true });
		} catch (e) {
			console.error('[NotebookDetailView] delete failed:', e);
			toast.error('Failed to delete notebook');
		}
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
					<div class="head-actions">
						<Popover bind:open={overflowOpen} placement="bottom-end">
							{#snippet trigger({ toggle }: { toggle: () => void })}
								<button class="icon-btn" title="More" onclick={toggle}><Icon icon="ri:more-line" width="16" /></button>
							{/snippet}
							{#snippet children({ close }: { close: () => void })}
								<div class="menu">
									<button class="menu-item" onclick={() => { close(); colorOpen = true; }}>
										<Icon icon="ri:palette-line" width="15" /> Change color
									</button>
									<button class="menu-item danger" onclick={() => { close(); doDelete(); }}>
										<Icon icon="ri:delete-bin-line" width="15" /> Delete notebook
									</button>
								</div>
							{/snippet}
						</Popover>
					</div>
				</div>

				<!-- Title + description are always live: click the text and type.
				     No edit mode, no pencil — the page editor's pattern. -->
				<textarea
					class="title-input font-serif"
					bind:value={nameDraft}
					rows="1"
					placeholder="Untitled notebook"
					onfocus={() => (nameFocused = true)}
					onblur={commitName}
					onkeydown={(e) => {
						if (e.key === 'Enter') { e.preventDefault(); e.currentTarget.blur(); }
						if (e.key === 'Escape') { nameDraft = detail?.name ?? ''; e.currentTarget.blur(); }
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
						if (e.key === 'Escape') { memoDraft = detail?.current_status ?? ''; e.currentTarget.blur(); }
					}}
				></textarea>

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

	/* Title + description read as text and edit in place — no box at rest, no
	   edit mode. Only the caret and a faint baseline on focus say "editable". */
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
		margin-top: 0.6rem; min-height: 1.5rem; padding: 0;
		font: inherit; font-size: 0.95rem; line-height: 1.55;
		color: var(--color-foreground-muted);
	}
	.desc-input:focus { color: var(--color-foreground); }
	.title-input::placeholder, .desc-input::placeholder { color: var(--color-foreground-subtle, #9ca3af); }

	/* Overflow menu (••• in the header) */
	.menu { display: flex; flex-direction: column; min-width: 190px; padding: 4px; }
	.menu-item {
		display: flex; align-items: center; gap: 9px; width: 100%; text-align: left;
		padding: 7px 9px; border: none; border-radius: 7px; background: transparent;
		font: inherit; font-size: 0.85rem; color: var(--color-foreground); cursor: pointer;
	}
	.menu-item:hover { background: var(--color-surface-elevated); }
	.menu-item.danger { color: var(--color-error, #dc2626); }
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

	:global(.spin) { animation: spin 0.8s linear infinite; }
	@keyframes spin { to { transform: rotate(360deg); } }
</style>
