<script lang="ts">
	/**
	 * The projects a page is in, in the page's top bar: the same chip a chat
	 * wears, one per project, since a page can be in several. Nothing when it
	 * is in none; filing it is "Add to project…" in the page's menu, or a drag
	 * from the sidebar.
	 *
	 * Click opens the project; right-click is the project's menu (open it,
	 * file the page elsewhere too, take it out of this one).
	 */
	import ProjectChip from '$lib/components/ProjectChip.svelte';
	import { projectStore } from '$lib/stores/project.svelte';
	import { contextMenu } from '$lib/stores/contextMenu.svelte';
	import { openProject, projectChipMenuItems } from '$lib/utils/projectActions';
	import type { ProjectSummary } from '$lib/api/client';

	let { url }: { url: string } = $props();

	$effect(() => {
		void projectStore.loadHolders(url);
	});

	const projects = $derived(projectStore.holding(url));

	function showMenu(e: MouseEvent, project: ProjectSummary) {
		e.preventDefault();
		const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
		contextMenu.show({ x: rect.left, y: rect.bottom }, projectChipMenuItems(project, { url }), {
			anchor: { x: rect.left, y: rect.top, width: rect.width, height: rect.height },
			placement: 'bottom-start',
		});
	}
</script>

{#if projects.length > 0}
	<div class="page-projects" aria-label="Projects this page is in">
		{#each projects as project (project.id)}
			<ProjectChip
				{project}
				title={`Open ${project.name}`}
				onclick={() => openProject(project)}
				oncontextmenu={(e) => showMenu(e, project)}
			/>
		{/each}
	</div>
{/if}

<style>
	.page-projects {
		display: flex;
		align-items: center;
		gap: 4px;
		min-width: 0;
		overflow: hidden;
	}

	.page-projects > :global(*) {
		flex-shrink: 1;
		min-width: 48px;
	}
</style>
