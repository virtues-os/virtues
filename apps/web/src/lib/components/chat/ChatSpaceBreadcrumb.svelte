<script lang="ts">
	import Icon from '@iconify/svelte';
	import { spaceStore } from '$lib/stores/space.svelte';
	import type { SpaceSummary } from '$lib/api/client';

	// The room this chat lives in (null = unfiled), and a setter that persists.
	let {
		spaceId = null,
		onChange,
	}: {
		spaceId?: string | null;
		onChange: (spaceId: string | null) => void;
	} = $props();

	let menuOpen = $state(false);
	let query = $state('');
	let creating = $state(false);

	const current = $derived<SpaceSummary | undefined>(
		spaceId ? spaceStore.byId(spaceId) : undefined,
	);

	const filtered = $derived.by(() => {
		const q = query.trim().toLowerCase();
		const all = spaceStore.spaces;
		if (!q) return all;
		return all.filter((s) => s.name.toLowerCase().includes(q));
	});

	// Show "create" affordance when the query doesn't exactly match an existing room.
	const canCreate = $derived.by(() => {
		const q = query.trim();
		if (!q) return false;
		return !spaceStore.spaces.some((s) => s.name.toLowerCase() === q.toLowerCase());
	});

	function open() {
		query = '';
		menuOpen = true;
		// Spaces are loaded globally on app start; only fetch if we somehow have none.
		if (spaceStore.spaces.length === 0) spaceStore.load();
	}

	function pick(id: string | null) {
		onChange(id);
		menuOpen = false;
	}

	async function createAndPick() {
		const name = query.trim();
		if (!name || creating) return;
		creating = true;
		try {
			const space = await spaceStore.create(name);
			if (space) onChange(space.id);
			menuOpen = false;
		} finally {
			creating = false;
		}
	}

	function onKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			menuOpen = false;
		} else if (e.key === 'Enter' && canCreate) {
			e.preventDefault();
			createAndPick();
		}
	}

	// The room's accent tint, applied as an ambient wash (not a hard badge color).
	const accent = $derived(current?.accent_color || null);
</script>

