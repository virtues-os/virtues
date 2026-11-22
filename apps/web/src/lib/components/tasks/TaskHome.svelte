<script lang="ts">
	/**
	 * A task's home, and the menu that moves it (HomeMenu): no home, an open
	 * thread, a thing, or a person, place or organization found by search.
	 *
	 * With `linked`, the home's name opens the home, so a task leads to its
	 * thread, and "Move" beside it opens the menu. Without it, the name is the
	 * trigger.
	 */
	import type { Task, TaskLink, TaskPatch } from '$lib/api/client';
	import Icon from '$lib/components/Icon.svelte';
	import ProjectGlyph from '$lib/components/ProjectGlyph.svelte';
	import RefPicker from '$lib/components/RefPicker.svelte';
	import TextAction from '$lib/components/TextAction.svelte';
	import HomeMenu from '$lib/components/tasks/HomeMenu.svelte';
	import Popover from '$lib/floating/primitives/Popover.svelte';
	import { projectStore } from '$lib/stores/project.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { homeOf, homePatch, linkFromUrl, sameLink, subjectIcon, subjectName, subjectUrl } from '$lib/tasks/homes';

	let {
		task,
		onSave,
		showIcon = false,
		label,
		plain = false,
		linked = false,
	}: {
		task: Task;
		onSave: (patch: TaskPatch) => void;
		/** The home's icon before its name; a thread's own glyph and color. */
		showIcon?: boolean;
		/** The trigger's words, when they should not be the home's name. */
		label?: string;
		/** Full-size ink, for a column of properties. */
		plain?: boolean;
		/** The name opens the home; "Move" opens the menu. */
		linked?: boolean;
	} = $props();

	let open = $state(false);
	let searchAt = $state<{ x: number; y: number } | null>(null);
	let anchor = $state<HTMLElement | null>(null);

	const home = $derived(homeOf(task));
	const thread = $derived(
		home?.kind === 'thread' ? [...projectStore.projects, ...projectStore.archived].find((p) => p.id === home.id) : undefined,
	);
	const name = $derived(home ? (thread?.name ?? subjectName(home)) : 'No home');

	function pick(link: TaskLink | null) {
		open = false;
		if (link ? sameLink(link, home) : !home) return;
		onSave(homePatch(link));
	}

	/** The search opens under the control, so the keyboard gets it in the right place too. */
	function search() {
		open = false;
		const r = anchor?.getBoundingClientRect();
		searchAt = { x: r?.left ?? 0, y: (r?.bottom ?? 0) + 4 };
	}

	function found(entity: { url: string }) {
		searchAt = null;
		const link = linkFromUrl(entity.url);
		if (link) pick(link);
	}

	function openHome() {
		if (home) windowShellStore.openTabFromRoute(subjectUrl(home), { label: name, focusExisting: true });
	}
</script>

{#snippet glyph()}
	{#if showIcon && home}
		{#if thread}
			<ProjectGlyph project={thread} size={14} tinted />
		{:else}
			<Icon icon={subjectIcon(home.kind)} width="14" />
		{/if}
	{/if}
{/snippet}

<span class="home" bind:this={anchor}>
	<Popover bind:open placement="bottom-start" offset={4}>
		{#snippet trigger({ toggle })}
			{#if linked && home}
				<span class="linked">
					<button type="button" class="fact" class:plain onclick={openHome} data-tip={`Open ${name}`}>
						{@render glyph()}<span class="name">{label ?? name}</span>
					</button>
					<TextAction quiet onclick={toggle}>Move</TextAction>
				</span>
			{:else}
				<button type="button" class="fact" class:plain class:empty={!home && !label} onclick={toggle} aria-haspopup="menu">
					{@render glyph()}<span class="name">{label ?? name}</span>
				</button>
			{/if}
		{/snippet}
		{#snippet children()}
			<div class="menu" role="menu" aria-label="This task's home">
				<HomeMenu value={home} onPick={pick} onSearch={search} />
			</div>
		{/snippet}
	</Popover>
</span>

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
	.home {
		display: inline-flex;
		min-width: 0;
		max-width: 100%;
	}
	.linked {
		display: inline-flex;
		align-items: center;
		gap: 12px;
		min-width: 0;
		max-width: 100%;
	}
	.fact {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		min-width: 0;
		max-width: 100%;
		padding: 0 4px;
		margin: 0 -4px;
		border: 0;
		border-radius: var(--radius-sm);
		background: transparent;
		font: inherit;
		font-size: var(--text-small);
		color: var(--color-foreground-muted);
		cursor: pointer;
		transition: background-color var(--motion-hover-out) var(--ease-premium);
	}
	.fact:hover {
		background: var(--hover-bg);
		color: var(--color-foreground);
		transition-duration: var(--motion-hover-in);
	}
	.name {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.fact.plain {
		font-size: var(--text-label);
		color: var(--color-foreground);
	}
	/* No home yet: quieter than a real value, as every empty property is. */
	.fact.empty {
		color: var(--color-foreground-subtle);
	}
	.fact.empty:hover {
		color: var(--color-foreground);
	}
	.menu {
		display: flex;
		flex-direction: column;
		min-width: 240px;
		max-height: 360px;
		overflow-y: auto;
		padding: 4px;
	}
	@media (prefers-reduced-motion: reduce) {
		.fact {
			transition: none;
		}
	}
</style>
