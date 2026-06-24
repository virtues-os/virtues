<!--
	SpacesSection.svelte

	Sidebar list of Spaces (rooms). Click a room → open its detail tab; click the
	header → open the all-Spaces view; "+" → create. Right-click → delete.
	Renders nothing-but-the-header affordance when there are no rooms yet.
-->
<script lang="ts">
	import Icon from '$lib/components/Icon.svelte';
	import { spaceStore } from '$lib/stores/space.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { contextMenu } from '$lib/stores/contextMenu.svelte';

	let { collapsed = false }: { collapsed?: boolean } = $props();

	const spaces = $derived(spaceStore.spaces);

	function openList() {
		windowShellStore.openTabFromRoute('/spaces');
	}
	function open(id: string) {
		windowShellStore.openTabFromRoute(`/space/${id}`);
	}
	async function create() {
		const name = prompt('Name your Space');
		if (!name?.trim()) return;
		const space = await spaceStore.create(name.trim());
		if (space) open(space.id);
	}
	function menu(e: MouseEvent, id: string, name: string) {
		e.preventDefault();
		e.stopPropagation();
		contextMenu.show({ x: e.clientX, y: e.clientY }, [
			{
				id: 'delete',
				label: 'Delete Space',
				icon: 'ri:delete-bin-line',
				action: async () => { await spaceStore.remove(id); },
			},
		]);
	}
</script>

<div class="spaces-section" class:collapsed>
	{#if !collapsed}
		<div class="section-header">
			<button class="header-label" onclick={openList} title="All Spaces">Spaces</button>
			<button class="header-add" onclick={create} title="New Space"><Icon icon="ri:add-line" width="14" /></button>
		</div>
	{/if}

	{#if spaces.length === 0}
		{#if !collapsed}
			<button class="empty-row" onclick={create}>
				<Icon icon="ri:add-circle-line" width="14" />
				<span>New Space</span>
			</button>
		{/if}
	{:else}
		<ul class="room-list">
			{#each spaces as s (s.id)}
				<li>
					<button
						class="room-row"
						class:collapsed
						title={s.name}
						onclick={() => open(s.id)}
						oncontextmenu={(e) => menu(e, s.id, s.name)}
					>
						{#if s.accent_color}
							<span class="room-dot" style={`background:${s.accent_color}`}></span>
						{:else}
							<Icon icon={s.icon || 'ri:layout-masonry-line'} width="14" />
						{/if}
						{#if !collapsed}
							<span class="room-label">{s.name}</span>
							{#if s.chat_count > 0}
								<span class="room-count">{s.chat_count}</span>
							{/if}
						{/if}
					</button>
				</li>
			{/each}
		</ul>
	{/if}
</div>

<style>
	.spaces-section { display: flex; flex-direction: column; gap: 0.125rem; padding: 0 0.375rem 0.375rem; }
	.spaces-section.collapsed { padding: 0 0.25rem 0.25rem; }

	.section-header {
		display: flex; align-items: center; justify-content: space-between;
		padding: 0.375rem 0.5rem 0.125rem;
	}
	.header-label {
		border: none; background: transparent; cursor: pointer;
		font-size: 0.6875rem; font-weight: 600; text-transform: uppercase; letter-spacing: 0.06em;
		color: var(--color-foreground-subtle, #9ca3af); padding: 0;
	}
	.header-label:hover { color: var(--color-foreground); }
	.header-add {
		display: grid; place-items: center; width: 18px; height: 18px;
		border: none; border-radius: 5px; background: transparent;
		color: var(--color-foreground-subtle, #9ca3af); cursor: pointer;
	}
	.header-add:hover { background: var(--color-background-hover, #f3f4f6); color: var(--color-foreground); }

	.room-list { list-style: none; padding: 0; margin: 0; display: flex; flex-direction: column; gap: 1px; }
	.room-row {
		display: flex; align-items: center; gap: 0.5rem; width: 100%;
		padding: 0.3125rem 0.5rem; border: none; border-radius: 4px; background: transparent;
		cursor: pointer; font: inherit; font-size: 0.8125rem; color: var(--color-foreground, inherit); text-align: left;
	}
	.room-row.collapsed { justify-content: center; padding: 0.3125rem 0.375rem; }
	.room-row:hover { background: var(--color-background-hover, #f3f4f6); }
	.room-dot { width: 9px; height: 9px; border-radius: 50%; flex-shrink: 0; }
	.room-label { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.room-count { font-size: 0.6875rem; color: var(--color-foreground-subtle, #9ca3af); font-variant-numeric: tabular-nums; }

	.empty-row {
		display: flex; align-items: center; gap: 0.5rem; width: 100%;
		padding: 0.3125rem 0.5rem; border: none; border-radius: 4px; background: transparent;
		cursor: pointer; font: inherit; font-size: 0.8125rem; color: var(--color-foreground-subtle, #9ca3af); text-align: left;
	}
	.empty-row:hover { background: var(--color-background-hover, #f3f4f6); color: var(--color-foreground); }
</style>
