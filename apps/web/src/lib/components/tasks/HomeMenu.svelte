<script lang="ts">
	/**
	 * The choices for where a task lives, wherever a home is picked: the New
	 * task dialog and a task's page and preview. A field to narrow them, then
	 * no home, your open threads in their own glyph and color, your things,
	 * and a search for a person, place or organization. Return picks the
	 * first match.
	 *
	 * Only the contents: the caller draws the surface it sits on.
	 */
	import type { TaskLink } from '$lib/api/client';
	import MenuItem from '$lib/components/MenuItem.svelte';
	import ProjectGlyph from '$lib/components/ProjectGlyph.svelte';
	import { projectStore } from '$lib/stores/project.svelte';
	import { thingsStore } from '$lib/stores/things.svelte';
	import { sameLink, subjectIcon, subjectName } from '$lib/tasks/homes';
	import { onMount } from 'svelte';

	let {
		value,
		onPick,
		onSearch,
	}: {
		value: TaskLink | null;
		onPick: (link: TaskLink | null) => void;
		/** Open the search for a person, place or organization. */
		onSearch: () => void;
	} = $props();

	let filter = $state('');
	let field = $state<HTMLInputElement | null>(null);
	const q = $derived(filter.trim().toLowerCase());
	const threads = $derived(projectStore.projects.filter((p) => !q || p.name.toLowerCase().includes(q)));
	const things = $derived(thingsStore.things.filter((t) => !q || t.title.toLowerCase().includes(q)));
	/** A person, place or organization home is not in either list, so it is shown on its own. */
	const searched = $derived(value && (value.kind === 'person' || value.kind === 'place' || value.kind === 'org') ? value : null);

	// `autofocus` does nothing in content added after the page loads.
	onMount(() => {
		thingsStore.ensure();
		requestAnimationFrame(() => field?.focus({ preventScroll: true }));
	});

	function first(): TaskLink | null {
		if (threads[0]) return { kind: 'thread', id: threads[0].id };
		if (things[0]) return { kind: 'thing', id: things[0].id };
		return null;
	}
</script>

<input
	bind:this={field}
	class="filter"
	placeholder="Find a thread or thing"
	aria-label="Find a home"
	bind:value={filter}
	onkeydown={(e) => {
		if (e.key === 'Enter') {
			e.preventDefault();
			const link = first();
			if (link) onPick(link);
		}
	}}
/>
{#if !q}
	<MenuItem role="menuitemradio" icon="mynaui:circle" checked={!value} label="No home" onclick={() => onPick(null)} />
	{#if searched}
		<MenuItem role="menuitemradio" icon={subjectIcon(searched.kind)} checked label={subjectName(searched)} onclick={() => onPick(searched)} />
	{/if}
{/if}
{#if threads.length > 0}
	<p class="menu-label">Threads</p>
	{#each threads as p (p.id)}
		<MenuItem
			role="menuitemradio"
			checked={sameLink(value, { kind: 'thread', id: p.id })}
			label={p.name || 'Untitled thread'}
			onclick={() => onPick({ kind: 'thread', id: p.id })}
		>
			{#snippet leading()}<ProjectGlyph project={p} size={14} tinted />{/snippet}
		</MenuItem>
	{/each}
{/if}
{#if things.length > 0}
	<p class="menu-label">Things</p>
	{#each things as th (th.id)}
		<MenuItem
			role="menuitemradio"
			icon="atlas:things"
			checked={sameLink(value, { kind: 'thing', id: th.id })}
			label={th.title}
			onclick={() => onPick({ kind: 'thing', id: th.id })}
		/>
	{/each}
{/if}
<MenuItem icon="atlas:people" label="A person, place or organization…" onclick={onSearch} />

<style>
	.filter {
		height: 32px;
		margin: 0 0 4px;
		padding: 0 8px;
		border: 0;
		border-radius: var(--radius-md);
		background: var(--fill-bg);
		font: inherit;
		font-size: var(--text-label);
		color: var(--color-foreground);
		outline: none;
	}
	.filter::placeholder {
		color: var(--color-foreground-subtle);
	}
	.menu-label {
		margin: 0;
		padding: 8px 8px 4px;
		font-size: var(--text-caption);
		color: var(--color-foreground-subtle);
	}
</style>