<div class="space-breadcrumb" style={accent ? `--room-accent: ${accent}` : ''}>
	<button
		class="room-trigger"
		class:filed={!!current}
		class:tinted={!!accent}
		onclick={open}
		aria-haspopup="menu"
		aria-expanded={menuOpen}
		title={current ? `In ${current.name}` : 'Add this chat to a Space'}
	>
		{#if current}
			<Icon icon={current.icon || 'ri:layout-masonry-line'} width="13" />
			<span class="room-name">{current.name}</span>
			<Icon icon="ri:arrow-down-s-line" width="13" class="chevron" />
		{:else}
			<Icon icon="ri:add-line" width="13" />
			<span class="room-name muted">Space</span>
		{/if}
	</button>

	{#if menuOpen}
		<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
		<div class="menu-backdrop" onclick={() => (menuOpen = false)}></div>
		<div class="menu" role="menu">
			<input
				class="menu-search"
				placeholder="Find or create a Space…"
				bind:value={query}
				onkeydown={onKeydown}
				autofocus
			/>

			<div class="menu-list">
				{#if current}
					<button class="menu-item remove" onclick={() => pick(null)} role="menuitem">
						<Icon icon="ri:logout-box-r-line" width="14" />
						<span>Remove from {current.name}</span>
					</button>
				{/if}

				{#each filtered as space (space.id)}
					<button
						class="menu-item"
						class:active={space.id === spaceId}
						onclick={() => pick(space.id)}
						role="menuitem"
					>
						<span class="dot" style={space.accent_color ? `background:${space.accent_color}` : ''}></span>
						<Icon icon={space.icon || 'ri:layout-masonry-line'} width="14" />
						<span class="menu-item-name">{space.name}</span>
						{#if space.chat_count > 0}
							<span class="menu-item-count">{space.chat_count}</span>
						{/if}
						{#if space.id === spaceId}
							<Icon icon="ri:check-line" width="14" class="check" />
						{/if}
					</button>
				{/each}

				{#if canCreate}
					<button class="menu-item create" onclick={createAndPick} disabled={creating} role="menuitem">
						<Icon icon={creating ? 'ri:loader-4-line' : 'ri:add-circle-line'} width="14" class={creating ? 'spin' : ''} />
						<span>Create "<strong>{query.trim()}</strong>"</span>
					</button>
				{:else if filtered.length === 0}
					<div class="menu-empty">No Spaces yet — type a name to create one.</div>
				{/if}
			</div>
		</div>
	{/if}
</div>

<style>
	.space-breadcrumb {
		position: relative;
		display: inline-flex;
	}

	.room-trigger {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		height: 24px;
		padding: 0 8px;
		border: 1px solid transparent;
		border-radius: 7px;
		background: transparent;
		color: var(--color-foreground-muted);
		font-size: 12px;
		font-weight: 500;
		cursor: pointer;
		transition: background 0.12s ease, color 0.12s ease, border-color 0.12s ease;
		max-width: 220px;
	}
	.room-trigger:hover {
		background: var(--color-surface-elevated);
		color: var(--color-foreground);
	}
	.room-trigger.filed {
		color: var(--color-foreground);
	}
	/* Ambient room tint — a quiet wash + hairline, never a loud badge. */
	.room-trigger.tinted {
		background: color-mix(in srgb, var(--room-accent) 10%, transparent);
		border-color: color-mix(in srgb, var(--room-accent) 28%, transparent);
		color: color-mix(in srgb, var(--room-accent) 72%, var(--color-foreground));
	}
	.room-trigger.tinted:hover {
		background: color-mix(in srgb, var(--room-accent) 16%, transparent);
	}
	.room-name {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.room-name.muted {
		color: var(--color-foreground-muted);
	}
	.room-trigger :global(.chevron) {
		opacity: 0.6;
	}

	.menu-backdrop {
		position: fixed;
		inset: 0;
		z-index: 40;
	}
	.menu {
		position: absolute;
		top: calc(100% + 6px);
		left: 0;
		z-index: 41;
		width: 260px;
		padding: 6px;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 10px;
		box-shadow: 0 12px 32px rgba(0, 0, 0, 0.18);
	}
	.menu-search {
		width: 100%;
		height: 30px;
		padding: 0 9px;
		margin-bottom: 4px;
		border: 1px solid var(--color-border);
		border-radius: 7px;
		background: var(--color-surface-elevated);
		color: var(--color-foreground);
		font-size: 13px;
		outline: none;
	}
	.menu-search:focus {
		border-color: color-mix(in srgb, var(--color-primary) 50%, transparent);
	}
	.menu-list {
		display: flex;
		flex-direction: column;
		gap: 1px;
		max-height: 280px;
		overflow-y: auto;
	}
	.menu-item {
		display: flex;
		align-items: center;
		gap: 8px;
		height: 32px;
		padding: 0 8px;
		border: none;
		border-radius: 6px;
		background: transparent;
		color: var(--color-foreground);
		font-size: 13px;
		text-align: left;
		cursor: pointer;
	}
	.menu-item:hover {
		background: var(--color-surface-elevated);
	}
	.menu-item.active {
		background: color-mix(in srgb, var(--color-primary) 8%, transparent);
	}
	.menu-item-name {
		flex: 1;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.menu-item-count {
		font-size: 11px;
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
	}
	.menu-item .dot {
		width: 7px;
		height: 7px;
		border-radius: 50%;
		background: var(--color-foreground-muted);
		flex-shrink: 0;
	}
	.menu-item.remove {
		color: var(--color-foreground-muted);
	}
	.menu-item.create strong {
		font-weight: 600;
	}
	.menu-item :global(.check) {
		color: var(--color-primary);
	}
	.menu-empty {
		padding: 10px 8px;
		font-size: 12px;
		color: var(--color-foreground-muted);
	}
	:global(.spin) {
		animation: spin 0.8s linear infinite;
	}
	@keyframes spin {
		to {
			transform: rotate(360deg);
		}
	}
</style>
