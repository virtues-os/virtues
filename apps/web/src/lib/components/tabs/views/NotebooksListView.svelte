<script lang="ts">
	import { onMount, tick } from 'svelte';
	import Icon from '$lib/components/Icon.svelte';
	import { notebookStore } from '$lib/stores/notebook.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';

	let { active: _active }: { tab?: unknown; active?: boolean } = $props();

	let creating = $state(false);
	// Inline draft — `prompt()` is a no-op in the Tauri/WKWebView shell.
	let drafting = $state(false);
	let draftName = $state('');
	let inputEl = $state<HTMLInputElement | null>(null);

	onMount(() => {
		notebookStore.load();
	});

	const notebooks = $derived(notebookStore.notebooks);

	function open(id: string) {
		windowShellStore.openTabFromRoute(`/notebook/${id}`);
	}

	async function startDraft() {
		drafting = true;
		draftName = '';
		await tick();
		inputEl?.focus();
	}
	function cancelDraft() {
		drafting = false;
		draftName = '';
	}
	async function commitDraft() {
		const name = draftName.trim();
		if (!name || creating) {
			if (!name) cancelDraft();
			return;
		}
		creating = true;
		try {
			const notebook = await notebookStore.create(name);
			cancelDraft();
			if (notebook) open(notebook.id);
		} finally {
			creating = false;
		}
	}
	function onDraftKeydown(e: KeyboardEvent) {
		if (e.key === 'Enter') {
			e.preventDefault();
			commitDraft();
		} else if (e.key === 'Escape') {
			e.preventDefault();
			cancelDraft();
		}
	}
</script>

<div class="notebooks-list">
	<header class="head">
		<div>
			<h1>Notebooks</h1>
			<p class="sub">Rooms you return to — a project, pet, hobby, or goal. Each chat lives in one.</p>
		</div>
		{#if drafting}
			<input
				bind:this={inputEl}
				bind:value={draftName}
				class="name-input"
				placeholder="Name your Notebook"
				disabled={creating}
				onkeydown={onDraftKeydown}
				onblur={commitDraft}
			/>
		{:else}
			<button class="new-btn" onclick={startDraft} disabled={creating}>
				<Icon icon="ri:add-line" width="16" /> New Notebook
			</button>
		{/if}
	</header>

	{#if notebooks.length === 0}
		<div class="empty">
			<Icon icon="ri:layout-masonry-line" width="28" />
			<p>No Notebooks yet.</p>
			{#if !drafting}
				<button class="new-btn ghost" onclick={startDraft}>Create your first Notebook</button>
			{/if}
		</div>
	{:else}
		<div class="grid">
			{#each notebooks as s (s.id)}
				<button
					class="card"
					class:tinted={!!s.accent_color}
					style={s.accent_color ? `--room-accent: ${s.accent_color}` : ''}
					onclick={() => open(s.id)}
				>
					<div class="card-icon"><Icon icon={s.icon || 'ri:layout-masonry-line'} width="20" /></div>
					<div class="card-name">{s.name}</div>
					{#if s.current_status}
						<div class="card-memo">{s.current_status}</div>
					{/if}
					<div class="card-meta">
						<span>{s.chat_count} {s.chat_count === 1 ? 'chat' : 'chats'}</span>
						<span class="dot-sep">·</span>
						<span>{s.item_count} pinned</span>
					</div>
				</button>
			{/each}
		</div>
	{/if}
</div>

<style>
	.notebooks-list { height: 100%; overflow-y: auto; padding: 28px 32px 48px; max-width: 920px; margin: 0 auto; }
	.head { display: flex; align-items: flex-start; justify-content: space-between; gap: 16px; margin-bottom: 22px; }
	h1 { font-size: 24px; font-weight: 680; margin: 0; color: var(--color-foreground); }
	.sub { margin: 4px 0 0; font-size: 13px; color: var(--color-foreground-muted); max-width: 48ch; }
	.new-btn {
		display: inline-flex; align-items: center; gap: 5px;
		padding: 7px 12px; border: 1px solid var(--color-border); border-radius: 8px;
		background: var(--color-surface-elevated); color: var(--color-foreground);
		font-size: 13px; font-weight: 500; cursor: pointer; white-space: nowrap;
	}
	.new-btn:hover { background: var(--color-surface); }
	.new-btn.ghost { background: transparent; margin-top: 10px; }
	.name-input {
		padding: 7px 12px; border: 1px solid var(--color-border); border-radius: 8px;
		background: var(--color-surface-elevated); color: var(--color-foreground);
		font-size: 13px; outline: none; min-width: 200px;
	}
	.name-input:focus { border-color: var(--color-primary, var(--color-foreground-muted)); }

	.grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(200px, 1fr)); gap: 12px; }
	.card {
		display: flex; flex-direction: column; gap: 6px;
		padding: 14px; border: 1px solid var(--color-border); border-radius: 12px;
		background: var(--color-surface); text-align: left; cursor: pointer;
		transition: border-color 0.12s ease, background 0.12s ease;
	}
	.card:hover { background: var(--color-surface-elevated); }
	.card.tinted { box-shadow: inset 3px 0 0 var(--room-accent); border-color: color-mix(in srgb, var(--room-accent) 30%, var(--color-border)); }
	.card-icon {
		display: grid; place-items: center; width: 36px; height: 36px;
		border-radius: 9px; background: var(--color-surface-elevated); color: var(--color-foreground); margin-bottom: 2px;
	}
	.card.tinted .card-icon {
		background: color-mix(in srgb, var(--room-accent) 16%, transparent);
		color: color-mix(in srgb, var(--room-accent) 78%, var(--color-foreground));
	}
	.card-name { font-size: 14.5px; font-weight: 600; color: var(--color-foreground); }
	.card-memo {
		font-size: 12.5px; color: var(--color-foreground-muted); line-height: 1.4;
		display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden;
	}
	.card-meta { display: flex; align-items: center; gap: 6px; font-size: 11.5px; color: var(--color-foreground-subtle, #9ca3af); margin-top: 2px; }
	.dot-sep { opacity: 0.5; }

	.empty { display: flex; flex-direction: column; align-items: center; gap: 6px; padding: 64px 0; color: var(--color-foreground-muted); }
	.empty p { margin: 0; font-size: 14px; }
</style>
