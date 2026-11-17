<script lang="ts">
	/**
	 * What else a task is about: each as a link to it with a way to remove
	 * it, and Add, which searches threads and subjects. The task's home is
	 * not offered, because it is already there.
	 */
	import type { Task, TaskLink, TaskPatch } from '$lib/api/client';
	import Icon from '$lib/components/Icon.svelte';
	import RefPicker from '$lib/components/RefPicker.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { homeOf, linkFromUrl, sameLink, subjectIcon, subjectName, subjectUrl } from '$lib/tasks/homes';

	let {
		task,
		onSave,
		readonly = false,
		plain = false,
	}: {
		task: Task;
		onSave: (patch: TaskPatch) => void;
		readonly?: boolean;
		/** Full-size ink, for a column of properties. */
		plain?: boolean;
	} = $props();

	let searchAt = $state<{ x: number; y: number } | null>(null);
	const links = $derived(task.also_about ?? []);

	function save(next: TaskLink[]) {
		onSave({ also_about: next });
	}

	function found(entity: { url: string }) {
		searchAt = null;
		const link = linkFromUrl(entity.url);
		if (!link || links.some((l) => sameLink(l, link)) || sameLink(link, homeOf(task))) return;
		save([...links, link]);
	}
</script>

<span class="also" class:plain>
	{#each links as link (link.kind + link.id)}
		<span class="chip">
			<button
				type="button"
				class="fact"
				onclick={() => windowShellStore.openTabFromRoute(subjectUrl(link), { focusExisting: true })}
			>
				<Icon icon={subjectIcon(link.kind)} width="12" />
				{subjectName(link)}
			</button>
			{#if !readonly}
				<button
					type="button"
					class="remove"
					aria-label={`No longer about ${subjectName(link)}`}
					onclick={() => save(links.filter((l) => !sameLink(l, link)))}
				>
					<Icon icon="mynaui:x" width="12" />
				</button>
			{/if}
		</span>
	{/each}
	{#if !readonly}
		<button type="button" class="fact add" onclick={(e) => (searchAt = { x: e.clientX, y: e.clientY })}>Add</button>
	{:else if links.length === 0}
		<span class="none">Nothing else</span>
	{/if}
</span>

{#if searchAt}
	<RefPicker
		mode="single"
		position={searchAt}
		placeholder="Find a thread, person, place, organization or thing…"
		excludeIds={[...links.map(subjectUrl), ...(homeOf(task) ? [subjectUrl(homeOf(task)!)] : [])]}
		filter={(e) => !!linkFromUrl(e.url)}
		onSelect={found}
		onClose={() => (searchAt = null)}
	/>
{/if}

<style>
	.also {
		display: inline-flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 4px 12px;
	}
	.chip {
		display: inline-flex;
		align-items: center;
	}
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
	.remove {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 20px;
		height: 20px;
		margin-left: 4px;
		padding: 0;
		border: 0;
		border-radius: var(--radius-sm);
		background: transparent;
		color: var(--color-foreground-muted);
		cursor: pointer;
	}
	.remove:hover {
		background: var(--hover-bg);
		color: var(--color-foreground);
	}
	.plain .fact,
	.plain .none {
		font-size: 14px;
		color: var(--color-foreground);
	}
	.plain .add {
		color: var(--color-primary);
	}
	.none {
		font-size: 13px;
		color: var(--color-foreground-muted);
	}
	@media (prefers-reduced-motion: reduce) {
		.fact {
			transition: none;
		}
	}
</style>
