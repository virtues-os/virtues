<script lang="ts">
	/**
	 * A task's home, and the menu that moves it: no home, an open thread, a
	 * thing, or a person, place or organization found by search.
	 */
	import type { Task, TaskLink, TaskPatch } from '$lib/api/client';
	import Icon from '$lib/components/Icon.svelte';
	import MenuItem from '$lib/components/MenuItem.svelte';
	import RefPicker from '$lib/components/RefPicker.svelte';
	import Popover from '$lib/floating/primitives/Popover.svelte';
	import { projectStore } from '$lib/stores/project.svelte';
	import { thingsStore } from '$lib/stores/things.svelte';
	import { homeOf, homePatch, linkFromUrl, sameLink, subjectIcon, subjectName } from '$lib/tasks/homes';

	let {
		task,
		onSave,
		showIcon = false,
		label,
		plain = false,
	}: {
		task: Task;
		onSave: (patch: TaskPatch) => void;
		/** The home's icon before its name. */
		showIcon?: boolean;
		/** The trigger's words, when they should not be the home's name. */
		label?: string;
		/** Full-size ink, for a column of properties. */
		plain?: boolean;
	} = $props();

	let open = $state(false);
	let searchAt = $state<{ x: number; y: number } | null>(null);

	const home = $derived(homeOf(task));

	function pick(link: TaskLink | null) {
		open = false;
		if (link ? sameLink(link, home) : !home) return;
		onSave(homePatch(link));
	}

	function search(e: MouseEvent) {
		open = false;
		searchAt = { x: e.clientX, y: e.clientY };
	}

	function found(entity: { url: string }) {
		searchAt = null;
		const link = linkFromUrl(entity.url);
		if (link) pick(link);
	}
</script>

<Popover bind:open placement="bottom-start" offset={4}>
	{#snippet trigger({ toggle })}
		<button type="button" class="fact" class:plain onclick={toggle} aria-haspopup="menu">
			{#if showIcon && home}<Icon icon={subjectIcon(home.kind)} width="14" />{/if}
			{label ?? (home ? subjectName(home) : 'No home yet')}
		</button>
	{/snippet}
	{#snippet children()}
		<div class="menu" role="menu" aria-label="This task's home">
			<MenuItem role="menuitemradio" checked={!home} label="No home yet" onclick={() => pick(null)} />
			{#if projectStore.projects.length > 0}
				<p class="menu-label">Threads</p>
				{#each projectStore.projects as p (p.id)}
					<MenuItem
						role="menuitemradio"
						checked={task.thread_id === p.id}
						label={p.name}
						onclick={() => pick({ kind: 'thread', id: p.id })}
					/>
				{/each}
			{/if}
			{#if thingsStore.things.length > 0}
				<p class="menu-label">Things</p>
				{#each thingsStore.things as th (th.id)}
					<MenuItem
						role="menuitemradio"
						checked={task.thing_id === th.id}
						label={th.title}
						onclick={() => pick({ kind: 'thing', id: th.id })}
					/>
				{/each}
			{/if}
			{#if home && (home.kind === 'person' || home.kind === 'place' || home.kind === 'org')}
				<MenuItem role="menuitemradio" checked label={subjectName(home)} onclick={() => (open = false)} />
			{/if}
			<MenuItem label="A person, place or organization…" onclick={search} />
		</div>
	{/snippet}
</Popover>

{#if searchAt}
	<RefPicker
		mode="single"
		position={searchAt}
		placeholder="Find a person, place or organization…"
		filter={(e) => {
			const link = linkFromUrl(e.url);
			return !!link && (link.kind === 'person' || link.kind === 'place' || link.kind === 'org');
		}}
		onSelect={found}
		onClose={() => (searchAt = null)}
	/>
{/if}

<style>
	.fact {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		padding: 0 4px;
		margin: 0 -4px;
		border: 0;
		border-radius: var(--radius-sm);
		background: transparent;
		font: inherit;
		font-size: 13px;
		color: var(--color-foreground-muted);
		cursor: pointer;
		transition: background-color var(--motion-hover-out) var(--ease-premium);
	}
	.fact:hover {
		background: var(--hover-bg);
		color: var(--color-foreground);
		transition-duration: var(--motion-hover-in);
	}
	.fact.plain {
		font-size: 14px;
		color: var(--color-foreground);
	}
	.menu {
		display: flex;
		flex-direction: column;
		min-width: 12rem;
		max-height: 18rem;
		overflow-y: auto;
		padding: 4px;
		background: var(--color-background);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-lg);
	}
	.menu-label {
		margin: 0;
		padding: 8px 8px 4px;
		font-size: 12px;
		color: var(--color-foreground-subtle);
	}
	@media (prefers-reduced-motion: reduce) {
		.fact {
			transition: none;
		}
	}
</style>
